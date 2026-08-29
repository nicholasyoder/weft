# ADR-0002: Deterministic RNG algorithm and hecs iteration-order policy

- **Status**: accepted
- **Date**: 2026-08-28

## Context

Phase 0 (see [ROADMAP.md](../../ROADMAP.md)) requires "same seed → byte-identical world state, forever" as a hard, non-retrofittable constraint. Two implementation choices directly threaten that guarantee if made carelessly:

1. Which PRNG algorithm backs the engine's single seeded RNG resource.
2. Whether anything observable (JSON world dumps, snapshot tests) may depend on `hecs`'s internal entity storage order.

Both were flagged during Phase 0 implementation as decisions worth a paper trail rather than silent defaults.

## Decision

**1. RNG algorithm: `rand_chacha::ChaCha8Rng`, not `rand::rngs::SmallRng`.**

`engine-core::rng::EngineRng` is a type alias for `ChaCha8Rng`, seeded via `SeedableRng::seed_from_u64`. `SmallRng`'s documentation explicitly states its algorithm is unspecified and may change between `rand` minor versions — a routine `cargo update` could silently change every future recorded regression test's expected output with no compile error and no obvious symptom beyond "old recordings stopped matching." `ChaCha8Rng`'s output is spec'd (the ChaCha8 stream cipher) and the crate commits to output stability across versions, which is the actual property "byte-identical forever" requires.

**2. hecs iteration-order policy: never rely on raw archetype/query iteration order for anything byte-sensitive.**

`hecs::World` stores entities in per-archetype contiguous arrays and uses swap-remove on despawn, so an entity's position within its archetype's backing array is not guaranteed stable once anything has ever been removed. Phase 0's scenario never despawns, so this isn't yet a live bug, but the rule is adopted now rather than discovered later as a flaky test:

- Any code producing output whose exact byte content matters (JSON world dumps today; any future order-sensitive aggregation) must collect `(Entity, ..)` pairs and sort by a stable key derived from the `Entity` handle (`Entity::to_bits()`) before using them. `engine_core::inspect::world_to_json` does this.
- Iterating a `hecs` query directly inside a system body remains fine as long as each entity's update is independent of iteration order (true of Phase 0's movement system: `pos += vel * dt` per entity, no shared accumulator, no per-entity RNG draw). It stops being fine the moment a system draws RNG per-entity inside the loop or aggregates across entities in an order-sensitive way — such systems must sort first.

## Alternatives considered

- **`SmallRng`**: ROADMAP.md listed it as an acceptable option alongside `ChaCha8Rng`. Rejected specifically because its algorithm is not contractually stable — determinism-focused prior art in the Rust ecosystem (e.g. `bevy_rand`) defaults to ChaCha for the same reason.
- **Leaving hecs iteration order unaddressed until despawn exists (Phase 3+)**: would defer the decision to exactly the point where it becomes a live, hard-to-diagnose flaky-test bug instead of a one-line rule written down in advance. Rejected as inconsistent with ground rule 3 ("determinism enforced from Phase 0, not retrofitted").

## Consequences

- `ChaCha8Rng` is measurably slower than `SmallRng` for bulk RNG consumption; irrelevant at Phase 0's entity counts and not expected to matter until profiling says otherwise.
- Every future component-dumping or order-sensitive system must remember to sort by entity key first — not enforced by the type system, only by convention and this ADR. Worth revisiting if it becomes a repeated source of bugs (see Revisit when).

## Revisit when

- If profiling ever shows `ChaCha8Rng` as a meaningful bottleneck in RNG-heavy code, re-evaluate against a different *algorithm-stable* PRNG — not `SmallRng`, which reopens the original problem.
- If a system is found (via a bug, not preemptively) that violates the sort-before-using-order rule, consider whether the pattern should be enforced structurally (e.g. a wrapper type that only yields entities in sorted order) rather than left as a documented convention.
