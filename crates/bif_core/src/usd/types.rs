//! USD primitive types for intermediate representation.
//!
//! These types represent parsed USD prims before conversion to BIF scene graph types.

use bif_math::{Mat4, Quat, Vec3};

/// A parsed USD prim (generic container).
#[derive(Clone, Debug)]
pub enum UsdPrim {
    /// A transform node
    Xform(UsdXform),
    
    /// A mesh geometry
    Mesh(UsdMesh),
    
    /// A point instancer
    PointInstancer(UsdPointInstancer),
    
    /// A reference to an external USD file
    Reference(UsdReference),
    
    /// An unknown or unsupported prim type
    Unknown(String),
}

/// A USD Reference to an external file.
/// Syntax: `references = @path/to/file.usda@</PrimPath>`
#[derive(Clone, Debug, Default)]
pub struct UsdReference {
    /// Prim path in current file
    pub path: String,
    
    /// Prim name
    pub name: String,
    
    /// Path to the external USD file (relative or absolute)
    pub asset_path: String,
    
    /// Optional prim path within the referenced file (e.g., "/Lucy")
    pub target_prim_path: Option<String>,
    
    /// Local transform applied to the reference
    pub transform: Mat4,
    
    /// Child prims (overrides or additional content)
    pub children: Vec<UsdPrim>,
}

/// A USD Xform (transform) prim.
#[derive(Clone, Debug, Default)]
pub struct UsdXform {
    /// Prim path (e.g., "/World/Model")
    pub path: String,
    
    /// Prim name (last component of path)
    pub name: String,
    
    /// Combined transform matrix from xformOps
    pub transform: Mat4,
    
    /// Child prims
    pub children: Vec<UsdPrim>,
}

/// A USD Mesh prim.
#[derive(Clone, Debug, Default)]
pub struct UsdMesh {
    /// Prim path
    pub path: String,
    
    /// Prim name
    pub name: String,
    
    /// Vertex positions
    pub points: Vec<Vec3>,
    
    /// Number of vertices per face (for triangulation)
    pub face_vertex_counts: Vec<i32>,
    
    /// Vertex indices for each face
    pub face_vertex_indices: Vec<i32>,
    
    /// Vertex normals (optional)
    pub normals: Option<Vec<Vec3>>,
    
    /// Local transform
    pub transform: Mat4,
}

impl UsdMesh {
    /// Triangulate the mesh and return indices suitable for GPU rendering.
    ///
    /// USD meshes can have n-gons (faces with more than 3 vertices).
    /// This method converts them to triangles using fan triangulation.
    pub fn triangulate(&self) -> Vec<u32> {
        let mut indices = Vec::new();
        let mut vertex_offset = 0usize;
        
        for &count in &self.face_vertex_counts {
            let count = count as usize;
            if count < 3 {
                vertex_offset += count;
                continue;
            }
            
            // Fan triangulation: for a polygon with vertices [0, 1, 2, 3, ...n-1]
            // create triangles: (0,1,2), (0,2,3), (0,3,4), ... (0,n-2,n-1)
            for i in 1..(count - 1) {
                let i0 = self.face_vertex_indices[vertex_offset] as u32;
                let i1 = self.face_vertex_indices[vertex_offset + i] as u32;
                let i2 = self.face_vertex_indices[vertex_offset + i + 1] as u32;
                indices.push(i0);
                indices.push(i1);
                indices.push(i2);
            }
            
            vertex_offset += count;
        }
        
        indices
    }
}

/// A USD PointInstancer prim.
#[derive(Clone, Debug, Default)]
pub struct UsdPointInstancer {
    /// Prim path
    pub path: String,
    
    /// Prim name
    pub name: String,
    
    /// Prototype indices (which prototype each instance uses)
    pub proto_indices: Vec<i32>,
    
    /// Instance positions
    pub positions: Vec<Vec3>,
    
    /// Instance orientations (as quaternions)
    pub orientations: Option<Vec<Quat>>,
    
