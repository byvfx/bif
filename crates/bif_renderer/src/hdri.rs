//! HDRI environment map with importance sampling for path tracing.
//!
//! Provides environment lighting via equirectangular HDR images,
//! with luminance-weighted importance sampling for efficient
//! direct lighting estimation.

use std::f32::consts::PI;

use bif_core::hdr::HdrImage;
use rand::RngCore;
use rayon::prelude::*;

use crate::{gen_f32_generic, Color, Vec3};

/// HDRI environment map for path tracing with importance sampling.
pub struct HdriEnvironment {
    hdr: HdrImage,
    rotation: f32,
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
        let theta = (0.5 - v) * PI;
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

    fn test_hdr_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../legacy/go-raytracing/assets/hdri/abandoned_hall_01_1k.hdr")
    }

    #[test]
    fn create_hdri_environment() {
        let hdr = HdrImage::load(test_hdr_path()).expect("Failed to load HDR");
        let env = HdriEnvironment::new(hdr, 0.0, 1.0);
        assert!(env.total_power > 0.0);
    }

    #[test]
    fn sample_returns_positive() {
        let hdr = HdrImage::load(test_hdr_path()).expect("Failed to load HDR");
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
        let hdr = HdrImage::load(test_hdr_path()).expect("Failed to load HDR");
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
        let hdr = HdrImage::load(test_hdr_path()).expect("Failed to load HDR");
        let env = HdriEnvironment::new(hdr, 0.0, 1.0);

        let dir = Vec3::new(1.0, 0.5, 0.3).normalize();
        let pdf = env.pdf_for_direction(dir);
        assert!(pdf > 0.0 && pdf.is_finite());
    }
}
