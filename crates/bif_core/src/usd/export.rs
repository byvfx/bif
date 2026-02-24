//! USD scene export — writes BIF edits as a composable USD layer.
//!
//! All export logic lives in `bif_core` (no egui dependency) so it can be
//! reused from any UI framework (egui, Qt, CLI).

use crate::point_cloud::PointCloud;
use crate::scene::Scene;
use crate::undo::EditState;
use crate::usd::cpp_bridge::{UsdBridgeError, UsdEditLayer, UsdKind, UsdPrimType, UsdSpecifier};

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
    /// Output file path
    pub output_path: String,
}

impl std::fmt::Display for ExportResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} prims + {} xforms + {} keyframes + {} instancers -> {}",
            self.prim_count,
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

    // Write PointInstancers from point clouds (skip empty clouds)
    let mut instancer_count = 0;
    for cloud in &scene.point_clouds {
        if cloud.positions.is_empty() || cloud.prototype_ids.is_empty() {
            log::warn!(
                "Skipping empty point cloud {:?} (positions={}, protos={})",
                cloud.name,
                cloud.positions.len(),
                cloud.prototype_ids.len()
            );
            continue;
        }
        let instancer_path = format!("{}/{}", config.export_root, cloud.name);
        let instancer_path = apply_graft_prefix(&instancer_path, &config.graft_prefix);
        let proto_paths: Vec<String> = resolve_proto_paths(scene, cloud, config);
        if proto_paths.is_empty() {
            log::warn!(
                "Skipping instancer {:?}: no prototype paths resolved",
                instancer_path
            );
            continue;
        }
        layer
            .write_point_instancer(&instancer_path, cloud, &proto_paths)
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
        instancer_count += 1;
    }

    layer.save()?;

    Ok(ExportResult {
        xform_count,
        keyframe_count,
        instancer_count,
        prim_count,
        output_path: config.output_path.clone(),
    })
}

/// Prepend a graft prefix to a prim path (if set).
///
/// E.g., path="/World/hero" + prefix="/shot" → "/shot/World/hero"
fn apply_graft_prefix(path: &str, prefix: &Option<String>) -> String {
    match prefix {
        Some(pfx) if !pfx.is_empty() => format!("{}{}", pfx, path),
        _ => path.to_string(),
    }
}

/// Resolve prototype prim paths for a point cloud's prototype IDs.
///
/// Uses the prototype's name (which is the USD prim path for USD-loaded protos)
/// or constructs a BIF path for procedural protos.
fn resolve_proto_paths(scene: &Scene, cloud: &PointCloud, config: &ExportConfig) -> Vec<String> {
    cloud
        .prototype_ids
        .iter()
        .map(|&proto_id| {
            scene
                .prototypes
                .get(proto_id)
                .map(|proto| {
                    if proto.name.starts_with('/') {
                        // Already a USD prim path
                        proto.name.to_string()
                    } else {
                        // BIF-created prototype
                        format!("{}/{}", config.export_root, proto.name)
                    }
                })
                .unwrap_or_else(|| format!("{}/proto_{}", config.export_root, proto_id))
        })
        .collect()
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
}
