//! Multi-draw state for per-prototype rendering.
//!
//! Extracted from Renderer to reduce monolithic struct size.

use std::collections::HashMap;

use crate::gpu_types::{InstanceData, PrototypeGpuData};
use bif_math::Mat4;

/// State for multi-draw rendering (multiple prototypes with per-instance transforms).
///
/// Instead of combining all meshes into a single buffer, each prototype has its
/// own vertex/index buffers. This enables:
/// - Per-instance transforms (no baked transforms)
/// - Simpler vertex animation (update one prototype's buffer)
/// - Multiple draw calls indexed by prototype
pub struct MultiDrawState {
    /// Per-prototype GPU buffers (vertex/index) for multi-draw rendering
    pub prototype_gpu_data: Vec<PrototypeGpuData>,
    /// Instances grouped by prototype ID for multi-draw
    pub instance_groups: HashMap<usize, Vec<InstanceData>>,
    /// Whether using multi-draw mode (multiple prototypes) vs single buffer
    pub enabled: bool,
    /// Cached prototype_id -> tri_mat_offset for O(1) lookup during instance grouping
    tri_mat_offset_map: HashMap<usize, u32>,
}

impl Default for MultiDrawState {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiDrawState {
    /// Create empty multi-draw state.
    pub fn new() -> Self {
        Self {
            prototype_gpu_data: Vec::new(),
            instance_groups: HashMap::new(),
            enabled: false,
            tri_mat_offset_map: HashMap::new(),
        }
    }

    /// Reset to empty state (for scene reload).
    pub fn clear(&mut self) {
        self.prototype_gpu_data.clear();
        self.instance_groups.clear();
        self.enabled = false;
        self.tri_mat_offset_map.clear();
    }

    /// Rebuild the prototype_id -> tri_mat_offset lookup map.
    ///
    /// Call this whenever `prototype_gpu_data` changes (scene load, not per-frame).
    pub fn rebuild_tri_mat_offset_map(&mut self) {
        self.tri_mat_offset_map.clear();
        for p in &self.prototype_gpu_data {
            self.tri_mat_offset_map
                .insert(p.prototype_id, p.tri_mat_offset);
        }
    }

    /// Rebuild instance groups from transforms and prototype IDs.
    ///
    /// Called during animation to update instance transforms for each prototype.
    /// Uses cached `tri_mat_offset_map` for O(1) lookup per instance.
    pub fn rebuild_instance_groups(
        &mut self,
        transforms: &[Mat4],
        prototype_ids: &[usize],
        material_ids: &[u32],
        purposes: &[bif_core::Purpose],
        purpose_mode: crate::PurposeMode,
    ) {
        self.instance_groups.clear();

        // Rebuild offset map if stale (prototype_gpu_data changed without explicit rebuild)
        if self.tri_mat_offset_map.len() != self.prototype_gpu_data.len() {
            self.rebuild_tri_mat_offset_map();
        }

        for (i, model_matrix) in transforms.iter().enumerate() {
            // Skip instances not matching active purpose mode
            if let Some(&purpose) = purposes.get(i) {
                if !purpose_mode.includes(purpose) {
                    continue;
                }
            }

            let prototype_id = prototype_ids.get(i).copied().unwrap_or(0);
            let material_id = material_ids.get(i).copied().unwrap_or(0);

            self.instance_groups
                .entry(prototype_id)
                .or_default()
                .push(InstanceData {
                    model_matrix: model_matrix.to_cols_array_2d(),
                    material_id,
                    tri_mat_offset: self
                        .tri_mat_offset_map
                        .get(&prototype_id)
                        .copied()
                        .unwrap_or(0),
                });
        }
    }

