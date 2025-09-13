use crate::{EulerRot, Mat4, Quat, Vec3};

/// Rigid transform with uniform or non-uniform scale (Euler XYZ).
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub translation: Vec3,
    /// Euler angles in radians (XYZ order).
    pub rotation_euler: Vec3,
    pub scale: Vec3,
}

impl Transform {
    #[inline]
    pub const fn identity() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation_euler: Vec3::ZERO,
            scale: Vec3::ONE,
        }
    }

    #[inline]
    pub fn from_trs(translation: Vec3, rotation_euler: Vec3, scale: Vec3) -> Self {
        Self {
            translation,
            rotation_euler,
            scale,
        }
    }

    /// Build matrix = T * R * S (column-major Mat4 per glam).
    #[inline]
    pub fn matrix(&self) -> Mat4 {
        let q = Quat::from_euler(
            EulerRot::XYZ,
            self.rotation_euler.x,
            self.rotation_euler.y,
            self.rotation_euler.z,
        );
        Mat4::from_scale_rotation_translation(self.scale, q, self.translation)
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}
