use std::collections::HashMap;

use engine_assets::animation::AnimationClip;
use engine_assets::skeleton::Skeleton;
use engine_assets::{animation, skeleton, AssetError, AssetStore};
use engine_core::scheduler::{SystemArgs, SystemError};
use engine_core::{AssetsDir, JointPalette};

use crate::components::Animator;
use crate::sampling;

/// Lazily-decoded skeleton/clip data, held in a `Sim`'s `Resources` bag —
/// the same "cache keyed by content hash, decoded once, reused every tick"
/// shape `engine-render`'s `mesh_cache`/`texture_cache`/`font_cache`
/// already use, applied here to a cross-tick simulation resource instead
/// of a render-only one (see ADR-0015).
#[derive(Default)]
pub struct AnimCache {
    skeletons: HashMap<String, Skeleton>,
    clips: HashMap<String, AnimationClip>,
}

/// Converts an `AssetError` (unresolvable hash, corrupt asset bytes) into
/// the `SystemError` `animation_step` returns — can't be a `From` impl
/// (`SystemError` and `AssetError` are both foreign to this crate, so the
/// orphan rules block it), so it's a plain function instead.
fn to_system_error(e: AssetError) -> SystemError {
    SystemError::new(e.code(), e.to_string())
}

impl AnimCache {
    /// Decodes and caches the skeleton at `hash` if not already cached.
    /// Compare to a missing `AssetsDir` (see `animation_step`), which *is*
    /// a legitimate silent no-op — "no asset store configured at all" is a
    /// valid engine-wide state; "this specific hash doesn't resolve" is a
    /// content bug, reported as a structured `SystemError` rather than
    /// silently ignored or panicking the process.
    fn ensure_skeleton(&mut self, store: &AssetStore, hash: &str) -> Result<(), SystemError> {
        if self.skeletons.contains_key(hash) {
            return Ok(());
        }
        let bytes = store.get(hash).map_err(to_system_error)?;
        let decoded = skeleton::decode(&bytes).map_err(to_system_error)?;
        self.skeletons.insert(hash.to_string(), decoded);
        Ok(())
    }

    fn ensure_clip(&mut self, store: &AssetStore, hash: &str) -> Result<(), SystemError> {
        if self.clips.contains_key(hash) {
            return Ok(());
        }
        let bytes = store.get(hash).map_err(to_system_error)?;
        let decoded = animation::decode(&bytes).map_err(to_system_error)?;
        self.clips.insert(hash.to_string(), decoded);
        Ok(())
    }

    fn skeleton(&self, hash: &str) -> &Skeleton {
        self.skeletons
            .get(hash)
            .expect("ensure_skeleton must be called before skeleton")
    }

    fn clip(&self, hash: &str) -> &AnimationClip {
        self.clips
            .get(hash)
            .expect("ensure_clip must be called before clip")
    }
}

/// Advances every `Animator`'s clock by `dt * speed`, samples its clip
/// (see `sampling::sample`, a pure function — this is what keeps the whole
/// system deterministic), and writes the result as that entity's
/// `JointPalette`. Registered into `SystemRegistry` as `"animation"`,
/// mirroring `engine-physics`'s `physics_step` precedent (ADR-0008).
///
/// A silent no-op (not an error) if no `AssetsDir` resource is present —
/// e.g. `engine test`/`inspect`/`run` invoked with no asset store
/// configured at all. Once an `AssetsDir` does exist, an unresolvable or
/// corrupt hash is instead a structured `SystemError` (see
/// `AnimCache::ensure_skeleton`/`ensure_clip`) — "no asset store at all" is
/// a legitimate engine-wide state, "this specific hash doesn't resolve" is
/// a content bug the caller should see, not a second silent case.
pub fn animation_step(args: &mut SystemArgs) -> Result<(), SystemError> {
    let Some(assets_dir) = args.resources.get::<AssetsDir>() else {
        return Ok(());
    };
    let store = AssetStore::new(assets_dir.0.clone());
    let cache = args.resources.get_or_insert_with(AnimCache::default);

    let entities: Vec<hecs::Entity> = args
        .world
        .query::<&Animator>()
        .iter()
        .map(|(entity, _)| entity)
        .collect();

    for entity in entities {
        let matrices = {
            let mut animator = args.world.get::<&mut Animator>(entity).expect(
                "entity came from a query over &Animator taken moments ago on this same world",
            );

            cache.ensure_clip(&store, &animator.clip)?;
            let duration = cache.clip(&animator.clip).duration.max(f32::EPSILON);

            if animator.playing {
                animator.time += args.dt * animator.speed;
            }
            animator.time = if animator.looping {
                animator.time.rem_euclid(duration)
            } else {
                animator.time.clamp(0.0, duration)
            };

            cache.ensure_skeleton(&store, &animator.skeleton)?;
            let skeleton = cache.skeleton(&animator.skeleton);
            let clip = cache.clip(&animator.clip);
            sampling::sample(skeleton, clip, animator.time)
        };

        let palette = JointPalette {
            matrices: matrices.into_iter().map(|m| m.to_cols_array_2d()).collect(),
        };
        let _ = args.world.insert_one(entity, palette);
    }
    Ok(())
}
