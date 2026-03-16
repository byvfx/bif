use crate::{Interval, Ray, Vec3};

/// Axis-Aligned Bounding Box for spatial acceleration structures (BVH).
///
/// An AABB is defined by three intervals (one per axis) that bound a 3D volume.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Aabb {
    pub x: Interval,
    pub y: Interval,
    pub z: Interval,
}

impl Aabb {
    /// Create a new AABB from three intervals.
    pub fn new(x: Interval, y: Interval, z: Interval) -> Self {
        let mut aabb = Self { x, y, z };
        aabb.pad_to_minimums();
        aabb
    }

    /// Create an AABB from two corner points.
    pub fn from_points(a: Vec3, b: Vec3) -> Self {
        let x = Interval::new(a.x.min(b.x), a.x.max(b.x));
        let y = Interval::new(a.y.min(b.y), a.y.max(b.y));
        let z = Interval::new(a.z.min(b.z), a.z.max(b.z));

        let mut aabb = Self { x, y, z };
        aabb.pad_to_minimums();
        aabb
    }

    /// Create an AABB that surrounds two other AABBs.
    pub fn surrounding(box0: &Aabb, box1: &Aabb) -> Self {
        Self {
            x: Interval::surrounding(&box0.x, &box1.x),
            y: Interval::surrounding(&box0.y, &box1.y),
            z: Interval::surrounding(&box0.z, &box1.z),
        }
    }

    /// Get the interval for a specific axis (0=X, 1=Y, 2=Z).
    pub fn axis_interval(&self, n: usize) -> Interval {
        match n {
            0 => self.x,
            1 => self.y,
            2 => self.z,
            _ => panic!("axis_interval: invalid axis {n}, expected 0-2"),
        }
    }

    /// Test if a ray intersects this AABB within the given interval.
    ///
    /// Uses the slab method - efficient ray-box intersection test.
    #[inline]
    pub fn hit(&self, r: &Ray, mut ray_t: Interval) -> bool {
        let ray_orig = r.origin;
        let ray_dir = r.direction;

        // Robust slab test: use f32::min/f32::max which propagate NaN,
        // then check with finite-safe comparisons. When inv_d is +/-inf
        // and the slab difference is 0, we get NaN — treat as a miss only
        // if the ray is outside the slab.
        for axis in 0..3 {
            let (slab_min, slab_max, orig, dir) = match axis {
                0 => (self.x.min, self.x.max, ray_orig.x, ray_dir.x),
                1 => (self.y.min, self.y.max, ray_orig.y, ray_dir.y),
                _ => (self.z.min, self.z.max, ray_orig.z, ray_dir.z),
            };

            let inv_d = 1.0 / dir;
            let mut t0 = (slab_min - orig) * inv_d;
            let mut t1 = (slab_max - orig) * inv_d;

            if inv_d < 0.0 {
                std::mem::swap(&mut t0, &mut t1);
            }

            // Handle NaN from 0 * inf: if ray is inside slab, treat as hit
            if t0.is_nan() {
                t0 = f32::NEG_INFINITY;
            }
            if t1.is_nan() {
                t1 = f32::INFINITY;
            }

            ray_t.min = t0.max(ray_t.min);
            ray_t.max = t1.min(ray_t.max);
            if ray_t.max <= ray_t.min {
                return false;
            }
        }

        true
    }

    /// Pad intervals to avoid zero-width AABBs (degenerate cases).
    fn pad_to_minimums(&mut self) {
        let delta = 0.0001;
        if self.x.size() < delta {
            self.x = self.x.expand(delta);
        }
        if self.y.size() < delta {
            self.y = self.y.expand(delta);
        }
        if self.z.size() < delta {
            self.z = self.z.expand(delta);
        }
    }

    /// Translate (move) the AABB by an offset vector.
    pub fn translate(&self, offset: Vec3) -> Aabb {
        Aabb::new(
            self.x.add_scalar(offset.x),
            self.y.add_scalar(offset.y),
            self.z.add_scalar(offset.z),
        )
    }

