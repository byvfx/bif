//! Frustum culling and LOD selection for viewport instancing.
//!
//! Provides efficient per-frame culling and distance-based LOD selection.

use bif_math::{Aabb, Frustum, Mat4, Vec3};

use crate::gpu_types::{CullingScratch, InstanceData};

/// Result of frustum culling and LOD selection.
pub struct CullingResult {
    /// Number of instances rendered with full mesh (near).
    pub near_count: u32,
    /// Number of instances rendered with box proxy (far).
    pub far_count: u32,
}

/// Perform frustum culling and LOD selection on instances.
///
/// This function:
/// 1. Culls instances outside the view frustum
/// 2. Sorts visible instances by distance from camera
/// 3. Fills polygon budget with full mesh, rest become box LOD
///
/// Uses `lod_max_polys` as the polygon budget - nearest instances get full
/// mesh until budget is exhausted, then remaining use box proxy.
///
/// # Arguments
/// * `scratch` - Reusable scratch buffers to avoid per-frame allocations
/// * `frustum` - Current view frustum for culling
/// * `camera_pos` - Camera position for distance calculation
/// * `instance_aabbs` - Pre-computed world-space AABBs for each instance
/// * `instance_transforms` - Transform matrices for each instance
/// * `instance_material_ids` - Material IDs for each instance
/// * `triangles_per_instance` - Number of triangles in the prototype mesh
/// * `lod_max_polys` - Maximum polygon budget
///
/// # Returns
/// `CullingResult` with counts of near (full mesh) and far (box proxy) instances.
/// The `scratch` buffer is filled with `near_instances` and `far_instances`.
#[allow(clippy::too_many_arguments)]
pub fn update_visible_instances(
    scratch: &mut CullingScratch,
    frustum: &Frustum,
    camera_pos: Vec3,
    instance_aabbs: &[Aabb],
    instance_transforms: &[Mat4],
    instance_material_ids: &[u32],
    triangles_per_instance: u32,
    lod_max_polys: u32,
) -> CullingResult {
    // Clear scratch buffers (reuse pre-allocated capacity)
    scratch.clear();

    if instance_aabbs.is_empty() {
        return CullingResult {
            near_count: 0,
            far_count: 0,
        };
    }

    // Collect visible instances with their distances
    for (idx, aabb) in instance_aabbs.iter().enumerate() {
        // Frustum culling first
        if !frustum.intersects_aabb(aabb) {
            continue;
        }

        // Calculate distance for sorting
        let instance_center = aabb.center();
        let distance_sq = (instance_center - camera_pos).length_squared();
        scratch.visible_with_distance.push((distance_sq, idx));
    }

    // Calculate how many instances fit in polygon budget
    let tris_per_instance = triangles_per_instance as u64;
    let max_polys = lod_max_polys as u64;
    let budget_count = if tris_per_instance > 0 {
        (max_polys / tris_per_instance) as usize
    } else {
        scratch.visible_with_distance.len()
    };

    let visible_count = scratch.visible_with_distance.len();

    // Partition: O(n) instead of O(n log n) full sort
    // After this, indices 0..budget_count are the nearest (unordered among themselves)
    if budget_count > 0 && budget_count < visible_count {
        scratch
            .visible_with_distance
            .select_nth_unstable_by(budget_count, |a, b| {
                a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
            });
    }

    // Split into near (full mesh) and far (box proxy)
    let split_point = budget_count.min(visible_count);

    for &(_distance_sq, idx) in &scratch.visible_with_distance[..split_point] {
        let transform = &instance_transforms[idx];
        let material_id = instance_material_ids.get(idx).copied().unwrap_or(0);
        scratch.near_instances.push(InstanceData {
            model_matrix: transform.to_cols_array_2d(),
            material_id,
            tri_mat_offset: 0,
        });
    }

    for &(_distance_sq, idx) in &scratch.visible_with_distance[split_point..] {
        let transform = &instance_transforms[idx];
        let material_id = instance_material_ids.get(idx).copied().unwrap_or(0);
        scratch.far_instances.push(InstanceData {
            model_matrix: transform.to_cols_array_2d(),
            material_id,
            tri_mat_offset: 0,
        });
    }

    let near_count = scratch.near_instances.len() as u32;
    let far_count = scratch.far_instances.len() as u32;

    log::trace!(
        "LOD split: {} near (full mesh), {} far (box LOD), {}/{} total visible",
        near_count,
        far_count,
        near_count + far_count,
        instance_aabbs.len()
    );

    CullingResult {
        near_count,
        far_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bif_math::{Aabb, Frustum, Mat4, Vec3};

    #[test]
    fn test_empty_instances() {
        let mut scratch = CullingScratch::new(100);
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
        let result = update_visible_instances(
            &mut scratch,
            &frustum,
            Vec3::ZERO,
            &[],
            &[],
            &[],
            100,
            10000,
        );
        assert_eq!(result.near_count, 0);
        assert_eq!(result.far_count, 0);
    }

    /// Create a perspective frustum looking down -Z.
    fn create_test_frustum() -> Frustum {
        let view = Mat4::look_at_rh(Vec3::ZERO, -Vec3::Z, Vec3::Y);
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 1000.0);
        Frustum::from_view_projection(proj * view)
    }

    #[test]
    fn test_all_near_within_budget() {
        let mut scratch = CullingScratch::new(100);
        let frustum = create_test_frustum();
        // Place AABBs in front of camera (negative Z)
        let aabbs = vec![
            Aabb::from_points(Vec3::new(-0.5, -0.5, -5.5), Vec3::new(0.5, 0.5, -4.5)),
            Aabb::from_points(Vec3::new(-0.5, -0.5, -10.5), Vec3::new(0.5, 0.5, -9.5)),
        ];
        let transforms = vec![Mat4::IDENTITY; 2];
        let material_ids = vec![0u32; 2];

        let result = update_visible_instances(
            &mut scratch,
            &frustum,
            Vec3::ZERO,
            &aabbs,
            &transforms,
            &material_ids,
            100,
            10000,
        );
        // Both should be visible and within budget
        assert_eq!(result.near_count + result.far_count, 2);
        assert_eq!(result.near_count, 2);
    }

    #[test]
    fn test_lod_split_by_budget() {
        let mut scratch = CullingScratch::new(100);
        let frustum = create_test_frustum();
        // 10 instances at increasing distances
        let aabbs: Vec<_> = (0..10)
            .map(|i| {
                let z = -((i as f32 + 1.0) * 10.0);
                Aabb::from_points(Vec3::new(-0.5, -0.5, z - 0.5), Vec3::new(0.5, 0.5, z + 0.5))
            })
            .collect();
        let transforms = vec![Mat4::IDENTITY; 10];
        let material_ids = vec![0u32; 10];

        // Budget allows 3 full-mesh instances (300 tris / 100 tris each)
        let result = update_visible_instances(
            &mut scratch,
            &frustum,
            Vec3::ZERO,
            &aabbs,
            &transforms,
            &material_ids,
            100,
            300,
        );
        assert_eq!(result.near_count, 3);
        assert_eq!(result.far_count, 7);
    }

    #[test]
    fn test_culled_outside_frustum() {
        let mut scratch = CullingScratch::new(100);
        let frustum = create_test_frustum();
        // Place AABB behind the camera (positive Z) - should be culled
        let aabbs = vec![Aabb::from_points(
            Vec3::new(-0.5, -0.5, 4.5),
            Vec3::new(0.5, 0.5, 5.5),
        )];
        let transforms = vec![Mat4::IDENTITY];
        let material_ids = vec![0u32];

        let result = update_visible_instances(
            &mut scratch,
            &frustum,
            Vec3::ZERO,
            &aabbs,
            &transforms,
            &material_ids,
            100,
            10000,
        );
        assert_eq!(result.near_count + result.far_count, 0);
    }
}
