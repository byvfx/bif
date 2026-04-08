//! Read-only query API for scene data.
//!
//! Abstracts the internal Scene representation so downstream crates
//! (bif_viewport) don't depend on field layout. Enables future
//! alternative implementations (e.g., LayerAwareScene for M32
//! opinion trace) without rippling changes through the viewport.
//!
//! Mutations (add_prototype, add_instance, etc.) stay as direct
//! `Scene` methods — only used in scene loading / node dispatch.

use std::sync::Arc;

use crate::scene::{
    AnimatedTransform, Instance, Light, Material, Prototype, Scene, SceneCamera, TimelineInfo,
};
use crate::usd::cpp_bridge::UsdStageMetadata;
use crate::PointCloud;

/// Read-only query interface for scene data.
///
/// Implemented by `Scene` directly. Future implementations may wrap
/// a Scene with layer overrides for opinion-trace workflows.
pub trait SceneQuery {
    // -- Prototypes --

    /// Number of prototype definitions.
    fn prototype_count(&self) -> usize;

    /// Get a prototype by ID.
    fn prototype(&self, id: usize) -> Option<&Arc<Prototype>>;

    /// All prototypes as a slice.
    fn prototypes(&self) -> &[Arc<Prototype>];

    // -- Instances --

    /// Number of instances.
    fn instance_count(&self) -> usize;

    /// All instances as a slice.
    fn instances(&self) -> &[Instance];

    /// Instance animations parallel to instances (None = static).
    fn instance_animations(&self) -> &[Option<AnimatedTransform>];

    /// Get instance + animation pair by index.
    fn get_instance(&self, idx: usize) -> Option<(&Instance, Option<&AnimatedTransform>)>;

    /// Find instance index by prim path.
    ///
    /// Uses 3-strategy lookup:
    /// 1. Exact match on `prim_path`
    /// 2. Descendant prefix match (`{path}/...`)
    /// 3. Synthetic `/BIF/{path}` fallback (loader-generated paths)
    fn find_instance_by_prim_path(&self, path: &str) -> Option<usize>;

    // -- Materials --

    /// Number of materials.
    fn material_count(&self) -> usize;

    /// Get a material by ID.
    fn material(&self, id: usize) -> Option<&Arc<Material>>;

    /// All materials as a slice.
    fn materials(&self) -> &[Arc<Material>];

    // -- Collections --

    /// Scene cameras from Camera primitives.
    fn cameras(&self) -> &[SceneCamera];

    /// Point clouds (scatter sources).
    fn point_clouds(&self) -> &[PointCloud];

    /// Scene lights.
    fn lights(&self) -> &[Light];

    /// Timeline info (frame range, fps). None if no authored time range.
    fn timeline(&self) -> Option<&TimelineInfo>;

    /// Stage metadata (metersPerUnit, upAxis). None for non-USD scenes.
    fn stage_metadata(&self) -> Option<&UsdStageMetadata>;

    // -- Derived queries --

    /// Total triangle count across all instances.
    fn total_triangle_count(&self) -> usize;

    /// Whether the scene has any animation.
    fn has_animation(&self) -> bool;

    /// Look up the material bound to a prototype.
    fn material_for_prototype(&self, proto_id: usize) -> Option<&Arc<Material>> {
        let proto = self.prototype(proto_id)?;
        proto.material.as_ref()
    }
}

impl SceneQuery for Scene {
    fn prototype_count(&self) -> usize {
        self.prototypes.len()
    }

    fn prototype(&self, id: usize) -> Option<&Arc<Prototype>> {
        self.prototypes.get(id)
    }

    fn prototypes(&self) -> &[Arc<Prototype>] {
        &self.prototypes
    }

    fn instance_count(&self) -> usize {
        Scene::instance_count(self)
    }

    fn instances(&self) -> &[Instance] {
        Scene::instances(self)
    }

    fn instance_animations(&self) -> &[Option<AnimatedTransform>] {
        Scene::instance_animations(self)
    }

    fn get_instance(&self, idx: usize) -> Option<(&Instance, Option<&AnimatedTransform>)> {
        Scene::get_instance(self, idx)
    }

    fn find_instance_by_prim_path(&self, path: &str) -> Option<usize> {
        // 1. Exact match
        self.instances()
            .iter()
            .position(|i| i.prim_path.as_ref() == path)
            .or_else(|| {
                // 2. Descendant prefix match (clicking parent Xform)
                let prefix = format!("{}/", path);
                self.instances()
                    .iter()
                    .position(|i| i.prim_path.starts_with(&prefix))
            })
            .or_else(|| {
                // 3. Synthetic /BIF/ fallback (loader-generated paths)
                let trimmed = path.strip_prefix('/').unwrap_or(path);
                let synth_prefix = format!("/BIF/{}", trimmed);
                self.instances()
                    .iter()
                    .position(|i| i.prim_path.starts_with(&synth_prefix))
            })
    }

