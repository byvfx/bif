//! Mesh geometry representation for BIF scene graph.
//!
//! This module provides a GPU-agnostic mesh representation that can be
//! populated from various file formats (USD, OBJ, etc.) and converted
//! to GPU vertex buffers by the viewport.

use bif_math::{Aabb, Vec3};

/// A mesh consisting of vertex positions, optional normals, and triangle indices.
///
/// This is the core geometry type used throughout BIF. It is intentionally
/// decoupled from GPU-specific types (like `Vertex` with color) to allow
/// flexible loading from various file formats.
#[derive(Clone, Debug)]
pub struct Mesh {
    /// Vertex positions (one Vec3 per vertex)
    pub positions: Vec<Vec3>,
    
    /// Vertex normals (optional - will be computed if not provided)
    pub normals: Option<Vec<Vec3>>,
    
    /// Triangle indices (every 3 indices form a triangle)
    pub indices: Vec<u32>,
    
    /// Axis-aligned bounding box
    pub bounds: Aabb,
}

impl Mesh {
    /// Create a new mesh from positions and indices, optionally with normals.
    ///
    /// If normals are not provided, they will NOT be automatically computed.
    /// Call `compute_normals()` explicitly if you need them.
    pub fn new(positions: Vec<Vec3>, indices: Vec<u32>, normals: Option<Vec<Vec3>>) -> Self {
        let bounds = Self::compute_bounds(&positions);
        Self {
            positions,
            normals,
            indices,
            bounds,
        }
    }
    
    /// Compute axis-aligned bounding box from positions.
    fn compute_bounds(positions: &[Vec3]) -> Aabb {
        if positions.is_empty() {
            return Aabb::empty();
        }
        
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        
        for pos in positions {
            min = min.min(*pos);
            max = max.max(*pos);
        }
        
        Aabb::from_points(min, max)
    }
    
    /// Compute smooth vertex normals by averaging face normals.
    ///
    /// This generates normals if the mesh doesn't have them, or replaces
    /// existing normals. Each vertex normal is the normalized average of
    /// all face normals for faces that share that vertex.
    pub fn compute_normals(&mut self) {
        let vertex_count = self.positions.len();
        let mut normals = vec![Vec3::ZERO; vertex_count];
        
        // Accumulate face normals at each vertex
        for face in self.indices.chunks(3) {
            if face.len() < 3 {
                continue;
            }
            
            let i0 = face[0] as usize;
            let i1 = face[1] as usize;
            let i2 = face[2] as usize;
            
            if i0 >= vertex_count || i1 >= vertex_count || i2 >= vertex_count {
                continue;
            }
            
            let p0 = self.positions[i0];
            let p1 = self.positions[i1];
            let p2 = self.positions[i2];
            
            let edge1 = p1 - p0;
            let edge2 = p2 - p0;
            let face_normal = edge1.cross(edge2); // Not normalized - area-weighted
            
            normals[i0] += face_normal;
            normals[i1] += face_normal;
            normals[i2] += face_normal;
        }
        
        // Normalize accumulated normals
        for normal in &mut normals {
            let len = normal.length();
            if len > 0.0 {
                *normal /= len;
            } else {
                *normal = Vec3::Y; // Default up normal for degenerate cases
            }
        }
        
        self.normals = Some(normals);
    }
    
    /// Check if the mesh has normals.
    pub fn has_normals(&self) -> bool {
        self.normals.is_some()
    }
    
    /// Ensure the mesh has normals, computing them if necessary.
    /// Also recomputes if existing normals don't match vertex count (e.g., face-varying normals).
    pub fn ensure_normals(&mut self) {
        let should_compute = match &self.normals {
            None => true,
            Some(normals) => normals.len() != self.positions.len(),
        };
        
        if should_compute {
            if self.normals.is_some() {
                // Only log at debug level - this is expected for face-varying normals from USD
                log::debug!(
                    "Normals array length ({}) doesn't match vertex count ({}), computing smooth normals",
                    self.normals.as_ref().unwrap().len(),
                    self.positions.len()
                );
            }
            self.compute_normals();
        }
    }
    
    /// Get the mesh center (center of bounding box).
    pub fn center(&self) -> Vec3 {
        self.bounds.centroid()
    }
    
    /// Get the mesh size (diagonal length of bounding box).
    pub fn size(&self) -> f32 {
        let extent = Vec3::new(
            self.bounds.x.max - self.bounds.x.min,
            self.bounds.y.max - self.bounds.y.min,
            self.bounds.z.max - self.bounds.z.min,
        );
        extent.length()
    }
    
    /// Get the number of triangles in the mesh.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
    
    /// Get the number of vertices in the mesh.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mesh_creation() {
        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let indices = vec![0, 1, 2];
        
        let mesh = Mesh::new(positions.clone(), indices.clone(), None);
        
        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.triangle_count(), 1);
        assert!(!mesh.has_normals());
    }
    
    #[test]
    fn test_compute_normals() {
        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let indices = vec![0, 1, 2];
        
        let mut mesh = Mesh::new(positions, indices, None);
        mesh.compute_normals();
        
        assert!(mesh.has_normals());
        let normals = mesh.normals.as_ref().unwrap();
        
        // For a CCW triangle in XY plane, normal should point in +Z
        for normal in normals {
            assert!((normal.z - 1.0).abs() < 0.001);
        }
    }
    
    #[test]
    fn test_bounds_computation() {
        let positions = vec![
            Vec3::new(-1.0, -2.0, -3.0),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(0.0, 0.0, 0.0),
        ];
        let indices = vec![0, 1, 2];
        
        let mesh = Mesh::new(positions, indices, None);
        
        assert!((mesh.bounds.x.min - (-1.0)).abs() < 0.001);
        assert!((mesh.bounds.x.max - 4.0).abs() < 0.001);
        assert!((mesh.bounds.y.min - (-2.0)).abs() < 0.001);
        assert!((mesh.bounds.y.max - 5.0).abs() < 0.001);
        assert!((mesh.bounds.z.min - (-3.0)).abs() < 0.001);
        assert!((mesh.bounds.z.max - 6.0).abs() < 0.001);
    }
}
