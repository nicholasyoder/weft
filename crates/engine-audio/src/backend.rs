use kira::backend::cpal::CpalBackend;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels};

use crate::mixdown::Mixdown;

/// Converts a linear volume (`0.0` silent, `1.0` full) into kira's own
/// `Decibels` unit — kira's APIs (`StaticSoundData::volume`,
/// `TrackHandle::set_volume`) all take `Decibels`, not a linear scale, so
/// every volume this crate threads through from scene files/`AudioSettings`
/// needs this conversion at the point it actually reaches kira.
pub(crate) fn linear_to_decibels(volume: f32) -> Decibels {
    if volume <= 0.0001 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * volume.log10())
    }
}

/// Live device-output backend for `engine play` (see ADR-0016), opened
/// once alongside `WindowRenderer`. Two sub-tracks (`music`/`sfx`) exist to
/// keep one-shots and loops separately routable later (effects, ducking);
/// for now `audio_step` pre-multiplies `AudioSettings.master` and the
/// relevant group volume directly into each sound's own volume before it
/// ever reaches this backend, the same arithmetic the offline `Mixdown`
/// has to do (it has no track/sub-track concept at all) — one shared
/// volume formula for both backends rather than two.
pub struct LiveAudioBackend {
    // Never read after construction, but must stay alive: dropping the
    // `AudioManager` tears down its `cpal` output stream, stopping
    // playback entirely.
    #[allow(dead_code)]
    manager: AudioManager<CpalBackend>,
    music_track: TrackHandle,
    sfx_track: TrackHandle,
}

impl LiveAudioBackend {
    /// Opens the real audio device. Fails cleanly (no panic) if none is
    /// available — `kira::backend::cpal::Error::NoDefaultOutputDevice` is
    /// a real, expected outcome in a headless sandbox, not a programming
    /// error; callers (`live::play`) should log and continue with no
    /// backend at all rather than treat this as fatal.
    pub fn new() -> Result<Self, kira::backend::cpal::Error> {
        let mut manager = AudioManager::<CpalBackend>::new(AudioManagerSettings::default())?;
        let music_track = manager
            .add_sub_track(TrackBuilder::new())
            .expect("default sub-track capacity comfortably covers one music + one sfx track");
        let sfx_track = manager
            .add_sub_track(TrackBuilder::new())
            .expect("default sub-track capacity comfortably covers one music + one sfx track");
        Ok(Self {
            manager,
            music_track,
            sfx_track,
        })
    }
}

/// Which backend `audio_step` should render/trigger through, if any at
/// all — batch commands (`test`/`inspect`/`run`/`replay`) never insert
/// either variant, so `audio_step` always takes its tracking-only branch
/// (see ADR-0016).
pub enum AudioBackend {
    Live(Box<LiveAudioBackend>),
    Mix(Mixdown),
}

impl AudioBackend {
    /// Fires a one-shot at `volume` — the caller (`audio_step`) has
    /// already pre-multiplied `AudioSettings.master * .sfx` into it, the
    /// same formula for both backends (see this module's doc comment).
    pub(crate) fn play_one_shot(&mut self, clip: &StaticSoundData, volume: f32) {
        match self {
            AudioBackend::Live(live) => {
                let sound = clip.clone().volume(linear_to_decibels(volume));
                let _ = live.sfx_track.play(sound);
            }
            AudioBackend::Mix(mixdown) => {
                mixdown.play(clip.frames.clone(), clip.sample_rate, volume, false);
            }
        }
    }

    /// Starts a (typically looping) `AudioSource`. Returns a live handle
    /// for later `stop()`-on-despawn when this is the live backend — the
    /// offline `Mixdown` has no per-voice handle concept, so it returns
    /// `None` unconditionally and just runs the voice for the rest of the
    /// mixdown.
    pub(crate) fn play_source(
        &mut self,
        clip: &StaticSoundData,
        volume: f32,
        looping: bool,
    ) -> Option<StaticSoundHandle> {
        match self {
            AudioBackend::Live(live) => {
                let mut sound = clip.clone().volume(linear_to_decibels(volume));
                if looping {
                    sound = sound.loop_region(..);
                }
                live.music_track.play(sound).ok()
            }
            AudioBackend::Mix(mixdown) => {
                mixdown.play(clip.frames.clone(), clip.sample_rate, volume, looping);
                None
            }
        }
    }
}
