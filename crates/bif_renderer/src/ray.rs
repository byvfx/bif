//! Ray type for path tracing.
//!
//! A ray is defined by an origin point, a direction vector, and a time value
//! for motion blur support.

use bif_math::Vec3;

/// A ray with origin, direction, and time.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    /// Origin point of the ray
    origin: Vec3,
    /// Direction vector (not necessarily normalized)
    direction: Vec3,
    /// Time value for motion blur
    time: f32,
}

impl Ray {
    /// Create a new ray.
    #[inline]
    pub fn new(origin: Vec3, direction: Vec3, time: f32) -> Self {
        Self {
            origin,
            direction,
            time,
        }
    }

    /// Create a ray at time 0.
    #[inline]
    pub fn new_simple(origin: Vec3, direction: Vec3) -> Self {
        Self::new(origin, direction, 0.0)
    }

    /// Get the ray's origin point.
    #[inline]
    pub fn origin(&self) -> Vec3 {
        self.origin
    }

    /// Get the ray's direction vector.
    #[inline]
    pub fn direction(&self) -> Vec3 {
        self.direction
    }

    /// Get the ray's time value.
    #[inline]
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Compute a point along the ray at parameter t.
    /// P(t) = origin + t * direction
    #[inline]
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + t * self.direction
    }
}

impl Default for Ray {
    fn default() -> Self {
        Self {
            origin: Vec3::ZERO,
            direction: Vec3::Z,
            time: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_at() {
        let ray = Ray::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), 0.0);

        assert_eq!(ray.at(0.0), Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(ray.at(1.0), Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(ray.at(2.5), Vec3::new(2.5, 0.0, 0.0));
    }

    #[test]
    fn test_ray_accessors() {
        let origin = Vec3::new(1.0, 2.0, 3.0);
        let direction = Vec3::new(0.0, 1.0, 0.0);
        let ray = Ray::new(origin, direction, 0.5);

        assert_eq!(ray.origin(), origin);
        assert_eq!(ray.direction(), direction);
        assert_eq!(ray.time(), 0.5);
    }
}