    fn material_count(&self) -> usize {
        Scene::material_count(self)
    }

    fn material(&self, id: usize) -> Option<&Arc<Material>> {
        Scene::get_material(self, id)
    }

    fn materials(&self) -> &[Arc<Material>] {
        &self.materials
    }

    fn cameras(&self) -> &[SceneCamera] {
        &self.cameras
    }

    fn point_clouds(&self) -> &[PointCloud] {
        &self.point_clouds
    }

    fn lights(&self) -> &[Light] {
        &self.lights
    }

    fn timeline(&self) -> Option<&TimelineInfo> {
        self.timeline.as_ref()
    }

    fn stage_metadata(&self) -> Option<&UsdStageMetadata> {
        self.stage_metadata.as_ref()
    }

    fn total_triangle_count(&self) -> usize {
        Scene::total_triangle_count(self)
    }

    fn has_animation(&self) -> bool {
        Scene::has_animation(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Mesh;
    use crate::scene::Transform;

    fn make_test_scene() -> Scene {
        let mut scene = Scene::new("test");
        let mesh = Arc::new(Mesh::new(vec![], vec![], None));
        let proto_id = scene.add_prototype(mesh.clone(), "cube");
        scene.add_instance_with_path(proto_id, Transform::default(), "/World/Cube");
        scene.add_instance_with_path(proto_id, Transform::default(), "/World/Group/Sphere");
        scene
    }

    #[test]
    fn query_prototype_count() {
        let scene = make_test_scene();
        let q: &dyn SceneQuery = &scene;
        assert_eq!(q.prototype_count(), 1);
        assert!(q.prototype(0).is_some());
        assert!(q.prototype(99).is_none());
    }

    #[test]
    fn query_instance_count() {
        let scene = make_test_scene();
        let q: &dyn SceneQuery = &scene;
        assert_eq!(q.instance_count(), 2);
        assert_eq!(q.instances().len(), 2);
    }

    #[test]
    fn find_instance_exact_match() {
        let scene = make_test_scene();
        let q: &dyn SceneQuery = &scene;
        assert_eq!(q.find_instance_by_prim_path("/World/Cube"), Some(0));
        assert_eq!(q.find_instance_by_prim_path("/World/Group/Sphere"), Some(1));
    }

    #[test]
    fn find_instance_prefix_match() {
        let scene = make_test_scene();
        let q: &dyn SceneQuery = &scene;
        // /World/Group is a parent of /World/Group/Sphere
        assert_eq!(q.find_instance_by_prim_path("/World/Group"), Some(1));
    }

    #[test]
    fn find_instance_synthetic_fallback() {
        let mut scene = Scene::new("test");
        let mesh = Arc::new(Mesh::new(vec![], vec![], None));
        let proto_id = scene.add_prototype(mesh, "box");
        scene.add_instance_with_path(proto_id, Transform::default(), "/BIF/World/Box/0");

        let q: &dyn SceneQuery = &scene;
        assert_eq!(q.find_instance_by_prim_path("/World/Box"), Some(0));
    }

    #[test]
    fn find_instance_not_found() {
        let scene = make_test_scene();
        let q: &dyn SceneQuery = &scene;
        assert_eq!(q.find_instance_by_prim_path("/Nonexistent"), None);
    }

    #[test]
    fn query_materials_empty() {
        let scene = Scene::new("empty");
        let q: &dyn SceneQuery = &scene;
        assert_eq!(q.material_count(), 0);
        assert!(q.materials().is_empty());
        assert!(q.material(0).is_none());
    }

    #[test]
    fn query_collections_empty() {
        let scene = Scene::new("empty");
        let q: &dyn SceneQuery = &scene;
        assert!(q.cameras().is_empty());
        assert!(q.point_clouds().is_empty());
        assert!(q.lights().is_empty());
        assert!(q.timeline().is_none());
        assert!(q.stage_metadata().is_none());
        assert!(!q.has_animation());
        assert_eq!(q.total_triangle_count(), 0);
    }

    #[test]
    fn material_for_prototype_none_by_default() {
        let scene = make_test_scene();
        let q: &dyn SceneQuery = &scene;
        // Default prototypes have no material
        assert!(q.material_for_prototype(0).is_none());
    }
}
