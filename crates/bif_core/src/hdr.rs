//! HDR (Radiance RGBE) image loading and equirectangular sampling.
//!
//! Provides loading of `.hdr` files and bilinear-filtered lookups
//! via equirectangular projection for environment mapping.

use std::f32::consts::PI;
use std::io::BufReader;
use std::path::Path;

use image::codecs::hdr::HdrDecoder;
use thiserror::Error;

/// Errors from HDR loading.
#[derive(Debug, Error)]
pub enum HdrError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HDR decode error: {0}")]
    Decode(#[from] image::ImageError),
    #[cfg(feature = "oiio")]
    #[error("OIIO error: {0}")]
    Oiio(#[from] crate::oiio::OiioError),
}

pub type HdrResult<T> = Result<T, HdrError>;

/// An HDR image stored as linear RGB f32 pixels.
#[derive(Clone)]
pub struct HdrImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 3]>,
}

impl HdrImage {
    /// Load an HDR (Radiance RGBE) file from disk.
    pub fn load(path: impl AsRef<Path>) -> HdrResult<Self> {
        let path = path.as_ref();
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());

        #[cfg(feature = "oiio")]
        {
            if let Some(ext) = ext.as_deref() {
                if matches!(ext, "hdr" | "exr" | "tx") {
                    return Self::load_with_oiio(path);
                }
            }
        }

