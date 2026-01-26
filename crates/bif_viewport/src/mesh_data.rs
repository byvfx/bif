//! Mesh data types for viewport rendering.
//!
//! Contains `MeshData` struct with loading from OBJ, AABB, and bif_core::Mesh.

use std::path::Path;

use anyhow::Result;

use bif_math::{Aabb, Vec3};

use crate::gpu_types::Vertex;

/// GPU-ready mesh data with vertices, indices, and bounds.
#[derive(Clone)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
    /// Per-triangle material IDs (for GeomSubsets). If Some, use primitive_index to lookup.
    pub triangle_material_ids: Option<Vec<u32>>,
}

impl MeshData {
    /// Get mesh center.
    pub fn center(&self) -> Vec3 {
        (self.bounds_min + self.bounds_max) * 0.5
    }

    /// Get mesh size (diagonal of bounding box).
    pub fn size(&self) -> f32 {
        (self.bounds_max - self.bounds_min).length()
    }

    /// Create a box mesh from AABB (for LOD proxy rendering).
    ///
    /// Generates a simple box with 8 vertices, 36 indices (12 triangles).
    /// Uses clockwise winding to match USD mesh convention.
    #[allow(clippy::vec_init_then_push)]
    pub fn from_aabb(aabb: &Aabb) -> Self {
        let min = aabb.min_point();
        let max = aabb.max_point();

        // 8 corner vertices of the box
        let corners = [
            Vec3::new(min.x, min.y, min.z), // 0: front-bottom-left
            Vec3::new(max.x, min.y, min.z), // 1: front-bottom-right
            Vec3::new(max.x, max.y, min.z), // 2: front-top-right
            Vec3::new(min.x, max.y, min.z), // 3: front-top-left
            Vec3::new(min.x, min.y, max.z), // 4: back-bottom-left
            Vec3::new(max.x, min.y, max.z), // 5: back-bottom-right
            Vec3::new(max.x, max.y, max.z), // 6: back-top-right
            Vec3::new(min.x, max.y, max.z), // 7: back-top-left
        ];

        // Face normals
        let normals = [
            Vec3::new(0.0, 0.0, -1.0), // front (negative Z)
            Vec3::new(0.0, 0.0, 1.0),  // back (positive Z)
            Vec3::new(-1.0, 0.0, 0.0), // left (negative X)
            Vec3::new(1.0, 0.0, 0.0),  // right (positive X)
            Vec3::new(0.0, -1.0, 0.0), // bottom (negative Y)
            Vec3::new(0.0, 1.0, 0.0),  // top (positive Y)
        ];

        let grey = [0.4, 0.4, 0.4]; // Slightly darker grey for LOD boxes

        // Build vertices with per-face normals (24 vertices = 6 faces x 4 corners)
        let mut vertices = Vec::with_capacity(24);

        // Front face (z = min) - vertices 0,1,2,3, normal -Z
        vertices.push(Vertex {
            position: corners[0].into(),
            normal: normals[0].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[1].into(),
            normal: normals[0].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[2].into(),
            normal: normals[0].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[3].into(),
            normal: normals[0].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });

        // Back face (z = max) - vertices 5,4,7,6, normal +Z
        vertices.push(Vertex {
            position: corners[5].into(),
            normal: normals[1].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[4].into(),
            normal: normals[1].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[7].into(),
            normal: normals[1].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[6].into(),
            normal: normals[1].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });

        // Left face (x = min) - vertices 4,0,3,7, normal -X
        vertices.push(Vertex {
            position: corners[4].into(),
            normal: normals[2].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[0].into(),
            normal: normals[2].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[3].into(),
            normal: normals[2].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[7].into(),
            normal: normals[2].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });

        // Right face (x = max) - vertices 1,5,6,2, normal +X
        vertices.push(Vertex {
            position: corners[1].into(),
            normal: normals[3].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[5].into(),
            normal: normals[3].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[6].into(),
            normal: normals[3].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[2].into(),
            normal: normals[3].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });

        // Bottom face (y = min) - vertices 4,5,1,0, normal -Y
        vertices.push(Vertex {
            position: corners[4].into(),
            normal: normals[4].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[5].into(),
            normal: normals[4].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[1].into(),
            normal: normals[4].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[0].into(),
            normal: normals[4].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });

        // Top face (y = max) - vertices 3,2,6,7, normal +Y
        vertices.push(Vertex {
            position: corners[3].into(),
            normal: normals[5].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[2].into(),
            normal: normals[5].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[6].into(),
            normal: normals[5].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });
        vertices.push(Vertex {
            position: corners[7].into(),
            normal: normals[5].into(),
            color: grey,
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        });

        // Indices for 6 faces (clockwise winding for USD convention)
        // Each face has 4 vertices and 2 triangles (6 indices)
        let mut indices = Vec::with_capacity(36);
        for face in 0..6 {
            let base = face * 4;
            // CW winding: 0,2,1 and 0,3,2
            indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
        }

        Self {
            vertices,
            indices,
            bounds_min: min,
            bounds_max: max,
            triangle_material_ids: None,
        }
    }

