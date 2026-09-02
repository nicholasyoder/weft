use std::path::Path;
use std::sync::Arc;

use kira::Frame;

use crate::error::AudioError;

struct Voice {
    id: u64,
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
/// mixer), a voice here is identified by a plain `u64` id (`play`'s return
/// value) rather than a real device handle — but it's stoppable the same
/// way: a one-shot or looping voice, once started, keeps running until it
/// either finishes (one-shot), the mixdown ends (loop), or `stop_voice` is
/// called on it (e.g. by `audio_step`'s despawn-eviction).
#[derive(Default)]
pub struct Mixdown {
    sample_rate: u32,
    samples: Vec<Frame>,
    voices: Vec<Voice>,
    next_voice_id: u64,
    /// Fractional output-frame count left over from the last `render`
    /// call, carried into the next one — without this, rounding
    /// `dt * sample_rate` independently per call drifts the total frame
    /// count away from `elapsed_time * sample_rate` whenever `sample_rate`
    /// doesn't evenly divide the tick rate (true of this crate's own
    /// golden-WAV fixture, 8000Hz @ 60Hz).
    frame_accum: f64,
}

impl Mixdown {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            samples: Vec::new(),
            voices: Vec::new(),
            next_voice_id: 0,
            frame_accum: 0.0,
        }
    }

    /// Starts a voice and returns an id `stop_voice` can later use to end
    /// it early — the offline equivalent of the live backend's
    /// `StaticSoundHandle`, needed so despawn-eviction (`audio_step`'s
    /// `AudioCache::evict_despawned`) can stop a looping voice here too,
    /// not just on the live backend.
    pub fn play(
        &mut self,
        frames: Arc<[Frame]>,
        clip_rate: u32,
        volume: f32,
        looping: bool,
    ) -> u64 {
        let id = self.next_voice_id;
        self.next_voice_id += 1;
        self.voices.push(Voice {
            id,
            frames,
            clip_rate,
            pos: 0.0,
            volume,
            looping,
        });
        id
    }

    /// Ends a voice immediately (no fade — unlike the live backend's
    /// `Tween::default()` stop, `Mixdown` has no fade concept; this is a
    /// deliberate difference, not a bug). A no-op if `id` already finished
    /// (a one-shot that ran out) or was never valid.
    pub fn stop_voice(&mut self, id: u64) {
        self.voices.retain(|voice| voice.id != id);
    }

    /// Renders `dt` seconds of audio, appending to the internal buffer and
    /// advancing (or, for a finished non-looping voice, removing) every
    /// active voice.
    pub fn render(&mut self, dt: f32) {
        // The tiny epsilon before `floor` absorbs `dt`'s own f32->f64
        // promotion error (e.g. `0.01f32 as f64` is a hair under `0.01`) —
        // without it, a `dt`/`sample_rate` pair that's supposed to land on
        // a whole frame count (like the unit tests below deliberately use)
        // would floor one frame short essentially every time, which
        // `.round()`'s old per-call behavior happened to paper over.
        const EPSILON: f64 = 1e-6;
        self.frame_accum += dt as f64 * self.sample_rate as f64;
        let frame_count = (self.frame_accum + EPSILON).floor() as usize;
        self.frame_accum -= frame_count as f64;
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
    fn stop_voice_silences_it_immediately_but_leaves_other_voices_playing() {
        let mut mix = Mixdown::new(1000);
        let stopped = mix.play(constant_clip(0.3, 3), 1000, 1.0, true);
        mix.play(constant_clip(0.6, 3), 1000, 1.0, true);
        mix.render(0.002); // 2 samples with both voices active
        mix.stop_voice(stopped);
        mix.render(0.002); // 2 more samples with only the second voice active
        let samples = mix.samples();
        assert_eq!(samples.len(), 4);
        for s in &samples[0..2] {
            assert!((s.left - 0.9).abs() < 1e-6, "{s:?}");
        }
        for s in &samples[2..4] {
            assert!((s.left - 0.6).abs() < 1e-6, "{s:?}");
        }
    }

    #[test]
    fn stop_voice_on_an_already_finished_one_shot_is_a_no_op() {
        let mut mix = Mixdown::new(1000);
        let id = mix.play(constant_clip(0.5, 2), 1000, 1.0, false);
        mix.render(0.002); // exhausts the 2-sample one-shot
        mix.stop_voice(id); // must not panic on a since-removed voice
        assert_eq!(mix.samples().len(), 2);
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

    /// Regression test for known-issues.md's "no carried fractional
    /// remainder" bug: at 8000Hz/60Hz (this crate's own golden-WAV
    /// fixture rate — 8000.0 / 60.0 = 133.33... frames/tick, not a whole
    /// number), rendering many ticks independently used to round each
    /// tick's frame count on its own and drift away from the true
    /// elapsed-time frame count. 600 ticks of 1/60s at 8000Hz is exactly
    /// 10 seconds — 80000 frames — a boundary naive per-tick rounding
    /// would very likely miss.
    #[test]
    fn many_ticks_track_elapsed_time_without_drift() {
        let mut mix = Mixdown::new(8000);
        for _ in 0..600 {
            mix.render(1.0 / 60.0);
        }
        assert_eq!(mix.samples().len(), 80000);
    }
}
