//! HDRI environment map with importance sampling for path tracing.
//!
//! Provides environment lighting via equirectangular HDR images,
//! with luminance-weighted importance sampling for efficient
//! direct lighting estimation.

use std::f32::consts::PI;

#[cfg(feature = "bif-core")]
use bif_core::hdr::HdrImage;
use rand::RngCore;
use rayon::prelude::*;

use crate::{gen_f32_generic, Color, Vec3};

#[cfg(not(feature = "bif-core"))]
#[derive(Clone)]
pub struct HdrImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 3]>,
}

#[cfg(not(feature = "bif-core"))]
impl HdrImage {
    pub fn direction_to_uv(dir: [f32; 3], rotation: f32) -> (f32, f32) {
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if len < 1e-10 {
            return (0.5, 0.5);
        }
        let dx = dir[0] / len;
        let dy = dir[1] / len;
        let dz = dir[2] / len;

        let phi = dz.atan2(dx);
        let theta = dy.asin();
        let mut u = 0.5 + phi / (2.0 * PI);
        let v = 0.5 - theta / PI;
        u += rotation / (2.0 * PI);
        u -= u.floor();

        (u, v)
    }

    pub fn uv_to_direction(u: f32, v: f32, rotation: f32) -> [f32; 3] {
        let mut u = u - rotation / (2.0 * PI);
        u -= u.floor();

        let phi = (u - 0.5) * 2.0 * PI;
        let theta = (0.5 - v) * PI;

        let cos_theta = theta.cos();
        [cos_theta * phi.cos(), theta.sin(), cos_theta * phi.sin()]
    }

    pub fn pixel(&self, x: i32, y: i32) -> [f32; 3] {
        let x = x.max(0).min(self.width as i32 - 1) as usize;
        let y = y.max(0).min(self.height as i32 - 1) as usize;
        self.pixels[y * self.width as usize + x]
    }

    pub fn sample(&self, dir: [f32; 3], rotation: f32) -> [f32; 3] {
        let (u, v) = Self::direction_to_uv(dir, rotation);
        self.sample_uv(u, v)
    }

    pub fn sample_uv(&self, u: f32, v: f32) -> [f32; 3] {
        let w = self.width as f32;
        let h = self.height as f32;

        let px = u * w - 0.5;
        let py = v * h - 0.5;

        let x0 = px.floor() as i32;
        let y0 = py.floor() as i32;
        let x1 = x0 + 1;
        let y1 = y0 + 1;

        let fx = px - x0 as f32;
        let fy = py - y0 as f32;

        let wrap_x = |x: i32| ((x % self.width as i32) + self.width as i32) % self.width as i32;
        let x0w = wrap_x(x0);
        let x1w = wrap_x(x1);

        let y0c = y0.max(0).min(self.height as i32 - 1);
        let y1c = y1.max(0).min(self.height as i32 - 1);

        let c00 = self.pixel(x0w, y0c);
        let c10 = self.pixel(x1w, y0c);
        let c01 = self.pixel(x0w, y1c);
        let c11 = self.pixel(x1w, y1c);

        let lerp = |a: f32, b: f32, t: f32| a * (1.0 - t) + b * t;

        [
            lerp(lerp(c00[0], c10[0], fx), lerp(c01[0], c11[0], fx), fy),
            lerp(lerp(c00[1], c10[1], fx), lerp(c01[1], c11[1], fx), fy),
            lerp(lerp(c00[2], c10[2], fx), lerp(c01[2], c11[2], fx), fy),
        ]
    }

    pub fn luminance(rgb: [f32; 3]) -> f32 {
        0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
    }
}

/// HDRI environment map for path tracing with importance sampling.
pub struct HdriEnvironment {
    hdr: HdrImage,
    /// Rotation baked at construction time. Use `_with_params` methods for live overrides.
    rotation: f32,
    /// Intensity baked at construction time. Use `_with_params` methods for live overrides.
    intensity: f32,