    /// Load an OBJ file into mesh data.
    pub fn load_obj<P: AsRef<Path>>(path: P) -> Result<Self> {
        let (models, _materials) = tobj::load_obj(
            path.as_ref(),
            &tobj::LoadOptions {
                single_index: true,
                triangulate: true,
                ..Default::default()
            },
        )?;

        if models.is_empty() {
            anyhow::bail!("No models found in OBJ file");
        }

        // Take first model
        let model = &models[0];
        let mesh = &model.mesh;

        // Build vertices with normals
        let mut vertices = Vec::new();
        let vertex_count = mesh.positions.len() / 3;

        let has_normals = !mesh.normals.is_empty();
        log::info!("Mesh has normals: {}", has_normals);

        // If no normals, compute per-face normals
        let computed_normals = if !has_normals {
            log::info!("Computing per-face normals...");
            let mut normals = vec![[0.0f32; 3]; vertex_count];

            // Compute face normals and accumulate at vertices
            for face in mesh.indices.chunks(3) {
                let i0 = face[0] as usize;
                let i1 = face[1] as usize;
                let i2 = face[2] as usize;

                let p0 = Vec3::from_slice(&mesh.positions[i0 * 3..i0 * 3 + 3]);
                let p1 = Vec3::from_slice(&mesh.positions[i1 * 3..i1 * 3 + 3]);
                let p2 = Vec3::from_slice(&mesh.positions[i2 * 3..i2 * 3 + 3]);

                let edge1 = p1 - p0;
                let edge2 = p2 - p0;
                let face_normal = edge1.cross(edge2).normalize();

                // Accumulate at each vertex
                for &idx in &[i0, i1, i2] {
                    normals[idx][0] += face_normal.x;
                    normals[idx][1] += face_normal.y;
                    normals[idx][2] += face_normal.z;
                }
            }

            // Normalize accumulated normals
            for normal in &mut normals {
                let len =
                    (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
                if len > 0.0 {
                    normal[0] /= len;
                    normal[1] /= len;
                    normal[2] /= len;
                }
            }

            Some(normals)
        } else {
            None
        };

        for i in 0..vertex_count {
            let pos_idx = i * 3;

            // Use computed normals if available, otherwise from file
            let normal = if let Some(ref computed) = computed_normals {
                computed[i]
            } else if has_normals {
                let norm_idx = i * 3;
                [
                    mesh.normals[norm_idx],
                    mesh.normals[norm_idx + 1],
                    mesh.normals[norm_idx + 2],
                ]
            } else {
                [0.0, 1.0, 0.0]
            };

            // Color from normal (not needed anymore, shader uses normal directly)
            let color = [normal[0].abs(), normal[1].abs(), normal[2].abs()];

            vertices.push(Vertex {
                position: [
                    mesh.positions[pos_idx],
                    mesh.positions[pos_idx + 1],
                    mesh.positions[pos_idx + 2],
                ],
                normal,
                color,
                uv: [0.0, 0.0], // OBJ loading doesn't have UVs yet
                material_id: 0xFFFFFFFF,
            });
        }

        // Calculate bounding box
        let mut bounds_min = Vec3::splat(f32::INFINITY);
        let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);

        for vertex in &vertices {
            let pos = Vec3::from_array(vertex.position);
            bounds_min = bounds_min.min(pos);
            bounds_max = bounds_max.max(pos);
        }

        Ok(Self {
            vertices,
            indices: mesh.indices.clone(),
            bounds_min,
            bounds_max,
            triangle_material_ids: None,
        })
    }

    /// Convert a bif_core::Mesh to GPU-ready MeshData.
    ///
    /// Keeps vertices indexed (shared) for memory efficiency.
    /// Per-triangle material IDs are stored separately for primitive_index lookup.
    pub fn from_core_mesh(mesh: &bif_core::Mesh) -> Self {
        let default_normal = Vec3::Y;
        let default_uv = [0.0f32, 0.0f32];

        // Build indexed vertices (shared across triangles)
        let mut vertices = Vec::with_capacity(mesh.positions.len());

        for (i, pos) in mesh.positions.iter().enumerate() {
            let normal = mesh
                .normals
                .as_ref()
                .and_then(|n| n.get(i))
                .unwrap_or(&default_normal);

            let uv = mesh
                .uvs
                .as_ref()
                .and_then(|uvs| uvs.get(i))
                .copied()
                .unwrap_or(default_uv);

            let color = [normal.x.abs(), normal.y.abs(), normal.z.abs()];

            vertices.push(Vertex {
                position: [pos.x, pos.y, pos.z],
                normal: [normal.x, normal.y, normal.z],
                color,
                uv,
                material_id: 0xFFFFFFFF, // Not used - material comes from triangle buffer
            });
        }

        let bounds_min = Vec3::new(mesh.bounds.x.min, mesh.bounds.y.min, mesh.bounds.z.min);
        let bounds_max = Vec3::new(mesh.bounds.x.max, mesh.bounds.y.max, mesh.bounds.z.max);

        // Store per-triangle material IDs if present (for primitive_index lookup in shader)
        let triangle_material_ids = mesh.face_material_ids.as_ref().map(|face_mat_ids| {
            let unique: std::collections::HashSet<_> = face_mat_ids.iter().collect();
            log::info!(
                "Mesh with per-face materials: {} triangles, {} unique materials (IDs: {:?})",
                face_mat_ids.len(),
                unique.len(),
                unique.iter().take(10).collect::<Vec<_>>()
            );
            face_mat_ids.clone()
        });

        Self {
            vertices,
            indices: mesh.indices.clone(),
            bounds_min,
            bounds_max,
            triangle_material_ids,
        }
    }

