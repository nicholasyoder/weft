# ADR-0018: Physics gameplay substrate

- **Status**: accepted
- **Date**: 2026-09-03

## Context

Tier 2's ["Physics gameplay substrate"](../roadmap/tier-2-visual-and-gameplay-realism.md#physics-gameplay-substrate) item called out a real gap: `PhysicsState` exposed only `apply_force`, `BodyType` was `Dynamic`/`Fixed` only, `ColliderShape` was `Box`/`Sphere` only, and `games/sandbox`'s player drove a rolling sphere with raw per-tick forces — "not anything resembling a controller," in the tier doc's own words. No raycasts, no sensors/triggers, no collision-layer filtering, no kinematic bodies, no capsule collider.

[`docs/roadmap/physics-substrate-plan.md`](https://github.com/nicholasyoder/weft/blob/4fcb7a9/docs/roadmap/physics-substrate-plan.md) (now retired — this ADR and `docs/roadmap/completed-phases.md`'s Phase 15 entry are its permanent record, the same pattern `debt-cleanup-plan.md` used) split the work into 8 phases: four engine-native mechanisms (capsule collider + kinematic bodies; collision groups + sensor overlap queries; raycasts; the character-controller mechanism), three sandbox proofs (a moving platform; sensor-based pickups; a real character-controlled player), and this docs closeout.

## Decisions

**Kinematic bodies: position-based only.** `BodyType` gained one new variant, `KinematicPositionBased` (rapier's `RigidBodyBuilder::kinematic_position_based()` + `set_next_kinematic_translation`/`_rotation`). Velocity-based kinematics weren't needed by either concrete use here (a moving platform, a character controller) — both want "I decide the exact next pose," not "I set a velocity and let rapier integrate it."

**Collision groups: raw `u32` bitmasks, not a named-layer registry.** `Collider` gained `membership: u32` (default `1`) and `filter: u32` (default `u32::MAX`), mapped straight to rapier's `InteractionGroups`/`Group::from_bits_truncate` — matches the existing narrow, serde-friendly style of `BodyType`/`ColliderShape` (ADR-0008's "never leak a raw rapier type into a scene file" rule) without building a name↔bit registry nobody has asked for yet.

**Sensors/triggers: a poll-based overlap query, not an event channel.** `Collider` gained `sensor: bool`. `PhysicsState::overlapping(entity) -> Vec<Entity>` reads rapier's already-computed narrow-phase intersection-pair state (`intersection_pairs_with`) — no `ActiveEvents`/event-channel plumbing needed, same "poll `PhysicsState` each tick" mechanism style `apply_force` already established.

**Raycasts: one narrow, engine-native `RaycastHit` type**, never a leaked rapier `RayIntersection`. `PhysicsState::cast_ray(origin, direction, max_toi, exclude) -> Option<RaycastHit>` (`RaycastHit { entity, distance, point, normal }`).

**Character controller: a mechanism method, not a new stateful resource.** `PhysicsState::move_character(entity, desired_translation) -> Option<CharacterMoveResult>` wraps rapier's `rapier3d::control::KinematicCharacterController::move_shape`. Per-entity gravity/jump state (`CharacterVelocity { vertical: f32 }`) is an ordinary `games/sandbox`-owned ECS component, not a second stateful entity-keyed map on `PhysicsState` — ADR-0011 flagged that a second such occupant is the trigger to generalize `evict_despawned`; avoided entirely, since a component dies with its entity automatically.

**`PhysicsState` bookkeeping grew a second map**: `Entity → ColliderHandle` plus the reverse `ColliderHandle → Entity` index, needed to translate raycast/overlap results back to entities — populated at the same lazy-registration point `bodies` already was, evicted in the same `evict_despawned` pass.

**Character-controller tuning uses rapier's own defaults** (`KinematicCharacterController::default()`) rather than exposing every parameter as a new authorable field — no concrete need yet for per-scene tuning.

**`games/sandbox`'s player changed shape, not just additively**: sphere/`apply_force` → capsule/`KinematicPositionBased` + `move_character`. WASD drives horizontal movement, Space jumps, `CharacterVelocity.vertical == 0.0` both means "resting" and gates jumping — no separately stored `grounded` flag needed, since it's reset to exactly `0.0` on the tick a landing is detected. `CameraFollow` needed no change (it only ever read `Transform`).

**Pickups switched from manual distance math to a real sensor trigger**: `RigidBody{Fixed}` + `Collider{sensor: true, Sphere}`, plus `engine.overlapping()` (mirroring `engine.query`'s shape) replacing `pickup.lua`'s hand-rolled distance-squared loop.

## A real rapier gotcha, found and documented, not just worked around

Phase 7 found a genuine landmine in rapier's kinematic character controller, cheap to hit and expensive to diagnose: **registering a kinematic character's first pose in exact zero-gap contact with a surface** (e.g. spawning it directly at an analytically-computed resting height, for test determinism) **hits a degenerate case in rapier's shape-cast TOI solver.** Confirmed via isolated scratch tests (not committed — the finding is preserved as a doc comment instead): repeatedly calling `move_character` with the same small downward `desired_translation` from such a pose does not reliably block at the true surface. Depending on the exact magnitude, it either leaks a small amount through on every single call (unbounded linear sinking over many ticks, no sign of convergence — observed y=0.9 drifting down through y=0.68 over 40 ticks) or doesn't block at all (a larger constant fell straight through). The identical setup with even a small real initial gap (0.05 units) settles rock-stable on the very first landing and stays put indefinitely.

This is now documented permanently on `PhysicsState::move_character`'s own doc comment (`crates/engine-physics/src/character.rs`), not just in this ADR, so a future consumer doesn't rediscover it from scratch. `games/sandbox`'s real player spawn was never at risk (it already spawns well above its resting height, the ordinary case under gravity) — this only bit the test suite's convenience shortcut of spawning test fixtures exactly at rest.

## Alternatives rejected

- **Velocity-based kinematic bodies**: no concrete consumer wants "set a velocity, let rapier integrate it" over "I decide the exact next pose" — add if a real need shows up.
- **A named collision-layer registry** (string names mapped to bits, like Unity's layer system): no concrete need yet for more than raw bitmasks; a registry is easy to add later without breaking the raw-bitmask fields underneath it.
- **An event-channel sensor mechanism** (rapier's `ActiveEvents`/collision-event collection): rejected in favor of polling `PhysicsState::overlapping` each tick — matches every other physics query's mechanism style in this codebase and avoids a second, differently-shaped event-plumbing path for one use case.
- **Per-entity character state (`CharacterVelocity`) living on `PhysicsState`** instead of as a component: rejected per ADR-0011's own flagged trigger — a second stateful entity-keyed resource map is exactly the signal to generalize `evict_despawned`, avoided by using an ordinary component that despawns with its entity for free.

## Consequences

- `games/sandbox`'s player is now a real capsule with wall sliding, step handling, and ground snapping from rapier's own character controller — a genuine gameplay/feel change, not just an additive one.
- Pickup collection is a real sensor overlap rather than hand-rolled distance math, proving the sensor mechanism end-to-end in live gameplay.
- The character visual is still a `sphere` mesh non-uniformly scaled into an ellipsoid (`[0.3, 0.8, 0.3]`) approximating the capsule collider's silhouette — no dedicated capsule mesh primitive exists in `engine-render` yet. This is purely cosmetic (the physics settles correctly; confirmed by direct measurement, not just visual impression) but was flagged by the user as looking like it's floating, since the engine renders no contact shadows and a fully-rounded ellipsoid bottom gives no grounded-looking silhouette. Left open — see "Revisit when" below.
- The gotcha above is now load-bearing documentation for anyone writing a new `move_character`-driven test or scene: always give a kinematic character a real initial gap above any surface it should land on.

## Revisit when

- A ramp, staircase, or moving platform that needs `autostep`/slope tuning shows up — `KinematicCharacterController::default()`'s fixed tuning (no autostep, 45° slope limits) will need to become per-scene-authorable at that point, not before.
- A second sensor-trigger use case wants a push notification instead of a poll (e.g. a large number of sensors where polling every tick is wasteful) — that's the trigger to reconsider the event-channel alternative rejected above.
- A future gameplay feature genuinely wants a moving platform or vehicle to carry momentum on jump-off — that's the trigger to reconsider velocity-based kinematics.
- Someone wants the sandbox character to visually read as a proper capsule (or a real character model) rather than an ellipsoid approximation — either add a dedicated `MeshKind::Capsule` primitive to `engine-render`, or replace it with an imported/generated asset via the existing `engine import`/Blender-headless pipeline (ADR-0009). Either is Tier 3 polish, not required for this ADR's own scope.
