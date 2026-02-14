//! Point cloud type for instancing and scattering.
//!
//! A `PointCloud` stores positions and per-point attributes that expand
//! to `Instance` objects for rendering. This enables scatter-on-surface,
//! seed-based regeneration, and future USD export.

use bif_math::{Mat4, Quat, Vec3};

use crate::scene::{Instance, Transform};

/// How the point positions were generated.
#[derive(Clone, Debug)]
pub enum DistributionMethod {
    /// Loaded from a USD PointInstancer prim.
    UsdPointInstancer { path: String },
    /// Random scatter on a mesh surface.
    RandomScatter { seed: u64, count: usize },
    /// Poisson disk scatter on a mesh surface.
    PoissonDisk { seed: u64, min_distance: f32 },
    /// Regular grid distribution.
    Grid { spacing: f32 },
    /// Spherical distribution (surface or volume).
    Sphere { seed: u64, on_surface: bool },
    /// Hand-painted (future).
    Painted,
    /// Manually placed points.
    Manual,
}

/// Per-point attribute arrays (all optional except proto_indices).
#[derive(Clone, Debug, Default)]
pub struct PointAttributes {
    /// Per-point scale (uniform or non-uniform).
    pub scales: Option<Vec<Vec3>>,
    /// Per-point orientation as quaternion.
    pub orientations: Option<Vec<Quat>>,
    /// Index into `prototype_ids` for each point.
    pub proto_indices: Vec<u32>,
    /// Per-point random ID (0..1) for variation.
    pub ids: Option<Vec<f32>>,
}

/// A point cloud that expands to instances for rendering.
#[derive(Clone, Debug)]
pub struct PointCloud {
    /// Unique identifier within the scene.
    pub id: usize,
    /// Display name.
    pub name: String,
    /// World-space positions for each point.
    pub positions: Vec<Vec3>,
    /// Per-point attributes.
    pub attributes: PointAttributes,
    /// Scene prototype IDs that this cloud references.
    pub prototype_ids: Vec<usize>,
    /// Parent transform applied to all points.
    pub transform: Transform,
    /// How the points were generated.
    pub distribution: DistributionMethod,
    /// Number of instances produced by last expand() call.
    /// Set by the caller after expansion; used to remove instances on undo.
    pub expanded_instance_count: usize,
}

impl PointCloud {
    /// Expand this point cloud into concrete instances.
    ///
    /// For each point, builds a transform matrix from position +
    /// optional scale/orientation, then maps proto_indices to prototype_ids.
    pub fn expand(&self) -> Vec<Instance> {
        let parent_mat = self.transform.to_matrix();
        let mut instances = Vec::with_capacity(self.positions.len());

        for (i, &pos) in self.positions.iter().enumerate() {
            let scale = self
                .attributes
                .scales
                .as_ref()
                .and_then(|s| s.get(i))
                .copied()
                .unwrap_or(Vec3::ONE);

            let rotation = self
                .attributes
                .orientations
                .as_ref()
                .and_then(|o| o.get(i))
                .copied()
                .unwrap_or(Quat::IDENTITY);

            let local_mat = Mat4::from_scale_rotation_translation(scale, rotation, pos);
            let world_mat = parent_mat * local_mat;
            let transform = Transform::from_matrix(world_mat);

            let proto_idx = self.attributes.proto_indices.get(i).copied().unwrap_or(0) as usize;

            let prototype_id = self
                .prototype_ids
                .get(proto_idx)
                .copied()
                .unwrap_or_else(|| self.prototype_ids.first().copied().unwrap_or(0));

            instances.push(Instance::new(prototype_id, transform));
        }

        instances
    }

    /// Number of points in this cloud.
    pub fn point_count(&self) -> usize {
        self.positions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cloud(
        positions: Vec<Vec3>,
        proto_indices: Vec<u32>,
        prototype_ids: Vec<usize>,
    ) -> PointCloud {
        PointCloud {
            id: 0,
            name: "test".into(),
            positions,
            attributes: PointAttributes {
                proto_indices,
                ..Default::default()
            },
            prototype_ids,
            transform: Transform::default(),
            distribution: DistributionMethod::Manual,
            expanded_instance_count: 0,
        }
    }

    #[test]
    fn expand_single_proto() {
        let cloud = make_cloud(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 0.0),
            ],
            vec![0, 0, 0, 0],
            vec![42],
        );

        let instances = cloud.expand();
        assert_eq!(instances.len(), 4);
        for inst in &instances {
            assert_eq!(inst.prototype_id, 42);
        }
        // Check positions match
        assert!((instances[0].transform.translation - Vec3::new(0.0, 0.0, 0.0)).length() < 0.001);
        assert!((instances[2].transform.translation - Vec3::new(2.0, 0.0, 0.0)).length() < 0.001);
    }

    #[test]
    fn expand_multi_proto() {
        let cloud = make_cloud(
            vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            vec![0, 1, 0],
            vec![10, 20],
        );

        let instances = cloud.expand();
        assert_eq!(instances.len(), 3);
        assert_eq!(instances[0].prototype_id, 10);
        assert_eq!(instances[1].prototype_id, 20);
        assert_eq!(instances[2].prototype_id, 10);
    }

    #[test]
    fn expand_with_scale_rotation() {
        let mut cloud = make_cloud(vec![Vec3::new(1.0, 0.0, 0.0)], vec![0], vec![0]);
        cloud.attributes.scales = Some(vec![Vec3::new(2.0, 2.0, 2.0)]);
        cloud.attributes.orientations =
            Some(vec![Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)]);

        let instances = cloud.expand();
        assert_eq!(instances.len(), 1);

        let t = &instances[0].transform;
        // Translation should still be (1,0,0)
        assert!((t.translation - Vec3::new(1.0, 0.0, 0.0)).length() < 0.01);
        // Scale should be (2,2,2)
        assert!((t.scale - Vec3::new(2.0, 2.0, 2.0)).length() < 0.01);
    }

    #[test]
    fn expand_empty() {
        let cloud = make_cloud(vec![], vec![], vec![0]);
        let instances = cloud.expand();
        assert!(instances.is_empty());
    }

    #[test]
    fn expand_with_parent_transform() {
        let mut cloud = make_cloud(vec![Vec3::new(1.0, 0.0, 0.0)], vec![0], vec![0]);
        cloud.transform = Transform::from_translation(Vec3::new(10.0, 0.0, 0.0));

        let instances = cloud.expand();
        assert_eq!(instances.len(), 1);
        // Point at (1,0,0) + parent offset (10,0,0) = (11,0,0)
        assert!((instances[0].transform.translation - Vec3::new(11.0, 0.0, 0.0)).length() < 0.01);
    }

    #[test]
    fn expand_proto_index_out_of_range_falls_back() {
        // proto_indices has index 5, but only 2 prototype_ids
        let cloud = make_cloud(vec![Vec3::ZERO], vec![5], vec![10, 20]);

        let instances = cloud.expand();
        assert_eq!(instances.len(), 1);
        // Should fall back to first prototype
        assert_eq!(instances[0].prototype_id, 10);
    }
}
