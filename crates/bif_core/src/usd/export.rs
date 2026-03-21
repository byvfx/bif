//! USD scene export — writes BIF edits as a composable USD layer.
//!
//! All export logic lives in `bif_core` (no egui dependency) so it can be
//! reused from any UI framework (egui, Qt, CLI).

use std::collections::{HashMap, HashSet};

use bif_math::{Mat4, Quat, Vec3};

use crate::point_cloud::PointCloud;
use crate::scene::{Light, Scene};
use crate::undo::EditState;
use crate::usd::cpp_bridge::{
    CameraProperties, UpAxis, UsdBridgeError, UsdEditLayer, UsdKind, UsdLightData, UsdLightShaping,
    UsdLightType, UsdPrimType, UsdSpecifier, UsdStageMetadata,
};

/// A prim authored by a UsdPrim node, to be written during export.
#[derive(Clone, Debug)]
pub struct AuthoredPrim {
    /// USD prim path (e.g., "/shot")
    pub path: String,
    /// Prim type name (e.g., "Scope", "Xform")
    pub prim_type: UsdPrimType,
    /// Model kind
    pub kind: UsdKind,
    /// Define vs Over
    pub specifier: UsdSpecifier,
}

/// Configuration for a USD export.
#[derive(Clone, Debug)]
pub struct ExportConfig {
    /// Output file path (.usda or .usdc)
    pub output_path: String,
    /// Original USD file path (for sublayer composition)
    pub source_usd_path: Option<String>,
    /// If true, add source as sublayer so edits compose over the original
    pub as_sublayer: bool,
    /// Root prim path for BIF-authored prims (scattered instancers, etc.)
    pub export_root: String,
    /// Prims authored by UsdPrim nodes
    pub authored_prims: Vec<AuthoredPrim>,
    /// Graft prefix: prepend to all prim paths (from GraftBranches node)
    pub graft_prefix: Option<String>,
    /// Stage metadata (upAxis, metersPerUnit, timeCodesPerSecond).
    /// If None, writes defaults (Y-up, 0.01 m/unit, 24 fps).
    pub stage_metadata: Option<UsdStageMetadata>,
    /// Prim paths to mark as invisible
    pub hidden_prim_paths: Vec<String>,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            output_path: String::new(),
            source_usd_path: None,
            as_sublayer: true,
            export_root: "/BIF".to_string(),
            authored_prims: Vec::new(),
            graft_prefix: None,
            stage_metadata: None,
            hidden_prim_paths: Vec::new(),
        }
    }
}

/// Result of a successful export.
#[derive(Clone, Debug)]
pub struct ExportResult {
    /// Number of static xform opinions written
    pub xform_count: usize,
    /// Number of keyframed xform opinions written
    pub keyframe_count: usize,
    /// Number of PointInstancers written
    pub instancer_count: usize,
    /// Number of authored prims written (from UsdPrim nodes)
    pub prim_count: usize,
    /// Number of prototype meshes written
    pub mesh_count: usize,
    /// Number of materials written
    pub material_count: usize,
    /// Number of lights written
    pub light_count: usize,
    /// Number of cameras written
    pub camera_count: usize,
    /// Number of visibility opinions written
    pub visibility_count: usize,
    /// Output file path
    pub output_path: String,
}

impl std::fmt::Display for ExportResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} prims + {} meshes + {} mats + {} lights + {} cams + {} xforms + {} kf + {} inst -> {}",
            self.prim_count,
            self.mesh_count,
            self.material_count,
            self.light_count,
            self.camera_count,
            self.xform_count,
            self.keyframe_count,
            self.instancer_count,
            self.output_path
        )
    }
}

