# ADR-0011: Entity despawn via reactive per-tick eviction, not a generic hook

- **Status**: accepted
- **Date**: 2026-09-01

## Context

[ADR-0008](0008-physics-and-scheduler-resources.md) flagged, twice, that no entity in the engine was ever despawned: `Resources` only ever grows (insert-only, no `remove<T>`), and `engine-physics`'s `PhysicsState.bodies: HashMap<hecs::Entity, RigidBodyHandle>` would leak an entry forever for any despawned entity, pointing at a rapier body that no longer needs to exist. Its "Revisit when" named "entity despawn is added to the engine" as the exact trigger to design an eviction path. The Tier 1 roadmap ([docs/roadmap/tier-1-foundational.md](../roadmap/tier-1-foundational.md)) called this "likely the single cheapest, most-leveraged item" on the board — it blocks scripting from getting spawn/despawn access and blocks any future trigger/pickup-removal gameplay.

Investigation found the mechanism itself needed almost nothing new: `Sim.world` is a raw `pub hecs::World` (hecs 0.10.5), and `hecs::World::despawn(entity) -> Result<(), NoSuchEntity>` is already callable from any system via `SystemArgs::world` with zero `Scheduler`/`Sim` changes — it immediately bumps the entity's generation, so a stale handle fails cleanly rather than corrupting state. `engine_core::inspect::world_to_json` already iterates only live entities and already sorts by `Entity::to_bits()` (ADR-0002's determinism rule for hecs's swap-remove iteration order), so it needed no changes either. The one real gap was `engine-physics`'s handle map.

## Decision

`engine-physics::system::physics_step` gained a private `evict_despawned` helper, called at the top of every tick before the existing lazy-registration scan: it collects any `PhysicsState.bodies` key no longer live in `world` (`!world.contains(entity)`), sorts them by `Entity::to_bits()` (ADR-2002-style determinism, even though removal order has no externally observable effect today — kept for consistency with every other order-sensitive pass in this codebase), and calls rapier's `PhysicsWorld::remove_body` (which removes attached colliders/joints too) plus removes the map entry.

To prove this end-to-end through the real CLI/scene surface — the same "ride the existing `run`/`test`/`inspect` verbs, add a component + system, not a new CLI verb" pattern Phase 6 established for physics — a minimal `DespawnAfter { ticks_remaining: u32 }` component and `despawn_after_system` were added, scenario-local (defined in `crates/engine-cli/src/scenarios/despawn_demo.rs`, registered into the shared `crate::registry` the same way `basic::{Position, Velocity, movement_system}` are) rather than given a permanent home in `engine-core` or `engine-physics`. It is deliberately *not* a general-purpose lifetime/TTL gameplay feature — just enough to despawn both a physics-attached and a physics-free entity from a real scene file, proving the mechanism works, not designing the eventual trigger/pickup system.

## Alternatives considered

- **A despawn event/hook mechanism** (e.g. a wrapped `Sim::despawn` that fires registered listeners so any interested resource can react). Rejected as premature: `PhysicsState` is the only consumer that needs to react to a despawn today, and a reactive per-tick scan solves that with no new core mechanism at all.
- **Generalizing `Resources` itself with a `remove<T>`/lifecycle API.** Rejected for the same one-consumer reason — `Resources` bag entries (like `PhysicsState`) still live for the whole `Sim`'s lifetime; it's only the *data inside* one resource that needed eviction, not the resource slot itself.
- **A permanent `Lifetime`/TTL component in `engine-core`.** Rejected as scope creep for this step — despawn's roadmap justification is triggers/pickups (Tier 2+) and scripting access (a separate Tier 1 item), neither of which this step needed to design ahead of time.

## Consequences

- `physics_step` gains one O(bodies) scan per tick, cheap at current scale.
- Any future stateful, entity-keyed `Resources` occupant (beyond `PhysicsState`) will need to write its own eviction scan following this same pattern, until a second consumer actually exists to justify generalizing it.
- Scripting's despawn access (Tier 1's "Expanded scripting API" item) and any future trigger/pickup system are now unblocked, but neither was designed here.

## Revisit when

A second stateful, entity-keyed `Resources` occupant needs the same kind of eviction `PhysicsState.bodies` needed here — that's the concrete trigger to factor this into a shared/generic mechanism (on `Resources` itself, or a lightweight despawn-listener registration) instead of duplicating `evict_despawned`'s pattern per-resource.
