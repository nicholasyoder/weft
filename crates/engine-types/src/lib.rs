//! Leaf crate for the small set of types that exist purely so a
//! producer/consumer crate pair (e.g. `engine-anim` → `engine-render` for
//! `JointPalette`) can share a type without either depending on the other,
//! or on all of `engine-core`'s heavier simulation-kernel surface (`hecs`
//! re-export, `Resources`, `Scheduler`/`Sim`) just to name one type. No
//! dependency on `engine-core` or anything else in the workspace — see
//! `docs/roadmap/debt-cleanup-plan.md`'s Phase 3. `engine-core` re-exports
//! everything here for convenience, so existing `engine_core::Transform`
//! (etc.) call sites are unaffected.

pub mod assets_dir;
pub mod audio_events;
pub mod audio_settings;
pub mod input;
pub mod joint_palette;
pub mod mouse;
pub mod transform;

pub use assets_dir::AssetsDir;
pub use audio_events::{SoundEvent, SoundEventQueue};
pub use audio_settings::AudioSettings;
pub use input::{Input, KeyCode};
pub use joint_palette::JointPalette;
pub use mouse::MouseDelta;
pub use transform::Transform;
