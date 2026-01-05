//! High-level USD scene loading.
//!
//! This module provides the main entry point for loading USD files
//! and converting them to BIF scene graph representation.
//!
//! Supports all USD formats via the C++ bridge:
//! - `.usda` - ASCII text format
//! - `.usdc` - Binary crate format  
//! - `.usd` - Auto-detect format

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bif_math::Mat4;
use thiserror::Error;

use crate::mesh::Mesh;
use crate::scene::{Scene, Transform};
use crate::usd::parser::{parse_usda, ParseError};
use crate::usd::types::{UsdMesh, UsdPointInstancer, UsdPrim, UsdReference, UsdXform};
use crate::usd::cpp_bridge::{UsdStage, UsdBridgeError};

/// Errors that can occur during USD loading.
#[derive(Error, Debug)]
pub enum LoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),
    
    #[error("USD bridge error: {0}")]
    Bridge(#[from] UsdBridgeError),
    
    #[error("No geometry found in USD file")]
    NoGeometry,
    
    #[error("Invalid prototype reference: {0}")]
    InvalidPrototype(String),
}

/// Result type for loading operations.
pub type LoadResult<T> = Result<T, LoadError>;

/// Load a USD file and return a BIF Scene.
///
/// This function supports all USD formats via the C++ bridge:
/// - `.usda` - ASCII text format
/// - `.usdc` - Binary crate format
/// - `.usd` - Auto-detect format
///
/// References are automatically resolved.
///
/// # Example
///
/// ```ignore
/// use bif_core::usd::load_usd;
///
/// let scene = load_usd("scene.usdc")?;
/// println!("Loaded {} instances", scene.instance_count());
/// ```
pub fn load_usd<P: AsRef<Path>>(path: P) -> LoadResult<Scene> {
    let path = path.as_ref();
    let name = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed");
    
    // Open stage via C++ bridge
    let stage = UsdStage::open(path)?;
    
    let mut scene = Scene::new(name);
    let mut prototype_map: HashMap<String, usize> = HashMap::new();
    
    // Mesh deduplication: (vertex_count, index_count, first_vertex_hash) -> proto_id
    // This handles referenced meshes that appear multiple times with different transforms
    let mut mesh_dedup: HashMap<(usize, usize, u64), usize> = HashMap::new();
    
    // Load all meshes as prototypes (with deduplication)
    let meshes = stage.meshes()?;
    for mesh_data in &meshes {
        let vertices = mesh_data.vertices.clone();
        let indices = mesh_data.indices.clone();
        let normals = mesh_data.normals.clone();
        
        // Create a hash key based on mesh geometry
        // Use vertex count, index count, and hash of first few vertices
        let vertex_hash = if vertices.len() >= 3 {
            // Hash first 3 vertices (as Vec3)
            let bits: u64 = vertices.iter().take(3)
                .enumerate()
                .map(|(i, v)| {
                    let x = v.x.to_bits() as u64;
                    let y = v.y.to_bits() as u64;
                    let z = v.z.to_bits() as u64;
                    (x ^ y.rotate_left(21) ^ z.rotate_left(42)).wrapping_mul(i as u64 + 1)
                })
                .fold(0, |acc, x| acc ^ x);
            bits
        } else {
            0
        };
        let dedup_key = (vertices.len(), indices.len(), vertex_hash);
        
        let proto_id = if let Some(&existing_id) = mesh_dedup.get(&dedup_key) {
            // Mesh already exists, reuse prototype
            existing_id
        } else {
            // New unique mesh, create prototype
            let mut mesh = Mesh::new(vertices, indices, normals);
            mesh.ensure_normals();
            
            let mesh_arc = Arc::new(mesh);
            let proto_id = scene.add_prototype(mesh_arc, mesh_data.path.clone());
            mesh_dedup.insert(dedup_key, proto_id);
            prototype_map.insert(mesh_data.path.clone(), proto_id);
            proto_id
        };
        
        // Add an instance with this mesh's world transform
        let transform = Transform::from_matrix(mesh_data.transform);
        scene.add_instance(proto_id, transform);
    }
    
    log::info!("Loaded {} unique prototypes from {} meshes", 
               scene.prototype_count(), meshes.len());
    
    // Load point instancers
    let instancers = stage.instancers()?;
    for instancer_data in &instancers {
        // Resolve prototypes
        let proto_ids: Vec<usize> = instancer_data.prototype_paths.iter()
            .filter_map(|path| prototype_map.get(path).copied())
            .collect();
        
        if proto_ids.is_empty() {
            log::warn!("Instancer {} has no resolvable prototypes", instancer_data.path);
            continue;
        }
        
        // Create instances
        for (i, transform) in instancer_data.transforms.iter().enumerate() {
            let proto_idx = instancer_data.proto_indices.get(i)
                .copied()
                .unwrap_or(0) as usize;
            
            let proto_id = proto_ids.get(proto_idx)
                .or(proto_ids.first())
                .copied()
                .unwrap_or(0);
            
            scene.add_instance(proto_id, Transform::from_matrix(*transform));
        }
    }
    
    if scene.prototypes.is_empty() {
        return Err(LoadError::NoGeometry);
    }
    
    Ok(scene)
}

