# ADR-0004: Headless rendering via `wgpu` restricted to Vulkan + software rasterizer

- **Status**: accepted
- **Date**: 2026-08-28

## Context

Phase 2 (see [ROADMAP.md](../roadmap/completed-phases.md)) requires `engine render` to produce a correct PNG "with no window/display server present," verified explicitly rather than assumed. The development sandbox this was implemented in has no real GPU driver, no Vulkan ICD, and no `$DISPLAY` — a real instance of exactly the environment the roadmap worries about, not a hypothetical. Two decisions followed directly from that: how to get *any* working GPU backend in such an environment, and how to keep pixel output reproducible once one exists.

## Decision

**1. Install Mesa's `lavapipe` (`apt install mesa-vulkan-drivers`) as the headless rendering backend.** It's a software Vulkan implementation that needs no display server and no real GPU — confirmed working in this sandbox by enumerating adapters via `wgpu` (`AdapterInfo { name: "llvmpipe...", device_type: Cpu, backend: Vulkan, .. }`) with `$DISPLAY` unset. This is the same approach `wgpu`'s own CI uses. It's an environment setup step (like Phase 0's `rustup` install), not something the engine or its build can install for itself — any environment this engine runs headless rendering in needs it present.

**2. `engine-render` requests only `wgpu::Backends::VULKAN`, never `Backends::all()`.** With `Backends::all()`, which backend gets picked depends on what happens to be installed on a given machine (Vulkan, GL, or something else) — an accidental source of cross-machine pixel drift that has nothing to do with the scene itself. Pinning to one backend removes that variable; this environment's adapter enumeration also confirmed lavapipe is the *only* Vulkan adapter found (the `virtio_icd` also present in `/usr/share/vulkan/icd.d/` never appeared in `enumerate_adapters` output), so there was no additional ambiguity to resolve between a virtual-GPU passthrough path and the software one.

**3. Accepted non-goal: bit-identical pixels are a same-machine, same-Mesa-version property, not a portable one.** The Phase 0/1 "same seed → byte-identical forever" guarantee is about CPU-side simulation state and remains absolute. Rendering is a different guarantee: correct and reproducible on one machine/build, but Mesa/`lavapipe` version drift across machines can shift software-rasterized pixels slightly for an unchanged scene. This is exactly the same shape of caveat the roadmap already accepts for `rapier3d` in Phase 6 ("reliable across runs on the same machine/build but not guaranteed bit-identical across different hardware/compiler versions"). The golden-image test therefore compares with a fixed per-channel tolerance (`crates/engine-cli/tests/render.rs`), not byte equality.

## Alternatives considered

- **OpenGL/EGL surfaceless backend** instead of Vulkan+lavapipe: more finicky to set up headlessly (EGL platform selection, context creation without a window) and less commonly used for CI-grade headless rendering than the Vulkan+lavapipe path; rejected as more moving parts for no benefit here.
- **`Backends::all()` with adapter-preference logic** (e.g. prefer `DeviceType::Cpu`): would have handled the "pick a deterministic adapter" problem, but is unnecessary complexity given `VULKAN`-only already produces exactly one adapter in every environment tested; revisit if a second backend or a real GPU passthrough path is ever exercised (see Revisit when).
- **Byte-identical golden-image comparison**: rejected in favor of a tolerance check specifically because the roadmap already anticipated Mesa-version drift by calling for "a defined tolerance," and Phase 0's ADR-0002 already established the precedent of not over-promising determinism where the underlying tech doesn't actually guarantee it.

## Consequences

- Any environment that wants to run `engine render` headlessly (dev sandboxes, CI) must have a Vulkan ICD installed — `mesa-vulkan-drivers` on Debian/Ubuntu, or the equivalent for other distros/CI images. This is now a documented environment prerequisite, not an assumption baked silently into the code.
- If this engine ever runs on a machine with a real GPU and a real Vulkan driver, `engine render` will use that GPU instead of software rendering — `wgpu`'s adapter selection doesn't distinguish "real" from "software" beyond what's installed, which is the intended behavior (headless correctness doesn't require *slow* rendering, just *display-server-independent* rendering).

## Revisit when

- If a windowed/interactive mode is added (per the roadmap's "winit is a thin wrapper" principle), decide then whether to broaden beyond `Backends::VULKAN` (e.g. add `DX12`/`METAL` for native windowed performance on non-Linux dev machines) — headless CI can stay Vulkan-only regardless.
- If golden-image test flakiness is ever observed on a real CI machine with a different Mesa version than this one, that's the signal to widen `MAX_CHANNEL_DIFF` in `crates/engine-cli/tests/render.rs` or switch to a structural-similarity-style comparison instead of a flat per-channel tolerance.