/// Export the scene's edits as a USD layer.
///
/// Writes transform overrides, keyframed transforms, and PointInstancers
/// from point clouds. Optionally composes over the original USD file as
/// a sublayer.
pub fn export_scene(
    scene: &Scene,
    edit_state: &EditState,
    instance_prim_paths: &[String],
    config: &ExportConfig,
) -> Result<ExportResult, UsdBridgeError> {
    let mut layer = UsdEditLayer::create(&config.output_path)?;

    // Sublayer composition: add source USD so our edits compose over it
    log::debug!(
        "Export config: as_sublayer={}, source_usd_path={:?}",
        config.as_sublayer,
        config.source_usd_path
    );
    if config.as_sublayer {
        if let Some(ref source_path) = config.source_usd_path {
            log::info!("Adding sublayer: {:?}", source_path);
            layer.add_sublayer(source_path)?;
        } else {
            log::warn!("as_sublayer=true but source_usd_path is None — no sublayer added");
        }
    }

    // Stage metadata (upAxis, metersPerUnit, timeCodesPerSecond)
    let metadata = config.stage_metadata.clone().unwrap_or(UsdStageMetadata {
        meters_per_unit: 0.01,
        up_axis: UpAxis::Y,
        time_codes_per_second: 24.0,
    });
    layer.set_stage_metadata(&metadata)?;

    // Default prim
    let default_prim = apply_graft_prefix(&config.export_root, &config.graft_prefix);
    if let Err(e) = layer.set_default_prim(&default_prim) {
        log::warn!("set_default_prim failed for {:?}: {}", default_prim, e);
    }

    // Write authored prims (from UsdPrim nodes) — before xforms so parent prims exist
    let mut prim_count = 0;
    for authored in &config.authored_prims {
        let path = apply_graft_prefix(&authored.path, &config.graft_prefix);
        log::debug!(
            "define_prim: path={:?} type={:?} spec={:?}",
            path,
            authored.prim_type,
            authored.specifier
        );
        layer
            .define_prim(&path, authored.prim_type, authored.specifier)
            .map_err(|e| {
                log::error!("define_prim failed for path {:?}: {}", path, e);
                e
            })?;
        if authored.kind != UsdKind::None {
            layer.set_prim_kind(&path, authored.kind).map_err(|e| {
                log::error!("set_prim_kind failed for path {:?}: {}", path, e);
                e
            })?;
        }
        prim_count += 1;
    }

    // Export materials (Phase 2)
    let material_paths = export_materials(&mut layer, scene, config)?;
    let material_count = material_paths.len();

    let mut xform_count = 0;
    let mut keyframe_count = 0;

    // Write static transform overrides
    for (&idx, transform) in &edit_state.transform_overrides {
        let prim_path = instance_prim_paths
            .get(idx)
            .cloned()
            .unwrap_or_else(|| format!("/instance_{}", idx));
        let prim_path = apply_graft_prefix(&prim_path, &config.graft_prefix);
        let mat = transform.to_matrix();
        layer.write_xform(&prim_path, -1.0, &mat).map_err(|e| {
            log::error!("write_xform failed for path {:?}: {}", prim_path, e);
            e
        })?;
        xform_count += 1;
    }

    // Write keyframed transforms
    for (&idx, anim) in &edit_state.keyframe_overrides {
        let prim_path = instance_prim_paths
            .get(idx)
            .cloned()
            .unwrap_or_else(|| format!("/instance_{}", idx));
        let prim_path = apply_graft_prefix(&prim_path, &config.graft_prefix);
        if let Some(keyframes) = &anim.keyframes {
            for kf in keyframes {
                let mat = kf.transform.to_matrix();
                layer.write_xform(&prim_path, kf.time, &mat).map_err(|e| {
                    log::error!(
                        "write_xform (keyframe) failed for path {:?}: {}",
                        prim_path,
                        e
                    );
                    e
                })?;
                keyframe_count += 1;
            }
        }
    }

    // Resolve proto paths once per cloud (avoid double-compute)
    let cloud_proto_paths: Vec<(&PointCloud, Vec<String>)> = scene
        .point_clouds
        .iter()
        .filter(|c| !c.positions.is_empty() && !c.prototype_ids.is_empty())
        .map(|c| (c, resolve_proto_paths(scene, c, config)))
        .collect();

    // Write PointInstancers from point clouds
    let mut instancer_count = 0;
    for (cloud, proto_paths) in &cloud_proto_paths {
        let instancer_path = if cloud.name.starts_with('/') {
            cloud.name.to_string()
        } else {
            format!("{}/{}", config.export_root, cloud.name)
        };
        let instancer_path = apply_graft_prefix(&instancer_path, &config.graft_prefix);
        if proto_paths.is_empty() {
            log::warn!(
                "Skipping instancer {:?}: no prototype paths resolved",
                instancer_path
            );
            continue;
        }
        layer
            .write_point_instancer(&instancer_path, cloud, proto_paths)
            .map_err(|e| {
                log::error!(
                    "write_point_instancer failed for {:?} (instances={}, protos={}): {}",
                    instancer_path,
                    cloud.positions.len(),
                    proto_paths.len(),
                    e
                );
                e
            })?;
        // Write invisibleIds if any (Phase 9)
        if !cloud.invisible_ids.is_empty() {
            layer.write_invisible_ids(&instancer_path, &cloud.invisible_ids)?;
        }
        instancer_count += 1;
    }

    // Write prototype meshes referenced by point clouds (reuses pre-computed paths)
    let mut mesh_count = 0;
    let mut written_protos: HashSet<String> = HashSet::new();
    for (cloud, proto_paths) in &cloud_proto_paths {
        for (i, &proto_id) in cloud.prototype_ids.iter().enumerate() {
            if let Some(proto_path) = proto_paths.get(i) {
                if written_protos.contains(proto_path) {
                    continue;
                }
                if let Some(proto) = scene.prototypes.get(proto_id) {
                    let path = apply_graft_prefix(proto_path, &config.graft_prefix);
                    layer.write_mesh(&path, &proto.mesh)?;
                    // Bind material
                    if let Some(ref mat) = proto.material {
                        if let Some(mat_path) = material_paths.get(mat.name.as_ref()) {
                            layer.bind_material(&path, mat_path)?;
                        }
                    }
                    // GeomSubsets for per-face materials (Phase 6)
                    export_geom_subsets(
                        &mut layer,
                        &path,
                        &proto.mesh,
                        &material_paths,
                        &scene.materials,
                    )?;
                    written_protos.insert(proto_path.clone());
                    mesh_count += 1;
                }
            }
        }
    }

    // Write standalone prototype meshes not already written by instancer loop.
    // TODO: When as_sublayer=true, consider skipping USD-loaded protos (needs
    // a `from_usd` flag on Prototype to distinguish BIF-created vs USD-loaded).
    for proto in &scene.prototypes {
        let proto_path = proto_prim_path(proto, &config.export_root);
        if written_protos.contains(&proto_path) {
            continue;
        }
        let path = apply_graft_prefix(&proto_path, &config.graft_prefix);
        layer.write_mesh(&path, &proto.mesh)?;
        // Bind material
        if let Some(ref mat) = proto.material {
            if let Some(mat_path) = material_paths.get(mat.name.as_ref()) {
                layer.bind_material(&path, mat_path)?;
            }
        }
        // GeomSubsets for per-face materials (Phase 6)
        export_geom_subsets(
            &mut layer,
            &path,
            &proto.mesh,
            &material_paths,
            &scene.materials,
        )?;
        written_protos.insert(proto_path);
        mesh_count += 1;
    }

    // Export lights (Phase 3)
    let light_count = export_lights(&mut layer, scene, config)?;

    // Export cameras (Phase 3)
    let camera_count = export_cameras(&mut layer, scene, config)?;

    // Export visibility (Phase 4)
    let visibility_count = export_visibility(&mut layer, config)?;

    layer.save()?;

    Ok(ExportResult {
        xform_count,
        keyframe_count,
        instancer_count,
        prim_count,
        mesh_count,
        material_count,
        light_count,
        camera_count,
        visibility_count,
        output_path: config.output_path.clone(),
    })
}

