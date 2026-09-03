# Physics gameplay substrate plan

**Working checklist, not a spec — same living-document rule as the rest of `docs/roadmap/`.** Addresses Tier 2's ["Physics gameplay substrate"](tier-2-visual-and-gameplay-realism.md#physics-gameplay-substrate) item: raycast/shape-cast queries, triggers/sensors, collision layers/filtering, a character controller, kinematic bodies, and capsule colliders. Today `PhysicsState` (`crates/engine-physics/src/system.rs`) exposes only `apply_force`, `BodyType` is `Dynamic`/`Fixed` only, `ColliderShape` is `Box`/`Sphere` only, and `games/sandbox`'s player (`games/sandbox/src/player_control.rs`) drives a rolling sphere with raw per-tick forces — the tier doc calls this out by name as "not anything resembling a controller."

Split into 8 phases, same discipline [`debt-cleanup-plan.md`](https://github.com/nicholasyoder/weft/blob/cf5b48aa823b3aa4e6aeeaa0a510793498078da9/docs/roadmap/debt-cleanup-plan.md) used (now retired, its work done): each phase lands as its own reviewable, tested commit, full gate clean before moving to the next — `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`.

**Design decisions made up front** (don't re-litigate without a reason — see the [ADR](../decisions/0018-physics-gameplay-substrate.md) once Phase 8 writes it):

- **Kinematic bodies: position-based only.** `BodyType` gains one new variant, `KinematicPositionBased` (rapier's `RigidBodyBuilder::kinematic_position_based()` + `set_next_kinematic_translation`/`_rotation`). Velocity-based kinematics aren't needed by any concrete use here (moving platforms and the character controller both want "I decide the exact next pose") — deferred.
- **Collision groups: raw `u32` bitmasks, not a named-layer registry.** `Collider` gains `#[serde(default = "default_membership")] membership: u32` (default `1`, rapier's `Group::GROUP_1`) and `#[serde(default = "default_filter")] filter: u32` (default `u32::MAX`, `Group::ALL`) — mapped to rapier's `InteractionGroups`/`Group::from_bits_truncate`. Matches the existing narrow/serde-friendly style of `BodyType`/`ColliderShape` (ADR-0008's "never leak a raw rapier type into a scene file" rule) without building a name↔bit registry nobody's asked for yet.
- **Sensors/triggers: poll-based overlap query, not an event channel.** `Collider` gains `#[serde(default)] sensor: bool`. `PhysicsState::overlapping(entity) -> Vec<Entity>` uses rapier's `PhysicsWorld::intersection_pairs_with` (already-computed narrow-phase state, no `ActiveEvents`/event-channel plumbing needed) — matches `apply_force`'s "just poll `PhysicsState` each tick" mechanism style.
- **Raycasts: one narrow, engine-native `RaycastHit` type**, not a leaked rapier `RayIntersection`. `PhysicsState::cast_ray(origin, direction, max_toi, exclude: Option<Entity>) -> Option<RaycastHit>` (`RaycastHit { entity, distance, point, normal }`).
- **Character controller: a mechanism method, not a new stateful resource.** `PhysicsState::move_character(entity, desired_translation) -> Option<CharacterMoveResult>` wraps rapier's `rapier3d::control::KinematicCharacterController::move_shape` (needs a capsule collider + a `KinematicPositionBased` body already registered on the entity). Per-entity gravity/jump state (vertical velocity) is kept as an ordinary ECS component (`CharacterVelocity`, owned by `games/sandbox`) rather than added to `PhysicsState` — ADR-0011 flagged that a second stateful entity-keyed `Resources` occupant is the trigger to generalize `evict_despawned`; avoided entirely, since a component dies with its entity automatically.
- **`PhysicsState` bookkeeping grows a second map**: `Entity → ColliderHandle` plus the reverse `ColliderHandle → Entity` index (today it only tracks `Entity → RigidBodyHandle`), needed to translate raycast/overlap results back to entities. Populated at the same lazy-registration point as today, evicted in the same `evict_despawned` pass.
- **Character-controller tuning uses rapier's own defaults** (`KinematicCharacterController::default()`) rather than exposing every parameter as a new authorable field — no concrete need for per-scene tuning yet.
- **`games/sandbox`'s player changes shape**: sphere/`apply_force` → capsule/`KinematicPositionBased` + `move_character`, WASD→horizontal movement, Space→jump, gravity accumulated in `CharacterVelocity`. Real gameplay/feel change, not just additive — direct fix for the gap the tier doc names. `CameraFollow` needs no change (only reads `Transform`).
- **Pickups switch from manual distance math to a real sensor trigger**: `RigidBody{Fixed}` + `Collider{sensor: true, Sphere}`, plus a new `engine.overlapping()` Lua binding (mirrors `engine.query`'s shape) replacing `pickup.lua`'s hand-rolled distance-squared loop.

Recommended order: **1 → 2 → 3 → 4 → 5 → 6 → 7 → 8** — each engine-native mechanism (1-4) before its sandbox proof (5-7), docs closeout last. Phases 1-4 are independent of each other's *sandbox* proofs but not of each other's *engine* code where noted (4 depends on 1). Not load-bearing — just a reasonable default.

---

## Phase 1 — Foundation: capsule collider + kinematic bodies + handle bookkeeping

`crates/engine-physics/src/components.rs`: add `ColliderShape::Capsule { half_height: f32, radius: f32 }`; add `BodyType::KinematicPositionBased`. `crates/engine-physics/src/system.rs`: `PhysicsState` tracks collider handles + the reverse `ColliderHandle → Entity` index (needed by Phases 2-4, not this phase); `build_collider` handles the capsule shape; body construction handles the kinematic variant (pose driven externally via `set_next_kinematic_translation`/`_rotation`, not overwritten by the solver, not force-reset the way dynamic bodies are); new `set_kinematic_translation`. New unit tests: capsule collider builds correctly (serde round-trip + `build_collider` mapping), a `KinematicPositionBased` body's pose is driven by `set_kinematic_translation` and *not* overwritten by the solver step, existing dynamic/fixed body tests stay passing unchanged. No `games/sandbox` or scene-file change yet — purely the engine-physics primitive.

**Verify**: full gate.

---

## Phase 2 — Collision groups/filtering + sensor overlap queries

`Collider::sensor/membership/filter` fields; `build_collider` sets `.sensor(...)`/`.collision_groups(InteractionGroups::new(...))`. New `crates/engine-physics/src/queries.rs` with `PhysicsState::overlapping(entity) -> Vec<Entity>`. New `engine.overlapping()` Lua binding in `crates/engine-script/src/host.rs` (same `Lua::scope`/`RefCell` pattern `engine.query`/`engine.despawn` already use, reading `PhysicsState` out of `ctx.resources`). New tests: collision groups actually preventing/allowing contact; `overlapping` reporting sensor overlaps; `engine-script` crate-level test for `engine.overlapping` (same fixture-file pattern `tests/host.rs` already uses).

**Verify**: full gate.

---

## Phase 3 — Raycast queries

`RaycastHit` + `PhysicsState::cast_ray(...)` in `queries.rs` (using `PhysicsWorld::cast_ray_and_get_normal` + `QueryFilter::exclude_rigid_body`). New `engine.raycast(origin, direction, max_distance)` Lua binding (always excludes the calling entity, mirroring `engine.despawn`'s self-default). New tests: `cast_ray` hitting/missing/excluding-self; `engine-script` crate-level test for `engine.raycast`.

**Verify**: full gate.

---

## Phase 4 — Character controller mechanism

New `crates/engine-physics/src/character.rs`: `CharacterMoveResult { translation: Vec3, grounded: bool }` + `PhysicsState::move_character(entity, desired_translation) -> Option<CharacterMoveResult>`, using `rapier3d::control::KinematicCharacterController` (**not** re-exported via `rapier3d::prelude` — needs its own `use rapier3d::control::...`). Depends on Phase 1 (capsule + kinematic); does not depend on Phase 3's public raycast API (rapier's character controller does its own internal shape-casting). New unit test: `move_character` resolving a blocked horizontal move and reporting `grounded` on a floor.

**Verify**: full gate.

---

## Phase 5 — Sandbox proof: moving platform

Small `moving_platform_system` (sandbox-local, `games/sandbox`) oscillating a kinematic entity via `set_kinematic_translation` (Phase 1) on a tick-based sine/ping-pong pattern. New entity in `games/sandbox/scenes/playground.toml`; register the system in `games/sandbox::registry()`. New headless test for the platform's motion, same pattern as `games/sandbox/tests/player_control.rs`.

**Verify**: full gate, `xvfb-run cargo run -p sandbox` sanity run.

---

## Phase 6 — Sandbox proof: sensor-based pickups

Pickups in `playground.toml` gain `RigidBody{Fixed}` + `Collider{sensor: true, shape: Sphere}` (radius matching the old `COLLECT_RANGE_SQ` distance). Rewrite `games/sandbox/scripts/pickup.lua` to use `engine.overlapping()` (Phase 2) instead of `engine.query` + manual distance-squared math — same player-facing behavior (walk near, hold E), real sensor mechanism underneath. Update/extend the existing pickup test for sensor-based collection.

**Verify**: full gate, `xvfb-run cargo run -p sandbox` sanity run, throwaway visual check.

---

## Phase 7 — Sandbox proof: real character-controlled player

Rewrite `games/sandbox/src/player_control.rs`: `PlayerControl { speed, jump_speed, gravity }` (replaces `force`), new `CharacterVelocity { vertical: f32 }` component (registered in `registry()`), `player_control_system` computes desired horizontal movement from WASD, applies/resets vertical velocity on Space/grounded, calls `PhysicsState::move_character` (Phase 4). `playground.toml`'s player entity becomes `RigidBody{KinematicPositionBased}` + `Collider{Capsule}` + the new `PlayerControl` fields + `CharacterVelocity{0.0}`. `CameraFollow` needs no change. Rewrite `games/sandbox/tests/player_control.rs` for the new mechanic (movement + jump + grounded reset). This is the phase that actually closes the tier doc's named complaint.

**Verify**: full gate, `xvfb-run cargo run -p sandbox` sanity run, throwaway visual check of the capsule player moving/jumping in the arena.

---

## Phase 8 — Docs closeout

New `docs/decisions/0018-physics-gameplay-substrate.md` recording the design decisions at the top of this file and the alternatives rejected (velocity-based kinematics, named-layer registry, event-channel sensors, per-entity state in `Resources` vs. as a component). `docs/roadmap/tier-2-visual-and-gameplay-realism.md`: close the "Physics gameplay substrate" section; explicitly note what's still open (velocity-based kinematic bodies, a generic Lua-exposed shape-cast, scene-authorable character-controller tuning, a named collision-layer registry) so it isn't silently lost. `docs/roadmap/completed-phases.md`: new Phase 15 entry. This plan doc gets retired the same way `debt-cleanup-plan.md` was — deleted outright once done, `git log` is the record.

**Verify**: full gate, re-run the full existing test suite to confirm no regression outside what Phases 6-7 intentionally rewrote.
