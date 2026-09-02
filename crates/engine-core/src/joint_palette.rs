use serde::{Deserialize, Serialize};

/// The computed per-joint skinning matrix palette for one animated entity
/// (`joint_world * inverse_bind_matrix`, one per skeleton joint, root-first
/// order) — written every tick by `engine-anim`'s `animation_step`, read by
/// `engine-render`'s skinned draw path.
///
/// Lives here, alongside `Transform`, rather than in `engine-anim` itself:
/// it's produced by simulation and consumed by rendering, the same
/// relationship `Transform` already has with `engine-physics`'s
/// `physics_step` — putting it in the crate that *produces* it would force
/// `engine-render` to depend on `engine-anim` just to name this one type,
/// the same layering problem `Transform` living here already avoids for
/// physics. See ADR-0015.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JointPalette {
    pub matrices: Vec<[[f32; 4]; 4]>,
}
