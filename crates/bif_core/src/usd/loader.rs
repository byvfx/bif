//! High-level USD scene loading.
//!
//! This module provides the main entry point for loading USD files
//! and converting them to BIF scene graph representation.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bif_math::Mat4;
use thiserror::Error;

use crate::mesh::Mesh;
use crate::scene::{Scene, Transform};
use crate::usd::parser::{parse_usda, ParseError};
use crate::usd::types::{UsdMesh, UsdPointInstancer, UsdPrim, UsdXform};

/// Errors that can occur during USD loading.
#[derive(Error, Debug)]
pub enum LoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),
    
    #[error("No geometry found in USD file")]
    NoGeometry,
    
    #[error("Invalid prototype reference: {0}")]
    InvalidPrototype(String),
}

/// Result type for loading operations.
pub type LoadResult<T> = Result<T, LoadError>;

/// Load a USDA file and return a BIF Scene.
///
/// This function reads the USDA file, parses it, and converts the USD prims
/// to BIF scene graph types. Meshes without normals will have them computed
/// automatically.
///
/// # Example
///
/// ```ignore
/// use bif_core::usd::load_usda;
///
/// let scene = load_usda("scene.usda")?;
/// println!("Loaded {} instances", scene.instance_count());
/// ```
pub fn load_usda<P: AsRef<Path>>(path: P) -> LoadResult<Scene> {
    let content = std::fs::read_to_string(path.as_ref())?;
    load_usda_from_string(&content, path.as_ref().to_string_lossy().as_ref())
}

/// Load USDA from a string (useful for testing).
pub fn load_usda_from_string(content: &str, name: &str) -> LoadResult<Scene> {
    let prims = parse_usda(content)?;
    
    let mut builder = SceneBuilder::new(name);
    
    for prim in prims {
        builder.process_prim(&prim, Mat4::IDENTITY)?;
    }
    
    builder.finish()
}

/// Internal builder for constructing a Scene from USD prims.
struct SceneBuilder {
    scene: Scene,
    /// Map from USD prim path to prototype ID
    prototype_map: HashMap<String, usize>,
}

impl SceneBuilder {
    fn new(name: &str) -> Self {
        Self {
            scene: Scene::new(name),
            prototype_map: HashMap::new(),
        }
    }
    
    /// Process a USD prim recursively.
    fn process_prim(&mut self, prim: &UsdPrim, parent_transform: Mat4) -> LoadResult<()> {
        match prim {
            UsdPrim::Xform(xform) => self.process_xform(xform, parent_transform),
            UsdPrim::Mesh(mesh) => self.process_mesh(mesh, parent_transform),
            UsdPrim::PointInstancer(instancer) => self.process_point_instancer(instancer, parent_transform),
            UsdPrim::Unknown(_) => Ok(()), // Skip unknown prims
        }
    }
    
    /// Process an Xform (transform) prim.
    fn process_xform(&mut self, xform: &UsdXform, parent_transform: Mat4) -> LoadResult<()> {
        let world_transform = parent_transform * xform.transform;
        
        // Process children with accumulated transform
        for child in &xform.children {
            self.process_prim(child, world_transform)?;
        }
        
        Ok(())
    }
    
    /// Process a Mesh prim.
    fn process_mesh(&mut self, usd_mesh: &UsdMesh, parent_transform: Mat4) -> LoadResult<()> {
        let world_transform = parent_transform * usd_mesh.transform;
        
        // Convert USD mesh to BIF mesh
        let mut mesh = self.convert_mesh(usd_mesh)?;
        
        // Ensure normals exist - compute if not provided in USD
        mesh.ensure_normals();
        
        let mesh = Arc::new(mesh);
        
        // Check if we already have this prototype
        let proto_id = if let Some(&id) = self.prototype_map.get(&usd_mesh.path) {
            id
        } else {
            let id = self.scene.add_prototype(mesh, usd_mesh.name.clone());
            self.prototype_map.insert(usd_mesh.path.clone(), id);
            id
        };
        
        // Add an instance with the accumulated transform
        self.scene.add_instance(proto_id, Transform::from_matrix(world_transform));
        
        Ok(())
    }
    
