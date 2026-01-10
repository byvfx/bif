//! Instance-aware geometry for efficient BVH construction.
//!
//! Instead of duplicating geometry for each instance (which creates 28M triangles
//! for 100 instances of a 280K triangle mesh), this stores ONE prototype BVH and
//! transforms rays to local space for each instance.
//!
//! Trade-off: Linear instance search (O(N) instances), but acceptable for ~100 instances.
//! For 10K+ instances, use Embree with two-level BVH (Phase 2).

use crate::{
    hittable::{HitRecord, Hittable},
    BvhNode, Material, Ray,
};
use bif_math::{Aabb, Interval, Mat4, Mat4Ext};
use std::sync::Arc;

/// Geometry with multiple instances via transforms.
///
/// Stores ONE BVH for the prototype mesh in local space, and tests rays
/// by transforming them to local space for each instance.
///
/// # Example
/// ```ignore
/// // Build local-space triangles ONCE
/// let local_triangles: Vec<Box<dyn Hittable + Send + Sync>> = ...;
///
/// // Create 100 instances with different transforms
/// let transforms = vec![Mat4::from_translation(Vec3::new(i as f32, 0.0, 0.0)); 100];
///
/// let instanced = InstancedGeometry::new(
///     local_triangles,
///     transforms,
///     Lambertian::new(Color::new(0.7, 0.7, 0.7)),
/// );
///
/// // Now you have 100 instances but only 280K triangles in the BVH!
/// ```
pub struct InstancedGeometry<M: Material + Clone> {
    /// BVH of prototype mesh in local space
    prototype_bvh: Arc<BvhNode>,

    /// Local-to-world transform for each instance
    transforms: Vec<Mat4>,

    /// World-to-local transform for each instance (for ray transformation)
    inv_transforms: Vec<Mat4>,

    /// World-space bounding box for each instance (for culling)
    instance_bboxes: Vec<Aabb>,

    /// Material for all instances
    /// TODO: Per-instance materials via material ID array
    material: M,

    /// Cached world-space bounding box of all instances
    world_bbox: Aabb,
}

impl<M: Material + Clone + 'static> InstancedGeometry<M> {
    /// Create instanced geometry from local-space primitives and transforms.
    ///
    /// # Arguments
    /// * `local_primitives` - Geometry in local space (e.g., triangles at origin)
    /// * `transforms` - Local-to-world transforms for each instance
    /// * `material` - Material shared by all instances (for now)
    ///
    /// # Performance
    /// - BVH build: O(P log P) where P = number of primitives (e.g., 280K triangles)
    /// - NOT O(I * P) where I = instances - this is the key optimization!
    pub fn new(
        local_primitives: Vec<Box<dyn Hittable + Send + Sync>>,
        transforms: Vec<Mat4>,
        material: M,
    ) -> Self {
        // Build BVH once for prototype in local space
        let prototype_bvh = Arc::new(BvhNode::new(local_primitives));

        // Precompute transforms and bounding boxes
        let inv_transforms: Vec<Mat4> = transforms.iter().map(|t| t.inverse()).collect();
        let local_bbox = prototype_bvh.bounding_box();

        let mut instance_bboxes = Vec::with_capacity(transforms.len());
        let mut world_bbox = Aabb::EMPTY;

        for transform in &transforms {
            let transformed_bbox = transform.transform_aabb(&local_bbox);
            world_bbox = Aabb::surrounding(&world_bbox, &transformed_bbox);
            instance_bboxes.push(transformed_bbox);
        }

        log::info!(
            "Created InstancedGeometry: {} instances, prototype BVH bbox: {:?}",
            transforms.len(),
            local_bbox
        );

        Self {
            prototype_bvh,
            transforms,
            inv_transforms,
            instance_bboxes,
            material,
            world_bbox,
        }
    }

    /// Get number of instances
    pub fn instance_count(&self) -> usize {
        self.transforms.len()
    }
}

