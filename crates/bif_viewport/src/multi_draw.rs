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
        }
    }

    /// Reset to empty state (for scene reload).
    pub fn clear(&mut self) {
        self.prototype_gpu_data.clear();
        self.instance_groups.clear();
        self.enabled = false;
    }

    /// Rebuild instance groups from transforms and prototype IDs.
    ///
    /// Called during animation to update instance transforms for each prototype.
    pub fn rebuild_instance_groups(
        &mut self,
        transforms: &[Mat4],
        prototype_ids: &[usize],
        material_ids: &[u32],
    ) {
        self.instance_groups.clear();

        for (i, model_matrix) in transforms.iter().enumerate() {
            let prototype_id = prototype_ids.get(i).copied().unwrap_or(0);
            let material_id = material_ids.get(i).copied().unwrap_or(0);

            self.instance_groups
                .entry(prototype_id)
                .or_default()
                .push(InstanceData {
                    model_matrix: model_matrix.to_cols_array_2d(),
                    material_id,
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

        state.rebuild_instance_groups(&transforms, &prototype_ids, &material_ids);

        assert_eq!(state.instance_groups.get(&0).map(|v| v.len()), Some(2));
        assert_eq!(state.instance_groups.get(&1).map(|v| v.len()), Some(1));
    }
}
