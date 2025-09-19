//! CPU-side mesh representation used by loaders.

/// Vertex with position/normal/uv. Values are in object space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl MeshVertex {
    pub fn new(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            uv,
        }
    }
}

/// Indexed triangle mesh with tightly-packed vertices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn new(vertices: Vec<MeshVertex>, indices: Vec<u32>) -> Self {
        Self { vertices, indices }
    }

    /// Returns `true` if both vertex and index buffers are non-empty.
    pub fn is_valid(&self) -> bool {
        !self.vertices.is_empty() && !self.indices.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_data_validity() {
        let data = MeshData::new(vec![MeshVertex::default()], vec![0]);
        assert!(data.is_valid());
    }
}
