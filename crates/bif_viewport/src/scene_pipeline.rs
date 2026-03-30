//! Pure scene computation — extracted from scene_loader.rs for testability.
//!
//! These functions compute geometry, instances, materials, and bounds
//! without touching GPU state. Testable without wgpu.

#![allow(dead_code)] // Will be called from scene_loader.rs after wiring

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bif_math::{Aabb, Mat4, Mat4Ext, Vec3};

/// Result of pure scene computation — no GPU resources.
#[derive(Debug)]
pub struct ExpandedInstances {
    pub transforms: Vec<Mat4>,
    pub material_ids: Vec<u32>,
    pub prototype_ids: Vec<usize>,
    pub prim_paths: Vec<String>,
    pub purposes: Vec<bif_core::Purpose>,
}

/// World-space bounding box.
#[derive(Debug, Clone)]
pub struct WorldBounds {
    pub min: Vec3,
    pub max: Vec3,
}

/// Axis/unit correction matrix computed from USD stage metadata.
#[derive(Debug, Clone, Copy)]
pub struct AxisCorrection {
    pub matrix: Mat4,
}

// ---------------------------------------------------------------------------
// Pure functions
// ---------------------------------------------------------------------------

/// Resolve the USD prim path for an instance — use the instance's prim_path
/// if set, otherwise synthesise one from the prototype name and index.
pub fn resolve_prim_path(inst: &bif_core::Instance, scene: &bif_core::Scene, idx: usize) -> String {
    if !inst.prim_path.is_empty() {
        inst.prim_path.to_string()
    } else {
        let proto_name = scene
            .prototypes
            .get(inst.prototype_id)
            .map(|p| &*p.name)
            .unwrap_or("unknown");
        format!("/BIF/{}/{}", proto_name, idx)
    }
}

/// Build a fast lookup from material name to scene-material index.
///
/// Returns `(index_map, default_material_index)` where the default index is
/// `materials.len()` (one past the last valid index, matching the sentinel
/// used in scene_loader.rs).
pub fn build_material_index_lookup(
    materials: &[Arc<bif_core::Material>],
) -> (HashMap<Arc<str>, u32>, u32) {
    let map: HashMap<Arc<str>, u32> = materials
        .iter()
        .enumerate()
        .map(|(idx, mat)| (mat.name.clone(), idx as u32))
        .collect();
    let default = materials.len() as u32;
    (map, default)
}

