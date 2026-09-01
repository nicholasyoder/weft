# ADR-0008: Physics integration shape and the `Resources` extension point

- **Status**: accepted
- **Date**: 2026-08-31

## Context

[ROADMAP.md](../roadmap/completed-phases.md) Phase 6 integrates `rapier3d` behind engine-native component/system types. Every phase through Phase 5 built systems that are stateless between ticks: `type System = fn(&mut SystemArgs)` is a bare fn pointer, and `SystemArgs` (`engine-core/src/scheduler.rs`) carries only `world`, `rng`, `tick`, `dt`. `Sim` (`engine-core/src/sim.rs`) special-cases exactly two pieces of cross-tick state: the ECS `world` itself and the seeded `rng`.

Physics genuinely needs a third kind of cross-tick state that doesn't fit either: rapier's own `RigidBodySet`/`ColliderSet`/`PhysicsPipeline`/`IslandManager`/`BroadPhase`/`NarrowPhase`/`CCDSolver`, plus a `hecs::Entity` → `RigidBodyHandle` map. This isn't ECS data (scene files/agents should never see or author it directly) and it isn't a single well-known singleton like `rng` — it needs to persist across ticks *within one `Sim`'s lifetime* so rapier's contact/sleep state and body velocities aren't lost every tick. A grep across `engine-core/src` confirmed no existing extension point covers this.

## Decision

Add a small type-erased resource bag, `engine_core::Resources` (`engine-core/src/resources.rs`): a `HashMap<TypeId, Box<dyn Any>>` behind `insert<T>`/`get<T>`/`get_mut<T>`/`get_or_insert_with<T>`. `Sim` gains a `resources: Resources` field alongside `world`/`rng`; `SystemArgs` gains `resources: &'a mut Resources`; `Scheduler::tick`/`Sim::step` thread it through exactly the way `rng` already is.

`engine-physics`'s `physics_step` system lazily does `args.resources.get_or_insert_with(PhysicsWorld::default)` on first use — no special init phase needed, keeping every system a plain, uniformly-registered `fn(&mut SystemArgs)`.

Determinism: `Resources` is keyed by type and never iterated, so it introduces no collection-iteration-order question of the kind ADR-0002 cares about. The one rapier-specific determinism caveat (deterministic per machine/build, not bit-identical across hardware/compiler versions) is pre-accepted by the roadmap's own Phase 6 text, not a new decision here.

## Alternatives considered

- **Special-case physics state as new named fields on `Sim`, the same way `rng` is special-cased.** Rejected: doesn't generalize — the next stateful subsystem (audio? networking transport state?) would need its own bespoke field and its own threading through `SystemArgs` again. A generic bag pays that cost once.
- **Make `System` a `Box<dyn FnMut(&mut SystemArgs)>` closure so systems can close over their own state.** Rejected: touches every existing system's registration call site for a capability most systems don't need, and doesn't help when the state must be *shared* across systems (e.g. a future "physics debug overlay" system reading the same `PhysicsWorld` a `physics_step` system writes) — closures only give a system access to state it captured itself.
- **Give `engine-physics` its own bespoke persistence mechanism entirely outside `engine-core`** (e.g. a global/thread-local). Rejected outright — contradicts the project's explicit no-ambient-state discipline (ADR-0002) that every prior phase has held to.

## Consequences

- `engine-core` now has exactly one generic extension point for cross-tick, non-ECS, non-`rng` state, used first by `engine-physics`'s `PhysicsWorld` (rapier's sets/pipeline + the entity↔handle map) and available to any future stateful subsystem without another `engine-core` change.
- `SystemArgs` grew a field; every existing system function only *receives* `&mut SystemArgs` and never constructs one, so no call site outside `engine-core::scheduler` needed to change.
- `Resources` has no despawn/eviction story yet — entries only get inserted, never removed, matching the fact that no entity in this engine is despawned yet either (see the corresponding note in `engine-physics`'s own scope for Phase 6).

## Revisit when

- A second stateful subsystem lands and its usage pattern reveals `get_or_insert_with`'s implicit lazy-init isn't the right shape for it (e.g. it needs configuration at `Sim::new` time rather than lazily on first tick).
- Entity despawn is added to the engine — at that point `PhysicsWorld`'s (and any other resource's) entity-keyed maps need an eviction path, and `Resources` itself may need one too if a resource's lifetime should ever be shorter than the `Sim`'s.
