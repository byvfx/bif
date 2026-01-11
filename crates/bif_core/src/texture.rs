//! Texture loading and caching for materials.
//!
//! Provides a texture cache that loads images from disk and stores them
//! in a format suitable for both CPU (Ivar) and GPU (viewport) rendering.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bif_math::Vec3;
use thiserror::Error;

/// Errors that can occur during texture loading.
#[derive(Error, Debug)]
pub enum TextureError {
    #[error("Failed to load texture: {0}")]
    LoadError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image decoding error: {0}")]
    ImageError(#[from] image::ImageError),

    #[error("Unsupported texture format: {0}")]
    UnsupportedFormat(String),
}

pub type TextureResult<T> = Result<T, TextureError>;

/// A loaded texture with pixel data.
///
/// Stores pixels in linear RGB(A) float format for rendering.
#[derive(Clone, Debug)]
pub struct Texture {
    /// Texture width in pixels
    pub width: u32,

    /// Texture height in pixels
    pub height: u32,

    /// Pixel data in RGBA format (linear, 0-1 range)
    /// Stored as [R, G, B, A] per pixel, row-major order
    pub pixels: Vec<[f32; 4]>,

    /// Original file path (for debugging)
    pub path: String,
}

impl Texture {
    /// Create a new texture from pixel data.
    pub fn new(width: u32, height: u32, pixels: Vec<[f32; 4]>, path: impl Into<String>) -> Self {
        Self {
            width,
            height,
            pixels,
            path: path.into(),
        }
    }

    /// Create a solid color texture (1x1).
    pub fn solid_color(color: Vec3) -> Self {
        Self {
            width: 1,
            height: 1,
            pixels: vec![[color.x, color.y, color.z, 1.0]],
            path: "<solid>".to_string(),
        }
    }

    /// Sample the texture at UV coordinates (bilinear filtering).
    ///
    /// UV coordinates are in [0, 1] range, with (0, 0) at bottom-left.
    pub fn sample(&self, u: f32, v: f32) -> Vec3 {
        // Wrap UV coordinates
        let u = u.rem_euclid(1.0);
        let v = v.rem_euclid(1.0);

        // Convert to pixel coordinates
        let x = u * (self.width as f32 - 1.0);
        let y = (1.0 - v) * (self.height as f32 - 1.0); // Flip V for image coordinates

        // Bilinear interpolation
        let x0 = x.floor() as u32;
        let y0 = y.floor() as u32;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);

        let fx = x.fract();
        let fy = y.fract();

        let p00 = self.get_pixel(x0, y0);
        let p10 = self.get_pixel(x1, y0);
        let p01 = self.get_pixel(x0, y1);
        let p11 = self.get_pixel(x1, y1);

        // Bilinear blend
        let top = Vec3::new(
            p00[0] * (1.0 - fx) + p10[0] * fx,
            p00[1] * (1.0 - fx) + p10[1] * fx,
            p00[2] * (1.0 - fx) + p10[2] * fx,
        );
        let bottom = Vec3::new(
            p01[0] * (1.0 - fx) + p11[0] * fx,
            p01[1] * (1.0 - fx) + p11[1] * fx,
            p01[2] * (1.0 - fx) + p11[2] * fx,
        );

        top * (1.0 - fy) + bottom * fy
    }

    /// Sample a single channel (for roughness/metallic maps).
    pub fn sample_channel(&self, u: f32, v: f32, channel: usize) -> f32 {
        let u = u.rem_euclid(1.0);
        let v = v.rem_euclid(1.0);

        let x = (u * (self.width as f32 - 1.0)) as u32;
        let y = ((1.0 - v) * (self.height as f32 - 1.0)) as u32;

        self.get_pixel(x.min(self.width - 1), y.min(self.height - 1))[channel.min(3)]
    }

    /// Get pixel at integer coordinates.
    fn get_pixel(&self, x: u32, y: u32) -> [f32; 4] {
        let idx = (y * self.width + x) as usize;
        self.pixels
            .get(idx)
            .copied()
            .unwrap_or([0.0, 0.0, 0.0, 1.0])
    }