/// Load a USDA file using the pure Rust parser (legacy).
///
/// For new code, prefer `load_usd()` which uses the C++ bridge
/// and supports all USD formats including references.
///
/// # Example
///
/// ```ignore
/// use bif_core::usd::load_usda;
///
/// let scene = load_usda("scene.usda")?;
/// ```
pub fn load_usda<P: AsRef<Path>>(path: P) -> LoadResult<Scene> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)?;
    let base_dir = path.parent().map(|p| p.to_path_buf());
    load_usda_from_string(&content, path.to_string_lossy().as_ref(), base_dir)
}

/// Load USDA from a string (useful for testing).
pub fn load_usda_from_string(content: &str, name: &str, base_dir: Option<PathBuf>) -> LoadResult<Scene> {
    let prims = parse_usda(content)?;
    
    let mut builder = SceneBuilder::new(name, base_dir);
    
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
    /// Base directory for resolving relative references
    base_dir: Option<PathBuf>,
    /// Cache of loaded reference files to avoid re-loading
    reference_cache: HashMap<String, Vec<UsdPrim>>,
}

impl SceneBuilder {
    fn new(name: &str, base_dir: Option<PathBuf>) -> Self {
        Self {
            scene: Scene::new(name),
            prototype_map: HashMap::new(),
            base_dir,
            reference_cache: HashMap::new(),
        }
    }
    
    /// Process a USD prim recursively.
    fn process_prim(&mut self, prim: &UsdPrim, parent_transform: Mat4) -> LoadResult<()> {
        match prim {
            UsdPrim::Xform(xform) => self.process_xform(xform, parent_transform),
            UsdPrim::Mesh(mesh) => self.process_mesh(mesh, parent_transform),
            UsdPrim::PointInstancer(instancer) => self.process_point_instancer(instancer, parent_transform),
            UsdPrim::Reference(reference) => self.process_reference(reference, parent_transform),
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
    
    /// Process a Reference prim by loading the referenced file.
    fn process_reference(&mut self, reference: &UsdReference, parent_transform: Mat4) -> LoadResult<()> {
        let world_transform = parent_transform * reference.transform;
        
        // Resolve the asset path relative to the base directory
        let asset_path = if let Some(base_dir) = &self.base_dir {
            base_dir.join(&reference.asset_path)
        } else {
            PathBuf::from(&reference.asset_path)
        };
        
        // Check cache first
        let cache_key = asset_path.to_string_lossy().to_string();
        let prims = if let Some(cached) = self.reference_cache.get(&cache_key) {
            cached.clone()
        } else {
            // Load and parse the referenced file
            let content = std::fs::read_to_string(&asset_path)
                .map_err(|e| LoadError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to load reference '{}': {}", reference.asset_path, e)
                )))?;
            
            let prims = crate::usd::parser::parse_usda(&content)?;
            self.reference_cache.insert(cache_key, prims.clone());
            prims
        };
        
        // Find the target prim (if specified) or process all root prims
        if let Some(target_path) = &reference.target_prim_path {
            // Find the specific prim by path
            for prim in &prims {
                if self.prim_matches_path(prim, target_path) {
                    self.process_prim(prim, world_transform)?;
                    break;
                }
            }
        } else {
            // Process all root prims from the referenced file
            for prim in &prims {
                self.process_prim(prim, world_transform)?;
            }
        }
        
        // Process any child overrides
        for child in &reference.children {
            self.process_prim(child, world_transform)?;
        }
        
        Ok(())
    }
    