    /// Combine multiple meshes with transforms into a single MeshData.
    ///
    /// This is used when a scene has multiple different prototypes that need
    /// to be rendered together. Each mesh is transformed by its instance transform
    /// before being combined.
    pub fn combine_with_transforms(
        meshes: &[(&bif_core::Mesh, bif_math::Mat4)],
    ) -> Self {
        let default_normal = Vec3::Y;
        let default_uv = [0.0f32, 0.0f32];

        let mut all_vertices = Vec::new();
        let mut all_indices = Vec::new();
        let mut all_triangle_material_ids = Vec::new();
        let mut bounds_min = Vec3::splat(f32::INFINITY);
        let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);

        for (mesh, transform) in meshes {
            let vertex_offset = all_vertices.len() as u32;

            // Transform and add vertices
            for (i, pos) in mesh.positions.iter().enumerate() {
                let normal = mesh
                    .normals
                    .as_ref()
                    .and_then(|n| n.get(i))
                    .unwrap_or(&default_normal);

                let uv = mesh
                    .uvs
                    .as_ref()
                    .and_then(|uvs| uvs.get(i))
                    .copied()
                    .unwrap_or(default_uv);

                // Transform position by instance matrix
                let pos4 = bif_math::Vec4::new(pos.x, pos.y, pos.z, 1.0);
                let transformed = *transform * pos4;
                let transformed_pos = Vec3::new(transformed.x, transformed.y, transformed.z);

                // Transform normal (use upper-left 3x3, ignore translation)
                let normal4 = bif_math::Vec4::new(normal.x, normal.y, normal.z, 0.0);
                let transformed_normal = *transform * normal4;
                let transformed_normal = Vec3::new(
                    transformed_normal.x,
                    transformed_normal.y,
                    transformed_normal.z,
                )
                .normalize();

                let color = [
                    transformed_normal.x.abs(),
                    transformed_normal.y.abs(),
                    transformed_normal.z.abs(),
                ];

                all_vertices.push(Vertex {
                    position: [transformed_pos.x, transformed_pos.y, transformed_pos.z],
                    normal: [transformed_normal.x, transformed_normal.y, transformed_normal.z],
                    color,
                    uv,
                    material_id: 0xFFFFFFFF,
                });

                // Update bounds
                bounds_min = bounds_min.min(transformed_pos);
                bounds_max = bounds_max.max(transformed_pos);
            }

            // Add indices with offset
            for idx in &mesh.indices {
                all_indices.push(*idx + vertex_offset);
            }

            // Add per-triangle material IDs if present
            if let Some(ref face_mat_ids) = mesh.face_material_ids {
                all_triangle_material_ids.extend(face_mat_ids.iter().copied());
            } else {
                // No face materials - fill with default
                let triangle_count = mesh.indices.len() / 3;
                all_triangle_material_ids.extend(std::iter::repeat_n(0u32, triangle_count));
            }
        }

        let triangle_material_ids = if all_triangle_material_ids.iter().all(|&id| id == 0) {
            None
        } else {
            Some(all_triangle_material_ids)
        };

        Self {
            vertices: all_vertices,
            indices: all_indices,
            bounds_min,
            bounds_max,
            triangle_material_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bif_math::{Aabb, Vec3};

    #[test]
    fn test_from_aabb_vertex_count() {
        let aabb = Aabb::from_points(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5));
        let mesh = MeshData::from_aabb(&aabb);
        assert_eq!(mesh.vertices.len(), 24); // 6 faces * 4 verts
        assert_eq!(mesh.indices.len(), 36); // 6 faces * 2 tris * 3
    }

    #[test]
    fn test_from_aabb_bounds() {
        let aabb = Aabb::from_points(Vec3::new(0.0, 1.0, 2.0), Vec3::new(2.0, 3.0, 4.0));
        let mesh = MeshData::from_aabb(&aabb);
        assert_eq!(mesh.bounds_min, Vec3::new(0.0, 1.0, 2.0));
        assert_eq!(mesh.bounds_max, Vec3::new(2.0, 3.0, 4.0));
    }

    #[test]
    fn test_center_and_size() {
        let aabb = Aabb::from_points(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let mesh = MeshData::from_aabb(&aabb);
        assert!((mesh.center() - Vec3::ZERO).length() < 0.001);
        // size = diagonal of 2x2x2 cube = sqrt(12) ≈ 3.46
        assert!((mesh.size() - 3.464).abs() < 0.01);
    }
}
