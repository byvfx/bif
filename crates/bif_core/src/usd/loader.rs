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
use std::time::Instant;

use bif_math::Mat4;
use thiserror::Error;

use crate::mesh::Mesh;
use crate::point_cloud::{DistributionMethod, PointAttributes, PointCloud};
use crate::scene::{AnimatedTransform, Light, Scene, TimelineInfo, Transform, TransformKeyframe};
use crate::usd::cpp_bridge::{UsdBridgeError, UsdLightType, UsdStage};
use crate::usd::parser::{parse_usda, ParseError};
use crate::usd::types::{UsdMesh, UsdPointInstancer, UsdPrim, UsdReference, UsdXform};

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

/// Format a number with comma separators for readability.
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

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
    let (scene, _stage) = load_usd_with_stage(path)?;
    Ok(scene)
}

/// Load a USD file and return both the BIF Scene and the UsdStage.
///
/// Use this when you need access to the USD stage hierarchy for
/// scene browsing or inspection.
///
/// # Example
///
/// ```ignore
/// use bif_core::usd::load_usd_with_stage;
///
/// let (scene, stage) = load_usd_with_stage("scene.usdc")?;
/// println!("Loaded {} prims", stage.prim_count());
/// ```
pub fn load_usd_with_stage<P: AsRef<Path>>(path: P) -> LoadResult<(Scene, UsdStage)> {
    let path = path.as_ref();
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed");

    let load_start = Instant::now();

    // Open stage via C++ bridge
    let stage_start = Instant::now();
    let stage = UsdStage::open(path)?;
    let stage_time = stage_start.elapsed();

    let mut scene = Scene::new(name);
    let mut prototype_map: HashMap<String, usize> = HashMap::new();

    // Extract timeline metadata
    if let Ok(timeline_data) = stage.get_timeline() {
        if timeline_data.has_authored_time_range {
            scene.timeline = Some(TimelineInfo {
                start_frame: timeline_data.start_time_code,
                end_frame: timeline_data.end_time_code,
                fps: timeline_data.frames_per_second,
            });
            log::info!(
                "Timeline: frames {:.0}-{:.0} @ {:.0} fps",
                timeline_data.start_time_code,
                timeline_data.end_time_code,
                timeline_data.frames_per_second
            );
        }
    }

    // Mesh deduplication: (vertex_count, index_count, first_vertex_hash) -> proto_id
    // This handles referenced meshes that appear multiple times with different transforms
    let mut mesh_dedup: HashMap<(usize, usize, u64), usize> = HashMap::new();

    // Load all meshes as prototypes (with deduplication)
    let mesh_start = Instant::now();
    let meshes = stage.meshes()?;
    for mesh_data in &meshes {
        let vertices = mesh_data.vertices.clone();
        let indices = mesh_data.indices.clone();
        let normals = mesh_data.normals.clone();
        let uvs = mesh_data.uvs.clone();

        // Create a hash key based on mesh geometry
        // Use vertex count, index count, and hash of first few vertices
        let vertex_hash = if vertices.len() >= 3 {
            // Hash first 3 vertices (as Vec3)
            let bits: u64 = vertices
                .iter()
                .take(3)
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
            let face_material_ids = mesh_data.face_material_ids.clone();
            let mut mesh =
                Mesh::new_with_materials(vertices, indices, normals, uvs, face_material_ids);
            mesh.ensure_normals();

            let mesh_arc = Arc::new(mesh);
            let proto_id = scene.add_prototype(mesh_arc, mesh_data.path.clone());
            mesh_dedup.insert(dedup_key, proto_id);
            prototype_map.insert(mesh_data.path.clone(), proto_id);
            proto_id
        };

        // Add an instance with this mesh's world transform
        let transform = Transform::from_matrix(mesh_data.transform);

        // Check for animation data
        let mesh_idx = meshes
            .iter()
            .position(|m| m.path == mesh_data.path)
            .unwrap_or(0);
        let animation = if scene.timeline.is_some() {
            match stage.get_mesh_animation(mesh_idx) {
                Ok(anim_data) => {
                    log::debug!(
                        "Mesh {} (idx {}): {} xform samples",
                        mesh_data.path,
                        mesh_idx,
                        anim_data.xform_samples.len()
                    );
                    if anim_data.xform_samples.is_empty() {
                        None
                    } else {
                        let keyframes: Vec<TransformKeyframe> = anim_data
                            .xform_samples
                            .iter()
                            .map(|sample| TransformKeyframe {
                                time: sample.time,
                                transform: Transform::from_matrix(sample.transform),
                            })
                            .collect();
                        log::debug!(
                            "Mesh {} has {} animation keyframes",
                            mesh_data.path,
                            keyframes.len()
                        );
                        Some(AnimatedTransform::with_keyframes(
                            transform.clone(),
                            keyframes,
                        ))
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to get animation for mesh {}: {:?}",
                        mesh_data.path,
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        if let Some(anim) = animation {
            scene.add_animated_instance_with_path(
                proto_id,
                transform,
                anim,
                mesh_data.path.clone(),
            );
        } else {
            scene.add_instance_with_path(proto_id, transform, mesh_data.path.clone());
        }
    }
    let mesh_time = mesh_start.elapsed();
    let total_verts: usize = meshes.iter().map(|m| m.vertices.len()).sum();

    // Load materials
    let material_start = Instant::now();
    let usd_materials = stage.materials().unwrap_or_default();
    let mut material_map: HashMap<String, usize> = HashMap::new();
    let mut materialx_count = 0;

    for mat_data in &usd_materials {
        if mat_data.is_materialx {
            materialx_count += 1;
            log::debug!(
                "MaterialX material: {} (diffuse={:?}, metallic={:.2}, roughness={:.2})",
                mat_data.path,
                mat_data.diffuse_color,
                mat_data.metallic,
                mat_data.roughness
            );
        }
        let material = crate::scene::Material {
            name: mat_data.path.clone(),
            diffuse_color: mat_data.diffuse_color,
            metallic: mat_data.metallic,
            roughness: mat_data.roughness,
            emissive_color: mat_data.emissive_color,
            opacity: mat_data.opacity,
            specular: mat_data.specular,
            diffuse_texture: mat_data.diffuse_texture.clone(),
            roughness_texture: mat_data.roughness_texture.clone(),
            metallic_texture: mat_data.metallic_texture.clone(),
            normal_texture: mat_data.normal_texture.clone(),
            emissive_texture: mat_data.emissive_texture.clone(),
            opacity_texture: None, // TODO: extract from USD when available
        };
        let mat_id = scene.add_material(material);
        material_map.insert(mat_data.path.clone(), mat_id);
    }

    if materialx_count > 0 {
        log::info!("Loaded {} MaterialX materials", materialx_count);
    }

    // Bind materials to prototypes via mesh material paths
    for (mesh_idx, mesh_data) in meshes.iter().enumerate() {
        if let Ok(Some(mat_path)) = stage.get_mesh_material_path(mesh_idx) {
            if let Some(&mat_id) = material_map.get(&mat_path) {
                // Find the prototype for this mesh and bind the material
                if let Some(&proto_id) = prototype_map.get(&mesh_data.path) {
                    if let Some(proto) = scene.prototypes.get(proto_id) {
                        let mut updated_proto: crate::scene::Prototype = (**proto).clone();
                        let mat = &scene.materials[mat_id];
                        log::debug!(
                            "Binding {} to prototype {} (diffuse={:?})",
                            mat.name,
                            proto_id,
                            mat.diffuse_color
                        );
                        updated_proto.material = Some(mat.clone());
                        scene.prototypes[proto_id] = Arc::new(updated_proto);
                    }
                }
            }
        }
    }

    let material_time = material_start.elapsed();

    // Load lights (UsdLux)
    let light_start = Instant::now();
    let usd_lights = stage.lights().unwrap_or_default();
    for light_data in &usd_lights {
        // Extract direction/position from transform
        // For directional lights, -Z axis is the light direction
        // For point lights, the translation is the position
        let transform = light_data.transform;

        let light = match light_data.light_type {
            UsdLightType::Distant => {
                // Direction is -Z axis of the transform (forward direction)
                let direction = -bif_math::Vec3::new(
                    transform.col(2).x,
                    transform.col(2).y,
                    transform.col(2).z,
                )
                .normalize();
                Light::Distant {
                    direction,
                    color: light_data.color,
                    intensity: light_data.intensity,
                    angle: light_data.angle,
                }
            }
            UsdLightType::Sphere => {
                // Position is the translation component
                let position =
                    bif_math::Vec3::new(transform.col(3).x, transform.col(3).y, transform.col(3).z);
                Light::Point {
                    position,
                    color: light_data.color,
                    intensity: light_data.intensity,
                    radius: light_data.radius,
                }
            }
            UsdLightType::Rect => Light::Rect {
                transform,
                color: light_data.color,
                intensity: light_data.intensity,
                width: light_data.width,
                height: light_data.height,
            },
            UsdLightType::Dome => {
                // Extract Y rotation from transform (if any)
                // For now, just use 0 rotation - could decompose transform later
                Light::Dome {
                    rotation: 0.0,
                    intensity: light_data.intensity,
                    texture_path: light_data.texture_path.clone(),
                }
            }
        };

        scene.lights.push(light);
        log::debug!(
            "Loaded light: {} ({:?}, intensity={})",
            light_data.path,
            light_data.light_type,
            light_data.intensity
        );
    }
    let light_time = light_start.elapsed();

    if !usd_lights.is_empty() {
        log::info!("Loaded {} lights", usd_lights.len());
    }

    log::info!(
        "Loaded {} unique prototypes from {} meshes, {} materials",
        scene.prototype_count(),
        meshes.len(),
        scene.material_count()
    );

    // Load point instancers as PointClouds
    let instancer_start = Instant::now();
    let instancers = stage.instancers()?;
    for (instancer_idx, instancer_data) in instancers.iter().enumerate() {
        // Resolve prototypes
        let proto_ids: Vec<usize> = instancer_data
            .prototype_paths
            .iter()
            .filter_map(|path| prototype_map.get(path).copied())
            .collect();

        if proto_ids.is_empty() {
            log::warn!(
                "Instancer {} has no resolvable prototypes",
                instancer_data.path
            );
            continue;
        }

        // Extract positions from instancer transforms
        let positions: Vec<bif_math::Vec3> = instancer_data
            .transforms
            .iter()
            .map(|mat| bif_math::Vec3::new(mat.col(3).x, mat.col(3).y, mat.col(3).z))
            .collect();

        // Extract per-point scale and orientation from transforms
        let mut scales = Vec::with_capacity(positions.len());
        let mut orientations = Vec::with_capacity(positions.len());
        for mat in &instancer_data.transforms {
            let (s, r, _) = mat.to_scale_rotation_translation();
            scales.push(s);
            orientations.push(r);
        }

        let proto_indices: Vec<u32> = instancer_data
            .proto_indices
            .iter()
            .take(positions.len())
            .map(|&idx| idx as u32)
            .collect();

        let cloud = PointCloud {
            id: scene.point_clouds.len(),
            name: instancer_data.path.clone(),
            positions,
            attributes: PointAttributes {
                scales: Some(scales),
                orientations: Some(orientations),
                proto_indices,
                ids: None,
            },
            prototype_ids: proto_ids,
            transform: Transform::default(),
            distribution: DistributionMethod::UsdPointInstancer {
                path: instancer_data.path.clone(),
            },
        };

        log::info!(
            "PointInstancer '{}': {} points, {} protos -> PointCloud",
            instancer_data.path,
            cloud.point_count(),
            cloud.prototype_ids.len()
        );

        // Expand immediately so instances appear (animation handled below)
        let expanded = cloud.expand();

        // Get animation data for instancer if timeline exists
        let instancer_anim = if scene.timeline.is_some() {
            stage.get_instancer_animation(instancer_idx).ok()
        } else {
            None
        };

        for (i, inst) in expanded.into_iter().enumerate() {
            // Build animation for this instance if available
            let animation = instancer_anim.as_ref().and_then(|anim| {
                if anim.time_samples.is_empty() || i >= anim.instance_count {
                    return None;
                }

                let keyframes: Vec<TransformKeyframe> = anim
                    .time_samples
                    .iter()
                    .enumerate()
                    .filter_map(|(time_idx, &time)| {
                        anim.transforms.get(time_idx).and_then(|instances| {
                            instances.get(i).map(|mat| TransformKeyframe {
                                time,
                                transform: Transform::from_matrix(*mat),
                            })
                        })
                    })
                    .collect();

                if keyframes.is_empty() {
                    None
                } else {
                    Some(AnimatedTransform::with_keyframes(
                        inst.transform.clone(),
                        keyframes,
                    ))
                }
            });

            let prim_path = format!("{}/i{}", instancer_data.path, i);
            if let Some(anim) = animation {
                scene.add_animated_instance_with_path(
                    inst.prototype_id,
                    inst.transform,
                    anim,
                    prim_path,
                );
            } else {
                scene.add_instance_with_path(inst.prototype_id, inst.transform, prim_path);
            }
        }

        scene.add_point_cloud(cloud);
    }
    let instancer_time = instancer_start.elapsed();

    if scene.prototypes.is_empty() {
        return Err(LoadError::NoGeometry);
    }

    // Log timing breakdown
    let total_time = load_start.elapsed();
    let instance_count: usize = instancers.iter().map(|i| i.transforms.len()).sum();
    log::info!(
        "USD Load: {} ({} meshes, {} verts, {} materials, {} instances)",
        path.display(),
        meshes.len(),
        format_number(total_verts),
        scene.material_count(),
        format_number(instance_count)
    );
    log::info!("  Stage open: {:>7.1}ms", stage_time.as_secs_f64() * 1000.0);
    log::info!("  Meshes:     {:>7.1}ms", mesh_time.as_secs_f64() * 1000.0);
    log::info!(
        "  Materials:  {:>7.1}ms",
        material_time.as_secs_f64() * 1000.0
    );
    log::info!(
        "  Lights:     {:>7.1}ms ({} lights)",
        light_time.as_secs_f64() * 1000.0,
        scene.lights.len()
    );
    log::info!(
        "  Instancers: {:>7.1}ms",
        instancer_time.as_secs_f64() * 1000.0
    );
    log::info!("  Total:      {:>7.1}ms", total_time.as_secs_f64() * 1000.0);

    Ok((scene, stage))
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
pub fn load_usda_from_string(
    content: &str,
    name: &str,
    base_dir: Option<PathBuf>,
) -> LoadResult<Scene> {
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
            UsdPrim::PointInstancer(instancer) => {
                self.process_point_instancer(instancer, parent_transform)
            }
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
        self.scene
            .add_instance(proto_id, Transform::from_matrix(world_transform));

        Ok(())
    }

    /// Process a PointInstancer prim.
    fn process_point_instancer(
        &mut self,
        instancer: &UsdPointInstancer,
        parent_transform: Mat4,
    ) -> LoadResult<()> {
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
            let proto_id = inline_prototypes
                .get(proto_idx)
                .copied()
                .or_else(|| inline_prototypes.first().copied())
                .unwrap_or(0);

            // Build instance transform
            let instance_matrix = instancer.instance_matrix(i);
            let final_matrix = world_transform * instance_matrix;

            self.scene
                .add_instance(proto_id, Transform::from_matrix(final_matrix));
        }

        Ok(())
    }

    /// Process a Reference prim by loading the referenced file.
    fn process_reference(
        &mut self,
        reference: &UsdReference,
        parent_transform: Mat4,
    ) -> LoadResult<()> {
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
            let content = std::fs::read_to_string(&asset_path).map_err(|e| {
                LoadError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to load reference '{}': {}", reference.asset_path, e),
                ))
            })?;

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
        let matrix = scene.instances()[0].model_matrix();
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
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
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
        assert!(
            scene.prototype_count() >= 1,
            "Should have at least 1 prototype"
        );
        assert!(
            scene.instance_count() >= 2,
            "Should have at least 2 instances (2 referenced cubes)"
        );
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
