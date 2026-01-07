//! Triangle primitive for ray tracing.
//!
//! Uses the Möller-Trumbore algorithm for ray-triangle intersection.

use crate::{
    hittable::{HitRecord, Hittable},
    Material, Ray,
};
use bif_math::{Aabb, Interval, Vec3};

/// A triangle primitive.
pub struct Triangle<M: Material> {
    /// Vertices
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
    /// Pre-computed face normal (unit length)
    normal: Vec3,
    /// Material
    material: M,
    /// Bounding box
    bbox: Aabb,
}

impl<M: Material> Triangle<M> {
    /// Create a new triangle from three vertices.
    pub fn new(v0: Vec3, v1: Vec3, v2: Vec3, material: M) -> Self {
        // Calculate edges
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;

        // Calculate normal using cross product
        let normal = edge1.cross(edge2).normalize();

        // Create bounding box
        let min = Vec3::new(
            v0.x.min(v1.x).min(v2.x),
            v0.y.min(v1.y).min(v2.y),
            v0.z.min(v1.z).min(v2.z),
        );
        let max = Vec3::new(
            v0.x.max(v1.x).max(v2.x),
            v0.y.max(v1.y).max(v2.y),
            v0.z.max(v1.z).max(v2.z),
        );

        // Pad thin dimensions to avoid degenerate AABBs
        let delta = 0.0001;
        let bbox = Aabb::from_points(min - Vec3::splat(delta), max + Vec3::splat(delta));

        Self {
            v0,
            v1,
            v2,
            normal,
            material,
            bbox,
        }
    }

    /// Create a triangle with a pre-computed normal (for smooth shading).
    pub fn with_normal(v0: Vec3, v1: Vec3, v2: Vec3, normal: Vec3, material: M) -> Self {
        let min = Vec3::new(
            v0.x.min(v1.x).min(v2.x),
            v0.y.min(v1.y).min(v2.y),
            v0.z.min(v1.z).min(v2.z),
        );
        let max = Vec3::new(
            v0.x.max(v1.x).max(v2.x),
            v0.y.max(v1.y).max(v2.y),
            v0.z.max(v1.z).max(v2.z),
        );

        let delta = 0.0001;
        let bbox = Aabb::from_points(min - Vec3::splat(delta), max + Vec3::splat(delta));

        Self {
            v0,
            v1,
            v2,
            normal: normal.normalize(),
            material,
            bbox,
        }
    }
}

impl<M: Material + 'static> Hittable for Triangle<M> {
    /// Möller-Trumbore ray-triangle intersection algorithm.
    fn hit<'a>(&'a self, ray: &Ray, ray_t: Interval, rec: &mut HitRecord<'a>) -> bool {
        let edge1 = self.v1 - self.v0;
        let edge2 = self.v2 - self.v0;

        let h = ray.direction().cross(edge2);
        let a = edge1.dot(h);

        // Ray is parallel to triangle
        if a.abs() < 1e-8 {
            return false;
        }

        let f = 1.0 / a;
        let s = ray.origin() - self.v0;
        let u = f * s.dot(h);

        // Check if intersection is outside triangle (u parameter)
        if !(0.0..=1.0).contains(&u) {
            return false;
        }

        let q = s.cross(edge1);
        let v = f * ray.direction().dot(q);

        // Check if intersection is outside triangle (v parameter)
        if v < 0.0 || u + v > 1.0 {
            return false;
        }

        // Calculate t parameter
        let t = f * edge2.dot(q);

        if !ray_t.contains(t) {
            return false;
        }

        // Valid intersection found
        rec.t = t;
        rec.p = ray.at(t);
        rec.set_face_normal(ray, self.normal);
        rec.u = u;
        rec.v = v;
        rec.material = &self.material;

        true
    }

    fn bounding_box(&self) -> Aabb {
        self.bbox
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::Lambertian;

    #[test]
    fn test_triangle_hit() {
        // Triangle in XY plane at z=-1
        let tri = Triangle::new(
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(0.0, 1.0, -1.0),
            Lambertian::new(Vec3::new(0.5, 0.5, 0.5)),
        );

        // Ray pointing at triangle center
        let ray = Ray::new_simple(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
        let interval = Interval::new(0.001, f32::INFINITY);

        let dummy_mat = Lambertian::new(Vec3::ONE);
        let mut rec = HitRecord {
            p: Vec3::ZERO,
            normal: Vec3::ZERO,
            material: &dummy_mat,
            u: 0.0,
            v: 0.0,
            t: 0.0,
            front_face: false,
        };

        assert!(tri.hit(&ray, interval, &mut rec));
        assert!((rec.t - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_triangle_miss() {
        let tri = Triangle::new(
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(0.0, 1.0, -1.0),
            Lambertian::new(Vec3::new(0.5, 0.5, 0.5)),
        );

        // Ray pointing away
        let ray = Ray::new_simple(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        let interval = Interval::new(0.001, f32::INFINITY);

        let dummy_mat = Lambertian::new(Vec3::ONE);
        let mut rec = HitRecord {
            p: Vec3::ZERO,
            normal: Vec3::ZERO,
            material: &dummy_mat,
            u: 0.0,
            v: 0.0,
            t: 0.0,
            front_face: false,
        };

        assert!(!tri.hit(&ray, interval, &mut rec));
    }
}
