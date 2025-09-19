//! Minimal OBJ parser supporting positions, normals and texture coordinates.

use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, BufReader},
    path::Path,
};

use anyhow::{Context, Result, anyhow};

use crate::mesh::{MeshData, MeshVertex};

/// Load an OBJ mesh from a file path.
pub fn load_obj_from_path(path: impl AsRef<Path>) -> Result<MeshData> {
    let file = File::open(&path)
        .with_context(|| format!("Failed to open OBJ file: {}", path.as_ref().display()))?;
    load_obj_from_reader(BufReader::new(file))
}

/// Load an OBJ mesh from a [`BufRead`] implementation.
pub fn load_obj_from_reader<R: BufRead>(reader: R) -> Result<MeshData> {
    parse_obj(reader)
}

/// Convenience helper to parse an OBJ string literal.
pub fn load_obj_from_str(contents: &str) -> Result<MeshData> {
    parse_obj(io::Cursor::new(contents))
}

fn parse_obj<R: BufRead>(reader: R) -> Result<MeshData> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut texcoords: Vec<[f32; 2]> = Vec::new();

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
    struct Key(usize, Option<usize>, Option<usize>);

    let mut unique: HashMap<Key, u32> = HashMap::new();
    let mut vertices: Vec<MeshVertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for (line_no, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("Failed to read line {}", line_no + 1))?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let tag = parts
            .next()
            .ok_or_else(|| anyhow!("Malformed OBJ line {}: '{}'", line_no + 1, trimmed))?;

        match tag {
            "v" => {
                let x = parse_f32(parts.next(), line_no, "x coordinate")?;
                let y = parse_f32(parts.next(), line_no, "y coordinate")?;
                let z = parse_f32(parts.next(), line_no, "z coordinate")?;
                positions.push([x, y, z]);
            }
            "vt" => {
                let u = parse_f32(parts.next(), line_no, "u coordinate")?;
                let v = parse_f32(parts.next(), line_no, "v coordinate")?;
                texcoords.push([u, v]);
            }
            "vn" => {
                let nx = parse_f32(parts.next(), line_no, "nx coordinate")?;
                let ny = parse_f32(parts.next(), line_no, "ny coordinate")?;
                let nz = parse_f32(parts.next(), line_no, "nz coordinate")?;
                normals.push([nx, ny, nz]);
            }
            "f" => {
                let mut face_indices: Vec<u32> = Vec::new();
                for part in parts {
                    let (vi, vti, vni) = parse_face_vertex(
                        part,
                        positions.len(),
                        texcoords.len(),
                        normals.len(),
                        line_no,
                    )?;
                    let key = Key(vi, vti, vni);
                    let index = match unique.get(&key) {
                        Some(&idx) => idx,
                        None => {
                            let position = positions.get(vi).copied().ok_or_else(|| {
                                anyhow!("Position index out of bounds on line {}", line_no + 1)
                            })?;
                            let uv = vti
                                .and_then(|i| texcoords.get(i).copied())
                                .unwrap_or([0.0, 0.0]);
                            let normal = vni
                                .and_then(|i| normals.get(i).copied())
                                .unwrap_or([0.0, 0.0, 1.0]);

                            let idx = u32::try_from(vertices.len())
                                .map_err(|_| anyhow!("Too many vertices in OBJ (>{})", u32::MAX))?;
                            vertices.push(MeshVertex::new(position, normal, uv));
                            unique.insert(key, idx);
                            idx
                        }
                    };
                    face_indices.push(index);
                }

                if face_indices.len() < 3 {
                    continue;
                }
                // Triangulate fan
                for tri in 1..(face_indices.len() - 1) {
                    indices.push(face_indices[0]);
                    indices.push(face_indices[tri]);
                    indices.push(face_indices[tri + 1]);
                }
            }
            _ => {
                // Ignore other directives (o/g/s/usemtl/etc.)
            }
        }
    }

    if vertices.is_empty() || indices.is_empty() {
        anyhow::bail!("OBJ contained no triangles");
    }

    Ok(MeshData::new(vertices, indices))
}

fn parse_f32(value: Option<&str>, line_no: usize, what: &str) -> Result<f32> {
    let token = value.ok_or_else(|| anyhow!("Missing {} on line {}", what, line_no + 1))?;
    token
        .parse::<f32>()
        .with_context(|| format!("Failed to parse {} on line {}", what, line_no + 1))
}

fn parse_face_vertex(
    token: &str,
    pos_count: usize,
    tex_count: usize,
    norm_count: usize,
    line_no: usize,
) -> Result<(usize, Option<usize>, Option<usize>)> {
    let mut split = token.split('/');
    let pos = split
        .next()
        .ok_or_else(|| anyhow!("Malformed face element '{}' on line {}", token, line_no + 1))?;
    let pos_idx = resolve_index(pos, pos_count, line_no)?;

    let tex_idx = match split.next() {
        Some(value) if !value.is_empty() => Some(resolve_index(value, tex_count, line_no)?),
        _ => None,
    };

    let norm_idx = match split.next() {
        Some(value) if !value.is_empty() => Some(resolve_index(value, norm_count, line_no)?),
        _ => None,
    };

    Ok((pos_idx, tex_idx, norm_idx))
}

fn resolve_index(token: &str, len: usize, line_no: usize) -> Result<usize> {
    let raw = token
        .parse::<i32>()
        .with_context(|| format!("Invalid index '{}' on line {}", token, line_no + 1))?;
    if raw == 0 {
        anyhow::bail!("OBJ indices are 1-based; found 0 on line {}", line_no + 1);
    }

    let idx = if raw > 0 {
        (raw - 1) as isize
    } else {
        (len as isize) + (raw as isize)
    };

    if idx < 0 || idx as usize >= len {
        anyhow::bail!(
            "OBJ index {} resolved out of bounds (len={}) on line {}",
            raw,
            len,
            line_no + 1
        );
    }

    Ok(idx as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_triangle() {
        let src = r#"
            v 0.0 0.0 0.0
            v 1.0 0.0 0.0
            v 0.0 1.0 0.0
            vn 0.0 0.0 1.0
            vt 0.0 0.0
            vt 1.0 0.0
            vt 0.0 1.0
            f 1/1/1 2/2/1 3/3/1
        "#;
        let mesh = load_obj_from_str(src).expect("parse triangle");
        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.indices.len(), 3);
        assert!(mesh.is_valid());
    }
}