    /// Instance scales (uniform or per-axis)
    pub scales: Option<Vec<Vec3>>,
    
    /// Paths to prototype prims
    pub prototypes: Vec<String>,
    
    /// Local transform
    pub transform: Mat4,
    
    /// Inline prototype definitions (children)
    pub children: Vec<UsdPrim>,
}

impl UsdPointInstancer {
    /// Get the transform matrix for a specific instance.
    pub fn instance_matrix(&self, index: usize) -> Mat4 {
        if index >= self.positions.len() {
            return Mat4::IDENTITY;
        }
        
        let translation = self.positions[index];
        
        let rotation = self.orientations
            .as_ref()
            .and_then(|o| o.get(index))
            .copied()
            .unwrap_or(Quat::IDENTITY);
        
        let scale = self.scales
            .as_ref()
            .and_then(|s| s.get(index))
            .copied()
            .unwrap_or(Vec3::ONE);
        
        Mat4::from_scale_rotation_translation(scale, rotation, translation)
    }
    
    /// Get the number of instances.
    pub fn instance_count(&self) -> usize {
        self.positions.len()
    }
}

/// Transform operation types found in USD xformOps.
#[derive(Clone, Debug)]
pub enum XformOp {
    /// Translation (xformOp:translate)
    Translate(Vec3),
    
    /// Rotation in degrees around X axis
    RotateX(f32),
    
    /// Rotation in degrees around Y axis
    RotateY(f32),
    
    /// Rotation in degrees around Z axis
    RotateZ(f32),
    
    /// Euler rotation XYZ in degrees
    RotateXYZ(Vec3),
    
    /// Scale (uniform or non-uniform)
    Scale(Vec3),
    
    /// Full 4x4 transform matrix
    Transform(Mat4),
}

impl XformOp {
    /// Convert this operation to a transformation matrix.
    pub fn to_matrix(&self) -> Mat4 {
        match self {
            XformOp::Translate(t) => Mat4::from_translation(*t),
            XformOp::RotateX(deg) => Mat4::from_rotation_x(deg.to_radians()),
            XformOp::RotateY(deg) => Mat4::from_rotation_y(deg.to_radians()),
            XformOp::RotateZ(deg) => Mat4::from_rotation_z(deg.to_radians()),
            XformOp::RotateXYZ(euler) => {
                Mat4::from_rotation_x(euler.x.to_radians())
                    * Mat4::from_rotation_y(euler.y.to_radians())
                    * Mat4::from_rotation_z(euler.z.to_radians())
            }
            XformOp::Scale(s) => Mat4::from_scale(*s),
            XformOp::Transform(m) => *m,
        }
    }
}

/// Combine a list of xformOps into a single matrix.
pub fn compose_xform_ops(ops: &[XformOp]) -> Mat4 {
    let mut result = Mat4::IDENTITY;
    for op in ops {
        result = result * op.to_matrix();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_triangulate_triangle() {
        let mesh = UsdMesh {
            face_vertex_counts: vec![3],
            face_vertex_indices: vec![0, 1, 2],
            ..Default::default()
        };
        
        let indices = mesh.triangulate();
        assert_eq!(indices, vec![0, 1, 2]);
    }
    
    #[test]
    fn test_triangulate_quad() {
        let mesh = UsdMesh {
            face_vertex_counts: vec![4],
            face_vertex_indices: vec![0, 1, 2, 3],
            ..Default::default()
        };
        
        let indices = mesh.triangulate();
        // Quad (0,1,2,3) -> triangles (0,1,2) and (0,2,3)
        assert_eq!(indices, vec![0, 1, 2, 0, 2, 3]);
    }
    
    #[test]
    fn test_xform_ops() {
        let translate = XformOp::Translate(Vec3::new(1.0, 2.0, 3.0));
        let matrix = translate.to_matrix();
        
        let origin = matrix.transform_point3(Vec3::ZERO);
        assert!((origin - Vec3::new(1.0, 2.0, 3.0)).length() < 0.001);
    }
}