    /// Returns the index (0=X, 1=Y, 2=Z) of the axis with the longest extent.
    pub fn longest_axis(&self) -> usize {
        let x_size = self.x.size();
        let y_size = self.y.size();
        let z_size = self.z.size();

        if x_size > y_size && x_size > z_size {
            0
        } else if y_size > z_size {
            1
        } else {
            2
        }
    }

    /// Returns the center point of the bounding box.
    pub fn centroid(&self) -> Vec3 {
        Vec3::new(
            (self.x.min + self.x.max) * 0.5,
            (self.y.min + self.y.max) * 0.5,
            (self.z.min + self.z.max) * 0.5,
        )
    }

    /// Returns the center point of the bounding box (alias for centroid).
    #[inline]
    pub fn center(&self) -> Vec3 {
        self.centroid()
    }

    /// Returns the minimum corner point of the bounding box.
    pub fn min_point(&self) -> Vec3 {
        Vec3::new(self.x.min, self.y.min, self.z.min)
    }

    /// Returns the maximum corner point of the bounding box.
    pub fn max_point(&self) -> Vec3 {
        Vec3::new(self.x.max, self.y.max, self.z.max)
    }

    /// Static constants
    pub const EMPTY: Aabb = Aabb {
        x: Interval::EMPTY,
        y: Interval::EMPTY,
        z: Interval::EMPTY,
    };

    pub const UNIVERSE: Aabb = Aabb {
        x: Interval::UNIVERSE,
        y: Interval::UNIVERSE,
        z: Interval::UNIVERSE,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aabb_from_points() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(10.0, 10.0, 10.0);
        let aabb = Aabb::from_points(a, b);

        assert_eq!(aabb.x.min, 0.0);
        assert_eq!(aabb.x.max, 10.0);
        assert_eq!(aabb.y.min, 0.0);
        assert_eq!(aabb.y.max, 10.0);
        assert_eq!(aabb.z.min, 0.0);
        assert_eq!(aabb.z.max, 10.0);
    }

    #[test]
    fn test_aabb_surrounding() {
        let box1 = Aabb::from_points(Vec3::ZERO, Vec3::new(5.0, 5.0, 5.0));
        let box2 = Aabb::from_points(Vec3::new(3.0, 3.0, 3.0), Vec3::new(10.0, 10.0, 10.0));
        let surrounding = Aabb::surrounding(&box1, &box2);

        assert_eq!(surrounding.x.min, 0.0);
        assert_eq!(surrounding.x.max, 10.0);
    }

    #[test]
    fn test_aabb_hit() {
        let aabb = Aabb::from_points(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));

        // Ray pointing at center
        let ray = Ray::new(Vec3::new(0.0, 0.0, -5.0), Vec3::new(0.0, 0.0, 1.0), 0.0);
        assert!(aabb.hit(&ray, Interval::new(0.0, 100.0)));

        // Ray pointing away
        let ray = Ray::new(Vec3::new(0.0, 0.0, -5.0), Vec3::new(0.0, 0.0, -1.0), 0.0);
        assert!(!aabb.hit(&ray, Interval::new(0.0, 100.0)));

