//! BIF Renderer - CPU Path Tracing
//!
//! A Monte Carlo path tracer for physically-based rendering.
//! Ported from the Go raytracer in legacy/go-raytracing.

mod ray;
mod hittable;
mod material;
mod sphere;
mod triangle;
mod camera;
mod bvh;

pub use ray::Ray;
pub use hittable::{HitRecord, Hittable, HittableList};
pub use material::{Material, Color, Lambertian, Metal, Dielectric, DiffuseLight};
pub use sphere::Sphere;
pub use triangle::Triangle;
pub use camera::Camera;
pub use bvh::BvhNode;

/// Re-export Vec3 and common math types from bif_math
pub use bif_math::{Vec3, Aabb, Interval};
