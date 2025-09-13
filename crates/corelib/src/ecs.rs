//! Tiny ECS: World, Entity, components: Transform + Renderable.

use crate::transform::Transform;

/// Entity id (dense, index into component arrays).
pub type Entity = u32;

/// Simple mesh kind we can render without external assets.
#[derive(Clone, Copy, Debug)]
pub enum MeshKind {
    Cube,
}

/// Marker component: renderable with given mesh kind.
/// (Материалы добавим позже)
#[derive(Clone, Copy, Debug)]
pub struct Renderable {
    pub mesh: MeshKind,
}

/// Very small ECS world with dense parallel arrays.
/// No allocations per-frame; spawn may allocate to grow capacity.
#[derive(Default)]
pub struct World {
    transforms: Vec<Transform>,
    renderables: Vec<Option<Renderable>>,
    alive: Vec<bool>,
    len: u32,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn entity with Transform and optional Renderable.
    pub fn spawn(&mut self, t: Transform, r: Option<Renderable>) -> Entity {
        let id = self.len;
        let idx = id as usize;
        self.len += 1;

        if idx >= self.transforms.len() {
            // grow all arrays equally
            let new_len = (idx + 1).next_power_of_two().max(8);
            self.transforms.resize(new_len, Transform::identity());
            self.renderables.resize(new_len, None);
            self.alive.resize(new_len, false);
        }

        self.transforms[idx] = t;
        self.renderables[idx] = r;
        self.alive[idx] = true;
        id
    }

    #[inline]
    pub fn is_alive(&self, e: Entity) -> bool {
        let i = e as usize;
        i < self.alive.len() && self.alive[i]
    }

    /// Mutable access to a transform (for animation).
    #[inline]
    pub fn transform_mut(&mut self, e: Entity) -> Option<&mut Transform> {
        let i = e as usize;
        if self.is_alive(e) {
            Some(&mut self.transforms[i])
        } else {
            None
        }
    }

    /// Iterate over (Transform, Renderable) pairs.
    pub fn iter_renderables(&self) -> impl Iterator<Item = (&Transform, &Renderable)> {
        // No alloc: zip and filter by alive + has Some(Renderable)
        (0..self.len as usize).filter_map(move |i| {
            if self.alive.get(i).copied().unwrap_or(false) {
                if let Some(r) = self.renderables[i].as_ref() {
                    return Some((&self.transforms[i], r));
                }
            }
            None
        })
    }

    /// System example: rotate all transforms by given Euler speed * dt.
    pub fn system_rotate_all(&mut self, dt: f32, speed_xyz: [f32; 3]) {
        let [sx, sy, sz] = speed_xyz;
        for i in 0..(self.len as usize) {
            if self.alive[i] {
                let t = &mut self.transforms[i];
                t.rotation_euler.x += sx * dt;
                t.rotation_euler.y += sy * dt;
                t.rotation_euler.z += sz * dt;
            }
        }
    }
}
