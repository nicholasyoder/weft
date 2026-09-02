use std::collections::HashMap;

use engine_assets::animation::AnimationClip;
use engine_assets::skeleton::Skeleton;
use engine_assets::{animation, skeleton, AssetStore};
use engine_core::scheduler::SystemArgs;
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

impl AnimCache {
    /// Decodes and caches the skeleton at `hash` if not already cached. An
    /// unresolvable or corrupt hash panics with a clear message — this is
    /// scene-authored data reaching a system with no `Result`-returning
    /// path (`System = fn(&mut SystemArgs)`), so a loud failure here is the
    /// closest available equivalent to this codebase's usual structured
    /// `{code, message}` errors, not a deliberately silent no-op. Compare
    /// to a missing `AssetsDir` (see `animation_step`), which *is* a
    /// legitimate silent no-op — "no asset store configured at all" is a
    /// valid engine-wide state; "this specific hash doesn't resolve" is a
    /// content bug.
    fn ensure_skeleton(&mut self, store: &AssetStore, hash: &str) {
        if self.skeletons.contains_key(hash) {
            return;
        }
        let bytes = store
            .get(hash)
            .unwrap_or_else(|e| panic!("Animator references unknown skeleton '{hash}': {e}"));
        let decoded = skeleton::decode(&bytes)
            .unwrap_or_else(|e| panic!("failed to decode skeleton asset '{hash}': {e}"));
        self.skeletons.insert(hash.to_string(), decoded);
    }

    fn ensure_clip(&mut self, store: &AssetStore, hash: &str) {
        if self.clips.contains_key(hash) {
            return;
        }
        let bytes = store
            .get(hash)
            .unwrap_or_else(|e| panic!("Animator references unknown clip '{hash}': {e}"));
        let decoded = animation::decode(&bytes)
            .unwrap_or_else(|e| panic!("failed to decode animation clip '{hash}': {e}"));
        self.clips.insert(hash.to_string(), decoded);
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
/// A silent no-op (not a panic) if no `AssetsDir` resource is present —
/// e.g. `engine test`/`inspect`/`run` invoked with no asset store
/// configured at all. See `AnimCache`'s doc comment for why an
/// *unresolvable hash*, once an `AssetsDir` does exist, is instead a loud
/// panic rather than a second silent case.
pub fn animation_step(args: &mut SystemArgs) {
    let Some(assets_dir) = args.resources.get::<AssetsDir>() else {
        return;
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

            cache.ensure_clip(&store, &animator.clip);
            let duration = cache.clip(&animator.clip).duration.max(f32::EPSILON);

            if animator.playing {
                animator.time += args.dt * animator.speed;
            }
            animator.time = if animator.looping {
                animator.time.rem_euclid(duration)
            } else {
                animator.time.clamp(0.0, duration)
            };

            cache.ensure_skeleton(&store, &animator.skeleton);
            let skeleton = cache.skeleton(&animator.skeleton);
            let clip = cache.clip(&animator.clip);
            sampling::sample(skeleton, clip, animator.time)
        };

        let palette = JointPalette {
            matrices: matrices.into_iter().map(|m| m.to_cols_array_2d()).collect(),
        };
        let _ = args.world.insert_one(entity, palette);
    }
}