    /// Get total size in bytes (approximate).
    pub fn size_bytes(&self) -> usize {
        self.pixels.len() * std::mem::size_of::<[f32; 4]>()
    }
}

/// Cache for loaded textures.
///
/// Textures are loaded on-demand and cached for reuse.
pub struct TextureCache {
    /// Cached textures by file path
    textures: HashMap<String, Arc<Texture>>,

    /// Base directory for resolving relative paths
    base_dir: Option<PathBuf>,
}

impl TextureCache {
    /// Create a new empty texture cache.
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            base_dir: None,
        }
    }

    /// Create a texture cache with a base directory for relative paths.
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            textures: HashMap::new(),
            base_dir: Some(base_dir.into()),
        }
    }

    /// Set the base directory for resolving relative paths.
    pub fn set_base_dir(&mut self, base_dir: impl Into<PathBuf>) {
        self.base_dir = Some(base_dir.into());
    }

    /// Load a texture from file, using cache if available.
    pub fn load(&mut self, path: &str) -> TextureResult<Arc<Texture>> {
        // Check cache first
        if let Some(texture) = self.textures.get(path) {
            return Ok(texture.clone());
        }

        // Resolve path
        let full_path = self.resolve_path(path);

        // Load the texture
        let texture = load_texture_file(&full_path)?;
        let texture = Arc::new(texture);

        // Cache it
        self.textures.insert(path.to_string(), texture.clone());

        log::debug!(
            "Loaded texture: {} ({}x{}, {:.1} KB)",
            path,
            texture.width,
            texture.height,
            texture.size_bytes() as f32 / 1024.0
        );

        Ok(texture)
    }

    /// Get a cached texture without loading.
    pub fn get(&self, path: &str) -> Option<Arc<Texture>> {
        self.textures.get(path).cloned()
    }

    /// Check if a texture is cached.
    pub fn is_cached(&self, path: &str) -> bool {
        self.textures.contains_key(path)
    }

    /// Get the number of cached textures.
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Clear all cached textures.
    pub fn clear(&mut self) {
        self.textures.clear();
    }

    /// Get total memory usage of cached textures.
    pub fn total_size_bytes(&self) -> usize {
        self.textures.values().map(|t| t.size_bytes()).sum()
    }

    /// Resolve a path relative to the base directory.
    fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);

        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(base) = &self.base_dir {
            base.join(path)
        } else {
            path.to_path_buf()
        }
    }
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Load a texture from a file path.
fn load_texture_file(path: &Path) -> TextureResult<Texture> {
    // Load image using the image crate
    let img = image::open(path).map_err(|e| {
        TextureError::LoadError(format!("Failed to open {}: {}", path.display(), e))
    })?;

    // Convert to RGBA8
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    // Convert to linear float RGBA
    let pixels: Vec<[f32; 4]> = rgba
        .pixels()
        .map(|p| {
            [
                srgb_to_linear(p[0]),
                srgb_to_linear(p[1]),
                srgb_to_linear(p[2]),
                p[3] as f32 / 255.0, // Alpha is linear
            ]
        })
        .collect();

    Ok(Texture::new(
        width,
        height,
        pixels,
        path.to_string_lossy().to_string(),
    ))
}

/// Convert sRGB byte value to linear float.
fn srgb_to_linear(value: u8) -> f32 {
    let v = value as f32 / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solid_color_texture() {
        let tex = Texture::solid_color(Vec3::new(1.0, 0.5, 0.0));
        assert_eq!(tex.width, 1);
        assert_eq!(tex.height, 1);

        let sample = tex.sample(0.5, 0.5);
        assert!((sample.x - 1.0).abs() < 0.001);
        assert!((sample.y - 0.5).abs() < 0.001);
        assert!((sample.z - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_texture_cache() {
        let cache = TextureCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_srgb_to_linear() {
        // Black stays black
        assert!((srgb_to_linear(0) - 0.0).abs() < 0.001);

        // White stays white
        assert!((srgb_to_linear(255) - 1.0).abs() < 0.001);

        // Mid-gray is darker in linear
        let mid = srgb_to_linear(128);
        assert!(mid < 0.5);
        assert!(mid > 0.1);
    }
}
