use crate::Vec3;

/// A ray in 3D space with origin, direction, and time.
///
/// Rays are used for raytracing - they represent a line starting at `origin`
/// and traveling in `direction`. The `time` field is used for motion blur.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
    pub time: f32,
}

impl Ray {
    /// Create a new ray.
    pub fn new(origin: Vec3, direction: Vec3, time: f32) -> Self {
        Self {
            origin,
            direction,
            time,
        }
    }

    /// Get the origin point of the ray.
    ///
    /// Note: Since `origin` is public, you can also access it directly via `ray.origin`.
    #[inline]
    pub fn origin(&self) -> Vec3 {
        self.origin
    }

    /// Get the direction vector of the ray.
    ///
    /// Note: Since `direction` is public, you can also access it directly via `ray.direction`.
    #[inline]
    pub fn direction(&self) -> Vec3 {
        self.direction
    }

    /// Get the time value of the ray (used for motion blur).
    ///
    /// Note: Since `time` is public, you can also access it directly via `ray.time`.
    #[inline]
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Get the point along the ray at parameter t.
    ///
    /// Returns: origin + t * direction
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_creation() {
        let origin = Vec3::new(1.0, 2.0, 3.0);
        let direction = Vec3::new(0.0, 1.0, 0.0);
        let ray = Ray::new(origin, direction, 0.5);

        assert_eq!(ray.origin, origin);
        assert_eq!(ray.direction, direction);
        assert_eq!(ray.time, 0.5);
    }

    #[test]
    fn test_ray_at() {
        let ray = Ray::new(Vec3::ZERO, Vec3::X, 0.0);

        assert_eq!(ray.at(0.0), Vec3::ZERO);
        assert_eq!(ray.at(1.0), Vec3::X);
        assert_eq!(ray.at(2.0), Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(ray.at(-1.0), Vec3::new(-1.0, 0.0, 0.0));
    }

    #[test]
    fn test_ray_copy() {
        let ray1 = Ray::new(Vec3::ZERO, Vec3::Y, 1.0);
        let ray2 = ray1; // Copy, not move

        // Both should be usable
        assert_eq!(ray1.origin, ray2.origin);
        assert_eq!(ray1.at(1.0), ray2.at(1.0));
    }

    #[test]
    fn test_ray_getters() {
        let origin = Vec3::new(1.0, 2.0, 3.0);
        let direction = Vec3::new(4.0, 5.0, 6.0);
        let time = 0.5;
        let ray = Ray::new(origin, direction, time);

        // Test getter methods
        assert_eq!(ray.origin(), origin);
        assert_eq!(ray.direction(), direction);
        assert_eq!(ray.time(), time);

        // Should be same as direct field access
        assert_eq!(ray.origin(), ray.origin);
        assert_eq!(ray.direction(), ray.direction);
        assert_eq!(ray.time(), ray.time);
    }
}
