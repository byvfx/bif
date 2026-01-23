//! HDRI environment map with importance sampling for path tracing.
//!
//! Provides environment lighting via equirectangular HDR images,
//! with luminance-weighted importance sampling for efficient
//! direct lighting estimation.

use std::f32::consts::PI;

use bif_core::hdr::HdrImage;
use rand::RngCore;

use crate::{gen_f32, Color, Vec3};

/// HDRI environment map for path tracing with importance sampling.
pub struct HdriEnvironment {
    hdr: HdrImage,
    rotation: f32,
    intensity: f32,

    // Importance sampling tables
    marginal_cdf: Vec<f32>,
    conditional_cdfs: Vec<Vec<f32>>,
    row_sums: Vec<f32>,
    total_power: f32,
    pdf: Vec<f32>,
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
            row_sums: Vec::new(),
            total_power: 0.0,
            pdf: Vec::new(),
        };
        env.build_distribution();
        env
    }

    /// Sample the environment in a given direction.
    pub fn sample(&self, dir: Vec3) -> Color {
        let dir_arr = [dir.x, dir.y, dir.z];
        let rgb = self.hdr.sample(dir_arr, self.rotation);
        Color::new(rgb[0], rgb[1], rgb[2]) * self.intensity
    }

    /// Importance-sample a direction from the environment.
    ///
    /// Returns (direction, emission_color, pdf_value).
    pub fn sample_direction(&self, rng: &mut dyn RngCore) -> (Vec3, Color, f32) {
        if self.total_power <= 0.0 {
            // Fallback: uniform sphere
            let dir = random_unit_sphere(rng);
            let emission = self.sample(dir);
            let pdf = 1.0 / (4.0 * PI);
            return (dir, emission, pdf);
        }

        let xi1 = gen_f32(rng);
        let xi2 = gen_f32(rng);

        // Sample row via marginal CDF
        let y = search_cdf(&self.marginal_cdf, xi1);
        // Sample column via conditional CDF for this row
        let x = search_cdf(&self.conditional_cdfs[y], xi2);

        let width = self.hdr.width as f32;
        let height = self.hdr.height as f32;

        let u = (x as f32 + 0.5) / width;
        let v = (y as f32 + 0.5) / height;

        let dir_arr = HdrImage::uv_to_direction(u, v, self.rotation);
        let dir = Vec3::new(dir_arr[0], dir_arr[1], dir_arr[2]);

        let emission = Color::new(
            self.hdr.pixels[y * self.hdr.width as usize + x][0],
            self.hdr.pixels[y * self.hdr.width as usize + x][1],
            self.hdr.pixels[y * self.hdr.width as usize + x][2],
        ) * self.intensity;

        let pdf = self.pdf_for_direction(dir);

        (dir, emission, pdf)
    }

    /// Get the PDF value for a given direction.
    pub fn pdf_for_direction(&self, dir: Vec3) -> f32 {
        if self.total_power <= 0.0 {
            return 1.0 / (4.0 * PI);
        }

        let dir_arr = [dir.x, dir.y, dir.z];
        let (u, v) = HdrImage::direction_to_uv(dir_arr, self.rotation);

        let x = (u * self.hdr.width as f32) as usize;
        let y = (v * self.hdr.height as f32) as usize;
        let x = x.min(self.hdr.width as usize - 1);
        let y = y.min(self.hdr.height as usize - 1);

        let idx = y * self.hdr.width as usize + x;

        // Convert pixel PDF to solid angle PDF
        let theta = (0.5 - v) * PI;
        let sin_theta = theta.cos().max(1e-10); // cos(elevation) = sin(polar)

        let width = self.hdr.width as f32;
        let height = self.hdr.height as f32;

        let pdf_solid_angle = self.pdf[idx] * (width * height) / (2.0 * PI * PI * sin_theta);

        pdf_solid_angle.max(1e-10)
    }

    /// Build the importance sampling distribution from pixel luminances.
    fn build_distribution(&mut self) {
        let width = self.hdr.width as usize;
        let height = self.hdr.height as usize;
        let total_pixels = width * height;

        self.pdf = vec![0.0; total_pixels];
        self.marginal_cdf = vec![0.0; height + 1];
        self.conditional_cdfs = vec![vec![0.0; width + 1]; height];
        self.row_sums = vec![0.0; height];
        self.total_power = 0.0;

        // Compute luminance-weighted PDF with sin(theta) correction
        for y in 0..height {
            let v = (y as f32 + 0.5) / height as f32;
            let theta = (0.5 - v) * PI;
            let sin_theta = theta.cos(); // cos(elevation) = sin(polar angle)

            self.conditional_cdfs[y][0] = 0.0;

            for x in 0..width {
                let idx = y * width + x;
                let pixel = self.hdr.pixels[idx];
                let luminance = HdrImage::luminance(pixel);
                let weight = (luminance * sin_theta).max(0.0);

                self.pdf[idx] = weight;
                self.row_sums[y] += weight;
                self.total_power += weight;

                self.conditional_cdfs[y][x + 1] = self.conditional_cdfs[y][x] + weight;
            }
        }

        // Normalize conditional CDFs
        for y in 0..height {
            if self.row_sums[y] > 0.0 {
                for x in 0..=width {
                    self.conditional_cdfs[y][x] /= self.row_sums[y];
                }
            }
        }

        // Build marginal CDF
        self.marginal_cdf[0] = 0.0;
        for y in 0..height {
            self.marginal_cdf[y + 1] = self.marginal_cdf[y] + self.row_sums[y];
        }

        // Normalize
        if self.total_power > 0.0 {
            for y in 0..=height {
                self.marginal_cdf[y] /= self.total_power;
            }
            for p in &mut self.pdf {
                *p /= self.total_power;
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

/// Binary search on a CDF to find the sample index.
fn search_cdf(cdf: &[f32], xi: f32) -> usize {
    let n = cdf.len() - 1;
    let mut low = 0usize;
    let mut high = n;

    while low < high {
        let mid = (low + high) / 2;
        if cdf[mid + 1] <= xi {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    low.min(n.saturating_sub(1))
}

/// Random unit vector on sphere (uniform distribution).
fn random_unit_sphere(rng: &mut dyn RngCore) -> Vec3 {
    let z = 2.0 * gen_f32(rng) - 1.0;
    let r = (1.0 - z * z).sqrt();
    let phi = 2.0 * PI * gen_f32(rng);
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