/// Expand scene prototypes + instances into flat parallel arrays.
///
/// When there are no instances, each non-hidden prototype gets an identity
/// instance (matching scene_loader.rs behaviour for single-proto scenes).
///
/// `instancer_instances` are extra instances from PointInstancer / Scatter
/// nodes — they are appended after the scene's own instances.
pub fn expand_scene_instances(
    scene: &bif_core::Scene,
    hidden_proto_ids: &HashSet<usize>,
    instancer_instances: &[&bif_core::Instance],
    material_index: &HashMap<Arc<str>, u32>,
    default_mat_index: u32,
    max_instances: usize,
) -> ExpandedInstances {
    let scene_insts = scene.instances();
    let instancer_count = instancer_instances.len();
    let total_capacity = scene.instance_count() + instancer_count;

    let mut transforms = Vec::with_capacity(total_capacity);
    let mut material_ids = Vec::with_capacity(total_capacity);
    let mut prototype_ids = Vec::with_capacity(total_capacity);
    let mut purposes = Vec::with_capacity(total_capacity);
    let mut prim_paths = Vec::with_capacity(total_capacity);

    if scene_insts.is_empty() {
        // No instances: create one identity instance per non-hidden prototype
        for (proto_id, proto) in scene.prototypes.iter().enumerate() {
            if hidden_proto_ids.contains(&proto_id) {
                continue;
            }
            transforms.push(Mat4::IDENTITY);
            prototype_ids.push(proto_id);
            purposes.push(bif_core::Purpose::Default);
            let mat_id = proto
                .material
                .as_ref()
                .and_then(|mat| material_index.get(&mat.name).copied())
                .unwrap_or(default_mat_index);
            material_ids.push(mat_id);
            prim_paths.push(format!("/BIF/{}/{}", &*proto.name, proto_id));
        }
    } else {
        // Expand scene instances, filtering hidden prototypes
        for (idx, inst) in scene_insts.iter().enumerate() {
            if hidden_proto_ids.contains(&inst.prototype_id) {
                continue;
            }
            transforms.push(inst.model_matrix());
            prototype_ids.push(inst.prototype_id);
            purposes.push(inst.purpose);
            let mat_id = scene
                .prototypes
                .get(inst.prototype_id)
                .and_then(|proto| proto.material.as_ref())
                .and_then(|mat| material_index.get(&mat.name).copied())
                .unwrap_or(default_mat_index);
            material_ids.push(mat_id);
            prim_paths.push(resolve_prim_path(inst, scene, idx));
        }
    }

    // Append instancer-expanded instances
    let scene_inst_count = prim_paths.len();
    for (i, inst) in instancer_instances.iter().enumerate() {
        transforms.push(inst.model_matrix());
        prototype_ids.push(inst.prototype_id);
        purposes.push(inst.purpose);
        let mat_id = scene
            .prototypes
            .get(inst.prototype_id)
            .and_then(|p| p.material.as_ref())
            .and_then(|mat| material_index.get(&mat.name).copied())
            .unwrap_or(default_mat_index);
        material_ids.push(mat_id);
        let proto_name = scene
            .prototypes
            .get(inst.prototype_id)
            .map(|p| &*p.name)
            .unwrap_or("unknown");
        prim_paths.push(format!(
            "/BIF/{}/instancer_{}",
            proto_name,
            scene_inst_count + i
        ));
    }

    // Truncate to max_instances
    if transforms.len() > max_instances {
        log::warn!(
            "Instance count {} exceeds cap {}. Truncating.",
            transforms.len(),
            max_instances
        );
        transforms.truncate(max_instances);
        material_ids.truncate(max_instances);
        prototype_ids.truncate(max_instances);
        prim_paths.truncate(max_instances);
        purposes.truncate(max_instances);
    }

    ExpandedInstances {
        transforms,
        material_ids,
        prototype_ids,
        prim_paths,
        purposes,
    }
}

/// Compute the world-space bounding box by transforming each prototype's AABB
/// by its instance transforms.
///
/// When `per_proto_aabbs` is empty (single-prototype scene), the single
/// `prototype_aabb` is used for all instances.
pub fn compute_world_bounds(
    instance_transforms: &[Mat4],
    instance_prototype_ids: &[usize],
    prototype_aabbs: &[Aabb],
) -> WorldBounds {
    if instance_transforms.is_empty() || prototype_aabbs.is_empty() {
        return WorldBounds {
            min: Vec3::ZERO,
            max: Vec3::ZERO,
        };
    }

    let mut world_min = Vec3::splat(f32::INFINITY);
    let mut world_max = Vec3::splat(f32::NEG_INFINITY);

    for (transform, &proto_id) in instance_transforms
        .iter()
        .zip(instance_prototype_ids.iter())
    {
        let proto_aabb = prototype_aabbs
            .get(proto_id)
            .copied()
            .unwrap_or_else(|| prototype_aabbs[0]);
        let transformed = transform.transform_aabb(&proto_aabb);
        let t_min = transformed.min_point();
        let t_max = transformed.max_point();
        world_min = world_min.min(t_min);
        world_max = world_max.max(t_max);
    }

    WorldBounds {
        min: world_min,
        max: world_max,
    }
}

/// Compute axis/unit correction matrix from USD stage metadata.
///
/// - Z-up stages get a -90deg X rotation to convert to Y-up.
/// - Non-meter stages get a uniform scale by `meters_per_unit`.
///
/// Returns `None` when no correction is needed (Y-up, 1.0 meters_per_unit).
pub fn apply_axis_correction(
    up_axis: bif_core::usd::cpp_bridge::UpAxis,
    meters_per_unit: f64,
    do_axis: bool,
    do_unit: bool,
) -> Option<AxisCorrection> {
    let mut correction = Mat4::IDENTITY;

    if do_axis && up_axis == bif_core::usd::cpp_bridge::UpAxis::Z {
        // Rotate -90 degrees around X to convert Z-up -> Y-up
        correction = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    }

    if do_unit && (meters_per_unit - 1.0).abs() > 1e-6 {
        let s = meters_per_unit as f32;
        correction = Mat4::from_scale(Vec3::splat(s)) * correction;
    }

    if correction == Mat4::IDENTITY {
        None
    } else {
        Some(AxisCorrection { matrix: correction })
    }
}

