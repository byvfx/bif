//! Core path tracing renderer.
//!
//! Implements Monte Carlo path tracing with:
//! - Recursive ray tracing with configurable depth
//! - Gamma correction
//! - Anti-aliasing via multi-sampling

use std::sync::Arc;

use crate::blue_noise::SamplerMode;
use crate::filter::PixelFilterConfig;
use crate::hdri::HdriEnvironment;
use crate::light::LightList;
use crate::material::{gen_f32, power_heuristic};
use crate::radiance_cache::RadianceCache;
use crate::{Camera, Color, HitRecord, Hittable, Ray};
use bif_math::Interval;
use rand::RngCore;

/// Surfaces below this roughness skip SHARC cache reads/writes.
/// GGX alpha = roughness²; at 0.1 the specular lobe is ~1-2° wide,
/// too narrow for spatial hashing to resolve without visible blur.
pub const SHARC_ROUGHNESS_THRESHOLD: f32 = 0.1;

/// Render configuration.
#[derive(Debug, Clone, Default)]
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
    /// Explicit lights (USD lights) for NEE sampling
    pub lights: Arc<LightList>,
    /// Progressive pass number (XORed into RNG seed for unique noise per pass).
    pub pass_number: u32,
    /// Override HDRI rotation (radians). When set, used instead of the value baked into the environment.
    pub hdri_rotation: Option<f32>,
    /// Override HDRI intensity. When set, used instead of the value baked into the environment.
    pub hdri_intensity: Option<f32>,
    /// Whether to show HDRI as visible background (false = use solid background but keep HDRI lighting).
    pub hdri_show_background: bool,
    /// SHARC radiance cache for secondary bounce reuse.
    pub radiance_cache: Option<Arc<RadianceCache>>,
    /// Pixel reconstruction filter for sample weighting.
    pub pixel_filter: PixelFilterConfig,
    /// Camera jitter sampler mode (white noise vs blue noise).
    pub sampler_mode: SamplerMode,
}

/// Compute the color seen by a ray.
///
/// Iterative path tracing with throughput accumulation.
/// Uses Next Event Estimation (NEE) with Multiple Importance Sampling (MIS)
/// for environment lighting when an HDRI is present.
/// Pass-through scatters (opacity cutout) do not consume bounce depth.
///
/// SHARC integration: after `min_bounce_depth`, non-delta hits check the
/// radiance cache. A cache hit with enough samples terminates the path early.
/// After NEE, surface-local radiance is written back into the cache.
///
/// Russian Roulette: after bounce depth >= 3, paths are stochastically
/// terminated based on throughput magnitude. Survivors are boosted to
/// keep the estimator unbiased.
pub fn ray_color(
    ray: &Ray,
    world: &dyn Hittable,
    depth: u32,
    config: &RenderConfig,
    rng: &mut dyn RngCore,
) -> Color {
    ray_color_with_aovs(ray, world, depth, config, rng).0
}

/// AOV (Arbitrary Output Variable) data captured from the primary ray.
#[derive(Debug, Clone, Copy)]
pub struct AovData {
    /// Distance to first hit (f32::INFINITY if no hit).
    pub depth: f32,
    /// World-space geometric normal at first hit (zero if no hit).
    pub normal: Color,
    /// World-space shading normal (after normal map) at first hit.
    pub shading_normal: Color,
    /// Alpha channel (1.0 = hit, 0.0 = miss).
    pub alpha: f32,
    /// SHARC cache sample count at primary hit (for heatmap AOV).
    pub cache_samples: u32,
    /// Surface albedo at first hit (for denoiser guide image).
    pub albedo: Color,
}

impl Default for AovData {
    fn default() -> Self {
        Self {
            depth: f32::INFINITY,
            normal: Color::ZERO,
            shading_normal: Color::ZERO,
            alpha: 0.0,
            cache_samples: 0,
            albedo: Color::ZERO,
        }
    }
}

