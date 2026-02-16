//! Culling and LOD management for viewport rendering.
//!
//! Extracted from Renderer to reduce monolithic struct size.

use wgpu::util::DeviceExt;

use bif_math::{Aabb, Camera, Frustum, Mat4, Mat4Ext};

use crate::gpu_types::{CullingScratch, InstanceData};
use crate::ivar_state::CameraSnapshot;
use crate::mesh_data::MeshData;

/// Result from culling update.
pub struct CullingResult {
    /// Number of instances rendered with full mesh (near)
    pub near_count: u32,
    /// Number of instances rendered with box proxy (far)
    pub far_count: u32,
}

/// Manages frustum culling and LOD selection for viewport rendering.
pub struct CullingManager {
    /// Precomputed world-space AABBs for each instance (for frustum culling)
    pub instance_aabbs: Vec<Aabb>,
    /// Local-space AABB of the prototype mesh
    pub prototype_aabb: Aabb,
    /// Number of visible instances after frustum culling (updated per frame)
    pub visible_count: u32,
    /// Pre-allocated scratch buffers for frustum culling
    scratch: CullingScratch,
    /// Cached frustum (recomputed only when camera changes)
    cached_frustum: Frustum,
    /// Camera snapshot for frustum cache invalidation
    camera_snapshot: CameraSnapshot,

    // LOD state
    /// Maximum polygon budget before LOD kicks in (user-adjustable)
    pub lod_max_polys: u32,
    /// Number of instances rendered as box proxies
    pub lod_box_count: u32,
    /// Triangles per instance (for polygon budget calculation)
    pub triangles_per_instance: u32,
    /// Box proxy vertex buffer (generated from prototype AABB)
    lod_box_vertex_buffer: wgpu::Buffer,
    /// Box proxy index buffer
    lod_box_index_buffer: wgpu::Buffer,
    /// Number of indices in box proxy mesh (36 = 12 triangles)
    lod_box_num_indices: u32,
    /// Maximum instance capacity of the GPU instance buffer.
    max_instances: usize,
}