/// Apply a correction matrix to a slice of transforms in-place.
pub fn apply_correction_to_transforms(transforms: &mut [Mat4], correction: &AxisCorrection) {
    for t in transforms.iter_mut() {
        *t = correction.matrix * *t;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bif_core::{Instance, Material, Mesh, Scene, Transform};
    use bif_math::Vec3;
    use std::sync::Arc;

    /// Helper: create a minimal triangle mesh with known bounds.
    fn make_triangle_mesh() -> Arc<Mesh> {
        Arc::new(Mesh::new(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            vec![0, 1, 2],
            None,
        ))
    }

    /// Helper: create a unit cube mesh (bounds 0..1 on all axes).
    fn make_unit_cube_mesh() -> Arc<Mesh> {
        // 8 vertices of a unit cube
        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(0.0, 1.0, 1.0),
        ];
        // 12 triangles (2 per face)
        let indices = vec![
            0, 1, 2, 0, 2, 3, // front
            4, 6, 5, 4, 7, 6, // back
            0, 4, 5, 0, 5, 1, // bottom
            2, 6, 7, 2, 7, 3, // top
            0, 3, 7, 0, 7, 4, // left
            1, 5, 6, 1, 6, 2, // right
        ];
        Arc::new(Mesh::new(positions, indices, None))
    }

    /// Helper: build a simple scene with N prototypes and a set of instances.
    fn make_scene_with_instances(
        proto_count: usize,
        instance_specs: &[(usize, Vec3)], // (prototype_id, translation)
    ) -> Scene {
        let mesh = make_triangle_mesh();
        let mut scene = Scene::new("test");
        for i in 0..proto_count {
            scene.add_prototype(mesh.clone(), format!("proto_{}", i));
        }
        for &(proto_id, translation) in instance_specs {
            scene.add_instance(proto_id, Transform::from_translation(translation));
        }
        scene
    }

    // -----------------------------------------------------------------------
    // resolve_prim_path
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_prim_path_with_path() {
        let mesh = make_triangle_mesh();
        let mut scene = Scene::new("test");
        scene.add_prototype(mesh, "cube");

        let inst = Instance::with_prim_path(0, Transform::default(), Arc::from("/World/my_cube"));
        let result = resolve_prim_path(&inst, &scene, 0);
        assert_eq!(result, "/World/my_cube");
    }

    #[test]
    fn test_resolve_prim_path_synthesized() {
        let mesh = make_triangle_mesh();
        let mut scene = Scene::new("test");
        scene.add_prototype(mesh, "sphere");

        let inst = Instance::new(0, Transform::default());
        let result = resolve_prim_path(&inst, &scene, 42);
        assert_eq!(result, "/BIF/sphere/42");
    }

    // -----------------------------------------------------------------------
    // build_material_index_lookup
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_material_lookup_empty() {
        let materials: Vec<Arc<Material>> = vec![];
        let (map, default) = build_material_index_lookup(&materials);
        assert!(map.is_empty());
        assert_eq!(default, 0);
    }

    #[test]
    fn test_build_material_lookup_with_materials() {
        let materials: Vec<Arc<Material>> = vec![
            Arc::new(Material::new("wood", Vec3::new(0.6, 0.3, 0.1))),
            Arc::new(Material::new("metal", Vec3::new(0.8, 0.8, 0.8))),
            Arc::new(Material::new("glass", Vec3::new(0.9, 0.9, 1.0))),
        ];
        let (map, default) = build_material_index_lookup(&materials);
        assert_eq!(map.len(), 3);
        assert_eq!(map[&Arc::from("wood") as &Arc<str>], 0);
        assert_eq!(map[&Arc::from("metal") as &Arc<str>], 1);
        assert_eq!(map[&Arc::from("glass") as &Arc<str>], 2);
        assert_eq!(default, 3);
    }

    // -----------------------------------------------------------------------
    // expand_scene_instances
    // -----------------------------------------------------------------------

    #[test]
    fn test_expand_instances_single_proto() {
        let scene = make_scene_with_instances(
            1,
            &[
                (0, Vec3::new(0.0, 0.0, 0.0)),
                (0, Vec3::new(1.0, 0.0, 0.0)),
                (0, Vec3::new(2.0, 0.0, 0.0)),
            ],
        );
        let (mat_index, default_mat) = build_material_index_lookup(&scene.materials);
        let hidden = HashSet::new();
        let result =
            expand_scene_instances(&scene, &hidden, &[], &mat_index, default_mat, 1_000_000);
        assert_eq!(result.transforms.len(), 3);
        assert_eq!(result.prototype_ids, vec![0, 0, 0]);
        assert_eq!(result.material_ids, vec![default_mat; 3]);
        assert_eq!(result.prim_paths.len(), 3);
    }

    #[test]
    fn test_expand_instances_hidden_proto() {
        let scene = make_scene_with_instances(2, &[(0, Vec3::ZERO), (1, Vec3::X), (0, Vec3::Y)]);
        let (mat_index, default_mat) = build_material_index_lookup(&scene.materials);
        let mut hidden = HashSet::new();
        hidden.insert(1_usize);
        let result =
            expand_scene_instances(&scene, &hidden, &[], &mat_index, default_mat, 1_000_000);
        // Only proto 0 instances should remain
        assert_eq!(result.transforms.len(), 2);
        assert!(result.prototype_ids.iter().all(|&id| id == 0));
    }

    #[test]
    fn test_expand_instances_max_cap() {
        let scene = make_scene_with_instances(
            1,
            &[
                (0, Vec3::ZERO),
                (0, Vec3::X),
                (0, Vec3::Y),
                (0, Vec3::Z),
                (0, Vec3::ONE),
            ],
        );
        let (mat_index, default_mat) = build_material_index_lookup(&scene.materials);
        let hidden = HashSet::new();
        let result = expand_scene_instances(
            &scene,
            &hidden,
            &[],
            &mat_index,
            default_mat,
            3, // cap at 3
        );
        assert_eq!(result.transforms.len(), 3);
        assert_eq!(result.material_ids.len(), 3);
        assert_eq!(result.prototype_ids.len(), 3);
        assert_eq!(result.prim_paths.len(), 3);
        assert_eq!(result.purposes.len(), 3);
    }

    #[test]
    fn test_expand_instances_empty_scene() {
        let mesh = make_triangle_mesh();
        let mut scene = Scene::new("empty");
        scene.add_prototype(mesh, "cube");
        // No instances added — should get one identity instance per prototype
        let (mat_index, default_mat) = build_material_index_lookup(&scene.materials);
        let hidden = HashSet::new();
        let result =
            expand_scene_instances(&scene, &hidden, &[], &mat_index, default_mat, 1_000_000);
        assert_eq!(result.transforms.len(), 1);
        assert_eq!(result.transforms[0], Mat4::IDENTITY);
        assert_eq!(result.prototype_ids, vec![0]);
    }

    #[test]
    fn test_expand_instances_with_instancer() {
        let scene = make_scene_with_instances(1, &[(0, Vec3::ZERO)]);
        let (mat_index, default_mat) = build_material_index_lookup(&scene.materials);
        let hidden = HashSet::new();

        // Extra instancer instances
        let inst_a = Instance::with_translation(0, Vec3::new(5.0, 0.0, 0.0));
        let inst_b = Instance::with_translation(0, Vec3::new(10.0, 0.0, 0.0));
        let instancer_insts: Vec<&Instance> = vec![&inst_a, &inst_b];

        let result = expand_scene_instances(
            &scene,
            &hidden,
            &instancer_insts,
            &mat_index,
            default_mat,
            1_000_000,
        );
        // 1 scene instance + 2 instancer instances
        assert_eq!(result.transforms.len(), 3);
        assert_eq!(result.prototype_ids.len(), 3);
        // Instancer prim paths use /BIF/<proto>/instancer_N convention
        assert!(result.prim_paths[1].contains("instancer_"));
        assert!(result.prim_paths[2].contains("instancer_"));
    }

    // -----------------------------------------------------------------------
    // compute_world_bounds
    // -----------------------------------------------------------------------

    #[test]
    fn test_world_bounds_identity() {
        let mesh = make_unit_cube_mesh();
        let aabb = mesh.bounds;
        let transforms = vec![Mat4::IDENTITY];
        let proto_ids = vec![0_usize];
        let proto_aabbs = vec![aabb];

        let bounds = compute_world_bounds(&transforms, &proto_ids, &proto_aabbs);
        assert!((bounds.min - Vec3::ZERO).length() < 1e-5);
        assert!((bounds.max - Vec3::ONE).length() < 1e-5);
    }

    #[test]
    fn test_world_bounds_translated() {
        let mesh = make_unit_cube_mesh();
        let aabb = mesh.bounds;
        let transforms = vec![
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)),
        ];
        let proto_ids = vec![0_usize, 0_usize];
        let proto_aabbs = vec![aabb];

        let bounds = compute_world_bounds(&transforms, &proto_ids, &proto_aabbs);
        // min should be (0,0,0) from first instance
        assert!((bounds.min - Vec3::ZERO).length() < 1e-5);
        // max should be (11,1,1) from translated instance
        assert!((bounds.max - Vec3::new(11.0, 1.0, 1.0)).length() < 1e-5);
    }

    #[test]
    fn test_world_bounds_empty() {
        let bounds = compute_world_bounds(&[], &[], &[]);
        assert_eq!(bounds.min, Vec3::ZERO);
        assert_eq!(bounds.max, Vec3::ZERO);
    }

    // -----------------------------------------------------------------------
    // apply_axis_correction
    // -----------------------------------------------------------------------

    #[test]
    fn test_axis_correction_z_up() {
        let correction =
            apply_axis_correction(bif_core::usd::cpp_bridge::UpAxis::Z, 1.0, true, false);
        assert!(correction.is_some(), "Z-up should produce a correction");
        let mat = correction.unwrap().matrix;
        // After -90deg X rotation, a Z-up point (0,0,1) should become (0,1,0)
        let p = mat.transform_point3(Vec3::new(0.0, 0.0, 1.0));
        assert!((p.x).abs() < 1e-5);
        assert!((p.y - 1.0).abs() < 1e-5);
        assert!((p.z).abs() < 1e-5);
    }

    #[test]
    fn test_axis_correction_y_up() {
        let correction =
            apply_axis_correction(bif_core::usd::cpp_bridge::UpAxis::Y, 1.0, true, false);
        assert!(correction.is_none(), "Y-up should need no correction");
    }

    #[test]
    fn test_axis_correction_unit_scaling() {
        // 0.01 = centimeters
        let correction =
            apply_axis_correction(bif_core::usd::cpp_bridge::UpAxis::Y, 0.01, false, true);
        assert!(correction.is_some(), "Non-meter unit needs correction");
        let mat = correction.unwrap().matrix;
        let p = mat.transform_point3(Vec3::new(1.0, 1.0, 1.0));
        assert!((p.x - 0.01).abs() < 1e-6);
        assert!((p.y - 0.01).abs() < 1e-6);
        assert!((p.z - 0.01).abs() < 1e-6);
    }

    #[test]
    fn test_apply_correction_to_transforms() {
        let correction =
            apply_axis_correction(bif_core::usd::cpp_bridge::UpAxis::Z, 1.0, true, false).unwrap();
        let mut transforms = vec![Mat4::from_translation(Vec3::new(0.0, 0.0, 5.0))];
        apply_correction_to_transforms(&mut transforms, &correction);
        // Z-up translation (0,0,5) should become Y-up (0,5,0)
        let col3 = transforms[0].col(3);
        assert!((col3.x).abs() < 1e-5);
        assert!((col3.y - 5.0).abs() < 1e-5);
        assert!((col3.z).abs() < 1e-5);
    }
}