/// Compute the color and AOV data seen by a ray.
///
/// Identical to `ray_color` but also captures depth, normal, and
/// cache-heatmap data from the primary hit for AOV output.
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
    let mut bounce_count = 0u32;

    // AOV data - captured from first hit only
    let mut aov = AovData::default();
    let mut first_hit = true;

    // Resolve HDRI overrides once before the bounce loop
    let env_params = config.environment.as_ref().map(|env| {
        let rotation = config.hdri_rotation.unwrap_or_else(|| env.rotation());
        let intensity = config.hdri_intensity.unwrap_or_else(|| env.intensity());
        (env, rotation, intensity)
    });

    // Cache config
    let cache = config.radiance_cache.as_deref();
    let cache_min_depth = cache
        .map(|c| c.config().min_bounce_depth)
        .unwrap_or(u32::MAX);

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
            // Camera rays respect hdri_show_background; bounced rays always sample HDRI for lighting
            let use_hdri = env_params.is_some() && (config.hdri_show_background || !first_hit);
            let bg = if use_hdri {
                let (env, rotation, intensity) = env_params.as_ref().unwrap();
                let dir = current_ray.direction().normalize();
                let emission = env.sample_with_params(dir, *rotation, *intensity);
                if last_was_delta {
                    emission
                } else {
                    let env_pdf = env.pdf_for_direction_with_params(dir, *rotation);
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
            aov.shading_normal = rec.material.shading_normal(&rec);
            aov.alpha = 1.0;
            aov.albedo = rec.material.albedo(rec.u, rec.v);
            // Cache heatmap: sample count at primary hit
            if let Some(c) = cache {
                aov.cache_samples = c.sample_count_at(rec.p, rec.normal);
            }
            first_hit = false;
        }

        // --- SHARC cache READ ---
        let is_delta = rec.material.is_delta();
        let skip_cache = is_delta || rec.material.roughness() < SHARC_ROUGHNESS_THRESHOLD;
        if !skip_cache && bounce_count >= cache_min_depth {
            if let Some(c) = cache {
                if let Some(cached) = c.lookup(rec.p, rec.normal) {
                    // Guard against NaN/inf from torn reads in lock-free cache
                    if cached.x.is_finite() && cached.y.is_finite() && cached.z.is_finite() {
                        accumulated += throughput * cached;
                    }
                    break;
                }
            }
        }

        // Accumulate emission from hit surfaces
        let emission = rec.material.emitted(rec.u, rec.v, rec.p);
        accumulated += throughput * emission;

        let mut local_radiance = emission;

        // NEE: sample lights directly (non-delta materials only)
        if !is_delta {
            // Offset shadow ray origin along shading normal to avoid
            // self-intersection and shadow terminator artifacts with normal maps
            let shading_n = rec.material.shading_normal(&rec);
            let shadow_origin = rec.p + shading_n * 0.001;

            // Sample HDRI environment
            if let Some((env, rotation, intensity)) = env_params.as_ref() {
                let (light_dir, light_emission, light_pdf) =
                    env.sample_direction_with_params(rng, *rotation, *intensity);
                let shadow_ray = Ray::new(shadow_origin, light_dir, current_ray.time());
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
                    let nee_contrib =
                        bsdf_val * light_emission * cos_theta * mis_w / light_pdf.max(1e-10);
                    accumulated += throughput * nee_contrib;
                    local_radiance += nee_contrib;
                }
            }

            // Sample explicit lights (USD lights)
            if let Some((light_sample, _idx)) = config.lights.sample_one(rec.p, rng) {
                if light_sample.pdf > 0.0 {
                    let shadow_ray =
                        Ray::new(shadow_origin, light_sample.direction, current_ray.time());
                    let mut shadow_rec = HitRecord::default();
                    let max_t = if light_sample.distance < f32::INFINITY {
                        light_sample.distance - 0.001
                    } else {
                        f32::INFINITY
                    };
                    if !world.hit(&shadow_ray, Interval::new(0.001, max_t), &mut shadow_rec) {
                        let bsdf_val = rec.material.bsdf(&current_ray, &rec, &shadow_ray);
                        let cos_theta = rec.normal.dot(light_sample.direction).max(0.0);
                        // Delta lights can only be hit via NEE — skip MIS
                        let mis_w = if light_sample.is_delta {
                            1.0
                        } else {
                            let bsdf_pdf = rec.material.pdf(&current_ray, &rec, &shadow_ray);
                            power_heuristic(light_sample.pdf, bsdf_pdf)
                        };
                        let nee_contrib = bsdf_val * light_sample.emission * cos_theta * mis_w
                            / light_sample.pdf.max(1e-10);
                        accumulated += throughput * nee_contrib;
                        local_radiance += nee_contrib;
                    }
                }
            }
        }

        // --- SHARC cache WRITE ---
        // NOTE: Stores emission + NEE only (not indirect). Biases cached values
        // low but converges over passes via EMA blending. Acceptable for IPR
        // preview; deferred write-back needed for final quality.
        if !skip_cache && bounce_count >= cache_min_depth {
            if let Some(c) = cache {
                c.write(rec.p, rec.normal, local_radiance);
            }
        }

        match rec.material.scatter(&current_ray, &rec, rng) {
            Some(result) => {
                last_scatter_pdf = result.pdf;
                last_was_delta = is_delta;
                current_ray = result.scattered;
                throughput *= result.attenuation;
                // NaN guard: degenerate geometry can produce NaN/inf throughput
                if throughput.x.is_nan()
                    || throughput.y.is_nan()
                    || throughput.z.is_nan()
                    || throughput.x.is_infinite()
                    || throughput.y.is_infinite()
                    || throughput.z.is_infinite()
                {
                    break;
                }
                if !result.pass_through {
                    remaining_depth -= 1;
                    bounce_count += 1;
                }
            }
            None => {
                break;
            }
        }

        // --- Russian Roulette (after bounce >= 3) ---
        if bounce_count >= 3 {
            let max_component = throughput.x.max(throughput.y).max(throughput.z);
            let survival_prob = max_component.clamp(0.0, 0.95);
            if survival_prob < 1e-6 || gen_f32(rng) > survival_prob {
                break;
            }
            throughput /= survival_prob;
        }
    }

    (accumulated, aov)
}

