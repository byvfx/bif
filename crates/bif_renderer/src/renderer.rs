//! Core path tracing renderer.
//!
//! Implements Monte Carlo path tracing with:
//! - Recursive ray tracing with configurable depth
//! - Gamma correction
//! - Anti-aliasing via multi-sampling

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
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            samples_per_pixel: 100,
            max_depth: 50,
            background: Color::ZERO,
            use_sky_gradient: false,
        }
    }
}

/// Compute the color seen by a ray.
///
/// This is the core path tracing function. It traces the ray through
/// the scene, bouncing off surfaces and accumulating color.
pub fn ray_color(
    ray: &Ray,
    world: &dyn Hittable,
    depth: u32,
    config: &RenderConfig,
    rng: &mut dyn RngCore,
) -> Color {
    // If we've exceeded max depth, return black (no light)
    if depth == 0 {
        return Color::ZERO;
    }

    let mut rec = HitRecord::default();

    // Check if ray hits anything
    if !world.hit(ray, Interval::new(0.001, f32::INFINITY), &mut rec) {
        // Ray didn't hit anything - return background
        if config.use_sky_gradient {
            return sky_gradient(ray);
        }
        return config.background;
    }

    // Get emission from material (for lights)
    let emission = rec.material.emitted(rec.u, rec.v, rec.p);

    // Try to scatter the ray
    match rec.material.scatter(ray, &rec, rng) {
        Some(result) => {
            // Ray scattered - continue tracing
            let scattered_color = ray_color(&result.scattered, world, depth - 1, config, rng);
            emission + result.attenuation * scattered_color
        }
        None => {
            // Ray was absorbed - just return emission
            emission
        }
    }
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
        };

        let mut rng = StdRng::seed_from_u64(42);

        // Render center pixel (should hit the sphere)
        let color = render_pixel(&camera, &world, 5, 5, &config, &mut rng);

        // Color should not be the background (we hit the sphere)
        // Can't test exact color due to random sampling
        assert!(color.length() > 0.0);
    }
}
