# Known issues & engineering debt

> **This is a different kind of list than the tier files.** Tiers 1–4 track *capabilities the engine doesn't have yet*. This file tracks *places where the code doesn't do what its own docs/tests/design already claim it does*, plus architectural debt that's cheap to fix now and expensive to fix once more is built on top of it. Living list, not a queue — pull whatever's actually biting. Fixed items are removed on the spot rather than kept as a changelog; `git log` is the record of what was fixed and why.

---

## Architectural debt worth addressing before it compounds

- **`Resources` (`engine-core/src/resources.rs`) is an `Option`-typed grab-bag** with no compile-time distinction between "always present" (e.g. `AudioSettings`) and "genuinely optional" (e.g. `AssetsDir`) resources — that distinction lives only in doc comments. A heavier required/optional struct split was considered and deliberately deferred; `Resources::remove::<T>()` already closes the sharpest edge (a resource no longer has to live for the `Sim`'s full lifetime).