    /// Process a PointInstancer prim.
    fn process_point_instancer(&mut self, instancer: &UsdPointInstancer, parent_transform: Mat4) -> LoadResult<()> {
        let world_transform = parent_transform * instancer.transform;
        
        // First, collect inline prototype definitions
        let mut inline_prototypes: Vec<usize> = Vec::new();
        
        for child in &instancer.children {
            if let UsdPrim::Mesh(mesh) = child {
                let mut bif_mesh = self.convert_mesh(mesh)?;
                bif_mesh.ensure_normals();
                
                let name = mesh.name.clone();
                let mesh_arc = Arc::new(bif_mesh);
                let id = self.scene.add_prototype(mesh_arc, name.clone());
                self.prototype_map.insert(mesh.path.clone(), id);
                inline_prototypes.push(id);
            }
        }
        
        // If no inline prototypes, try to resolve prototype paths
        // For now, we only support inline prototypes
        if inline_prototypes.is_empty() && !instancer.prototypes.is_empty() {
            // Try to find prototypes by path
            for proto_path in &instancer.prototypes {
                if let Some(&id) = self.prototype_map.get(proto_path) {
                    inline_prototypes.push(id);
                } else {
                    log::warn!("Could not resolve prototype path: {}", proto_path);
                }
            }
        }
        
        // Create instances
        for i in 0..instancer.positions.len() {
            let proto_idx = instancer.proto_indices.get(i).copied().unwrap_or(0) as usize;
            
            // Get the prototype ID (from inline prototypes or fallback to first)
            let proto_id = inline_prototypes.get(proto_idx).copied()
                .or_else(|| inline_prototypes.first().copied())
                .unwrap_or(0);
            
            // Build instance transform
            let instance_matrix = instancer.instance_matrix(i);
            let final_matrix = world_transform * instance_matrix;
            
            self.scene.add_instance(proto_id, Transform::from_matrix(final_matrix));
        }
        
        Ok(())
    }
    
    /// Convert a USD mesh to a BIF mesh.
    fn convert_mesh(&self, usd_mesh: &UsdMesh) -> LoadResult<Mesh> {
        // Triangulate the mesh
        let indices = usd_mesh.triangulate();
        
        // Convert normals if present
        let normals = usd_mesh.normals.clone();
        
        Ok(Mesh::new(usd_mesh.points.clone(), indices, normals))
    }
    
    /// Finish building and return the Scene.
    fn finish(self) -> LoadResult<Scene> {
        if self.scene.prototypes.is_empty() {
            return Err(LoadError::NoGeometry);
        }
        
        Ok(self.scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_load_simple_mesh() {
        let usda = r#"
def Mesh "Triangle" {
    point3f[] points = [(0, 0, 0), (1, 0, 0), (0.5, 1, 0)]
    int[] faceVertexCounts = [3]
    int[] faceVertexIndices = [0, 1, 2]
}
"#;
        
        let scene = load_usda_from_string(usda, "test").unwrap();
        
        assert_eq!(scene.prototype_count(), 1);
        assert_eq!(scene.instance_count(), 1);
        assert_eq!(scene.total_triangle_count(), 1);
        
        // Check that normals were computed
        assert!(scene.prototypes[0].mesh.has_normals());
    }
    
    #[test]
    fn test_load_mesh_with_normals() {
        let usda = r#"
def Mesh "Triangle" {
    point3f[] points = [(0, 0, 0), (1, 0, 0), (0.5, 1, 0)]
    normal3f[] normals = [(0, 0, 1), (0, 0, 1), (0, 0, 1)]
    int[] faceVertexCounts = [3]
    int[] faceVertexIndices = [0, 1, 2]
}
"#;
        
        let scene = load_usda_from_string(usda, "test").unwrap();
        
        // Check that provided normals were used
        let normals = scene.prototypes[0].mesh.normals.as_ref().unwrap();
        assert_eq!(normals.len(), 3);
        assert!((normals[0].z - 1.0).abs() < 0.001);
    }
    
    #[test]
    fn test_load_point_instancer() {
        let usda = r#"
def PointInstancer "Grid" {
    int[] protoIndices = [0, 0, 0, 0]
    point3f[] positions = [(0, 0, 0), (2, 0, 0), (0, 0, 2), (2, 0, 2)]
    
    def Mesh "Proto" {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0.5, 1, 0)]
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
    }
}
"#;
        
        let scene = load_usda_from_string(usda, "test").unwrap();
        
        assert_eq!(scene.prototype_count(), 1);
        assert_eq!(scene.instance_count(), 4);
        assert_eq!(scene.total_triangle_count(), 4); // 1 triangle × 4 instances
    }
    
    #[test]
    fn test_load_transformed_mesh() {
        let usda = r#"
def Xform "World" {
    double3 xformOp:translate = (10, 0, 0)
    
    def Mesh "Cube" {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
        int[] faceVertexCounts = [4]
        int[] faceVertexIndices = [0, 1, 2, 3]
    }
}
"#;
        
        let scene = load_usda_from_string(usda, "test").unwrap();
        
        assert_eq!(scene.prototype_count(), 1);
        assert_eq!(scene.instance_count(), 1);
        
        // Check that the transform was applied to the instance
        let matrix = scene.instances[0].model_matrix();
        let origin = matrix.transform_point3(bif_math::Vec3::ZERO);
        assert!((origin.x - 10.0).abs() < 0.001);
    }
}
