//! Sphere primitive for ray tracing.

use crate::{
    hittable::{HitRecord, Hittable},
    Material, Ray,
};
use bif_math::{Aabb, Interval, Vec3};
use std::f32::consts::PI;

/// A sphere primitive.
pub struct Sphere<M: Material> {
    center: Vec3,
    radius: f32,
    material: M,
    bbox: Aabb,
}

impl<M: Material> Sphere<M> {
    /// Create a new sphere.
    pub fn new(center: Vec3, radius: f32, material: M) -> Self {
        let radius = radius.max(0.0);
        let rvec = Vec3::splat(radius);
        let bbox = Aabb::from_points(center - rvec, center + rvec);

        Self {
            center,
            radius,
            material,
            bbox,
        }
    }

    /// Get the UV coordinates for a point on the unit sphere.
    fn get_sphere_uv(p: Vec3) -> (f32, f32) {
        // p is a point on the unit sphere centered at origin
        // theta: angle down from +Y
        // phi: angle around Y axis from +X
        let theta = (-p.y).acos();
        let phi = (-p.z).atan2(p.x) + PI;

        let u = phi / (2.0 * PI);
        let v = theta / PI;
        (u, v)
    }
}

impl<M: Material + 'static> Hittable for Sphere<M> {
    fn hit<'a>(&'a self, ray: &Ray, ray_t: Interval, rec: &mut HitRecord<'a>) -> bool {
        let oc = self.center - ray.origin();
        let a = ray.direction().length_squared();
        let h = ray.direction().dot(oc);
        let c = oc.length_squared() - self.radius * self.radius;

        let discriminant = h * h - a * c;
        if discriminant < 0.0 {
            return false;
        }

        let sqrtd = discriminant.sqrt();

        // Find the nearest root in the acceptable range
        let mut root = (h - sqrtd) / a;
        if !ray_t.surrounds(root) {
            root = (h + sqrtd) / a;
            if !ray_t.surrounds(root) {
                return false;
            }
        }

        rec.t = root;
        rec.p = ray.at(rec.t);
        let outward_normal = (rec.p - self.center) / self.radius;
        rec.set_face_normal(ray, outward_normal);
        (rec.u, rec.v) = Self::get_sphere_uv(outward_normal);
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
    fn test_sphere_hit() {
        let sphere = Sphere::new(
            Vec3::new(0.0, 0.0, -1.0),
            0.5,
            Lambertian::new(Vec3::new(0.5, 0.5, 0.5)),
        );

        let ray = Ray::new_simple(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
        let interval = Interval::new(0.001, f32::INFINITY);

        // Create a dummy record (we need a material reference)
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

        assert!(sphere.hit(&ray, interval, &mut rec));
        assert!((rec.t - 0.5).abs() < 0.001); // Should hit at t=0.5
    }

    #[test]
    fn test_sphere_miss() {
        let sphere = Sphere::new(
            Vec3::new(0.0, 0.0, -1.0),
            0.5,
            Lambertian::new(Vec3::new(0.5, 0.5, 0.5)),
        );

        // Ray pointing away from sphere
        let ray = Ray::new_simple(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0));
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

        assert!(!sphere.hit(&ray, interval, &mut rec));
    }
}
