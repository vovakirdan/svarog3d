use crate::{Mat4, Vec3};

/// Simple perspective camera (right-handed).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y_rad: f32,
    pub z_near: f32,
    pub z_far: f32,
    pub aspect: f32,
}

impl Camera {
    #[allow(clippy::too_many_arguments)]
    pub fn new_perspective(
        eye: Vec3,
        target: Vec3,
        up: Vec3,
        fov_y_rad: f32,
        z_near: f32,
        z_far: f32,
        aspect: f32,
    ) -> Self {
        Self {
            eye,
            target,
            up,
            fov_y_rad,
            z_near,
            z_far,
            aspect,
        }
    }

    #[inline]
    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye, self.target, self.up)
    }

    ///*  NOTE: This returns OpenGL-style projection (z ∈ [-1,1]).
    /// Renderer multiplies by OPENGL_TO_WGPU to match z ∈ [0,1].
    #[inline]
    pub fn proj(&self) -> Mat4 {
        Mat4::perspective_rh(
            self.fov_y_rad,
            self.aspect.max(1e-6),
            self.z_near,
            self.z_far,
        )
    }

    #[inline]
    pub fn proj_view(&self) -> Mat4 {
        self.proj() * self.view()
    }

    #[inline]
    pub fn with_aspect(mut self, aspect: f32) -> Self {
        self.aspect = aspect;
        self
    }
}
