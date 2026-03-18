//! Scene ownership — geometry, instances, materials, USD stage, undo/redo.
//!
//! Framework-agnostic: no egui imports. Owns all scene data that was
//! previously spread across Renderer fields.

use std::path::PathBuf;
use std::sync::Arc;

use bif_core::usd::UsdStage;
use bif_core::{AnimatedTransform, EditState, Material, SceneCamera, UndoStack};
use bif_math::Mat4;

use crate::gpu_types::InstanceData;
use crate::mesh_data::MeshData;
use crate::SceneInstances;

/// Owns all scene data: geometry, instances, materials, USD stage, undo/redo.
pub struct SceneManager {
    /// Persistent working scene that accumulates all primitives and USD objects.
    pub working_scene: bif_core::Scene,

    /// Per-instance parallel arrays (transforms, materials, prototype IDs, prim paths).
    pub instances: SceneInstances,

    /// Animated transforms for instances (parallel to instances).
    pub instance_animations: Vec<Option<AnimatedTransform>>,
    /// Last evaluated frame (for change detection).
    pub last_evaluated_frame: f64,
    /// Mesh indices that have vertex animation (deformation).
    pub vertex_animated_meshes: Vec<usize>,

    /// Reusable buffers for animation evaluation (avoid per-frame allocation).
    pub anim_instances_buf: Vec<InstanceData>,
    pub anim_transforms_buf: Vec<Mat4>,

    /// Default material for Ivar rendering (from loaded USD scene).
    pub scene_material: Material,
    /// All scene materials for multi-material Ivar rendering.
    pub scene_materials: Vec<Arc<Material>>,
    /// Base directory for resolving texture paths.
    pub texture_base_dir: Option<PathBuf>,

    /// Cached mesh data for Ivar scene building.
    pub mesh_data: MeshData,

    /// USD stage for scene browser hierarchy (None if loaded via pure Rust parser).
    /// Wrapped in Arc for sharing with batch render thread.
    pub usd_stage: Option<Arc<UsdStage>>,
    /// Path to the currently loaded USD file (for sublayer export).
    pub loaded_usd_path: Option<String>,

    /// Scene cameras (from Camera primitives).
    pub scene_cameras: Vec<SceneCamera>,

    /// Undo stack for reversible editing commands.
    pub undo_stack: UndoStack,
    /// Edit state with transform overrides.
    pub edit_state: EditState,
}

impl SceneManager {
    /// Create with empty scene.
    pub fn new() -> Self {
        Self {
            working_scene: bif_core::Scene::new("Working"),
            instances: SceneInstances::default(),
            instance_animations: vec![],
            last_evaluated_frame: -1.0,
            vertex_animated_meshes: vec![],
            anim_instances_buf: vec![],
            anim_transforms_buf: vec![],
            scene_material: Material::default(),
            scene_materials: vec![],
            texture_base_dir: None,
            mesh_data: MeshData::default(),
            usd_stage: None,
            loaded_usd_path: None,
            scene_cameras: vec![],
            undo_stack: UndoStack::new(),
            edit_state: EditState::default(),
        }
    }
}

impl Default for SceneManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_scene_manager_is_empty() {
        let sm = SceneManager::new();
        assert!(sm.instances.transforms.is_empty());
        assert!(sm.usd_stage.is_none());
        assert!(sm.scene_cameras.is_empty());
        assert_eq!(sm.working_scene.prototype_count(), 0);
    }
}
