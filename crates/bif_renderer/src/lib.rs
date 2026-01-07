//! BIF Renderer "Ivar" - CPU Path Tracing
//!
//! A Monte Carlo path tracer for physically-based rendering.
//! Ported from the Go raytracer in legacy/go-raytracing.
//!
//! Named "Ivar" to distinguish from the GPU viewport renderer.

mod bucket;
mod bvh;
mod camera;
mod embree;
mod hittable;
mod instanced_geometry;
mod material;
mod ray;
mod renderer;
mod sphere;
mod triangle;

pub use bucket::{generate_buckets, render_bucket, Bucket, BucketResult, DEFAULT_BUCKET_SIZE};
pub use bvh::BvhNode;
pub use camera::Camera;
pub use embree::EmbreeScene;
pub use hittable::{HitRecord, Hittable, HittableList};
pub use instanced_geometry::InstancedGeometry;
pub use material::{Color, Dielectric, DiffuseLight, Lambertian, Material, Metal};
pub use ray::Ray;
pub use renderer::{color_to_rgba, ray_color, render, render_pixel, ImageBuffer, RenderConfig};
pub use sphere::Sphere;
pub use triangle::Triangle;

/// Re-export Vec3 and common math types from bif_math
pub use bif_math::{Aabb, Interval, Vec3};
