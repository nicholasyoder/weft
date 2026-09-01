//! Conversions between the workspace's pinned `glam` (0.29 — what
//! `engine_core::Transform` uses) and the `glam` release `rapier3d::math`'s
//! `Vector`/`Rotation` aliases resolve to internally (via `glamx`, pulled
//! in transitively). These are two different, semver-incompatible releases
//! of the same crate — not interchangeable at the type level even though
//! both are "glam" — so this boundary needs real conversion code, not a
//! type-level no-op.

use rapier3d::math::{Rotation, Vector};

pub fn vec3_to_rapier(v: glam::Vec3) -> Vector {
    Vector::new(v.x, v.y, v.z)
}

pub fn vec3_from_rapier(v: Vector) -> glam::Vec3 {
    glam::Vec3::new(v.x, v.y, v.z)
}

pub fn quat_to_rapier(q: glam::Quat) -> Rotation {
    Rotation::from_xyzw(q.x, q.y, q.z, q.w)
}

pub fn quat_from_rapier(q: Rotation) -> glam::Quat {
    let [x, y, z, w] = q.to_array();
    glam::Quat::from_xyzw(x, y, z, w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec3_round_trips() {
        let v = glam::Vec3::new(1.5, -2.25, 3.0);
        assert_eq!(vec3_from_rapier(vec3_to_rapier(v)), v);
    }

    #[test]
    fn quat_round_trips() {
        let q = glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_3);
        let back = quat_from_rapier(quat_to_rapier(q));
        assert!((q.x - back.x).abs() < 1e-6);
        assert!((q.y - back.y).abs() < 1e-6);
        assert!((q.z - back.z).abs() < 1e-6);
        assert!((q.w - back.w).abs() < 1e-6);
    }
}