    /// Check if a prim matches a target path.
    fn prim_matches_path(&self, prim: &UsdPrim, target_path: &str) -> bool {
        let prim_path = match prim {
            UsdPrim::Xform(x) => &x.path,
            UsdPrim::Mesh(m) => &m.path,
            UsdPrim::PointInstancer(p) => &p.path,
            UsdPrim::Reference(r) => &r.path,
            UsdPrim::Unknown(_) => return false,
        };
        
        // Match full path or just the name part
        prim_path == target_path || prim_path.ends_with(target_path)
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
        
        let scene = load_usda_from_string(usda, "test", None).unwrap();
        
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
        
        let scene = load_usda_from_string(usda, "test", None).unwrap();
        
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
        
        let scene = load_usda_from_string(usda, "test", None).unwrap();
        
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
        
        let scene = load_usda_from_string(usda, "test", None).unwrap();
        
        assert_eq!(scene.prototype_count(), 1);
        assert_eq!(scene.instance_count(), 1);
        
        // Check that the transform was applied to the instance
        let matrix = scene.instances[0].model_matrix();
        let origin = matrix.transform_point3(bif_math::Vec3::ZERO);
        assert!((origin.x - 10.0).abs() < 0.001);
    }
    
    // ========================================================================
    // Integration tests for C++ bridge (require USD to be installed)
    // Run with: cargo test --package bif_core -- --ignored
    // ========================================================================
    
    /// Helper to get test asset path (works from any working directory)
    fn test_asset_path(relative: &str) -> std::path::PathBuf {
        // Get the crate root via CARGO_MANIFEST_DIR or use relative path
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| ".".to_string());
        let crate_root = std::path::Path::new(&manifest_dir);
        // Go up to workspace root
        let workspace_root = crate_root.parent().unwrap().parent().unwrap();
        workspace_root.join(relative)
    }
    
    #[test]
    #[ignore = "requires USD C++ library installed"]
    fn test_load_usd_cube() {
        // Load cube.usda via C++ bridge
        let path = test_asset_path("assets/ref_test/cube.usda");
        let scene = super::load_usd(&path).unwrap();
        
        assert_eq!(scene.prototype_count(), 1, "Should have 1 prototype (cube)");
        assert_eq!(scene.instance_count(), 1, "Should have 1 instance");
        
        // Cube has 6 faces × 2 triangles = 12 triangles
        assert_eq!(scene.total_triangle_count(), 12);
    }
    
    #[test]
    #[ignore = "requires USD C++ library installed"]
    fn test_load_usd_with_references() {
        // Load ref_test.usda which references cube.usda
        let path = test_asset_path("assets/ref_test/ref_test.usda");
        let scene = super::load_usd(&path).unwrap();
        
        // Should have resolved the reference and loaded 2 cube instances
        assert!(scene.prototype_count() >= 1, "Should have at least 1 prototype");
        assert!(scene.instance_count() >= 2, "Should have at least 2 instances (2 referenced cubes)");
    }
    
    #[test]
    #[ignore = "requires USD C++ library installed"]  
    fn test_usda_and_cpp_bridge_produce_same_mesh() {
        let cube_path = test_asset_path("assets/ref_test/cube.usda");
        
        // Load with pure Rust parser
        let rust_scene = super::load_usda(&cube_path).unwrap();
        
        // Load with C++ bridge
        let cpp_scene = super::load_usd(&cube_path).unwrap();
        
        // Compare vertex counts
        let rust_verts = rust_scene.prototypes[0].mesh.positions.len();
        let cpp_verts = cpp_scene.prototypes[0].mesh.positions.len();
        assert_eq!(rust_verts, cpp_verts, "Vertex count should match");
        
        // Compare triangle counts
        let rust_tris = rust_scene.total_triangle_count();
        let cpp_tris = cpp_scene.total_triangle_count();
        assert_eq!(rust_tris, cpp_tris, "Triangle count should match");
    }
}