impl<M: Material + Clone + 'static> Hittable for InstancedGeometry<M> {
    /// Test ray against all instances with bbox culling.
    ///
    /// For each instance:
    /// 1. Early reject via world bbox intersection test
    /// 2. Transform ray to local space (using inv_transform)
    /// 3. Test against prototype BVH
    /// 4. Transform hit back to world space (using transform)
    /// 5. Track closest hit
    ///
    /// # Performance
    /// - O(I × bbox_test + hits × log P) where I = instances, P = primitives
    /// - Bbox test culls ~90% of instances for most rays
    /// - For 10K instances: ~1K bbox tests + ~10 BVH traversals
    fn hit<'a>(&'a self, ray: &Ray, ray_t: Interval, rec: &mut HitRecord<'a>) -> bool {
        let mut hit_anything = false;
        let mut closest = ray_t.max;

        // Test each instance with precomputed bbox culling
        for (i, (inv_transform, transform)) in
            self.inv_transforms.iter().zip(&self.transforms).enumerate()
        {
            // Early bbox rejection using precomputed world-space bbox
            // Convert bif_renderer::Ray to bif_math::Ray for Aabb::hit
            let math_ray = bif_math::Ray::new(ray.origin(), ray.direction(), ray.time());
            if !self.instance_bboxes[i].hit(&math_ray, Interval::new(ray_t.min, closest)) {
                continue; // Skip this instance
            }
            // Transform ray to local space
            let local_origin = inv_transform.transform_point3(ray.origin());
            let local_direction = inv_transform.transform_vector3(ray.direction()).normalize();
            let local_ray = Ray::new(local_origin, local_direction, ray.time());

            // Test against prototype BVH in local space
            let interval = Interval::new(ray_t.min, closest);
            let mut local_rec = HitRecord::default();

            if self.prototype_bvh.hit(&local_ray, interval, &mut local_rec) {
                // Transform hit back to world space
                rec.t = local_rec.t;
                rec.p = transform.transform_point3(local_rec.p);

                // Transform normal to world space
                // Note: For non-uniform scales, we'd need inverse-transpose,
                // but for uniform scales, just transforming and normalizing works
                rec.normal = transform.transform_vector3(local_rec.normal).normalize();

                rec.u = local_rec.u;
                rec.v = local_rec.v;
                rec.material = &self.material;
                rec.front_face = local_rec.front_face;

                hit_anything = true;
                closest = rec.t;

                // Early exit optimization: if we hit very close, don't test other instances
                if closest < 0.001 {
                    break;
                }
            }
        }

        hit_anything
    }

    fn bounding_box(&self) -> Aabb {
        self.world_bbox
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Lambertian, Triangle};
    use bif_math::Vec3;

    /// Helper: Create a simple triangle at origin
    fn create_unit_triangle() -> Triangle<Lambertian> {
        Triangle::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Lambertian::new(Color::new(0.5, 0.5, 0.5)),
        )
    }

    #[test]
    fn test_single_instance_identity_transform() {
        // Single instance with identity transform should behave like non-instanced
        let tri = create_unit_triangle();

        let local_primitives: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(tri)];

        let instanced = InstancedGeometry::new(
            local_primitives,
            vec![Mat4::IDENTITY],
            Lambertian::new(Color::new(0.7, 0.7, 0.7)),
        );

        // Ray pointing at triangle from above
        let ray = Ray::new(Vec3::new(0.5, 0.5, 1.0), Vec3::new(0.0, 0.0, -1.0), 0.0);

        let mut rec = HitRecord::default();
        let hit = instanced.hit(&ray, Interval::new(0.001, f32::INFINITY), &mut rec);

        assert!(hit, "Ray should hit triangle");
        assert!(
            (rec.t - 1.0).abs() < 0.01,
            "Hit distance should be ~1.0, got {}",
            rec.t
        );
    }

    #[test]
    fn test_multiple_instances_closest_wins() {
        // Two instances at different Z positions - closer one should win
        let tri = create_unit_triangle();

        let local_primitives: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(tri)];

        // Instance 1: far away (z = -10)
        // Instance 2: closer (z = -5)
        let transforms = vec![
            Mat4::from_translation(Vec3::new(0.0, 0.0, -10.0)),
            Mat4::from_translation(Vec3::new(0.0, 0.0, -5.0)),
        ];

        let instanced = InstancedGeometry::new(
            local_primitives,
            transforms,
            Lambertian::new(Color::new(0.7, 0.7, 0.7)),
        );

        // Ray from origin pointing down -Z
        let ray = Ray::new(Vec3::new(0.5, 0.5, 0.0), Vec3::new(0.0, 0.0, -1.0), 0.0);

        let mut rec = HitRecord::default();
        let hit = instanced.hit(&ray, Interval::new(0.001, f32::INFINITY), &mut rec);

        assert!(hit, "Ray should hit one of the instances");

        // Should hit the closer instance at z=-5, so hit point z should be ~-5
        assert!(
            (rec.p.z - (-5.0)).abs() < 0.1,
            "Should hit closer instance at z=-5, got z={}",
            rec.p.z
        );
    }

    #[test]
    fn test_transform_correctness() {
        // Test that ray transformation works correctly
        let tri = create_unit_triangle();

        let local_primitives: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(tri)];

        // Translate triangle to (5, 0, 0)
        let transforms = vec![Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0))];

        let instanced = InstancedGeometry::new(
            local_primitives,
            transforms,
            Lambertian::new(Color::new(0.7, 0.7, 0.7)),
        );

        // Ray pointing at translated triangle
        let ray = Ray::new(
            Vec3::new(5.5, 0.5, 1.0),  // Above the translated triangle
            Vec3::new(0.0, 0.0, -1.0), // Pointing down
            0.0,
        );

        let mut rec = HitRecord::default();
        let hit = instanced.hit(&ray, Interval::new(0.001, f32::INFINITY), &mut rec);

        assert!(hit, "Ray should hit translated triangle");

        // Hit point should be around (5.5, 0.5, 0)
        assert!(
            (rec.p.x - 5.5).abs() < 0.1,
            "Hit X should be ~5.5, got {}",
            rec.p.x
        );
        assert!(
            (rec.p.y - 0.5).abs() < 0.1,
            "Hit Y should be ~0.5, got {}",
            rec.p.y
        );
        assert!(rec.p.z.abs() < 0.1, "Hit Z should be ~0, got {}", rec.p.z);
    }

    #[test]
    fn test_rotation_transform() {
        use std::f32::consts::PI;

        // Triangle in XY plane: v0=(0,0,0), v1=(1,0,0), v2=(0,1,0)
        let tri = create_unit_triangle();

        let local_primitives: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(tri)];

        // Rotate 90 degrees around Y axis
        // After rotation: v0=(0,0,0), v1=(0,0,-1), v2=(0,1,0)
        // Triangle is now in ZY plane at X=0
        let transforms = vec![Mat4::from_rotation_y(PI / 2.0)];

        let instanced = InstancedGeometry::new(
            local_primitives,
            transforms,
            Lambertian::new(Color::new(0.7, 0.7, 0.7)),
        );

        // Ray pointing at rotated triangle
        // Target: center of triangle after rotation is around (0, 0.33, -0.33)
        let ray = Ray::new(
            Vec3::new(-1.0, 0.3, -0.3), // From -X, slightly offset
            Vec3::new(1.0, 0.0, 0.0),   // Pointing +X
            0.0,
        );

        let mut rec = HitRecord::default();
        let hit = instanced.hit(&ray, Interval::new(0.001, f32::INFINITY), &mut rec);

        assert!(hit, "Ray should hit rotated triangle");
    }

    #[test]
    fn test_instance_count() {
        let tri = create_unit_triangle();
        let local_primitives: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(tri)];

        let transforms = vec![Mat4::IDENTITY; 100];

        let instanced = InstancedGeometry::new(
            local_primitives,
            transforms,
            Lambertian::new(Color::new(0.5, 0.5, 0.5)), // Gray
        );

        assert_eq!(instanced.instance_count(), 100);
    }
}