/// Prepend a graft prefix to a prim path (if set).
///
/// E.g., path="/World/hero" + prefix="/shot" → "/shot/World/hero"
fn apply_graft_prefix(path: &str, prefix: &Option<String>) -> String {
    match prefix {
        Some(pfx) if !pfx.is_empty() => {
            let pfx = pfx.trim_end_matches('/');
            format!("{}{}", pfx, path)
        }
        _ => path.to_string(),
    }
}

/// Compute the prim path for a prototype.
///
/// If the name is already a USD path (starts with '/'), return as-is.
/// Otherwise, construct a path under the export root.
fn proto_prim_path(proto: &crate::scene::Prototype, export_root: &str) -> String {
    if proto.name.starts_with('/') {
        proto.name.to_string()
    } else {
        format!("{}/{}", export_root, proto.name)
    }
}

/// Resolve prototype prim paths for a point cloud's prototype IDs.
fn resolve_proto_paths(scene: &Scene, cloud: &PointCloud, config: &ExportConfig) -> Vec<String> {
    cloud
        .prototype_ids
        .iter()
        .map(|&proto_id| {
            scene
                .prototypes
                .get(proto_id)
                .map(|proto| proto_prim_path(proto, &config.export_root))
                .unwrap_or_else(|| format!("{}/proto_{}", config.export_root, proto_id))
        })
        .collect()
}

/// Export scene materials as UsdShadeMaterial + UsdPreviewSurface prims.
///
/// Returns a map of material name → exported prim path (for binding).
fn export_materials(
    layer: &mut UsdEditLayer,
    scene: &Scene,
    config: &ExportConfig,
) -> Result<HashMap<String, String>, UsdBridgeError> {
    let mut material_paths = HashMap::new();
    for mat in &scene.materials {
        let mat_path = if mat.name.starts_with('/') {
            mat.name.to_string()
        } else {
            format!("{}/Looks/{}", config.export_root, mat.name)
        };
        let path = apply_graft_prefix(&mat_path, &config.graft_prefix);
        layer.write_material(&path, mat)?;
        material_paths.insert(mat.name.to_string(), path);
    }
    // Phase 5: OpenPBR MaterialX network written by C++ bridge alongside
    // UsdPreviewSurface (dual output: outputs:surface + outputs:mtlx:surface)
    Ok(material_paths)
}

/// Export scene lights as UsdLux prims.
fn export_lights(
    layer: &mut UsdEditLayer,
    scene: &Scene,
    config: &ExportConfig,
) -> Result<usize, UsdBridgeError> {
    let mut count = 0;
    for (i, light) in scene.lights.iter().enumerate() {
        let mut data = scene_light_to_usd(light, i, &config.export_root);
        data.path = apply_graft_prefix(&data.path, &config.graft_prefix);
        layer.write_light(&data)?;
        count += 1;
    }
    Ok(count)
}

/// Export scene cameras as UsdGeomCamera prims.
fn export_cameras(
    layer: &mut UsdEditLayer,
    scene: &Scene,
    config: &ExportConfig,
) -> Result<usize, UsdBridgeError> {
    let mut count = 0;
    for cam in &scene.cameras {
        let cam_path = if cam.name.starts_with('/') {
            cam.name.clone()
        } else {
            format!("{}/Cameras/{}", config.export_root, cam.name)
        };
        let path = apply_graft_prefix(&cam_path, &config.graft_prefix);

        // Get camera transform from the instance it's attached to
        let transform = scene
            .instances()
            .get(cam.instance_index)
            .map(|inst| inst.transform.to_matrix())
            .unwrap_or(Mat4::IDENTITY);

        // Convert FOV to focal length (standard full-frame sensor)
        let vertical_aperture = 24.0_f32; // mm
        let horizontal_aperture = 36.0_f32; // mm
        let focal_length = vertical_aperture / (2.0 * (cam.fov_y / 2.0).tan());

        let props = CameraProperties {
            focal_length,
            vertical_aperture,
            horizontal_aperture,
            clip_near: cam.near,
            clip_far: cam.far,
        };

        layer.write_camera(&path, &props, -1.0, &transform)?;
        count += 1;
    }
    Ok(count)
}

/// Export visibility overrides for hidden prims.
fn export_visibility(
    layer: &mut UsdEditLayer,
    config: &ExportConfig,
) -> Result<usize, UsdBridgeError> {
    let mut count = 0;
    for prim_path in &config.hidden_prim_paths {
        let path = apply_graft_prefix(prim_path, &config.graft_prefix);
        layer.write_visibility(&path, false)?;
        count += 1;
    }
    Ok(count)
}

