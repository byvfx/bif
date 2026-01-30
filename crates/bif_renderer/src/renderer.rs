//! Core path tracing renderer.
//!
//! Implements Monte Carlo path tracing with:
//! - Recursive ray tracing with configurable depth
//! - Gamma correction
//! - Anti-aliasing via multi-sampling

use std::sync::Arc;

use crate::hdri::HdriEnvironment;
use crate::material::power_heuristic;
use crate::{Camera, Color, HitRecord, Hittable, Ray};
use bif_math::Interval;
use rand::RngCore;

/// Render configuration.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Samples per pixel for anti-aliasing
    pub samples_per_pixel: u32,
    /// Maximum ray bounce depth
    pub max_depth: u32,
    /// Background color when ray doesn't hit anything
    pub background: Color,
    /// Whether to use sky gradient instead of solid background
    pub use_sky_gradient: bool,
    /// HDRI environment for lighting (overrides background/sky when set)
    pub environment: Option<Arc<HdriEnvironment>>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            samples_per_pixel: 100,
            max_depth: 50,
            background: Color::ZERO,
            use_sky_gradient: false,
            environment: None,
        }
    }
}

/// Compute the color seen by a ray.
///
/// Iterative path tracing with throughput accumulation.
/// Uses Next Event Estimation (NEE) with Multiple Importance Sampling (MIS)
/// for environment lighting when an HDRI is present.
/// Pass-through scatters (opacity cutout) do not consume bounce depth.
pub fn ray_color(
    ray: &Ray,
    world: &dyn Hittable,
    depth: u32,
    config: &RenderConfig,
    rng: &mut dyn RngCore,
) -> Color {
    let mut current_ray = *ray;
    let mut throughput = Color::ONE;
    let mut accumulated = Color::ZERO;
    let mut remaining_depth = depth;

    // Track last scatter PDF for MIS weighting when hitting environment
    let mut last_scatter_pdf = 0.0_f32;
    let mut last_was_delta = true; // First ray from camera is treated as delta (no MIS weight)

    loop {
        if remaining_depth == 0 {
            break;
        }

        let mut rec = HitRecord::default();

        if !world.hit(&current_ray, Interval::new(0.001, f32::INFINITY), &mut rec) {
            // Ray escaped - sample environment/background
            let bg = if let Some(ref env) = config.environment {
                let dir = current_ray.direction().normalize();
                let emission = env.sample(dir);
                // MIS weight for BSDF path hitting environment
                if last_was_delta {
                    emission
                } else {
                    let env_pdf = env.pdf_for_direction(dir);
                    let mis_w = power_heuristic(last_scatter_pdf, env_pdf);
                    emission * mis_w
                }
            } else if config.use_sky_gradient {
                sky_gradient(&current_ray)
            } else {
                config.background
            };
            accumulated += throughput * bg;
            break;
        }

        // Accumulate emission from hit surfaces
        let emission = rec.material.emitted(rec.u, rec.v, rec.p);
        accumulated += throughput * emission;

        // NEE: sample environment light directly (non-delta materials only)
        if !rec.material.is_delta() {
            if let Some(ref env) = config.environment {
                let (light_dir, light_emission, light_pdf) = env.sample_direction(rng);
                // Shadow ray
                let shadow_ray = Ray::new(rec.p, light_dir, current_ray.time());
                let mut shadow_rec = HitRecord::default();
                if !world.hit(
                    &shadow_ray,
                    Interval::new(0.001, f32::INFINITY),
                    &mut shadow_rec,
                ) {
                    // Unoccluded - evaluate BSDF and MIS weight
                    let bsdf_val = rec.material.bsdf(&current_ray, &rec, &shadow_ray);
                    let bsdf_pdf = rec.material.pdf(&current_ray, &rec, &shadow_ray);
                    let mis_w = power_heuristic(light_pdf, bsdf_pdf);
                    let cos_theta = rec.normal.dot(light_dir).max(0.0);
                    accumulated += throughput * bsdf_val * light_emission * cos_theta * mis_w
                        / light_pdf.max(1e-10);
                }
            }
        }

        match rec.material.scatter(&current_ray, &rec, rng) {
            Some(result) => {
                last_scatter_pdf = result.pdf;
                last_was_delta = rec.material.is_delta();
                current_ray = result.scattered;
                throughput *= result.attenuation;
                if !result.pass_through {
                    remaining_depth -= 1;
                }
            }
            None => {
                break;
            }
        }
    }

    accumulated
}

/// AOV (Arbitrary Output Variable) data captured from the primary ray.
#[derive(Debug, Clone, Copy)]
pub struct AovData {
    /// Distance to first hit (f32::INFINITY if no hit).
    pub depth: f32,
    /// World-space normal at first hit (zero if no hit).
    pub normal: Color,
}