        // Ray missing the box
        let ray = Ray::new(Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 0.0);
        assert!(!aabb.hit(&ray, Interval::new(0.0, 100.0)));
    }

    #[test]
    fn test_aabb_centroid() {
        let aabb = Aabb::from_points(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0));
        let centroid = aabb.centroid();

        assert_eq!(centroid, Vec3::new(5.0, 5.0, 5.0));
    }

    #[test]
    fn test_aabb_longest_axis() {
        let aabb_x = Aabb::from_points(Vec3::ZERO, Vec3::new(10.0, 1.0, 1.0));
        assert_eq!(aabb_x.longest_axis(), 0);

        let aabb_y = Aabb::from_points(Vec3::ZERO, Vec3::new(1.0, 10.0, 1.0));
        assert_eq!(aabb_y.longest_axis(), 1);

        let aabb_z = Aabb::from_points(Vec3::ZERO, Vec3::new(1.0, 1.0, 10.0));
        assert_eq!(aabb_z.longest_axis(), 2);
    }

    #[test]
    fn test_aabb_translate() {
        let aabb = Aabb::from_points(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let translated = aabb.translate(Vec3::new(5.0, 0.0, 0.0));

        assert_eq!(translated.x.min, 5.0);
        assert_eq!(translated.x.max, 6.0);
        assert_eq!(translated.y.min, 0.0);
        assert_eq!(translated.z.min, 0.0);
    }

    #[test]
    fn test_hit_axis_aligned_ray_along_x() {
        // Ray along +X hitting a box centered at (5, 0, 0)
        let aabb = Aabb::from_points(Vec3::new(4.0, -1.0, -1.0), Vec3::new(6.0, 1.0, 1.0));
        let ray = Ray::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 0.0);
        assert!(aabb.hit(&ray, Interval::new(0.0, 100.0)));
    }

    #[test]
    fn test_hit_axis_aligned_ray_along_y() {
        // Ray along +Y hitting a box centered at (0, 5, 0)
        let aabb = Aabb::from_points(Vec3::new(-1.0, 4.0, -1.0), Vec3::new(1.0, 6.0, 1.0));
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), 0.0);
        assert!(aabb.hit(&ray, Interval::new(0.0, 100.0)));
    }

    #[test]
    fn test_hit_axis_aligned_ray_along_z() {
        // Ray along +Z hitting a box centered at (0, 0, 5)
        let aabb = Aabb::from_points(Vec3::new(-1.0, -1.0, 4.0), Vec3::new(1.0, 1.0, 6.0));
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0), 0.0);
        assert!(aabb.hit(&ray, Interval::new(0.0, 100.0)));
    }

    #[test]
    fn test_hit_axis_aligned_ray_miss() {
        // Ray along +X but box is offset on Y, should miss
        let aabb = Aabb::from_points(Vec3::new(4.0, 4.0, -1.0), Vec3::new(6.0, 6.0, 1.0));
        let ray = Ray::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 0.0);
        assert!(!aabb.hit(&ray, Interval::new(0.0, 100.0)));
    }

    #[test]
    fn test_hit_ray_origin_inside_aabb() {
        // Ray starts inside the AABB — should still register a hit
        let aabb = Aabb::from_points(Vec3::new(-5.0, -5.0, -5.0), Vec3::new(5.0, 5.0, 5.0));
        let ray = Ray::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 0.0);
        assert!(aabb.hit(&ray, Interval::new(0.0, 100.0)));
    }

    #[test]
    fn test_hit_ray_origin_inside_any_direction() {
        // Ray starts inside and points in any direction — always hits
        let aabb = Aabb::from_points(Vec3::new(-5.0, -5.0, -5.0), Vec3::new(5.0, 5.0, 5.0));
        let directions = [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0).normalize(),
        ];
        for dir in &directions {
            let ray = Ray::new(Vec3::ZERO, *dir, 0.0);
            assert!(
                aabb.hit(&ray, Interval::new(0.0, 100.0)),
                "ray inside aabb with dir {:?} should hit",
                dir
            );
        }
    }

    #[test]
    fn test_hit_ray_origin_on_boundary() {
        // Ray origin exactly on the AABB boundary face, pointing inward
        let aabb = Aabb::from_points(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let ray = Ray::new(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), 0.0);
        assert!(aabb.hit(&ray, Interval::new(0.0, 100.0)));
    }

    #[test]
    fn test_hit_ray_origin_on_boundary_pointing_outward() {
        // Ray origin on boundary, pointing outward — still hits (exits through far side)
        // The slab intersection finds t_exit > 0 on the other axes
        let aabb = Aabb::from_points(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let ray = Ray::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), 0.0);
        // This is on the boundary; t_enter ~ 0 for x-slab, t_exit ~ 0 for x-slab
        // With the slab method, this is a degenerate case that may or may not hit
        // depending on floating point. We just assert it doesn't panic.
        let _ = aabb.hit(&ray, Interval::new(0.0, 100.0));
    }
}
