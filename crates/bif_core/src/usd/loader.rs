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

use indexmap::IndexMap;

use thiserror::Error;

use crate::mesh::Mesh;
use crate::point_cloud::{DistributionMethod, PointAttributes, PointCloud};
use crate::scene::{
    AnimatedTransform, CurvesPrim, Light, PointsPrim, Purpose, Scene, TimelineInfo, Transform,
    TransformKeyframe,
};
use crate::usd::cpp_bridge::{UsdBridgeError, UsdLightType, UsdStage};

/// Errors that can occur during USD loading.
#[derive(Error, Debug)]
pub enum LoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

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

    // Open stage via C++ bridge (LoadNone — hierarchy only, no geometry yet)
    let stage_start = Instant::now();
    let stage = UsdStage::open(path)?;

    // Load all payloads and cache mesh/material/animation data
    let prim_count = stage.load_payloads()?;
    log::info!("Stage loaded: {} prims", prim_count);
    let stage_time = stage_start.elapsed();

    let mut scene = Scene::new(name);
    let mut prototype_map: IndexMap<String, usize> = IndexMap::new();

    // Extract stage metadata (metersPerUnit, upAxis)
    if let Ok(meta) = stage.get_stage_metadata() {
        log::info!("Stage metadata: {}", meta);
        scene.stage_metadata = Some(meta);
    }

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

    // Load all meshes as prototypes (with deduplication).
    // All purposes are loaded; filtering happens at viewport culling time.
    let mesh_start = Instant::now();
    let meshes = stage.meshes()?;
    for (mesh_idx, mesh_data) in meshes.iter().enumerate() {
        // Skip invisible meshes (inherited visibility = invisible)
        if !mesh_data.visible {
            continue;
        }
        // Hash from references — no clone until we know it's unique
        let vertex_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            mesh_data.vertices.len().hash(&mut hasher);
            mesh_data.indices.len().hash(&mut hasher);
            let vlen = mesh_data.vertices.len();
            let sample_count = vlen.min(10);
            for i in 0..sample_count {
                let idx = if sample_count <= 1 {
                    0
                } else {
                    i * (vlen - 1) / (sample_count - 1)
                };
                if let Some(v) = mesh_data.vertices.get(idx) {
                    v.x.to_bits().hash(&mut hasher);
                    v.y.to_bits().hash(&mut hasher);
                    v.z.to_bits().hash(&mut hasher);
                }
            }
            let ilen = mesh_data.indices.len();
            let idx_sample_count = ilen.min(10);
            for i in 0..idx_sample_count {
                let idx = if idx_sample_count <= 1 {
                    0
                } else {
                    i * (ilen - 1) / (idx_sample_count - 1)
                };
                if let Some(&val) = mesh_data.indices.get(idx) {
                    val.hash(&mut hasher);
                }
            }
            hasher.finish()
        };
        let dedup_key = (
            mesh_data.vertices.len(),
            mesh_data.indices.len(),
            vertex_hash,
        );

        let proto_id = if let Some(&existing_id) = mesh_dedup.get(&dedup_key) {
            // Mesh already exists, reuse prototype
            existing_id
        } else {
            // New unique mesh — clone data only for unique meshes
            let mut mesh = Mesh::new_with_materials(
                mesh_data.vertices.clone(),
                mesh_data.indices.clone(),
                mesh_data.normals.clone(),
                mesh_data.uvs.clone(),
                mesh_data.face_material_ids.clone(),
            );
            mesh.ensure_normals();

            // Store subdivision data for Embree subd geometry
            mesh.subdivision_scheme = mesh_data.subdivision_scheme;
            mesh.face_vertex_counts = mesh_data.face_vertex_counts.clone();
            mesh.polygon_indices = mesh_data.face_vertex_indices.clone();
            mesh.crease_indices = mesh_data.crease_indices.clone();
            mesh.crease_lengths = mesh_data.crease_lengths.clone();
            mesh.crease_sharpnesses = mesh_data.crease_sharpnesses.clone();

            let mesh_arc = Arc::new(mesh);
            let proto_id = scene.add_prototype(mesh_arc, mesh_data.path.clone());
            mesh_dedup.insert(dedup_key, proto_id);
            prototype_map.insert(mesh_data.path.clone(), proto_id);
            proto_id
        };

        // Add an instance with this mesh's world transform
        let transform = Transform::from_matrix(mesh_data.transform);

        // Check for animation data
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

        scene.set_last_instance_purpose(mesh_data.purpose.into());
    }
    let mesh_time = mesh_start.elapsed();
    let total_verts: usize = meshes.iter().map(|m| m.vertices.len()).sum();

    // Fetch native instances early (processed after material binding below)
    let native_instances = stage.native_instances().unwrap_or_default();

    // Load materials
    let material_start = Instant::now();
    let usd_materials = stage.materials().unwrap_or_default();
    let mut material_map: IndexMap<String, usize> = IndexMap::new();
    let mut materialx_count = 0;

    for mat_data in &usd_materials {
        if mat_data.is_materialx {
            materialx_count += 1;
            log::debug!(
                "MaterialX material: {} (base_color={:?}, metalness={:.2}, roughness={:.2})",
                mat_data.path,
                mat_data.base_color,
                mat_data.base_metalness,
                mat_data.specular_roughness
            );
        }
        let material = crate::scene::Material {
            name: mat_data.path.clone().into(),
            base_color: mat_data.base_color,
            base_metalness: mat_data.base_metalness,
            specular_roughness: mat_data.specular_roughness,
            emission_color: mat_data.emission_color,
            emission_luminance: 0.0,
            transmission_weight: mat_data.transmission_weight,
            geometry_opacity: mat_data.geometry_opacity,
            specular_weight: mat_data.specular_weight,
            specular_ior: mat_data.specular_ior,
            base_color_texture: mat_data.base_color_texture.as_deref().map(Arc::from),
            specular_roughness_texture: mat_data
                .specular_roughness_texture
                .as_deref()
                .map(Arc::from),
            base_metalness_texture: mat_data.base_metalness_texture.as_deref().map(Arc::from),
            normal_texture: mat_data.normal_texture.as_deref().map(Arc::from),
            emission_texture: mat_data.emission_texture.as_deref().map(Arc::from),
            geometry_opacity_texture: mat_data.geometry_opacity_texture.as_deref().map(Arc::from),
            source_dir: path.parent().map(|p| p.to_path_buf()),
            double_sided: false,
        };
        let mat_id = scene.add_material(material);
        material_map.insert(mat_data.path.clone(), mat_id);
    }

    if materialx_count > 0 {
        log::info!("Loaded {} MaterialX materials", materialx_count);
    }

    // Bind materials to prototypes via mesh material paths
    let mut bound_count = 0usize;
    for (mesh_idx, mesh_data) in meshes.iter().enumerate() {
        if let Ok(Some(mat_path)) = stage.get_mesh_material_path(mesh_idx) {
            if let Some(&mat_id) = material_map.get(&mat_path) {
                if let Some(&proto_id) = prototype_map.get(&mesh_data.path) {
                    if let Some(proto) = scene.prototypes.get(proto_id) {
                        let mut updated_proto: crate::scene::Prototype = (**proto).clone();
                        let mat = &scene.materials[mat_id];
                        updated_proto.material = Some(mat.clone());
                        scene.prototypes[proto_id] = Arc::new(updated_proto);
                        bound_count += 1;
                    }
                }
            }
        }
    }
    log::info!(
        "Bound {} / {} meshes to materials",
        bound_count,
        meshes.len()
    );

    // Build bridge-material-index → scene-material-index lookup
    let bridge_mat_to_scene: Vec<Option<usize>> = usd_materials
        .iter()
        .map(|m| material_map.get(&m.path).copied())
        .collect();

    // Process native instances now that materials are bound
    let native_start = Instant::now();
    let mut override_proto_cache: HashMap<(usize, usize), usize> = HashMap::new();

    for native_inst in &native_instances {
        if let Some(mesh_data) = meshes.get(native_inst.proto_mesh_idx) {
            if let Some(&proto_id) = prototype_map.get(&mesh_data.path) {
                let transform = Transform::from_matrix(native_inst.transform);
                let prim_path = format!("{}/native_{}", mesh_data.path, scene.instance_count());

                let target_proto = if native_inst.material_override_idx >= 0 {
                    let bridge_idx = native_inst.material_override_idx as usize;
                    if let Some(Some(scene_mat_idx)) = bridge_mat_to_scene.get(bridge_idx) {
                        let override_mat = &scene.materials[*scene_mat_idx];
                        let needs_override = match &scene.prototypes[proto_id].material {
                            Some(proto_mat) => !Arc::ptr_eq(proto_mat, override_mat),
                            None => true,
                        };
                        if needs_override {
                            let key = (proto_id, bridge_idx);
                            if let Some(&cached) = override_proto_cache.get(&key) {
                                cached
                            } else {
                                let mut cloned = (*scene.prototypes[proto_id]).clone();
                                cloned.material = Some(override_mat.clone());
                                cloned.name = format!(
                                    "{}/mat_{}",
                                    cloned.name, scene.materials[*scene_mat_idx].name
                                )
                                .into();
                                let new_id = scene.prototypes.len();
                                cloned.id = new_id;
                                scene.prototypes.push(Arc::new(cloned));
                                override_proto_cache.insert(key, new_id);
                                new_id
                            }
                        } else {
                            proto_id
                        }
                    } else {
                        proto_id
                    }
                } else {
                    proto_id
                };

                scene.add_instance_with_path(target_proto, transform, prim_path);
                scene.set_last_instance_purpose(native_inst.purpose.into());
            }
        }
    }
    let native_time = native_start.elapsed();
    if !native_instances.is_empty() {
        log::info!(
            "Native instances: {} ({} overrides, {:.1}ms)",
            native_instances.len(),
            override_proto_cache.len(),
            native_time.as_secs_f64() * 1000.0
        );
    }

    let material_time = material_start.elapsed();

    // Free bulk mesh geometry from C++ cache — Rust now owns all mesh data.
    // Keeps vertices/indices/paths for animation queries via USD stage.
    stage.free_mesh_geometry_cache();

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
                // Extract Y-axis rotation from transform's Z basis vector
                let z_basis = bif_math::Vec3::new(transform.col(2).x, 0.0, transform.col(2).z);
                let rotation = z_basis.x.atan2(z_basis.z); // radians
                Light::Dome {
                    rotation,
                    intensity: light_data.intensity,
                    texture_path: light_data.texture_path.as_deref().map(Arc::from),
                }
            }
            UsdLightType::Cylinder => {
                // Map cylinder to point light (approximate — BIF doesn't have cylinder light yet)
                let position =
                    bif_math::Vec3::new(transform.col(3).x, transform.col(3).y, transform.col(3).z);
                Light::Point {
                    position,
                    color: light_data.color,
                    intensity: light_data.intensity,
                    radius: light_data.radius,
                }
            }
            UsdLightType::Disk => {
                // Map disk to rect light (approximate — BIF doesn't have disk light yet)
                let diameter = light_data.radius * 2.0;
                Light::Rect {
                    transform,
                    color: light_data.color,
                    intensity: light_data.intensity,
                    width: diameter,
                    height: diameter,
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

    // Load curves (UsdGeomBasisCurves)
    let curves_data = stage.curves().unwrap_or_default();
    for curve_data in &curves_data {
        scene.curves.push(CurvesPrim {
            path: curve_data.path.clone(),
            points: curve_data.points.clone(),
            widths: curve_data.widths.clone(),
            curve_vertex_counts: curve_data.curve_vertex_counts.clone(),
            curve_type: curve_data.curve_type,
            basis: curve_data.basis,
            wrap: curve_data.wrap,
            transform: curve_data.transform,
            // TODO: read purpose from C++ bridge (needs curves purpose in FFI)
            purpose: Purpose::Default,
        });
    }
    if !curves_data.is_empty() {
        log::info!("Loaded {} curves prims", curves_data.len());
    }

    // Load points (UsdGeomPoints)
    let points_data = stage.points().unwrap_or_default();
    for pt_data in &points_data {
        scene.points_prims.push(PointsPrim {
            path: pt_data.path.clone(),
            positions: pt_data.positions.clone(),
            widths: pt_data.widths.clone(),
            normals: pt_data.normals.clone(),
            ids: pt_data.ids.clone(),
            transform: pt_data.transform,
            // TODO: read purpose from C++ bridge (needs points purpose in FFI)
            purpose: Purpose::Default,
        });
    }
    if !points_data.is_empty() {
        log::info!("Loaded {} points prims", points_data.len());
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
            invisible_ids: instancer_data.invisible_ids.clone(),
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

        // Build set of invisible instance IDs for fast lookup
        let invisible_set: std::collections::HashSet<i64> =
            instancer_data.invisible_ids.iter().copied().collect();

        for (i, inst) in expanded.into_iter().enumerate() {
            // Skip invisible instances (from UsdGeomPointInstancer::GetInvisibleIdsAttr)
            if invisible_set.contains(&(i as i64)) {
                continue;
            }

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

/// Load a USDA file via the C++ bridge.
///
/// This is an alias for `load_usd()` — kept for backwards compatibility.
pub fn load_usda<P: AsRef<Path>>(path: P) -> LoadResult<Scene> {
    load_usd(path)
}

/// Load USDA from a string (writes to temp file, loads via C++ bridge).
///
/// Useful for testing. Adds `#usda 1.0` header if missing.
pub fn load_usda_from_string(
    content: &str,
    name: &str,
    _base_dir: Option<PathBuf>,
) -> LoadResult<Scene> {
    let content = if content.trim_start().starts_with("#usda") {
        content.to_string()
    } else {
        format!("#usda 1.0\n(\n)\n\n{}", content)
    };
    let temp_path = std::env::temp_dir().join(format!("bif_usda_{}.usda", name));
    std::fs::write(&temp_path, &content)?;
    let result = load_usd(&temp_path);
    let _ = std::fs::remove_file(&temp_path);
    result
}

// SceneBuilder removed — all loading goes through C++ bridge.

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

        let scene = match load_usda_from_string(usda, "test_simple", None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        assert_eq!(scene.prototype_count(), 1);
        assert_eq!(scene.instance_count(), 1);
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

        let scene = match load_usda_from_string(usda, "test_normals", None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        assert!(scene.prototypes[0].mesh.has_normals());
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

        let scene = match load_usda_from_string(usda, "test_instancer", None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        assert!(scene.prototype_count() >= 1);
        assert!(scene.instance_count() >= 1);
    }

    #[test]
    fn test_load_transformed_mesh() {
        let usda = r#"
def Xform "World" {
    double3 xformOp:translate = (10, 0, 0)
    uniform token[] xformOpOrder = ["xformOp:translate"]

    def Mesh "Cube" {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
        int[] faceVertexCounts = [4]
        int[] faceVertexIndices = [0, 1, 2, 3]
    }
}
"#;

        let scene = match load_usda_from_string(usda, "test_xform", None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        assert_eq!(scene.prototype_count(), 1);
        assert_eq!(scene.instance_count(), 1);
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

    // test_usda_and_cpp_bridge_produce_same_mesh removed — Rust parser eliminated,
    // both paths now use C++ bridge.
}
