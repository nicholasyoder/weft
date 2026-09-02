use std::path::Path;
use std::sync::Arc;

use kira::Frame;

use crate::error::AudioError;

struct Voice {
    frames: Arc<[Frame]>,
    clip_rate: u32,
    /// Fractional position in *source* (clip-rate) frame units — advanced
    /// by `clip_rate / sample_rate` per output sample, which is what makes
    /// this a linear-interpolation resampler rather than requiring every
    /// clip to already match the mixdown's output rate.
    pos: f64,
    volume: f32,
    looping: bool,
}

/// A deterministic, device-free mixer — the offline half of the
/// offscreen/live split `engine render`/`engine play` already established
/// (ADR-0004/0010), applied to audio (ADR-0016). Voices are summed via
/// simple linear-interpolation resampling; the whole thing is pure
/// arithmetic over already-decoded samples, so `engine mix` run twice with
/// the same seed/ticks produces byte-identical WAV output, the same
/// discipline every other deterministic-output command in this engine
/// holds to.
///
/// Unlike `LiveAudioBackend` (a thin wrapper over kira's own real-time
/// mixer), this doesn't hand back a per-voice handle — a one-shot or
/// looping voice, once started, just runs until it either finishes (one-
/// shot) or the mixdown ends (loop). `audio_step`'s despawn-eviction only
/// removes the live backend's tracked handles for this reason.
#[derive(Default)]
pub struct Mixdown {
    sample_rate: u32,
    samples: Vec<Frame>,
    voices: Vec<Voice>,
}

impl Mixdown {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            samples: Vec::new(),
            voices: Vec::new(),
        }
    }

    pub fn play(&mut self, frames: Arc<[Frame]>, clip_rate: u32, volume: f32, looping: bool) {
        self.voices.push(Voice {
            frames,
            clip_rate,
            pos: 0.0,
            volume,
            looping,
        });
    }

    /// Renders `dt` seconds of audio, appending to the internal buffer and
    /// advancing (or, for a finished non-looping voice, removing) every
    /// active voice.
    pub fn render(&mut self, dt: f32) {
        let frame_count = ((dt as f64) * self.sample_rate as f64).round() as usize;
        for _ in 0..frame_count {
            let mut mixed = Frame::ZERO;
            self.voices.retain_mut(|voice| {
                let sample = sample_at(&voice.frames, voice.pos);
                mixed.left += sample.left * voice.volume;
                mixed.right += sample.right * voice.volume;

                voice.pos += voice.clip_rate as f64 / self.sample_rate as f64;
                let len = voice.frames.len() as f64;
                if len <= 0.0 {
                    return false;
                }
                if voice.pos >= len {
                    if voice.looping {
                        voice.pos %= len;
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            });
            self.samples.push(Frame::new(
                mixed.left.clamp(-1.0, 1.0),
                mixed.right.clamp(-1.0, 1.0),
            ));
        }
    }

    pub fn write_wav(&self, path: &Path) -> Result<(), AudioError> {
        let map_err = |source: hound::Error| AudioError::MixWriteFailed {
            path: path.display().to_string(),
            source,
        };
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).map_err(map_err)?;
        for frame in &self.samples {
            writer
                .write_sample((frame.left * i16::MAX as f32) as i16)
                .map_err(map_err)?;
            writer
                .write_sample((frame.right * i16::MAX as f32) as i16)
                .map_err(map_err)?;
        }
        writer.finalize().map_err(map_err)
    }

    #[cfg(test)]
    pub(crate) fn samples(&self) -> &[Frame] {
        &self.samples
    }
}

fn sample_at(frames: &[Frame], pos: f64) -> Frame {
    if frames.is_empty() {
        return Frame::ZERO;
    }
    let i0 = pos.floor() as usize;
    let frac = (pos - i0 as f64) as f32;
    let f0 = frames.get(i0).copied().unwrap_or(Frame::ZERO);
    let f1 = frames.get(i0 + 1).copied().unwrap_or(f0);
    Frame::new(
        f0.left + (f1.left - f0.left) * frac,
        f0.right + (f1.right - f0.right) * frac,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant_clip(value: f32, len: usize) -> Arc<[Frame]> {
        vec![Frame::new(value, value); len].into()
    }

    #[test]
    fn silence_with_no_voices() {
        let mut mix = Mixdown::new(1000);
        mix.render(0.01);
        assert_eq!(mix.samples().len(), 10);
        assert!(mix.samples().iter().all(|f| *f == Frame::ZERO));
    }

    #[test]
    fn one_shot_at_matching_rate_reproduces_the_clip_then_stops() {
        let mut mix = Mixdown::new(1000);
        mix.play(constant_clip(0.5, 5), 1000, 1.0, false);
        mix.render(0.01); // 10 output samples, clip is 5 long
        let samples = mix.samples();
        assert_eq!(samples.len(), 10);
        for s in &samples[0..5] {
            assert!((s.left - 0.5).abs() < 1e-6, "{s:?}");
        }
        for s in &samples[5..10] {
            assert_eq!(*s, Frame::ZERO, "voice should have stopped after 5 samples");
        }
    }

    #[test]
    fn volume_scales_linearly() {
        let mut mix = Mixdown::new(1000);
        mix.play(constant_clip(1.0, 4), 1000, 0.25, false);
        mix.render(0.004);
        for s in mix.samples() {
            assert!((s.left - 0.25).abs() < 1e-6, "{s:?}");
        }
    }

    #[test]
    fn looping_voice_wraps_and_never_stops() {
        let mut mix = Mixdown::new(1000);
        mix.play(constant_clip(0.3, 3), 1000, 1.0, true);
        mix.render(0.01); // 10 samples from a 3-sample looping clip
        assert_eq!(mix.samples().len(), 10);
        for s in mix.samples() {
            assert!((s.left - 0.3).abs() < 1e-6, "{s:?}");
        }
    }

    #[test]
    fn two_voices_sum_and_clamp() {
        let mut mix = Mixdown::new(1000);
        mix.play(constant_clip(0.8, 4), 1000, 1.0, true);
        mix.play(constant_clip(0.8, 4), 1000, 1.0, true);
        mix.render(0.004);
        for s in mix.samples() {
            assert_eq!(s.left, 1.0, "expected clamping to +1.0, got {s:?}");
        }
    }

    #[test]
    fn same_inputs_produce_byte_identical_output() {
        let run = || {
            let mut mix = Mixdown::new(8000);
            mix.play(constant_clip(0.6, 200), 8000, 0.9, false);
            mix.play(constant_clip(-0.2, 50), 8000, 0.5, true);
            for _ in 0..5 {
                mix.render(1.0 / 60.0);
            }
            mix.samples().to_vec()
        };
        let a: Vec<(u32, u32)> = run()
            .iter()
            .map(|f| (f.left.to_bits(), f.right.to_bits()))
            .collect();
        let b: Vec<(u32, u32)> = run()
            .iter()
            .map(|f| (f.left.to_bits(), f.right.to_bits()))
            .collect();
        assert_eq!(a, b);
    }
}