    /// Update vertex buffer for meshes with vertex animation (deformation).
    ///
    /// Returns true if any updates were made.
    pub fn update_vertex_animation(
        &mut self,
        queue: &wgpu::Queue,
        vertex_animated_meshes: &[usize],
        get_positions: impl Fn(usize, f64) -> Option<Vec<f32>>,
        frame: f64,
    ) -> bool {
        if !self.enabled {
            return false;
        }

        log::debug!(
            "update_vertex_animation: enabled=true, vertex_animated_meshes={:?}",
            vertex_animated_meshes
        );

        let mut updated_any = false;

        for &mesh_idx in vertex_animated_meshes {
            log::debug!(
                "Looking for prototype with mesh_idx={}, available: {:?}",
                mesh_idx,
                self.prototype_gpu_data
                    .iter()
                    .map(|p| p.mesh_idx)
                    .collect::<Vec<_>>()
            );

            // Find the prototype GPU data for this mesh
            let proto_idx = match self
                .prototype_gpu_data
                .iter()
                .position(|p| p.mesh_idx == mesh_idx)
            {
                Some(idx) => idx,
                None => {
                    log::warn!(
                        "No prototype found for vertex-animated mesh_idx={}",
                        mesh_idx
                    );
                    continue;
                }
            };

            let positions = match get_positions(mesh_idx, frame) {
                Some(p) => p,
                None => continue,
            };

            let vertex_count = positions.len() / 3;
            let proto_data = &mut self.prototype_gpu_data[proto_idx];

            log::debug!(
                "Got {} vertices for mesh {} at frame {}, proto has {} vertices",
                vertex_count,
                mesh_idx,
                frame,
                proto_data.num_vertices
            );

            if vertex_count != proto_data.num_vertices as usize {
                log::warn!(
                    "Vertex count mismatch for mesh {}: USD {} vs proto {}",
                    mesh_idx,
                    vertex_count,
                    proto_data.num_vertices
                );
                continue;
            }

            // Update positions while preserving original normals/UVs
            for (i, vertex) in proto_data.vertices.iter_mut().enumerate() {
                vertex.position = [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
            }

            log::debug!(
                "Writing {} vertices to prototype {} buffer",
                vertex_count,
                proto_idx
            );
            queue.write_buffer(
                &proto_data.vertex_buffer,
                0,
                bytemuck::cast_slice(&proto_data.vertices),
            );

            updated_any = true;
        }

        updated_any
    }

    /// Update per-prototype vertex buffers for UsdSkel-bound meshes at the given frame.
    ///
    /// v0.13.5 Phase 3 (multi-draw path). For each `SkinnedMeshEntry`:
    ///   1. Fetch joint-skel transforms at `frame` via the caller-supplied closure
    ///      (typically `stage.compute_skel_xforms(skel_idx, t)`).
    ///   2. Build the skinning palette (`joint_skel * inv_bind * geom_bind`).
    ///   3. Run CPU LBS into the entry's scratch buffer.
    ///   4. Write skinned positions into the matching `PrototypeGpuData.vertices`
    ///      (found by `prototype_id`) and re-upload the vertex buffer.
    ///
    /// Leaves normals untouched — Phase 3 position-only pass. Blend shapes and
    /// skinned-normal GPU upload land in v0.13.6.
    ///
    /// Returns true if any updates were made.
    pub fn update_skinning(
        &mut self,
        queue: &wgpu::Queue,
        skinned_meshes: &mut [crate::scene_manager::SkinnedMeshEntry],
        get_joint_xforms: impl Fn(usize, f64) -> Option<Vec<Mat4>>,
        frame: f64,
    ) -> bool {
        if !self.enabled || skinned_meshes.is_empty() {
            return false;
        }

        let mut updated_any = false;

        for entry in skinned_meshes.iter_mut() {
            let Some(joint_xforms) = get_joint_xforms(entry.skel_idx, frame) else {
                continue;
            };

            let palette = bif_core::skinning::compute_skin_matrices(&entry.skin, &joint_xforms);
            if palette.is_empty() {
                continue;
            }

            // Ensure scratch buffer matches bind positions.
            if entry.skinned_scratch.len() != entry.bind_positions.len() {
                entry
                    .skinned_scratch
                    .resize(entry.bind_positions.len(), bif_math::Vec3::ZERO);
            }
            bif_core::skinning::skin_positions(
                &entry.skin,
                &entry.bind_positions,
                &palette,
                &mut entry.skinned_scratch,
            );

            // Find per-prototype GPU data by prototype_id (multi-draw registers
            // every prototype exactly once, so this is unique).
            let Some(proto_data) = self
                .prototype_gpu_data
                .iter_mut()
                .find(|p| p.prototype_id == entry.proto_id)
            else {
                log::warn!(
                    "update_skinning: no PrototypeGpuData for proto_id={} (skel_idx={})",
                    entry.proto_id,
                    entry.skel_idx
                );
                continue;
            };

            let gpu_vert_count = proto_data.vertices.len();
            let skinned_count = entry.skinned_scratch.len();
            if gpu_vert_count != skinned_count {
                log::warn!(
                    "update_skinning: vertex count mismatch for proto {}: \
                     gpu={gpu_vert_count}, skinned={skinned_count}; skipping",
                    entry.proto_id
                );
                continue;
            }

            for (i, vertex) in proto_data.vertices.iter_mut().enumerate() {
                let p = entry.skinned_scratch[i];
                vertex.position = [p.x, p.y, p.z];
            }

            queue.write_buffer(
                &proto_data.vertex_buffer,
                0,
                bytemuck::cast_slice(&proto_data.vertices),
            );
            updated_any = true;
        }

        updated_any
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state_is_disabled() {
        let state = MultiDrawState::new();
        assert!(!state.enabled);
        assert!(state.prototype_gpu_data.is_empty());
        assert!(state.instance_groups.is_empty());
    }

    #[test]
    fn test_clear_resets_state() {
        let mut state = MultiDrawState::new();
        state.enabled = true;
        state.instance_groups.insert(0, vec![]);
        state.clear();
        assert!(!state.enabled);
        assert!(state.instance_groups.is_empty());
    }

    #[test]
    fn test_rebuild_instance_groups() {
        let mut state = MultiDrawState::new();

        let transforms = vec![Mat4::IDENTITY, Mat4::IDENTITY, Mat4::IDENTITY];
        let prototype_ids = vec![0, 1, 0]; // 2 instances of proto 0, 1 of proto 1
        let material_ids = vec![0, 1, 2];

        let purposes = vec![bif_core::Purpose::Default; 3];
        state.rebuild_instance_groups(
            &transforms,
            &prototype_ids,
            &material_ids,
            &purposes,
            crate::PurposeMode::Render,
        );

        assert_eq!(state.instance_groups.get(&0).map(|v| v.len()), Some(2));
        assert_eq!(state.instance_groups.get(&1).map(|v| v.len()), Some(1));
    }

    #[test]
    fn test_rebuild_instance_groups_purpose_filter() {
        let mut state = MultiDrawState::new();

        let transforms = vec![Mat4::IDENTITY; 3];
        let prototype_ids = vec![0, 0, 0];
        let material_ids = vec![0, 0, 0];
        let purposes = vec![
            bif_core::Purpose::Default,
            bif_core::Purpose::Proxy,
            bif_core::Purpose::Render,
        ];

        // Render mode: Default + Render visible (skip Proxy)
        state.rebuild_instance_groups(
            &transforms,
            &prototype_ids,
            &material_ids,
            &purposes,
            crate::PurposeMode::Render,
        );
        assert_eq!(state.instance_groups.get(&0).map(|v| v.len()), Some(2));

        // Proxy mode: Default + Proxy visible (skip Render)
        state.rebuild_instance_groups(
            &transforms,
            &prototype_ids,
            &material_ids,
            &purposes,
            crate::PurposeMode::Proxy,
        );
        assert_eq!(state.instance_groups.get(&0).map(|v| v.len()), Some(2));

        // All mode: everything visible
        state.rebuild_instance_groups(
            &transforms,
            &prototype_ids,
            &material_ids,
            &purposes,
            crate::PurposeMode::All,
        );
        assert_eq!(state.instance_groups.get(&0).map(|v| v.len()), Some(3));
    }
}