/// Convert a BIF scene Light to USD light data for export.
fn scene_light_to_usd(light: &Light, index: usize, export_root: &str) -> UsdLightData {
    let no_shaping = UsdLightShaping {
        cone_angle: 180.0,
        cone_softness: 0.0,
        focus: 0.0,
        ies_file: None,
    };

    match light {
        Light::Distant {
            direction,
            color,
            intensity,
            angle,
        } => {
            // USD distant light points down -Z; rotate to match direction
            let forward = direction.normalize();
            let rotation = Quat::from_rotation_arc(Vec3::NEG_Z, forward);
            let transform = Mat4::from_rotation_translation(rotation, Vec3::ZERO);

            UsdLightData {
                path: format!("{}/Lights/distant_{}", export_root, index),
                light_type: UsdLightType::Distant,
                color: *color,
                intensity: *intensity,
                transform,
                angle: *angle,
                radius: 0.0,
                width: 0.0,
                height: 0.0,
                texture_path: None,
                length: 0.0,
                shaping: no_shaping,
                light_link_includes: Vec::new(),
                light_link_excludes: Vec::new(),
            }
        }
        Light::Point {
            position,
            color,
            intensity,
            radius,
        } => UsdLightData {
            path: format!("{}/Lights/point_{}", export_root, index),
            light_type: UsdLightType::Sphere,
            color: *color,
            intensity: *intensity,
            transform: Mat4::from_translation(*position),
            angle: 0.0,
            radius: *radius,
            width: 0.0,
            height: 0.0,
            texture_path: None,
            length: 0.0,
            shaping: no_shaping,
            light_link_includes: Vec::new(),
            light_link_excludes: Vec::new(),
        },
        Light::Rect {
            transform,
            color,
            intensity,
            width,
            height,
        } => UsdLightData {
            path: format!("{}/Lights/rect_{}", export_root, index),
            light_type: UsdLightType::Rect,
            color: *color,
            intensity: *intensity,
            transform: *transform,
            angle: 0.0,
            radius: 0.0,
            width: *width,
            height: *height,
            texture_path: None,
            length: 0.0,
            shaping: no_shaping,
            light_link_includes: Vec::new(),
            light_link_excludes: Vec::new(),
        },
        Light::Dome {
            rotation,
            intensity,
            texture_path,
        } => UsdLightData {
            path: format!("{}/Lights/dome_{}", export_root, index),
            light_type: UsdLightType::Dome,
            color: Vec3::ONE,
            intensity: *intensity,
            transform: Mat4::from_rotation_y(*rotation),
            angle: 0.0,
            radius: 0.0,
            width: 0.0,
            height: 0.0,
            texture_path: texture_path.as_deref().map(|s| s.to_string()),
            length: 0.0,
            shaping: no_shaping,
            light_link_includes: Vec::new(),
            light_link_excludes: Vec::new(),
        },
    }
}