    // Importance sampling tables
    marginal_cdf: Vec<f32>,
    conditional_cdfs: Vec<Vec<f32>>,
    total_power: f32,
}

impl std::fmt::Debug for HdriEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HdriEnvironment")
            .field("size", &format!("{}x{}", self.hdr.width, self.hdr.height))
            .field("rotation", &self.rotation)
            .field("intensity", &self.intensity)
            .field("total_power", &self.total_power)
            .finish()
    }
}

impl HdriEnvironment {
    /// Create a new HDRI environment from a loaded HDR image.
    pub fn new(hdr: HdrImage, rotation: f32, intensity: f32) -> Self {
        let mut env = Self {
            hdr,
            rotation,
            intensity,
            marginal_cdf: Vec::new(),
            conditional_cdfs: Vec::new(),
            total_power: 0.0,
        };
        env.build_distribution();
        env
    }

    /// Get the stored rotation (radians).
    pub fn rotation(&self) -> f32 {
        self.rotation
    }

    /// Get the stored intensity.
    pub fn intensity(&self) -> f32 {
        self.intensity
    }

    /// Sample the environment in a given direction with explicit rotation/intensity.
    pub fn sample_with_params(&self, dir: Vec3, rotation: f32, intensity: f32) -> Color {
        let dir_arr = [dir.x, dir.y, dir.z];
        let rgb = self.hdr.sample(dir_arr, rotation);
        Color::new(rgb[0], rgb[1], rgb[2]) * intensity
    }

    /// Sample the environment in a given direction (uses stored rotation/intensity).
    pub fn sample(&self, dir: Vec3) -> Color {
        self.sample_with_params(dir, self.rotation, self.intensity)
    }

    /// Importance-sample a direction with explicit rotation/intensity.
    ///
    /// Returns (direction, emission_color, pdf_value).
    /// CDFs are rotation-independent so rotation only affects direction mapping.
    pub fn sample_direction_with_params<R: RngCore + ?Sized>(
        &self,
        rng: &mut R,
        rotation: f32,
        intensity: f32,
    ) -> (Vec3, Color, f32) {
        if self.total_power <= 0.0 {
            let dir = random_unit_sphere(rng);
            let emission = self.sample_with_params(dir, rotation, intensity);
            let pdf = 1.0 / (4.0 * PI);
            return (dir, emission, pdf);
        }

        let xi1 = gen_f32_generic(rng);
        let xi2 = gen_f32_generic(rng);

        // Sample row via marginal CDF
        let y = self
            .marginal_cdf
            .partition_point(|&v| v <= xi1)
            .saturating_sub(1)
            .min(self.hdr.height as usize - 1);
        // Sample column via conditional CDF for this row
        let x = self.conditional_cdfs[y]
            .partition_point(|&v| v <= xi2)
            .saturating_sub(1)
            .min(self.hdr.width as usize - 1);

        let width = self.hdr.width as f32;
        let height = self.hdr.height as f32;

        let u = (x as f32 + 0.5) / width;
        let v = (y as f32 + 0.5) / height;

        let dir_arr = HdrImage::uv_to_direction(u, v, rotation);
        let dir = Vec3::new(dir_arr[0], dir_arr[1], dir_arr[2]);

        let emission = Color::new(
            self.hdr.pixels[y * self.hdr.width as usize + x][0],
            self.hdr.pixels[y * self.hdr.width as usize + x][1],
            self.hdr.pixels[y * self.hdr.width as usize + x][2],
        ) * intensity;

        let pdf = self.pdf_for_direction_with_params(dir, rotation);

        (dir, emission, pdf)
    }

    /// Importance-sample a direction (uses stored rotation/intensity).
    ///
    /// Returns (direction, emission_color, pdf_value).
    pub fn sample_direction<R: RngCore + ?Sized>(&self, rng: &mut R) -> (Vec3, Color, f32) {
        self.sample_direction_with_params(rng, self.rotation, self.intensity)
    }

