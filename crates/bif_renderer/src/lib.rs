//! BIF Renderer "Ivar" - CPU Path Tracing
//!
//! A Monte Carlo path tracer for physically-based rendering.
//! Ported from the Go raytracer in legacy/go-raytracing.
//!
//! Named "Ivar" to distinguish from the GPU viewport renderer.

pub mod blue_noise;
mod bucket;
mod bvh;
mod camera;
pub mod denoise;
mod embree;
pub mod embree_ffi;
pub mod exr_writer;
pub mod filter;
pub mod hdri;
mod hittable;
mod instanced_geometry;
pub mod light;
mod material;
pub mod openpbr;
pub mod pick_scene;
pub mod radiance_cache;
mod renderer;
mod sphere;
mod triangle;

pub(crate) use bif_math::{Ray, Vec3};
pub use blue_noise::SamplerMode;
pub use bucket::{
    generate_buckets, render_bucket, render_bucket_with_aovs, Bucket, BucketResult,
    BucketResultWithAovs, DEFAULT_BUCKET_SIZE,
};
pub use bvh::BvhNode;
pub use camera::Camera;
pub use denoise::{denoise_beauty, DenoiseError, DenoiseResult};
pub use embree::EmbreeScene;
pub use exr_writer::{format_frame_path, write_exr, ExrCompression, ExrError, ExrOutput};
pub use filter::{PixelFilter, PixelFilterConfig};
pub use hdri::HdriEnvironment;
pub use hittable::{HitRecord, Hittable, HittableList};
pub use instanced_geometry::InstancedGeometry;
pub use light::{DistantLight, Light, LightList, LightSample, RectLight, SphereLight};
pub use material::{
    cosine_weighted_hemisphere, gen_f32, gen_f32_generic, power_heuristic, random_in_hemisphere,
    random_unit_vector, Color, Dielectric, DiffuseLight, Lambertian, Material, Metal,
    ScatterResult,
};
pub use openpbr::OpenPbrSurface;
pub use pick_scene::{EmbreePickScene, PickError, PickResult};
pub use radiance_cache::{RadianceCache, RadianceCacheConfig};
pub use renderer::{
    color_to_rgba, ray_color, ray_color_with_aovs, render, render_pixel, render_pixel_with_aovs,
    AovData, ImageBuffer, RenderConfig,
};
pub use sphere::Sphere;
pub use triangle::Triangle;