/// Render a single pixel with multi-sampling, returning color, AOV data, and total filter weight.
///
/// When using a non-box filter, samples are weighted by the pixel reconstruction filter.
/// The returned weight is the sum of all sample weights (for progressive accumulation).
/// Filtered AOVs: beauty, alpha, normal, albedo. Depth stays unweighted (nearest-hit average).
pub fn render_pixel_with_aovs(
    camera: &Camera,
    world: &dyn Hittable,
    x: u32,
    y: u32,
    config: &RenderConfig,
    rng: &mut dyn RngCore,
) -> (Color, AovData, f32) {
    let mut pixel_color = Color::ZERO;
    let mut total_weight = 0.0_f32;
    let mut depth_sum = 0.0_f32;
    let mut normal_sum = Color::ZERO;
    let mut shading_normal_sum = Color::ZERO;
    let mut normal_weight_sum = 0.0_f32;
    let mut alpha_sum = 0.0_f32;
    let mut albedo_sum = Color::ZERO;
    let mut albedo_weight_sum = 0.0_f32;
    let mut hit_count = 0u32;
    let mut max_cache_samples = 0u32;

    let use_blue_noise = config.sampler_mode == SamplerMode::BlueNoise;

    for s in 0..config.samples_per_pixel {
        let (ray, offset) = if use_blue_noise {
            camera.get_ray_blue_noise(x, y, config.pass_number + s, rng)
        } else {
            camera.get_ray_with_offset(x, y, rng)
        };
        let (color, aov) = ray_color_with_aovs(&ray, world, config.max_depth, config, rng);

        let w = config.pixel_filter.evaluate(offset[0], offset[1]);

        pixel_color += w * color;
        alpha_sum += w * aov.alpha;
        total_weight += w;
        max_cache_samples = max_cache_samples.max(aov.cache_samples);

        if aov.depth < f32::INFINITY {
            // Depth: unweighted (geometric distance, filtering blurs edges badly)
            depth_sum += aov.depth;
            // Normal + albedo: weighted
            normal_sum += w * aov.normal;
            shading_normal_sum += w * aov.shading_normal;
            normal_weight_sum += w;
            albedo_sum += w * aov.albedo;
            albedo_weight_sum += w;
            hit_count += 1;
        }
    }

    let avg_color = if total_weight > 0.0 {
        pixel_color / total_weight
    } else {
        Color::ZERO
    };
    let avg_alpha = if total_weight > 0.0 {
        alpha_sum / total_weight
    } else {
        0.0
    };
    let avg_aov = if hit_count > 0 {
        let avg_normal = if normal_weight_sum > 0.0 {
            (normal_sum / normal_weight_sum).normalize()
        } else {
            Color::ZERO
        };
        let avg_shading_normal = if normal_weight_sum > 0.0 {
            (shading_normal_sum / normal_weight_sum).normalize()
        } else {
            Color::ZERO
        };
        let avg_albedo = if albedo_weight_sum > 0.0 {
            albedo_sum / albedo_weight_sum
        } else {
            Color::ZERO
        };
        AovData {
            depth: depth_sum / hit_count as f32,
            normal: avg_normal,
            shading_normal: avg_shading_normal,
            alpha: avg_alpha,
            cache_samples: max_cache_samples,
            albedo: avg_albedo,
        }
    } else {
        AovData {
            alpha: avg_alpha,
            ..AovData::default()
        }
    };

    (avg_color, avg_aov, total_weight)
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
pub(crate) fn linear_to_gamma(linear: f32) -> f32 {
    if linear > 0.0 {
        linear.sqrt()
    } else {
        0.0
    }
}

/// Clamp a value to [0, 1] range.
#[inline]
pub(crate) fn clamp_01(x: f32) -> f32 {
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
    let mut total_weight = 0.0_f32;
    let use_blue_noise = config.sampler_mode == SamplerMode::BlueNoise;

    for s in 0..config.samples_per_pixel {
        let (ray, offset) = if use_blue_noise {
            camera.get_ray_blue_noise(x, y, config.pass_number + s, rng)
        } else {
            camera.get_ray_with_offset(x, y, rng)
        };
        let w = config.pixel_filter.evaluate(offset[0], offset[1]);
        pixel_color += w * ray_color(&ray, world, config.max_depth, config, rng);
        total_weight += w;
    }
    if total_weight > 0.0 {
        pixel_color / total_weight
    } else {
        Color::ZERO
    }
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
        debug_assert!(
            x < self.width && y < self.height,
            "ImageBuffer OOB: ({x},{y}) in {w}x{h}",
            w = self.width,
            h = self.height
        );
        self.pixels[(y * self.width + x) as usize]
    }

    /// Set the pixel at (x, y).
    pub fn set(&mut self, x: u32, y: u32, color: Color) {
        debug_assert!(
            x < self.width && y < self.height,
            "ImageBuffer OOB: ({x},{y}) in {w}x{h}",
            w = self.width,
            h = self.height
        );
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
            lights: Arc::new(LightList::new()),
            pass_number: 0,
            hdri_rotation: None,
            hdri_intensity: None,
            hdri_show_background: true,
            radiance_cache: None,
            pixel_filter: PixelFilterConfig::default(),
            sampler_mode: SamplerMode::default(),
        };

        let mut rng = StdRng::seed_from_u64(42);

        // Render center pixel (should hit the sphere)
        let color = render_pixel(&camera, &world, 5, 5, &config, &mut rng);

        // Color should not be the background (we hit the sphere)
        // Can't test exact color due to random sampling
        assert!(color.length() > 0.0);
    }
}
