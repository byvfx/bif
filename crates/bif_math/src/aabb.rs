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

    /// Create an empty AABB (contains nothing).
    pub fn empty() -> Self {
        Self {
            x: Interval::EMPTY,
            y: Interval::EMPTY,
            z: Interval::EMPTY,
        }
    }

    /// Create a universe AABB (contains everything).
    pub fn universe() -> Self {
        Self {
            x: Interval::UNIVERSE,
            y: Interval::UNIVERSE,
            z: Interval::UNIVERSE,
        }
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
            _ => self.z,
        }
    }

    /// Test if a ray intersects this AABB within the given interval.
    /// 
    /// Uses the slab method - efficient ray-box intersection test.
    pub fn hit(&self, r: &Ray, mut ray_t: Interval) -> bool {
        let ray_orig = r.origin;
        let ray_dir = r.direction;

        // X axis
        let adinv = 1.0 / ray_dir.x;
        let mut t0 = (self.x.min - ray_orig.x) * adinv;
        let mut t1 = (self.x.max - ray_orig.x) * adinv;
        if adinv < 0.0 {
            std::mem::swap(&mut t0, &mut t1);
        }
        ray_t.min = t0.max(ray_t.min);
        ray_t.max = t1.min(ray_t.max);
        if ray_t.max <= ray_t.min {
            return false;
        }

        // Y axis
        let adinv = 1.0 / ray_dir.y;
        let mut t0 = (self.y.min - ray_orig.y) * adinv;
        let mut t1 = (self.y.max - ray_orig.y) * adinv;
        if adinv < 0.0 {
            std::mem::swap(&mut t0, &mut t1);
        }
        ray_t.min = t0.max(ray_t.min);
        ray_t.max = t1.min(ray_t.max);
        if ray_t.max <= ray_t.min {
            return false;
        }

        // Z axis
        let adinv = 1.0 / ray_dir.z;
        let mut t0 = (self.z.min - ray_orig.z) * adinv;
        let mut t1 = (self.z.max - ray_orig.z) * adinv;
        if adinv < 0.0 {
            std::mem::swap(&mut t0, &mut t1);
        }
        ray_t.min = t0.max(ray_t.min);
        ray_t.max = t1.min(ray_t.max);
        if ray_t.max <= ray_t.min {
            return false;
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
}
