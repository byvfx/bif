//! Bounding Volume Hierarchy (BVH) acceleration structure.
//!
//! Uses a binary tree structure for efficient ray-scene intersection testing.
//! Ported from legacy Go raytracer with Rust-idiomatic enhancements.

use bif_math::{Aabb, Interval};
use crate::{Ray, HitRecord, Hittable};

/// Maximum primitives per leaf node before splitting.
const LEAF_MAX_SIZE: usize = 4;

/// Threshold for parallel BVH construction.
#[allow(dead_code)]
const PARALLEL_THRESHOLD: usize = 4096;

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

/// Cached primitive data for efficient sorting during BVH construction.
#[derive(Clone)]
struct BvhPrimitive {
    index: usize,
    bbox: Aabb,
    centroid: bif_math::Vec3,
}

/// Convert our Ray to bif_math::Ray for AABB intersection.
#[inline]
fn to_math_ray(ray: &Ray) -> bif_math::Ray {
    bif_math::Ray::new(ray.origin(), ray.direction(), ray.time())
}

impl BvhNode {
    /// Create a BVH from a list of hittable objects.
    pub fn new(objects: Vec<Box<dyn Hittable + Send + Sync>>) -> Self {
        let n = objects.len();
        if n == 0 {
            return BvhNode::Empty;
        }

        // Pre-compute bounding boxes and centroids
        let primitives: Vec<BvhPrimitive> = objects
            .iter()
            .enumerate()
            .map(|(i, obj)| {
                let bbox = obj.bounding_box();
                BvhPrimitive {
                    index: i,
                    bbox,
                    centroid: bbox.centroid(),
                }
            })
            .collect();

        Self::build(objects, primitives, true)
    }

    /// Recursive BVH construction.
    fn build(
        objects: Vec<Box<dyn Hittable + Send + Sync>>,
        mut primitives: Vec<BvhPrimitive>,
        allow_parallel: bool,
    ) -> Self {
        let n = primitives.len();

        // Compute bounds of all primitives
        let bounds = primitives
            .iter()
            .fold(primitives[0].bbox, |acc, p| Aabb::surrounding(&acc, &p.bbox));

        let centroid_bounds = primitives.iter().fold(
            Aabb::from_points(primitives[0].centroid, primitives[0].centroid),
            |acc, p| Aabb::surrounding(&acc, &Aabb::from_points(p.centroid, p.centroid)),
        );

        // Create leaf for small sets
        if n <= LEAF_MAX_SIZE {
            // Collect objects for this leaf using primitive indices
            let needed: std::collections::HashSet<usize> = 
                primitives.iter().map(|p| p.index).collect();
            
            let leaf_objects: Vec<Box<dyn Hittable + Send + Sync>> = objects
                .into_iter()
                .enumerate()
                .filter(|(i, _)| needed.contains(i))
                .map(|(_, obj)| obj)
                .collect();
            
            return BvhNode::Leaf {
                objects: leaf_objects,
                bbox: bounds,
            };
        }

        // Choose split axis based on centroid spread
        let axis = centroid_bounds.longest_axis();

        // Sort by centroid on chosen axis
        primitives.sort_unstable_by(|a, b| {
            let a_val = match axis {
                0 => a.centroid.x,
                1 => a.centroid.y,
                _ => a.centroid.z,
            };
            let b_val = match axis {
                0 => b.centroid.x,
                1 => b.centroid.y,
                _ => b.centroid.z,
            };
            a_val.partial_cmp(&b_val).unwrap_or(std::cmp::Ordering::Equal)
        });

        let mid = n / 2;
        let (left_prims, right_prims) = primitives.split_at(mid);
        let left_prims = left_prims.to_vec();
        let right_prims = right_prims.to_vec();

        // Partition objects into left and right
        let left_indices: std::collections::HashSet<usize> = 
            left_prims.iter().map(|p| p.index).collect();
        
        let (left_objects, right_objects): (Vec<_>, Vec<_>) = objects
            .into_iter()
            .enumerate()
            .partition(|(i, _)| left_indices.contains(i));
        
        let left_objects: Vec<_> = left_objects.into_iter().map(|(_, o)| o).collect();
        let right_objects: Vec<_> = right_objects.into_iter().map(|(_, o)| o).collect();

        // Rebuild primitives with new indices
        let left_prims: Vec<BvhPrimitive> = left_prims
            .into_iter()
            .enumerate()
            .map(|(new_idx, p)| BvhPrimitive {
                index: new_idx,
                bbox: p.bbox,
                centroid: p.centroid,
            })
            .collect();
        
        let right_prims: Vec<BvhPrimitive> = right_prims
            .into_iter()
            .enumerate()
            .map(|(new_idx, p)| BvhPrimitive {
                index: new_idx,
                bbox: p.bbox,
                centroid: p.centroid,
            })
            .collect();

        // TODO: Add parallel construction with rayon for large trees
        let _ = allow_parallel; // Suppress unused warning for now

        // Sequential construction
        let left = Self::build(left_objects, left_prims, false);
        let right = Self::build(right_objects, right_prims, false);

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
    use crate::{Sphere, Lambertian};
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
        let ray = Ray::new(
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -1.0),
            0.0,
        );
        let mut rec = HitRecord::default();
        let hit = bvh.hit(&ray, Interval::new(0.001, f32::INFINITY), &mut rec);
        assert!(hit);
        
        // Hit point should be near z = -4.5 (sphere at z=-5, radius 0.5)
        assert!((rec.p.z - (-4.5)).abs() < 0.01);
    }
}