impl Default for AovData {
    fn default() -> Self {
        Self {
            depth: f32::INFINITY,
            normal: Color::ZERO,
        }
    }
}

/// Compute the color and AOV data seen by a ray.
///
/// This is identical to `ray_color` but also captures depth and normal
/// from the first hit for AOV output.
pub fn ray_color_with_aovs(
    ray: &Ray,
    world: &dyn Hittable,
    depth: u32,
    config: &RenderConfig,
    rng: &mut dyn RngCore,
) -> (Color, AovData) {
    let mut current_ray = *ray;
    let mut throughput = Color::ONE;
    let mut accumulated = Color::ZERO;
    let mut remaining_depth = depth;

    // AOV data - captured from first hit only
    let mut aov = AovData::default();
    let mut first_hit = true;

    // Track last scatter PDF for MIS weighting when hitting environment
    let mut last_scatter_pdf = 0.0_f32;
    let mut last_was_delta = true;

    loop {
        if remaining_depth == 0 {
            break;
        }

        let mut rec = HitRecord::default();

        if !world.hit(&current_ray, Interval::new(0.001, f32::INFINITY), &mut rec) {
            // Ray escaped - sample environment/background
            let bg = if let Some(ref env) = config.environment {
                let dir = current_ray.direction().normalize();
                let emission = env.sample(dir);
                if last_was_delta {
                    emission
                } else {
                    let env_pdf = env.pdf_for_direction(dir);
                    let mis_w = power_heuristic(last_scatter_pdf, env_pdf);
                    emission * mis_w
                }
            } else if config.use_sky_gradient {
                sky_gradient(&current_ray)
            } else {
                config.background
            };
            accumulated += throughput * bg;
            break;
        }

        // Capture AOV data from first hit
        if first_hit {
            aov.depth = rec.t;
            aov.normal = rec.normal;
            first_hit = false;
        }

        // Accumulate emission from hit surfaces
        let emission = rec.material.emitted(rec.u, rec.v, rec.p);
        accumulated += throughput * emission;

        // NEE: sample environment light directly (non-delta materials only)
        if !rec.material.is_delta() {
            if let Some(ref env) = config.environment {
                let (light_dir, light_emission, light_pdf) = env.sample_direction(rng);
                let shadow_ray = Ray::new(rec.p, light_dir, current_ray.time());
                let mut shadow_rec = HitRecord::default();
                if !world.hit(
                    &shadow_ray,
                    Interval::new(0.001, f32::INFINITY),
                    &mut shadow_rec,
                ) {
                    let bsdf_val = rec.material.bsdf(&current_ray, &rec, &shadow_ray);
                    let bsdf_pdf = rec.material.pdf(&current_ray, &rec, &shadow_ray);
                    let mis_w = power_heuristic(light_pdf, bsdf_pdf);
                    let cos_theta = rec.normal.dot(light_dir).max(0.0);
                    accumulated += throughput * bsdf_val * light_emission * cos_theta * mis_w
                        / light_pdf.max(1e-10);
                }
            }
        }

        match rec.material.scatter(&current_ray, &rec, rng) {
            Some(result) => {
                last_scatter_pdf = result.pdf;
                last_was_delta = rec.material.is_delta();
                current_ray = result.scattered;
                throughput *= result.attenuation;
                if !result.pass_through {
                    remaining_depth -= 1;
                }
            }
            None => {
                break;
            }
        }
    }

    (accumulated, aov)
}

/// Render a single pixel with multi-sampling, returning color and AOV data.
pub fn render_pixel_with_aovs(
    camera: &Camera,
    world: &dyn Hittable,
    x: u32,
    y: u32,
    config: &RenderConfig,
    rng: &mut dyn RngCore,
) -> (Color, AovData) {
    let mut pixel_color = Color::ZERO;
    let mut depth_sum = 0.0_f32;
    let mut normal_sum = Color::ZERO;
    let mut hit_count = 0u32;

    for _ in 0..config.samples_per_pixel {
        let ray = camera.get_ray(x, y, rng);
        let (color, aov) = ray_color_with_aovs(&ray, world, config.max_depth, config, rng);
        pixel_color += color;

        // Only average depth/normal from rays that hit something
        if aov.depth < f32::INFINITY {
            depth_sum += aov.depth;
            normal_sum += aov.normal;
            hit_count += 1;
        }
    }

    let avg_color = pixel_color / config.samples_per_pixel as f32;
    let avg_aov = if hit_count > 0 {
        AovData {
            depth: depth_sum / hit_count as f32,
            normal: (normal_sum / hit_count as f32).normalize(),
        }
    } else {
        AovData::default()
    };

    (avg_color, avg_aov)
}

/// Compute sky gradient background.
fn sky_gradient(ray: &Ray) -> Color {
    let unit_direction = ray.direction().normalize();
    let a = 0.5 * (unit_direction.y + 1.0);
    let white = Color::new(1.0, 1.0, 1.0);
    let blue = Color::new(0.5, 0.7, 1.0);
    white * (1.0 - a) + blue * a
}