impl CullingManager {
    /// Create a new CullingManager with default empty state.
    pub fn new(device: &wgpu::Device, camera: &Camera, max_instances: usize) -> Self {
        let dummy_aabb = Aabb::empty();
        let lod_box_mesh = MeshData::from_aabb(&dummy_aabb);

        let lod_box_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Box Vertex Buffer"),
            contents: bytemuck::cast_slice(&lod_box_mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let lod_box_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Box Index Buffer"),
            contents: bytemuck::cast_slice(&lod_box_mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let vp = camera.projection_matrix() * camera.view_matrix();

        Self {
            instance_aabbs: Vec::new(),
            prototype_aabb: Aabb::empty(),
            visible_count: 0,
            scratch: CullingScratch::new(max_instances),
            cached_frustum: Frustum::from_view_projection(vp),
            camera_snapshot: CameraSnapshot::from_camera(camera),
            lod_max_polys: 5_000_000, // 5M poly budget default
            lod_box_count: 0,
            triangles_per_instance: 0,
            lod_box_vertex_buffer,
            lod_box_index_buffer,
            lod_box_num_indices: lod_box_mesh.indices.len() as u32,
            max_instances,
        }
    }

    /// Set the prototype AABB and regenerate LOD box geometry.
    pub fn set_prototype_aabb(
        &mut self,
        device: &wgpu::Device,
        aabb: Aabb,
        tris_per_instance: u32,
    ) {
        self.prototype_aabb = aabb;
        self.triangles_per_instance = tris_per_instance;

        // Regenerate LOD box mesh for new prototype AABB
        let lod_box_mesh = MeshData::from_aabb(&aabb);
        self.lod_box_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Box Vertex Buffer"),
            contents: bytemuck::cast_slice(&lod_box_mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        self.lod_box_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Box Index Buffer"),
            contents: bytemuck::cast_slice(&lod_box_mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.lod_box_num_indices = lod_box_mesh.indices.len() as u32;

        log::debug!(
            "Set prototype AABB: min={:?}, max={:?}",
            aabb.min_point(),
            aabb.max_point()
        );
    }

    /// Update instance AABBs from transforms.
    pub fn update_instance_aabbs(&mut self, transforms: &[Mat4]) {
        let prototype_aabb = self.prototype_aabb;
        self.instance_aabbs = transforms
            .iter()
            .map(|t| t.transform_aabb(&prototype_aabb))
            .collect();
    }

    /// Invalidate frustum cache (force recompute next frame).
    pub fn invalidate_frustum(&mut self) {
        self.camera_snapshot = CameraSnapshot::default();
    }

    /// Perform frustum culling and LOD selection, writing visible instances to GPU buffer.
    ///
    /// Returns culling result with near/far instance counts.
    pub fn update_visible_instances(
        &mut self,
        queue: &wgpu::Queue,
        instance_buffer: &wgpu::Buffer,
        camera: &Camera,
        transforms: &[Mat4],
        material_ids: &[u32],
    ) -> CullingResult {
        if self.instance_aabbs.is_empty() {
            self.visible_count = transforms.len() as u32;
            self.lod_box_count = 0;
            return CullingResult {
                near_count: self.visible_count,
                far_count: 0,
            };
        }

        // Clear scratch buffers (reuse pre-allocated capacity)
        self.scratch.clear();

        // Update cached frustum only when camera changes
        let current_snapshot = CameraSnapshot::from_camera(camera);
        if current_snapshot.has_changed(&self.camera_snapshot) {
            let vp = camera.projection_matrix() * camera.view_matrix();
            self.cached_frustum = Frustum::from_view_projection(vp);
            self.camera_snapshot = current_snapshot;
        }

        let camera_pos = camera.position;

        // Collect visible instances with their distances
        for (idx, aabb) in self.instance_aabbs.iter().enumerate() {
            if !self.cached_frustum.intersects_aabb(aabb) {
                continue;
            }
            let instance_center = aabb.center();
            let distance_sq = (instance_center - camera_pos).length_squared();
            self.scratch.visible_with_distance.push((distance_sq, idx));
        }

        // Calculate how many instances fit in polygon budget
        let tris_per_instance = self.triangles_per_instance as u64;
        let max_polys = self.lod_max_polys as u64;
        let budget_count = if tris_per_instance > 0 {
            (max_polys / tris_per_instance) as usize
        } else {
            self.scratch.visible_with_distance.len()
        };

        let visible_count = self.scratch.visible_with_distance.len();

        // Partition: O(n) instead of O(n log n) full sort
        if budget_count > 0 && budget_count < visible_count {
            self.scratch
                .visible_with_distance
                .select_nth_unstable_by(budget_count, |a, b| {
                    a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                });
        }

        // Split into near (full mesh) and far (box proxy)
        let split_point = budget_count.min(visible_count);

        for &(_distance_sq, idx) in &self.scratch.visible_with_distance[..split_point] {
            let transform = &transforms[idx];
            let material_id = material_ids.get(idx).copied().unwrap_or(0);
            self.scratch.near_instances.push(InstanceData {
                model_matrix: transform.to_cols_array_2d(),
                material_id,
            });
        }

        for &(_distance_sq, idx) in &self.scratch.visible_with_distance[split_point..] {
            let transform = &transforms[idx];
            let material_id = material_ids.get(idx).copied().unwrap_or(0);
            self.scratch.far_instances.push(InstanceData {
                model_matrix: transform.to_cols_array_2d(),
                material_id,
            });
        }

        // Update GPU buffer: [near_instances... | far_instances...] in single contiguous write
        // Clamp to buffer capacity to avoid overflow
        let near_count = self.scratch.near_instances.len().min(self.max_instances);
        let remaining = self.max_instances.saturating_sub(near_count);
        let far_count = self.scratch.far_instances.len().min(remaining);

        if near_count > 0 || far_count > 0 {
            if near_count > 0 {
                queue.write_buffer(
                    instance_buffer,
                    0,
                    bytemuck::cast_slice(&self.scratch.near_instances[..near_count]),
                );
            }
            if far_count > 0 {
                let far_offset = (near_count * std::mem::size_of::<InstanceData>()) as u64;
                queue.write_buffer(
                    instance_buffer,
                    far_offset,
                    bytemuck::cast_slice(&self.scratch.far_instances[..far_count]),
                );
            }
        }

        self.visible_count = near_count as u32;
        self.lod_box_count = far_count as u32;

        log::trace!(
            "LOD split: {} near (full mesh), {} far (box LOD), {}/{} total visible",
            self.visible_count,
            self.lod_box_count,
            self.visible_count + self.lod_box_count,
            transforms.len()
        );

        CullingResult {
            near_count: self.visible_count,
            far_count: self.lod_box_count,
        }
    }

    /// Get LOD box vertex buffer slice for rendering.
    pub fn lod_box_vertex_buffer(&self) -> &wgpu::Buffer {
        &self.lod_box_vertex_buffer
    }

    /// Get LOD box index buffer slice for rendering.
    pub fn lod_box_index_buffer(&self) -> &wgpu::Buffer {
        &self.lod_box_index_buffer
    }

    /// Get number of indices in LOD box mesh.
    pub fn lod_box_num_indices(&self) -> u32 {
        self.lod_box_num_indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_culling_result_fields() {
        let result = CullingResult {
            near_count: 10,
            far_count: 5,
        };
        assert_eq!(result.near_count, 10);
        assert_eq!(result.far_count, 5);
    }
}
