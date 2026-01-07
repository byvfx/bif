//! Bounding Volume Hierarchy (BVH) acceleration structure.
//!
//! Uses a binary tree structure for efficient ray-scene intersection testing.
//! Ported from legacy Go raytracer with Rust-idiomatic enhancements.

use crate::{HitRecord, Hittable, Ray};
use bif_math::{Aabb, Interval};

/// Maximum primitives per leaf node before splitting.
const LEAF_MAX_SIZE: usize = 4;

/// BVH node - either a branch with two children or a leaf with primitives.
///
/// Using an enum allows for more cache-efficient traversal since
/// we avoid dynamic dispatch overhead.
pub enum BvhNode {
    /// Internal node with two children.
    Branch {
        left: Box<BvhNode>,
        right: Box<BvhNode>,
        bbox: Aabb,
    },
    /// Leaf node with a small number of primitives.
    Leaf {
        objects: Vec<Box<dyn Hittable + Send + Sync>>,
        bbox: Aabb,
    },
    /// Empty node (for edge cases).
    Empty,
}

/// Convert our Ray to bif_math::Ray for AABB intersection.
#[inline]
fn to_math_ray(ray: &Ray) -> bif_math::Ray {
    bif_math::Ray::new(ray.origin(), ray.direction(), ray.time())
}

impl BvhNode {
    /// Create a BVH from a list of hittable objects.
    pub fn new(objects: Vec<Box<dyn Hittable + Send + Sync>>) -> Self {
        if objects.is_empty() {
            return BvhNode::Empty;
        }
        Self::build(objects)
    }

    /// Recursive BVH construction.
    ///
    /// Simple median-split approach: sort objects by centroid on longest axis,
    /// split in half, recurse.
    fn build(mut objects: Vec<Box<dyn Hittable + Send + Sync>>) -> Self {
        let n = objects.len();

        // Compute bounding box of all objects
        let bounds = objects
            .iter()
            .map(|o| o.bounding_box())
            .fold(objects[0].bounding_box(), |acc, b| {
                Aabb::surrounding(&acc, &b)
            });

        // Create leaf for small sets
        if n <= LEAF_MAX_SIZE {
            return BvhNode::Leaf {
                objects,
                bbox: bounds,
            };
        }

        // Compute centroid bounds to choose split axis
        let centroid_bounds = objects.iter().fold(Aabb::EMPTY, |acc, obj| {
            let c = obj.bounding_box().centroid();
            Aabb::surrounding(&acc, &Aabb::from_points(c, c))
        });

        // Choose split axis based on centroid spread
        let axis = centroid_bounds.longest_axis();

        // Sort objects by centroid on chosen axis
        objects.sort_unstable_by(|a, b| {
            let a_centroid = a.bounding_box().centroid();
            let b_centroid = b.bounding_box().centroid();
            let a_val = match axis {
                0 => a_centroid.x,
                1 => a_centroid.y,
                _ => a_centroid.z,
            };
            let b_val = match axis {
                0 => b_centroid.x,
                1 => b_centroid.y,
                _ => b_centroid.z,
            };
            a_val
                .partial_cmp(&b_val)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Split at midpoint
        let mid = n / 2;
        let right_objects = objects.split_off(mid);
        let left_objects = objects;

        // Recurse
        let left = Self::build(left_objects);
        let right = Self::build(right_objects);

        BvhNode::Branch {
            left: Box::new(left),
            right: Box::new(right),
            bbox: bounds,
        }
    }
}

impl Hittable for BvhNode {
    fn hit<'a>(&'a self, ray: &Ray, ray_t: Interval, rec: &mut HitRecord<'a>) -> bool {
        let math_ray = to_math_ray(ray);

        match self {
            BvhNode::Empty => false,

            BvhNode::Leaf { objects, bbox } => {
                if !bbox.hit(&math_ray, ray_t) {
                    return false;
                }

                let mut hit_anything = false;
                let mut closest = ray_t.max;

                for obj in objects {
                    let interval = Interval::new(ray_t.min, closest);
                    if obj.hit(ray, interval, rec) {
                        hit_anything = true;
                        closest = rec.t;
                    }
                }
                hit_anything
            }

            BvhNode::Branch { left, right, bbox } => {
                if !bbox.hit(&math_ray, ray_t) {
                    return false;
                }

                let hit_left = left.hit(ray, ray_t, rec);

                // Only check right up to closest hit
                let right_max = if hit_left { rec.t } else { ray_t.max };
                let hit_right = right.hit(ray, Interval::new(ray_t.min, right_max), rec);

                hit_left || hit_right
            }
        }
    }

    fn bounding_box(&self) -> Aabb {
        match self {
            BvhNode::Empty => Aabb::EMPTY,
            BvhNode::Leaf { bbox, .. } => *bbox,
            BvhNode::Branch { bbox, .. } => *bbox,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lambertian, Sphere};
    use bif_math::Vec3;

    type Color = Vec3;

    #[test]
    fn test_bvh_empty() {
        let bvh = BvhNode::new(vec![]);
        assert!(matches!(bvh, BvhNode::Empty));
    }

    #[test]
    fn test_bvh_single_sphere() {
        let sphere = Sphere::new(
            Vec3::new(0.0, 0.0, -1.0),
            0.5,
            Lambertian::new(Color::new(0.5, 0.5, 0.5)),
        );

        let objects: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(sphere)];
        let bvh = BvhNode::new(objects);

        // Should create a leaf
        assert!(matches!(bvh, BvhNode::Leaf { .. }));

        // Test ray hit
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), 0.0);
        let mut rec = HitRecord::default();
        let hit = bvh.hit(&ray, Interval::new(0.001, f32::INFINITY), &mut rec);
        assert!(hit);
    }

    #[test]
    fn test_bvh_multiple_spheres() {
        let spheres: Vec<Box<dyn Hittable + Send + Sync>> = (0..10)
            .map(|i| {
                let sphere = Sphere::new(
                    Vec3::new(i as f32, 0.0, -5.0),
                    0.5,
                    Lambertian::new(Color::new(0.5, 0.5, 0.5)),
                );
                Box::new(sphere) as Box<dyn Hittable + Send + Sync>
            })
            .collect();

        let bvh = BvhNode::new(spheres);

        // Test ray that hits sphere at x=5
        let ray = Ray::new(Vec3::new(5.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0), 0.0);
        let mut rec = HitRecord::default();
        let hit = bvh.hit(&ray, Interval::new(0.001, f32::INFINITY), &mut rec);
        assert!(hit);

        // Hit point should be near z = -4.5 (sphere at z=-5, radius 0.5)
        assert!((rec.p.z - (-4.5)).abs() < 0.01);
    }
}