    /// Get the PDF value for a given direction with explicit rotation.
    pub fn pdf_for_direction_with_params(&self, dir: Vec3, rotation: f32) -> f32 {
        if self.total_power <= 0.0 {
            return 1.0 / (4.0 * PI);
        }

        let dir_arr = [dir.x, dir.y, dir.z];
        let (u, v) = HdrImage::direction_to_uv(dir_arr, rotation);

        let x = (u * self.hdr.width as f32) as usize;
        let y = (v * self.hdr.height as f32) as usize;
        let x = x.min(self.hdr.width as usize - 1);
        let y = y.min(self.hdr.height as usize - 1);

        // Reconstruct pixel PDF from CDFs
        let marginal_pdf = self.marginal_cdf[y + 1] - self.marginal_cdf[y];
        let conditional_pdf = self.conditional_cdfs[y][x + 1] - self.conditional_cdfs[y][x];
        let pixel_pdf = marginal_pdf * conditional_pdf;

        // Convert pixel PDF to solid angle PDF
        // Clamp v away from poles to avoid MIS fireflies at singularities
        let half_texel = 0.5 / self.hdr.height as f32;
        let v_clamped = v.clamp(half_texel, 1.0 - half_texel);
        let theta = (0.5 - v_clamped) * PI;
        let sin_polar = theta.cos().max(1e-10); // cos(elevation) = sin(polar angle)

        let width = self.hdr.width as f32;
        let height = self.hdr.height as f32;

        let pdf_solid_angle = pixel_pdf * (width * height) / (2.0 * PI * PI * sin_polar);

        pdf_solid_angle.max(1e-10)
    }

    /// Get the PDF value for a given direction (uses stored rotation).
    pub fn pdf_for_direction(&self, dir: Vec3) -> f32 {
        self.pdf_for_direction_with_params(dir, self.rotation)
    }

    /// Build the importance sampling distribution from pixel luminances.
    /// Uses rayon for parallel row processing.
    fn build_distribution(&mut self) {
        let width = self.hdr.width as usize;
        let height = self.hdr.height as usize;
        let pixels = &self.hdr.pixels;

        // Compute each row's conditional CDF and row sum in parallel
        let row_data: Vec<(f32, Vec<f32>)> = (0..height)
            .into_par_iter()
            .map(|y| {
                let v = (y as f32 + 0.5) / height as f32;
                let theta = (0.5 - v) * PI;
                let sin_polar = theta.cos(); // cos(elevation) = sin(polar angle)

                let mut conditional_cdf = vec![0.0f32; width + 1];
                let mut row_sum = 0.0f32;

                for x in 0..width {
                    let idx = y * width + x;
                    let pixel = pixels[idx];
                    let luminance = HdrImage::luminance(pixel);
                    let weight = (luminance * sin_polar).max(0.0);

                    row_sum += weight;
                    conditional_cdf[x + 1] = conditional_cdf[x] + weight;
                }

                // Normalize this row's CDF
                if row_sum > 0.0 {
                    for val in &mut conditional_cdf {
                        *val /= row_sum;
                    }
                }

                (row_sum, conditional_cdf)
            })
            .collect();

        // Unpack parallel results and build marginal CDF (sequential, small)
        let mut row_sums = Vec::with_capacity(height);
        self.conditional_cdfs = Vec::with_capacity(height);
        for (row_sum, cdf) in row_data {
            row_sums.push(row_sum);
            self.conditional_cdfs.push(cdf);
        }

        self.total_power = row_sums.iter().sum();
        self.marginal_cdf = vec![0.0; height + 1];
        for (y, row_sum) in row_sums.iter().enumerate() {
            self.marginal_cdf[y + 1] = self.marginal_cdf[y] + *row_sum;
        }
        if self.total_power > 0.0 {
            for val in &mut self.marginal_cdf {
                *val /= self.total_power;
            }
        }

        log::info!(
            "HDRI: Built importance sampling distribution ({}x{}, power: {:.2})",
            width,
            height,
            self.total_power
        );
    }
}

