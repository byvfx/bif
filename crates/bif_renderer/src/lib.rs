//! BIF Renderer - CPU Path Tracing
//!
//! A Monte Carlo path tracer for physically-based rendering.
//! Ported from the Go raytracer in legacy/go-raytracing.

mod ray;
mod hittable;
mod material;

pub use ray::Ray;
pub use hittable::{HitRecord, Hittable};
pub use material::Material;

/// Re-export Vec3 and common math types from bif_math
pub use bif_math::{Vec3, Aabb, Interval};