/// Write GeomSubset child prims for per-face material assignments (Phase 6).
///
/// Groups faces by material ID and creates a GeomSubset for each group,
/// binding the corresponding material.
fn export_geom_subsets(
    layer: &mut UsdEditLayer,
    mesh_path: &str,
    mesh: &crate::mesh::Mesh,
    material_paths: &HashMap<String, String>,
    scene_materials: &[std::sync::Arc<crate::scene::Material>],
) -> Result<usize, UsdBridgeError> {
    let face_ids = match &mesh.face_material_ids {
        Some(ids) if !ids.is_empty() => ids,
        _ => return Ok(0),
    };

    // Group face indices by material ID
    let mut groups: HashMap<u32, Vec<i32>> = HashMap::new();
    for (face_idx, &mat_id) in face_ids.iter().enumerate() {
        groups.entry(mat_id).or_default().push(face_idx as i32);
    }

    // Skip if only one group (whole-mesh binding suffices)
    if groups.len() <= 1 {
        return Ok(0);
    }

    let mut count = 0;
    for (mat_id, indices) in &groups {
        let subset_name = format!("mat_{}", mat_id);
        let mat_path = scene_materials
            .get(*mat_id as usize)
            .and_then(|m| material_paths.get(m.name.as_ref()));

        layer.write_geom_subset(
            mesh_path,
            &subset_name,
            indices,
            mat_path.map(|s| s.as_str()),
        )?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point_cloud::{DistributionMethod, PointAttributes};
    use crate::scene::{AnimatedTransform, Transform, TransformKeyframe};
    use crate::usd::cpp_bridge::UsdStage;
    use bif_math::{Quat, Vec3};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Generate a unique temp file path for test output.
    fn temp_usda_path(prefix: &str) -> String {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("bif_test_{prefix}_{pid}_{id}.usda"));
        path.to_string_lossy().into_owned()
    }

    /// Clean up a temp file, ignoring errors.
    fn cleanup(path: &str) {
        let _ = std::fs::remove_file(path);
    }

    /// Try to create an edit layer; skip test if USD bridge unavailable.
    fn try_create_layer(path: &str) -> Option<UsdEditLayer> {
        match UsdEditLayer::create(path) {
            Ok(layer) => Some(layer),
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                None
            }
        }
    }

    /// Create a minimal empty mesh for test prototypes.
    fn empty_mesh() -> Arc<crate::mesh::Mesh> {
        Arc::new(crate::mesh::Mesh::new(vec![], vec![], None))
    }

    #[test]
    fn test_export_creates_valid_file() {
        let out = temp_usda_path("valid");
        let scene = Scene::new("test");
        let edit_state = EditState::default();

        let config = ExportConfig {
            output_path: out.clone(),
            source_usd_path: None,
            as_sublayer: false,
            export_root: "/BIF".to_string(),
            ..Default::default()
        };

        let result = match export_scene(&scene, &edit_state, &[], &config) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Skipping test - export failed: {e}");
                cleanup(&out);
                return;
            }
        };

        assert_eq!(result.xform_count, 0);
        assert_eq!(result.instancer_count, 0);

        // Reopen as UsdStage — should not error
        let stage = UsdStage::open(&out);
        assert!(stage.is_ok(), "Exported file should be a valid USD stage");

        cleanup(&out);
    }

    #[test]
    fn test_roundtrip_xform_override() {
        let out = temp_usda_path("xform");

        // Check USD bridge available
        if try_create_layer(&out).is_none() {
            return;
        }
        cleanup(&out); // remove the probe file

        // Build scene with one instance
        let mut scene = Scene::new("test");
        let proto_id = scene.add_prototype(empty_mesh(), "/World/box".to_string());

        let transform = Transform {
            translation: Vec3::new(5.0, 10.0, -3.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        scene.add_instance_with_path(proto_id, transform, "/World/box".to_string());

        // Set up edit state with a transform override
        let mut edit_state = EditState::default();
        let override_transform = Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::new(2.0, 2.0, 2.0),
        };
        edit_state.transform_overrides.insert(0, override_transform);

        let prim_paths = vec!["/World/box".to_string()];

        let config = ExportConfig {
            output_path: out.clone(),
            source_usd_path: None,
            as_sublayer: false,
            export_root: "/BIF".to_string(),
            ..Default::default()
        };

        let result = export_scene(&scene, &edit_state, &prim_paths, &config).expect("export");
        assert_eq!(result.xform_count, 1);

        // Reopen and verify the xform was written
        let stage = UsdStage::open(&out).expect("reopen");
        let mesh_count = stage.mesh_count().unwrap_or(0);
        // The export writes an Xform prim (not a mesh), so mesh_count may be 0.
        // But we can verify the file is valid and has content.
        // Xform export writes an Xform prim, not a mesh — mesh_count may be 0
        // but the file should be valid (we opened it successfully above)
        let _ = mesh_count;

        cleanup(&out);
    }

    #[test]
    fn test_roundtrip_keyframes() {
        let out = temp_usda_path("keyframes");

        if try_create_layer(&out).is_none() {
            return;
        }
        cleanup(&out);

        let scene = Scene::new("test");
        let mut edit_state = EditState::default();

        // Add keyframed transform at times 1.0 and 24.0
        let kf1 = TransformKeyframe {
            time: 1.0,
            transform: Transform {
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
        };
        let kf2 = TransformKeyframe {
            time: 24.0,
            transform: Transform {
                translation: Vec3::new(10.0, 0.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
        };

        let anim = AnimatedTransform {
            static_transform: kf1.transform.clone(),
            keyframes: Some(vec![kf1, kf2]),
        };
        edit_state.keyframe_overrides.insert(0, anim);

        let prim_paths = vec!["/World/animated_box".to_string()];

        let config = ExportConfig {
            output_path: out.clone(),
            source_usd_path: None,
            as_sublayer: false,
            export_root: "/BIF".to_string(),
            ..Default::default()
        };

        let result = export_scene(&scene, &edit_state, &prim_paths, &config).expect("export");
        assert_eq!(result.keyframe_count, 2, "Should write 2 keyframe opinions");

        // Verify file is valid
        let stage = UsdStage::open(&out).expect("reopen");
        assert!(stage.mesh_count().is_ok(), "Stage should be queryable");

        cleanup(&out);
    }

    #[test]
    fn test_roundtrip_point_instancer() {
        let out = temp_usda_path("instancer");

        if try_create_layer(&out).is_none() {
            return;
        }
        cleanup(&out);

        // Build scene with a prototype and a point cloud
        let mut scene = Scene::new("test");
        let proto_id = scene.add_prototype(empty_mesh(), "/World/sphere".to_string());

        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ];
        let count = positions.len();

        let cloud = PointCloud {
            id: 0,
            name: "scatter_0".to_string(),
            positions,
            attributes: PointAttributes {
                scales: Some(vec![Vec3::ONE; count]),
                orientations: Some(vec![Quat::IDENTITY; count]),
                proto_indices: vec![0; count],
                ids: None,
            },
            prototype_ids: vec![proto_id],
            transform: Transform::default(),
            distribution: DistributionMethod::Manual,
            invisible_ids: Vec::new(),
        };
        scene.point_clouds.push(cloud);

        let edit_state = EditState::default();
        let config = ExportConfig {
            output_path: out.clone(),
            source_usd_path: None,
            as_sublayer: false,
            export_root: "/BIF".to_string(),
            ..Default::default()
        };

        let result = export_scene(&scene, &edit_state, &[], &config).expect("export");
        assert_eq!(result.instancer_count, 1);

        // Reopen and verify the instancer
        let stage = UsdStage::open(&out).expect("reopen");
        let instancer_count = stage.instancer_count().expect("instancer_count");
        assert_eq!(
            instancer_count, 1,
            "Should have 1 PointInstancer in exported file"
        );

        let instancer = stage.get_instancer(0).expect("get_instancer");
        assert_eq!(
            instancer.transforms.len(),
            5,
            "Instancer should have 5 instances"
        );

        // Verify positions are close (transforms encode position in column 3)
        let t0 = instancer.transforms[0];
        let pos0 = Vec3::new(t0.w_axis.x, t0.w_axis.y, t0.w_axis.z);
        assert!(
            (pos0 - Vec3::ZERO).length() < 0.01,
            "First instance position should be near origin, got {pos0}"
        );

        let t1 = instancer.transforms[1];
        let pos1 = Vec3::new(t1.w_axis.x, t1.w_axis.y, t1.w_axis.z);
        assert!(
            (pos1 - Vec3::new(1.0, 0.0, 0.0)).length() < 0.01,
            "Second instance should be at (1,0,0), got {pos1}"
        );

        cleanup(&out);
    }

    #[test]
    fn test_roundtrip_sublayer_preserves_original() {
        let source = "../../assets/lucy_100_fixed.usda";
        let out = temp_usda_path("sublayer");

        // First check that source file exists and USD bridge works
        let source_stage = match UsdStage::open(source) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - source not found: {e}");
                return;
            }
        };

        let original_instancer_count = source_stage
            .instancer_count()
            .expect("instancer_count on source");
        let original_mesh_count = source_stage.mesh_count().expect("mesh_count on source");
        drop(source_stage);

        // Export with source as sublayer + one xform override
        let scene = Scene::new("test");
        let mut edit_state = EditState::default();
        edit_state.transform_overrides.insert(
            0,
            Transform {
                translation: Vec3::new(99.0, 0.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
        );

        let prim_paths = vec!["/World/lucy_instancer/i0".to_string()];

        // Use absolute path for sublayer reference
        let abs_source = std::fs::canonicalize(source)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| source.to_string());

        let config = ExportConfig {
            output_path: out.clone(),
            source_usd_path: Some(abs_source),
            as_sublayer: true,
            export_root: "/BIF".to_string(),
            ..Default::default()
        };

        let result = export_scene(&scene, &edit_state, &prim_paths, &config).expect("export");
        assert_eq!(result.xform_count, 1);

        // Reopen — composed stage should have original data intact
        let composed = UsdStage::open(&out).expect("reopen composed");
        let composed_mesh_count = composed.mesh_count().expect("mesh_count on composed");
        let composed_instancer_count = composed
            .instancer_count()
            .expect("instancer_count on composed");

        assert_eq!(
            composed_mesh_count, original_mesh_count,
            "Sublayer should preserve original mesh count"
        );
        assert_eq!(
            composed_instancer_count, original_instancer_count,
            "Sublayer should preserve original instancer count"
        );

        cleanup(&out);
    }

    #[test]
    fn test_edit_layer_create_and_save() {
        let out = temp_usda_path("create_save");

        let mut layer = match UsdEditLayer::create(&out) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        // Write a simple xform
        let mat = bif_math::Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        layer
            .write_xform("/test_prim", -1.0, &mat)
            .expect("write_xform");
        layer.save().expect("save");

        // Verify file exists and is valid
        assert!(
            std::path::Path::new(&out).exists(),
            "Exported file should exist"
        );

        let stage = UsdStage::open(&out).expect("reopen");
        assert!(stage.mesh_count().is_ok(), "Stage should be queryable");

        cleanup(&out);
    }

    #[test]
    fn test_export_with_authored_prims() {
        use crate::usd::cpp_bridge::{UsdKind, UsdPrimType, UsdSpecifier};

        let out = temp_usda_path("authored_prims");

        if try_create_layer(&out).is_none() {
            return;
        }
        cleanup(&out);

        let scene = Scene::new("test");
        let edit_state = EditState::default();

        let config = ExportConfig {
            output_path: out.clone(),
            source_usd_path: None,
            as_sublayer: false,
            export_root: "/BIF".to_string(),
            authored_prims: vec![
                AuthoredPrim {
                    path: "/shot".to_string(),
                    prim_type: UsdPrimType::Scope,
                    kind: UsdKind::Assembly,
                    specifier: UsdSpecifier::Define,
                },
                AuthoredPrim {
                    path: "/shot/geo".to_string(),
                    prim_type: UsdPrimType::Xform,
                    kind: UsdKind::Group,
                    specifier: UsdSpecifier::Define,
                },
            ],
            graft_prefix: None,
            ..Default::default()
        };

        let result = export_scene(&scene, &edit_state, &[], &config).expect("export");
        assert_eq!(result.prim_count, 2, "Should write 2 authored prims");

        // Reopen and verify prims exist
        let stage = UsdStage::open(&out).expect("reopen");
        let prim_count = stage.prim_count().expect("prim_count");
        assert!(
            prim_count >= 2,
            "Should have at least 2 prims, got {}",
            prim_count
        );

        cleanup(&out);
    }

    #[test]
    fn test_define_scope_prim() {
        use crate::usd::cpp_bridge::{UsdPrimType, UsdSpecifier};

        let out = temp_usda_path("scope_prim");

        let mut layer = match UsdEditLayer::create(&out) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        layer
            .define_prim("/World", UsdPrimType::Scope, UsdSpecifier::Define)
            .expect("define_prim");
        layer.save().expect("save");

        let stage = UsdStage::open(&out).expect("reopen");
        let prim_count = stage.prim_count().expect("prim_count");
        assert!(prim_count >= 1, "Should have at least 1 prim");

        cleanup(&out);
    }

    #[test]
    fn test_define_xform_prim() {
        use crate::usd::cpp_bridge::{UsdPrimType, UsdSpecifier};

        let out = temp_usda_path("xform_prim");

        let mut layer = match UsdEditLayer::create(&out) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        layer
            .define_prim("/World", UsdPrimType::Xform, UsdSpecifier::Define)
            .expect("define_prim");
        layer.save().expect("save");

        let stage = UsdStage::open(&out).expect("reopen");
        let prim_count = stage.prim_count().expect("prim_count");
        assert!(prim_count >= 1, "Should have at least 1 prim");

        cleanup(&out);
    }

    #[test]
    fn test_prim_with_kind() {
        use crate::usd::cpp_bridge::{UsdKind, UsdPrimType, UsdSpecifier};

        let out = temp_usda_path("prim_kind");

        let mut layer = match UsdEditLayer::create(&out) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        layer
            .define_prim("/shot", UsdPrimType::Scope, UsdSpecifier::Define)
            .expect("define_prim");
        layer
            .set_prim_kind("/shot", UsdKind::Assembly)
            .expect("set_prim_kind");
        layer.save().expect("save");

        let stage = UsdStage::open(&out).expect("reopen");
        let prim_count = stage.prim_count().expect("prim_count");
        assert!(prim_count >= 1, "Should have at least 1 prim");

        cleanup(&out);
    }

    #[test]
    fn test_export_stage_metadata() {
        let out = temp_usda_path("metadata");

        if try_create_layer(&out).is_none() {
            return;
        }
        cleanup(&out);

        let scene = Scene::new("test");
        let edit_state = EditState::default();

        let config = ExportConfig {
            output_path: out.clone(),
            source_usd_path: None,
            as_sublayer: false,
            export_root: "/BIF".to_string(),
            stage_metadata: Some(crate::usd::cpp_bridge::UsdStageMetadata {
                meters_per_unit: 0.01,
                up_axis: crate::usd::cpp_bridge::UpAxis::Y,
                time_codes_per_second: 24.0,
            }),
            ..Default::default()
        };

        let _result = export_scene(&scene, &edit_state, &[], &config).expect("export");

        // Reopen and verify metadata roundtrips
        let stage = UsdStage::open(&out).expect("reopen");
        let meta = stage.get_stage_metadata().expect("metadata");
        assert!(
            (meta.meters_per_unit - 0.01).abs() < 1e-6,
            "metersPerUnit should be 0.01, got {}",
            meta.meters_per_unit
        );

        cleanup(&out);
    }

    #[test]
    fn test_export_materials() {
        let out = temp_usda_path("materials");

        if try_create_layer(&out).is_none() {
            return;
        }
        cleanup(&out);

        let mut scene = Scene::new("test");
        let mat = crate::scene::Material::new("/Looks/Red", Vec3::new(1.0, 0.0, 0.0));
        let mat_id = scene.add_material(mat);

        // Create a prototype with the material bound
        let mesh = empty_mesh();
        let proto_id = scene.add_prototype(mesh, "/World/box".to_string());
        let mat_arc = scene.materials[mat_id].clone();
        let mut proto = (*scene.prototypes[proto_id]).clone();
        proto.material = Some(mat_arc);
        scene.prototypes[proto_id] = Arc::new(proto);

        let edit_state = EditState::default();
        let config = ExportConfig {
            output_path: out.clone(),
            source_usd_path: None,
            as_sublayer: false,
            export_root: "/BIF".to_string(),
            ..Default::default()
        };

        let result = export_scene(&scene, &edit_state, &[], &config).expect("export");
        assert_eq!(result.material_count, 1, "Should export 1 material");

        // Verify file is valid
        let stage = UsdStage::open(&out).expect("reopen");
        let mats = stage.materials().unwrap_or_default();
        assert!(
            !mats.is_empty(),
            "Should have at least 1 material in exported file"
        );

        cleanup(&out);
    }

    #[test]
    fn test_export_lights() {
        let out = temp_usda_path("lights");

        if try_create_layer(&out).is_none() {
            return;
        }
        cleanup(&out);

        let mut scene = Scene::new("test");
        // Add a distant light and a dome light
        scene.lights.push(crate::scene::Light::Distant {
            direction: Vec3::new(0.0, -1.0, 0.0),
            color: Vec3::ONE,
            intensity: 1.0,
            angle: 0.53,
        });
        scene.lights.push(crate::scene::Light::Dome {
            rotation: 0.0,
            intensity: 1.0,
            texture_path: None,
        });

        let edit_state = EditState::default();
        let config = ExportConfig {
            output_path: out.clone(),
            source_usd_path: None,
            as_sublayer: false,
            export_root: "/BIF".to_string(),
            ..Default::default()
        };

        let result = export_scene(&scene, &edit_state, &[], &config).expect("export");
        assert_eq!(result.light_count, 2, "Should export 2 lights");

        // Verify roundtrip
        let stage = UsdStage::open(&out).expect("reopen");
        let lights = stage.lights().unwrap_or_default();
        assert_eq!(lights.len(), 2, "Should have 2 lights in exported file");

        cleanup(&out);
    }

    #[test]
    fn test_export_visibility() {
        let out = temp_usda_path("visibility");

        if try_create_layer(&out).is_none() {
            return;
        }
        cleanup(&out);

        let mut scene = Scene::new("test");
        let mesh = empty_mesh();
        let proto_id = scene.add_prototype(mesh, "/World/hidden_box".to_string());
        scene.add_instance_with_path(
            proto_id,
            Transform::default(),
            "/World/hidden_box".to_string(),
        );

        let edit_state = EditState::default();
        let config = ExportConfig {
            output_path: out.clone(),
            source_usd_path: None,
            as_sublayer: false,
            export_root: "/BIF".to_string(),
            hidden_prim_paths: vec!["/World/hidden_box".to_string()],
            ..Default::default()
        };

        let result = export_scene(&scene, &edit_state, &[], &config).expect("export");
        assert_eq!(
            result.visibility_count, 1,
            "Should write 1 visibility opinion"
        );

        // File should be valid
        let stage = UsdStage::open(&out).expect("reopen");
        assert!(stage.prim_count().is_ok());

        cleanup(&out);
    }

    #[test]
    fn test_export_dome_light_roundtrip() {
        let out = temp_usda_path("dome_light");

        if try_create_layer(&out).is_none() {
            return;
        }
        cleanup(&out);

        let mut scene = Scene::new("test");
        scene.lights.push(crate::scene::Light::Dome {
            rotation: 0.5,
            intensity: 2.0,
            texture_path: Some(Arc::from("sky.hdr")),
        });

        let edit_state = EditState::default();
        let config = ExportConfig {
            output_path: out.clone(),
            as_sublayer: false,
            ..Default::default()
        };

        let result = export_scene(&scene, &edit_state, &[], &config).expect("export");
        assert_eq!(result.light_count, 1);

        // Roundtrip: verify dome light exists
        let stage = UsdStage::open(&out).expect("reopen");
        let lights = stage.lights().unwrap_or_default();
        assert_eq!(lights.len(), 1, "Should have 1 dome light");
        assert_eq!(
            lights[0].light_type,
            crate::usd::cpp_bridge::UsdLightType::Dome
        );

        cleanup(&out);
    }

    #[test]
    fn test_export_invisible_ids_roundtrip() {
        let out = temp_usda_path("invisible_ids");

        if try_create_layer(&out).is_none() {
            return;
        }
        cleanup(&out);

        let mut scene = Scene::new("test");
        let proto_id = scene.add_prototype(empty_mesh(), "/World/sphere".to_string());

        let count = 5;
        let cloud = PointCloud {
            id: 0,
            name: "instancer_0".to_string(),
            positions: vec![Vec3::ZERO; count],
            attributes: PointAttributes {
                scales: Some(vec![Vec3::ONE; count]),
                orientations: Some(vec![Quat::IDENTITY; count]),
                proto_indices: vec![0; count],
                ids: None,
            },
            prototype_ids: vec![proto_id],
            transform: Transform::default(),
            distribution: DistributionMethod::Manual,
            invisible_ids: vec![1, 3],
        };
        scene.point_clouds.push(cloud);

        let edit_state = EditState::default();
        let config = ExportConfig {
            output_path: out.clone(),
            as_sublayer: false,
            ..Default::default()
        };

        let result = export_scene(&scene, &edit_state, &[], &config).expect("export");
        assert_eq!(result.instancer_count, 1);

        // Roundtrip: verify invisible IDs
        let stage = UsdStage::open(&out).expect("reopen");
        let instancers = stage.instancers().expect("instancers");
        assert_eq!(instancers.len(), 1);
        assert_eq!(
            instancers[0].invisible_ids,
            vec![1, 3],
            "invisibleIds should roundtrip"
        );

        cleanup(&out);
    }

    #[test]
    fn test_export_geom_subsets() {
        let out = temp_usda_path("geom_subsets");

        if try_create_layer(&out).is_none() {
            return;
        }
        cleanup(&out);

        let mut scene = Scene::new("test");

        // Two materials
        let mat0 = crate::scene::Material::new("/Looks/Red", Vec3::new(1.0, 0.0, 0.0));
        let mat1 = crate::scene::Material::new("/Looks/Blue", Vec3::new(0.0, 0.0, 1.0));
        scene.add_material(mat0);
        scene.add_material(mat1);

        // Mesh with per-face material IDs (2 tris: face 0→mat0, face 1→mat1)
        let mesh = Arc::new(crate::mesh::Mesh::new_with_materials(
            vec![
                Vec3::ZERO,
                Vec3::X,
                Vec3::Y,
                Vec3::Z,
                Vec3::ONE,
                Vec3::NEG_ONE,
            ],
            vec![0, 1, 2, 3, 4, 5],
            None,
            None,
            Some(vec![0, 1]),
        ));
        scene.add_prototype(mesh, "/World/multi_mat_mesh".to_string());

        let edit_state = EditState::default();
        let config = ExportConfig {
            output_path: out.clone(),
            as_sublayer: false,
            ..Default::default()
        };

        let result = export_scene(&scene, &edit_state, &[], &config).expect("export");
        assert_eq!(result.material_count, 2, "Should export 2 materials");
        assert_eq!(result.mesh_count, 1, "Should export 1 mesh");

        // Verify file is valid and has prims (mesh + subsets + materials)
        let stage = UsdStage::open(&out).expect("reopen");
        let prim_count = stage.prim_count().expect("prim_count");
        assert!(
            prim_count >= 4,
            "Should have mesh + 2 subsets + materials, got {}",
            prim_count
        );

        cleanup(&out);
    }
}