/// Random unit vector on sphere (uniform distribution).
fn random_unit_sphere<R: RngCore + ?Sized>(rng: &mut R) -> Vec3 {
    let z = 2.0 * gen_f32_generic(rng) - 1.0;
    let r = (1.0 - z * z).sqrt();
    let phi = 2.0 * PI * gen_f32_generic(rng);
    Vec3::new(r * phi.cos(), r * phi.sin(), z)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hdr_image() -> HdrImage {
        let width = 16;
        let height = 8;
        let mut pixels = Vec::with_capacity((width * height) as usize);

        for y in 0..height {
            let yf = y as f32 / (height - 1) as f32;
            for x in 0..width {
                let xf = x as f32 / (width - 1) as f32;
                let hotspot = if (6..=9).contains(&x) { 6.0 } else { 0.0 };
                pixels.push([
                    0.25 + xf * 1.5 + hotspot,
                    0.10 + yf * 0.5 + hotspot * 0.2,
                    0.05 + (1.0 - xf) * 0.35,
                ]);
            }
        }

        HdrImage {
            width,
            height,
            pixels,
        }
    }

    #[test]
    fn create_hdri_environment() {
        let hdr = test_hdr_image();
        let env = HdriEnvironment::new(hdr, 0.0, 1.0);
        assert!(env.total_power > 0.0);
    }

    #[test]
    fn sample_returns_positive() {
        let hdr = test_hdr_image();
        let env = HdriEnvironment::new(hdr, 0.0, 1.0);

        let dirs = [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];

        for dir in &dirs {
            let color = env.sample(*dir);
            assert!(color.x >= 0.0 && color.x.is_finite());
            assert!(color.y >= 0.0 && color.y.is_finite());
            assert!(color.z >= 0.0 && color.z.is_finite());
        }
    }

    #[test]
    fn importance_sample_directions() {
        let hdr = test_hdr_image();
        let env = HdriEnvironment::new(hdr, 0.0, 1.0);
        let mut rng = rand::thread_rng();

        for _ in 0..100 {
            let (dir, emission, pdf) = env.sample_direction(&mut rng);
            assert!(
                (dir.length() - 1.0).abs() < 0.01,
                "Direction not unit: {}",
                dir.length()
            );
            assert!(pdf > 0.0, "PDF should be positive");
            assert!(emission.x >= 0.0 && emission.x.is_finite());
        }
    }

    #[test]
    fn pdf_matches_direction() {
        let hdr = test_hdr_image();
        let env = HdriEnvironment::new(hdr, 0.0, 1.0);

        let dir = Vec3::new(1.0, 0.5, 0.3).normalize();
        let pdf = env.pdf_for_direction(dir);
        assert!(pdf > 0.0 && pdf.is_finite());
    }

    #[test]
    fn sample_with_params_rotates() {
        let hdr = test_hdr_image();
        let env = HdriEnvironment::new(hdr, 0.0, 1.0);
        let dir = Vec3::new(1.0, 0.0, 0.0);
        let c0 = env.sample_with_params(dir, 0.0, 1.0);
        let c1 = env.sample_with_params(dir, PI, 1.0);
        // 180 degree rotation should produce different color for non-uniform HDRI
        let diff = (c0.x - c1.x).abs() + (c0.y - c1.y).abs() + (c0.z - c1.z).abs();
        assert!(
            diff > 0.001,
            "Rotated sample should differ: c0={c0:?}, c1={c1:?}"
        );
    }

    #[test]
    fn sample_with_params_scales_intensity() {
        let hdr = test_hdr_image();
        let env = HdriEnvironment::new(hdr, 0.0, 1.0);
        let dir = Vec3::new(0.0, 1.0, 0.0);
        let c1 = env.sample_with_params(dir, 0.0, 1.0);
        let c2 = env.sample_with_params(dir, 0.0, 2.0);
        // Double intensity should double the color
        assert!((c2.x - c1.x * 2.0).abs() < 0.001);
        assert!((c2.y - c1.y * 2.0).abs() < 0.001);
        assert!((c2.z - c1.z * 2.0).abs() < 0.001);
    }
}
