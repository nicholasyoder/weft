use std::path::PathBuf;

/// The directory a `Sim`'s content-addressed asset store reads from,
/// carried as a `Resources` entry (see `Resources`, ADR-0008's precedent)
/// so per-tick systems that need read access to the asset store —
/// `engine-anim`'s `animation_step` is the first — don't need `Sim` itself
/// to grow an asset-store-shaped field. Absent for a `Sim` built with no
/// asset store configured; consumers must treat that as "nothing to do"
/// rather than panicking (see `engine-anim`'s `animation_step`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetsDir(pub PathBuf);