/// Apply gamma correction (gamma = 2.0).
#[inline]
pub fn linear_to_gamma(linear: f32) -> f32 {
    if linear > 0.0 {
        linear.sqrt()
    } else {
        0.0
    }
}

/// Clamp a value to [0, 1] range.
#[inline]
pub fn clamp_01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// Convert a color to 8-bit RGBA.
pub fn color_to_rgba(color: Color) -> [u8; 4] {
    // Apply gamma correction and convert to 0-255
    let r = (255.0 * clamp_01(linear_to_gamma(color.x))) as u8;
    let g = (255.0 * clamp_01(linear_to_gamma(color.y))) as u8;
    let b = (255.0 * clamp_01(linear_to_gamma(color.z))) as u8;
    [r, g, b, 255]
}

/// Render a single pixel with multi-sampling.
pub fn render_pixel(
    camera: &Camera,
    world: &dyn Hittable,
    x: u32,
    y: u32,
    config: &RenderConfig,
    rng: &mut dyn RngCore,
) -> Color {
    let mut pixel_color = Color::ZERO;

    for _ in 0..config.samples_per_pixel {
        // Camera.get_ray already adds random offset for anti-aliasing
        let ray = camera.get_ray(x, y, rng);
        pixel_color += ray_color(&ray, world, config.max_depth, config, rng);
    }

    // Average the samples
    pixel_color / config.samples_per_pixel as f32
}

/// Simple image buffer for storing render output.
pub struct ImageBuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Color>,
}

impl ImageBuffer {
    /// Create a new image buffer filled with black.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![Color::ZERO; (width * height) as usize],
        }
    }

    /// Get the pixel at (x, y).
    pub fn get(&self, x: u32, y: u32) -> Color {
        self.pixels[(y * self.width + x) as usize]
    }

    /// Set the pixel at (x, y).
    pub fn set(&mut self, x: u32, y: u32, color: Color) {
        self.pixels[(y * self.width + x) as usize] = color;
    }

    /// Convert to RGBA bytes (for display or saving).
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity((self.width * self.height * 4) as usize);
        for color in &self.pixels {
            let rgba = color_to_rgba(*color);
            bytes.extend_from_slice(&rgba);
        }
        bytes
    }
}

/// Render the entire scene to an image buffer.
///
/// This is a simple single-threaded renderer for testing.
pub fn render(
    camera: &Camera,
    world: &dyn Hittable,
    config: &RenderConfig,
    rng: &mut dyn RngCore,
) -> ImageBuffer {
    let mut image = ImageBuffer::new(camera.image_width, camera.image_height);

    for y in 0..camera.image_height {
        for x in 0..camera.image_width {
            let color = render_pixel(camera, world, x, y, config, rng);
            image.set(x, y, color);
        }
    }

    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BvhNode, Lambertian, Sphere, Vec3};
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_sky_gradient() {
        // Ray pointing up should be more blue (less red than white)
        let up_ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), 0.0);
        let up_color = sky_gradient(&up_ray);

        // Ray pointing down should be more white (more red)
        let down_ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, -1.0, 0.0), 0.0);
        let down_color = sky_gradient(&down_ray);

        // Up color should have less red (more blue-ish) than down color (white)
        // blue = (0.5, 0.7, 1.0), white = (1.0, 1.0, 1.0)
        assert!(
            up_color.x < down_color.x,
            "up_color.x={} should be < down_color.x={}",
            up_color.x,
            down_color.x
        );
    }

    #[test]
    fn test_linear_to_gamma() {
        assert_eq!(linear_to_gamma(0.0), 0.0);
        assert!((linear_to_gamma(1.0) - 1.0).abs() < 0.0001);
        assert!((linear_to_gamma(0.25) - 0.5).abs() < 0.0001);
    }

    #[test]
    fn test_render_pixel() {
        // Create a simple scene with one sphere
        let sphere = Sphere::new(
            Vec3::new(0.0, 0.0, -1.0),
            0.5,
            Lambertian::new(Color::new(0.5, 0.5, 0.5)),
        );

        let objects: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(sphere)];
        let world = BvhNode::new(objects);

        // Create a camera
        let mut camera = Camera::new().with_resolution(10, 10);
        camera.initialize();

        let config = RenderConfig {
            samples_per_pixel: 4,
            max_depth: 5,
            background: Color::new(0.5, 0.7, 1.0),
            use_sky_gradient: false,
            environment: None,
        };

        let mut rng = StdRng::seed_from_u64(42);

        // Render center pixel (should hit the sphere)
        let color = render_pixel(&camera, &world, 5, 5, &config, &mut rng);

        // Color should not be the background (we hit the sphere)
        // Can't test exact color due to random sampling
        assert!(color.length() > 0.0);
    }
}
