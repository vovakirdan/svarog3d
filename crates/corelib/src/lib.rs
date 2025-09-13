//! Core types: math re-exports, Transform, Camera.

pub use glam::{EulerRot, Mat4, Quat, Vec3, vec3};

pub mod camera;
pub mod ecs;
pub mod transform;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_transform_is_identity_matrix() {
        let t = transform::Transform::identity();
        assert_eq!(t.matrix(), Mat4::IDENTITY);
    }

    #[test]
    fn translate_then_scale_matrix() {
        let t = transform::Transform::from_trs(
            vec3(1.0, 2.0, 3.0),
            vec3(0.0, 0.0, 0.0),
            vec3(2.0, 2.0, 2.0),
        );
        // Проверим пару элементов: последний столбец = translation,
        // диагональ = scale (при нулевой ротации).
        let m = t.matrix().to_cols_array();
        assert!((m[12] - 1.0).abs() < 1e-6);
        assert!((m[13] - 2.0).abs() < 1e-6);
        assert!((m[14] - 3.0).abs() < 1e-6);
        assert!((m[0] - 2.0).abs() < 1e-6);
        assert!((m[5] - 2.0).abs() < 1e-6);
        assert!((m[10] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn camera_pv_is_finite() {
        let cam = camera::Camera::new_perspective(
            vec3(0.0, 0.0, 4.0),
            vec3(0.0, 0.0, 0.0),
            Vec3::Y,
            60f32.to_radians(),
            0.1,
            100.0,
            16.0 / 9.0,
        );
        let pv = cam.proj_view();
        let a = pv.to_cols_array();
        assert!(a.iter().all(|f| f.is_finite()));
    }
}
