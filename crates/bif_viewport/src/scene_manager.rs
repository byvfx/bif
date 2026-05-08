//! Scene ownership — geometry, instances, materials, USD stage, undo/redo.
//!
//! Framework-agnostic: no egui imports. Owns all scene data that was
//! previously spread across Renderer fields.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bif_core::usd::UsdStage;
use bif_core::{AnimatedTransform, EditState, Material, SceneCamera, UndoStack};
use bif_math::{Mat4, Vec3};

/// A skinned prototype registered for per-frame CPU LBS re-evaluation.
///
/// Added in v0.13.5 Phase 3. The entry caches everything needed to run the
/// skinning pass without re-reading from `working_scene.prototypes` each
/// frame: the snapshot of bind-pose positions, the per-vertex joint binding,
/// and the skeleton index into the USD stage (so `compute_skel_xforms` can
/// be called for the current time code).
#[derive(Clone, Debug)]
pub struct SkinnedMeshEntry {
    /// Prototype index in `working_scene.prototypes`.
    pub proto_id: usize,
    /// USD mesh index, matching `MeshRange::usd_mesh_index`. Used to locate
    /// the correct slice of the combined vertex buffer in multi-mesh mode.
    pub mesh_idx: usize,
    /// Skeleton index in the USD stage's skeleton list
    /// (as returned by `UsdStage::skeleton_count`).
    pub skel_idx: usize,
    /// Bind-pose positions snapshot (cloned from the prototype mesh at
    /// registration time so the hot path avoids `Arc<Mesh>` indirection).
    pub bind_positions: Vec<Vec3>,
    /// Pre-inverted per-joint bind matrices (cloned from the mesh's
    /// SkinBinding) so the update path only needs a shallow field read.
    pub skin: bif_core::SkinBinding,
    /// Scratch buffer for per-frame skinned positions (keeps allocation
    /// out of the hot path).
    pub skinned_scratch: Vec<Vec3>,

    // -- v0.13.6 blend shapes --
    /// Blend shape binding (morph targets + FFI binding index). `None` when
    /// the prototype has no blend shapes.
    pub blend_shapes: Option<bif_core::BlendShapeBinding>,
    /// Bind-pose normals snapshot (for blend shape normal deltas). `None` when
    /// no blend shapes or mesh had no normals.
    pub bind_normals: Option<Vec<Vec3>>,
    /// Scratch buffer for blend-shape-deformed positions (fed into skinning).
    pub blend_scratch_pos: Vec<Vec3>,
    /// Scratch buffer for blend-shape-deformed normals.
    pub blend_scratch_norm: Option<Vec<Vec3>>,
}

use crate::gpu_types::InstanceData;
use crate::mesh_data::MeshData;
use crate::SceneInstances;

/// Owns all scene data: geometry, instances, materials, USD stage, undo/redo.
pub struct SceneManager {
    /// Persistent working scene that accumulates all primitives and USD objects.
    pub working_scene: bif_core::Scene,

    /// Computed invisible USD prim paths mirrored from the live stage for viewport filtering.
    pub hidden_prim_paths: std::collections::HashSet<String>,

    /// Per-instance parallel arrays (transforms, materials, prototype IDs, prim paths).
    pub instances: SceneInstances,

    /// Animated transforms for instances (parallel to instances).
    pub instance_animations: Vec<Option<AnimatedTransform>>,
    /// Last evaluated frame (for change detection).
    pub last_evaluated_frame: f64,
    /// Mesh indices that have vertex animation (deformation).
    pub vertex_animated_meshes: Vec<usize>,
    /// Prototypes with a UsdSkel binding — re-skinned each frame via CPU LBS.
    pub skinned_meshes: Vec<SkinnedMeshEntry>,

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
    /// Wrapped in Arc<Mutex> — UsdStage is not Sync (C++ mutations through &self).
    pub usd_stage: Option<Arc<Mutex<UsdStage>>>,
    /// Path to the currently loaded USD file (for sublayer export).
    pub loaded_usd_path: Option<String>,

    /// Scene cameras (from Camera primitives).
    pub scene_cameras: Vec<SceneCamera>,

    /// Undo stack for reversible editing commands.
    pub undo_stack: UndoStack,
    /// Edit state with transform overrides.
    pub edit_state: EditState,

    /// Layer-aware stage state (v0.14.0). Populated by the scene loader
    /// when a USD stage is present. `None` for procedural-only scenes or
    /// before any stage has been opened.
    ///
    /// Lives on `SceneManager` rather than `working_scene.layer_state`
    /// because `finalize_usd_scene` pulls fields out of the parsed
    /// `Scene` individually instead of storing the whole struct — any
    /// assignment onto `working_scene` would get discarded.
    pub layer_state: Option<bif_core::SceneLayerState>,
}

impl SceneManager {
    /// Create with empty scene.
    pub fn new() -> Self {
        Self {
            working_scene: bif_core::Scene::new("Working"),
            hidden_prim_paths: std::collections::HashSet::new(),
            instances: SceneInstances::default(),
            instance_animations: vec![],
            last_evaluated_frame: -1.0,
            vertex_animated_meshes: vec![],
            skinned_meshes: vec![],
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
            layer_state: None,
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