        match ext.as_deref() {
            Some("exr") => Self::load_exr(path),
            _ => Self::load_hdr(path),
        }
    }

    fn load_hdr(path: &Path) -> HdrResult<Self> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        let decoder = HdrDecoder::new(reader).map_err(HdrError::Decode)?;

        let meta = decoder.metadata();
        let width = meta.width;
        let height = meta.height;

        let rgb_data = decoder.read_image_hdr().map_err(HdrError::Decode)?;

        let pixels: Vec<[f32; 3]> = rgb_data.into_iter().map(|p| [p[0], p[1], p[2]]).collect();

        log::info!(
            "Loaded HDR (Radiance): {}x{} ({} pixels)",
            width,
            height,
            pixels.len()
        );

        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    fn load_exr(path: &Path) -> HdrResult<Self> {
        let img = image::open(path).map_err(HdrError::Decode)?;
        let rgb = img.to_rgb32f();
        let (width, height) = rgb.dimensions();
        let pixels: Vec<[f32; 3]> = rgb
            .pixels()
            .map(|p| [p[0], p[1], p[2]])
            .collect();

        log::info!(
            "Loaded EXR: {}x{} ({} pixels)",
            width,
            height,
            pixels.len()
        );

        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    #[cfg(feature = "oiio")]
    fn load_with_oiio(path: &Path) -> HdrResult<Self> {
        let hdr = crate::oiio::load_hdr_image(path)?;
        log::info!(
            "Loaded HDRI via OIIO: {}x{} ({} pixels)",
            hdr.width,
            hdr.height,
            hdr.pixels.len()
        );
        Ok(Self {
            width: hdr.width,
            height: hdr.height,
            pixels: hdr.pixels,
        })
    }

    /// Convert a 3D direction to equirectangular UV coordinates.
    ///
    /// `rotation` is in radians. Returns (u, v) in [0, 1].
    pub fn direction_to_uv(dir: [f32; 3], rotation: f32) -> (f32, f32) {
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        let dx = dir[0] / len;
        let dy = dir[1] / len;
        let dz = dir[2] / len;

        let phi = dz.atan2(dx); // [-PI, PI]
        let theta = dy.asin(); // [-PI/2, PI/2]

        let mut u = 0.5 + phi / (2.0 * PI);
        let v = 0.5 - theta / PI;

        // Apply rotation
        u += rotation / (2.0 * PI);
        // Wrap to [0, 1]
        u -= u.floor();

        (u, v)
    }

    /// Convert equirectangular UV coordinates to a 3D direction.
    ///
    /// `rotation` is in radians.
    pub fn uv_to_direction(u: f32, v: f32, rotation: f32) -> [f32; 3] {
        let mut u = u - rotation / (2.0 * PI);
        u -= u.floor();

        let phi = (u - 0.5) * 2.0 * PI;
        let theta = (0.5 - v) * PI;

        let cos_theta = theta.cos();
        [cos_theta * phi.cos(), theta.sin(), cos_theta * phi.sin()]
    }

    /// Get a pixel by integer coordinates, clamped to valid range.
    pub fn pixel(&self, x: i32, y: i32) -> [f32; 3] {
        let x = x.max(0).min(self.width as i32 - 1) as usize;
        let y = y.max(0).min(self.height as i32 - 1) as usize;
        self.pixels[y * self.width as usize + x]
    }

    /// Sample the HDR image with bilinear filtering at the given direction.
    ///
    /// `rotation` is in radians.
    pub fn sample(&self, dir: [f32; 3], rotation: f32) -> [f32; 3] {
        let (u, v) = Self::direction_to_uv(dir, rotation);
        self.sample_uv(u, v)
    }

    /// Bilinear-filtered sample at UV coordinates in [0, 1].
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

        // Wrap x for seamless horizontal tiling
        let wrap_x = |x: i32| ((x % self.width as i32) + self.width as i32) % self.width as i32;
        let x0w = wrap_x(x0);
        let x1w = wrap_x(x1);

        // Clamp y
        let y0c = y0.max(0).min(self.height as i32 - 1);
        let y1c = y1.max(0).min(self.height as i32 - 1);

        let c00 = self.pixel(x0w, y0c);
        let c10 = self.pixel(x1w, y0c);
        let c01 = self.pixel(x0w, y1c);
        let c11 = self.pixel(x1w, y1c);

        // Bilinear interpolation
        let lerp = |a: f32, b: f32, t: f32| a * (1.0 - t) + b * t;

        [
            lerp(lerp(c00[0], c10[0], fx), lerp(c01[0], c11[0], fx), fy),
            lerp(lerp(c00[1], c10[1], fx), lerp(c01[1], c11[1], fx), fy),
            lerp(lerp(c00[2], c10[2], fx), lerp(c01[2], c11[2], fx), fy),
        ]
    }

    /// Luminance of an RGB pixel (Rec. 709).
    pub fn luminance(rgb: [f32; 3]) -> f32 {
        0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hdr_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../legacy/go-raytracing/assets/hdri/abandoned_hall_01_1k.hdr")
    }

    #[test]
    fn load_hdr_file() {
        let img = HdrImage::load(test_hdr_path()).expect("Failed to load HDR");
        assert_eq!(img.width, 1024);
        assert_eq!(img.height, 512);
        assert_eq!(img.pixels.len(), 1024 * 512);
    }

    #[test]
    fn hdr_values_are_hdr() {
        let img = HdrImage::load(test_hdr_path()).expect("Failed to load HDR");
        // HDR images should have some pixels > 1.0
        let max_val = img
            .pixels
            .iter()
            .flat_map(|p| p.iter())
            .copied()
            .fold(0.0f32, f32::max);
        assert!(
            max_val > 1.0,
            "HDR image should contain values > 1.0, got max {}",
            max_val
        );
    }

    #[test]
    fn direction_to_uv_roundtrip() {
        let directions: &[[f32; 3]] = &[
            [1.0, 0.0, 0.0],  // +X
            [-1.0, 0.0, 0.0], // -X
            [0.0, 1.0, 0.0],  // +Y (up)
            [0.0, -1.0, 0.0], // -Y (down)
            [0.0, 0.0, 1.0],  // +Z
            [0.0, 0.0, -1.0], // -Z
        ];

        for &dir in directions {
            let (u, v) = HdrImage::direction_to_uv(dir, 0.0);
            assert!(u >= 0.0 && u <= 1.0, "u out of range: {}", u);
            assert!(v >= 0.0 && v <= 1.0, "v out of range: {}", v);

            let recovered = HdrImage::uv_to_direction(u, v, 0.0);
            let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
            let norm = [dir[0] / len, dir[1] / len, dir[2] / len];

            for i in 0..3 {
                assert!(
                    (recovered[i] - norm[i]).abs() < 1e-5,
                    "Roundtrip failed for {:?}: got {:?}",
                    dir,
                    recovered
                );
            }
        }
    }

    #[test]
    fn direction_to_uv_with_rotation() {
        // +X with no rotation -> u=0.5
        let (u0, _) = HdrImage::direction_to_uv([1.0, 0.0, 0.0], 0.0);
        // +X with PI rotation -> u should shift by 0.5
        let (u1, _) = HdrImage::direction_to_uv([1.0, 0.0, 0.0], PI);
        let diff = (u1 - u0 - 0.5).abs();
        // Account for wrapping
        let diff = diff.min((diff - 1.0).abs());
        assert!(
            diff < 1e-5,
            "Rotation shift unexpected: u0={}, u1={}",
            u0,
            u1
        );
    }

    #[test]
    fn sample_bilinear_no_panic() {
        let img = HdrImage::load(test_hdr_path()).expect("Failed to load HDR");
        // Sample in various directions
        let dirs: &[[f32; 3]] = &[
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [-1.0, -1.0, -1.0],
            [0.5, 0.8, -0.3],
        ];
        for &dir in dirs {
            let color = img.sample(dir, 0.0);
            for c in &color {
                assert!(c.is_finite(), "Non-finite sample for dir {:?}", dir);
                assert!(*c >= 0.0, "Negative sample for dir {:?}", dir);
            }
        }
    }

    #[test]
    fn up_direction_is_top_of_image() {
        let (_, v) = HdrImage::direction_to_uv([0.0, 1.0, 0.0], 0.0);
        assert!(
            v < 0.01,
            "Up direction should map to top (v~0), got v={}",
            v
        );
    }
}
