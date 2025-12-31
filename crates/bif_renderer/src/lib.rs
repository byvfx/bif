//! BIF Renderer "Ivar" - CPU Path Tracing
//!
//! A Monte Carlo path tracer for physically-based rendering.
//! Ported from the Go raytracer in legacy/go-raytracing.
//!
//! Named "Ivar" to distinguish from the GPU viewport renderer.

mod ray;
mod hittable;
mod material;
mod sphere;
mod triangle;
mod camera;
mod bvh;
mod renderer;
mod bucket;
mod instanced_geometry;

pub use ray::Ray;
pub use hittable::{HitRecord, Hittable, HittableList};
pub use material::{Material, Color, Lambertian, Metal, Dielectric, DiffuseLight};
pub use sphere::Sphere;
pub use triangle::Triangle;
pub use camera::Camera;
pub use bvh::BvhNode;
pub use renderer::{RenderConfig, ImageBuffer, render, render_pixel, ray_color, color_to_rgba};
pub use bucket::{Bucket, BucketResult, generate_buckets, render_bucket, DEFAULT_BUCKET_SIZE};
pub use instanced_geometry::InstancedGeometry;

/// Re-export Vec3 and common math types from bif_math
pub use bif_math::{Vec3, Aabb, Interval};
