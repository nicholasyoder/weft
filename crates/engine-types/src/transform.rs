use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// An entity's position, orientation, and scale in world space. The first
/// engine-owned (not demo-scenario-owned) component: rendering and, later,
/// physics both need it, so it lives here rather than being redefined per
/// consumer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    #[serde(default = "default_position")]
    pub position: Vec3,
    #[serde(default = "default_rotation")]
    pub rotation: Quat,
    #[serde(default = "default_scale")]
    pub scale: Vec3,
}

fn default_position() -> Vec3 {
    Vec3::ZERO
}

fn default_rotation() -> Quat {
    Quat::IDENTITY
}

fn default_scale() -> Vec3 {
    Vec3::ONE
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Transform {
    pub fn from_position(position: Vec3) -> Self {
        Self {
            position,
            ..Default::default()
        }
    }

    pub fn to_matrix(self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_identity() {
        let t = Transform::default();
        assert_eq!(t.position, Vec3::ZERO);
        assert_eq!(t.rotation, Quat::IDENTITY);
        assert_eq!(t.scale, Vec3::ONE);
        assert_eq!(t.to_matrix(), glam::Mat4::IDENTITY);
    }
}
