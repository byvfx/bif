//! Texture loading and caching for materials.
//!
//! Provides a texture cache that loads images from disk and stores them
//! in a format suitable for both CPU (Ivar) and GPU (viewport) rendering.
//!
//! When the `oiio` feature is enabled, textures can be loaded via OpenImageIO
//! with automatic .tx conversion and mipmap support.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bif_math::Vec3;
use thiserror::Error;

#[cfg(feature = "oiio")]
use crate::oiio;

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

    #[cfg(feature = "oiio")]
    #[error("OIIO error: {0}")]
    OiioError(#[from] oiio::OiioError),
}

pub type TextureResult<T> = Result<T, TextureError>;

/// A single mipmap level.
#[derive(Clone, Debug)]
pub struct MipLevel {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Pixel data in RGBA format (linear, 0-1 range)
    pub pixels: Vec<[f32; 4]>,
}

/// A loaded texture with pixel data and optional mipmaps.
///
/// Stores pixels in linear RGB(A) float format for rendering.
#[derive(Clone, Debug)]
pub struct Texture {
    /// Texture width in pixels (base level)
    pub width: u32,

    /// Texture height in pixels (base level)
    pub height: u32,

    /// Pixel data in RGBA format (linear, 0-1 range)
    /// Stored as [R, G, B, A] per pixel, row-major order
    /// This is the base mip level (level 0)
    pub pixels: Vec<[f32; 4]>,

    /// Additional mip levels (level 1 and beyond)
    /// Empty if texture has no mipmaps
    pub mip_levels: Vec<MipLevel>,

    /// Original file path (for debugging)
    pub path: String,

    /// Whether the source was linear (EXR/HDR) or sRGB
    pub is_linear: bool,
}

impl Texture {
    /// Create a new texture from pixel data.
    pub fn new(width: u32, height: u32, pixels: Vec<[f32; 4]>, path: impl Into<String>) -> Self {
        Self {
            width,
            height,
            pixels,
            mip_levels: Vec::new(),
            path: path.into(),
            is_linear: false,
        }
    }

    /// Create a new texture with mipmaps.
    pub fn with_mips(
        width: u32,
        height: u32,
        pixels: Vec<[f32; 4]>,
        mip_levels: Vec<MipLevel>,
        path: impl Into<String>,
        is_linear: bool,
    ) -> Self {
        Self {
            width,
            height,
            pixels,
            mip_levels,
            path: path.into(),
            is_linear,
        }
    }

    /// Create a solid color texture (1x1).
    pub fn solid_color(color: Vec3) -> Self {
        Self {
            width: 1,
            height: 1,
            pixels: vec![[color.x, color.y, color.z, 1.0]],
            mip_levels: Vec::new(),
            path: "<solid>".to_string(),
            is_linear: false,
        }
    }

    /// Get the number of mip levels (including base).
    pub fn mip_count(&self) -> u32 {
        1 + self.mip_levels.len() as u32
    }

    /// Check if this texture has mipmaps.
    pub fn has_mipmaps(&self) -> bool {
        !self.mip_levels.is_empty()
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

    /// Sample a single channel with bilinear filtering (for roughness/metallic maps).
    pub fn sample_channel(&self, u: f32, v: f32, channel: usize) -> f32 {
        let u = u.rem_euclid(1.0);
        let v = v.rem_euclid(1.0);
        let ch = channel.min(3);

        let x = u * (self.width as f32 - 1.0);
        let y = (1.0 - v) * (self.height as f32 - 1.0);

        let x0 = x.floor() as u32;
        let y0 = y.floor() as u32;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);

        let fx = x.fract();
        let fy = y.fract();

        let p00 = self.get_pixel(x0, y0)[ch];
        let p10 = self.get_pixel(x1, y0)[ch];
        let p01 = self.get_pixel(x0, y1)[ch];
        let p11 = self.get_pixel(x1, y1)[ch];

        let top = p00 * (1.0 - fx) + p10 * fx;
        let bottom = p01 * (1.0 - fx) + p11 * fx;
        top * (1.0 - fy) + bottom * fy
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
        let base_size = self.pixels.len() * std::mem::size_of::<[f32; 4]>();
        let mip_size: usize = self
            .mip_levels
            .iter()
            .map(|m| m.pixels.len() * std::mem::size_of::<[f32; 4]>())
            .sum();
        base_size + mip_size
    }

    /// Get a specific mip level (0 = base).
    pub fn get_mip_level(&self, level: u32) -> Option<(&Vec<[f32; 4]>, u32, u32)> {
        if level == 0 {
            Some((&self.pixels, self.width, self.height))
        } else {
            self.mip_levels
                .get(level as usize - 1)
                .map(|m| (&m.pixels, m.width, m.height))
        }
    }
}

/// Cache for loaded textures.
///
/// Textures are loaded on-demand and cached for reuse.
/// When the `oiio` feature is enabled, textures can be loaded with mipmaps
/// and automatically converted to .tx format.
pub struct TextureCache {
    /// Cached textures by file path
    textures: HashMap<String, Arc<Texture>>,

    /// Base directory for resolving relative paths
    base_dir: Option<PathBuf>,

    /// Whether to prefer existing .tx files over source textures.
    /// Use `convert_textures_to_tx` to pre-convert before rendering.
    #[cfg(feature = "oiio")]
    pub prefer_tx: bool,

    /// Whether to generate mipmaps when loading (requires oiio feature)
    #[cfg(feature = "oiio")]
    pub generate_mipmaps: bool,
}

impl TextureCache {
    /// Create a new empty texture cache.
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            base_dir: None,
            #[cfg(feature = "oiio")]
            prefer_tx: false,
            #[cfg(feature = "oiio")]
            generate_mipmaps: true,
        }
    }

    /// Create a texture cache with a base directory for relative paths.
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            textures: HashMap::new(),
            base_dir: Some(base_dir.into()),
            #[cfg(feature = "oiio")]
            prefer_tx: false,
            #[cfg(feature = "oiio")]
            generate_mipmaps: true,
        }
    }

    /// Set the base directory for resolving relative paths.
    pub fn set_base_dir(&mut self, base_dir: impl Into<PathBuf>) {
        self.base_dir = Some(base_dir.into());
    }

    /// Load a texture from file, using cache if available.
    ///
    /// When the `oiio` feature is enabled and `prefer_tx` is true,
    /// existing .tx files will be preferred over source textures.
    /// Use `convert_textures_to_tx` to pre-generate .tx files.
    pub fn load(&mut self, path: &str) -> TextureResult<Arc<Texture>> {
        // Check cache first
        if let Some(texture) = self.textures.get(path) {
            return Ok(texture.clone());
        }

        // Resolve path
        let full_path = self.resolve_path(path);

        // Load the texture (OIIO or fallback)
        #[cfg(feature = "oiio")]
        let texture = self.load_with_oiio(&full_path, path)?;

        #[cfg(not(feature = "oiio"))]
        let texture = load_texture_file(&full_path)?;

        let texture = Arc::new(texture);

        // Cache it
        self.textures.insert(path.to_string(), texture.clone());

        log::debug!(
            "Loaded texture: {} ({}x{}, {} mips, {:.1} KB)",
            path,
            texture.width,
            texture.height,
            texture.mip_count(),
            texture.size_bytes() as f32 / 1024.0
        );

        Ok(texture)
    }

    /// Load a data texture (normal/roughness/metallic/opacity) without sRGB→linear.
    ///
    /// Data textures store linear values (not perceptual color), so we just
    /// divide by 255 instead of applying the sRGB transfer function.
    pub fn load_linear(&mut self, path: &str) -> TextureResult<Arc<Texture>> {
        let cache_key = format!("{}_linear", path);
        if let Some(texture) = self.textures.get(&cache_key) {
            return Ok(texture.clone());
        }

        let full_path = self.resolve_path(path);
        let texture = load_texture_linear(&full_path)?;
        let texture = Arc::new(texture);
        self.textures.insert(cache_key, texture.clone());

        log::debug!(
            "Loaded linear texture: {} ({}x{}, {:.1} KB)",
            path,
            texture.width,
            texture.height,
            texture.size_bytes() as f32 / 1024.0
        );

        Ok(texture)
    }

    /// Pre-convert a list of texture paths to .tx via subprocess.
    /// Returns number of successful conversions.
    /// Call this from GUI before rendering to pre-generate .tx files.
    #[cfg(feature = "oiio")]
    pub fn convert_textures_to_tx(&self, paths: &[String]) -> usize {
        let mut converted = 0;
        for path in paths {
            let full_path = self.resolve_path(path);
            let tx_path = oiio::get_tx_path(&full_path);
            if oiio::tx_is_valid(&full_path, &tx_path) {
                continue; // Already up to date
            }
            log::info!("Converting to .tx: {}", full_path.display());
            if Self::make_tx_subprocess(&full_path, &tx_path) {
                converted += 1;
            }
        }
        if converted > 0 {
            log::info!("Converted {} textures to .tx", converted);
        }
        converted
    }

    /// Convert single texture to .tx via subprocess (isolates OIIO crashes).
    /// Returns true only if conversion succeeded cleanly.
    #[cfg(feature = "oiio")]
    pub fn make_tx_subprocess(input: &Path, output: &Path) -> bool {
        use std::process::Command;

        // Find bif_maketx next to current executable
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));

        let maketx_name = if cfg!(windows) {
            "bif_maketx.exe"
        } else {
            "bif_maketx"
        };

        let maketx_path = exe_dir
            .as_ref()
            .map(|d| d.join(maketx_name))
            .filter(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from(maketx_name));

        let success = match Command::new(&maketx_path)
            .arg(input.to_string_lossy().as_ref())
            .arg(output.to_string_lossy().as_ref())
            .output()
        {
            Ok(result) if result.status.success() => {
                log::info!("Created .tx: {}", output.display());
                true
            }
            Ok(result) => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                log::warn!(
                    "bif_maketx failed (exit {}): {}",
                    result.status.code().unwrap_or(-1),
                    stderr.trim()
                );
                false
            }
            Err(e) => {
                log::warn!("Failed to spawn bif_maketx: {}", e);
                false
            }
        };

        // Remove partial .tx on failure
        if !success {
            let _ = std::fs::remove_file(output);
        }
        success
    }

    /// Load texture using OIIO with optional .tx preference and mipmaps.
    #[cfg(feature = "oiio")]
    fn load_with_oiio(&self, full_path: &Path, original_path: &str) -> TextureResult<Texture> {
        let load_path = if self.prefer_tx {
            let tx_path = oiio::get_tx_path(full_path);
            if oiio::tx_is_valid(full_path, &tx_path) {
                log::debug!("Using .tx: {}", tx_path.display());
                tx_path
            } else {
                full_path.to_path_buf()
            }
        } else {
            full_path.to_path_buf()
        };

        // Load with or without mipmaps
        let oiio_tex = if self.generate_mipmaps {
            oiio::load_texture_with_mips(&load_path)?
        } else {
            oiio::load_texture(&load_path)?
        };

        // Convert OIIO texture to our format
        self.convert_oiio_texture(oiio_tex, original_path)
    }

    /// Convert OIIO texture data to our Texture format.
    #[cfg(feature = "oiio")]
    fn convert_oiio_texture(
        &self,
        oiio_tex: oiio::OiioTexture,
        path: &str,
    ) -> TextureResult<Texture> {
        if oiio_tex.mip_levels.is_empty() {
            return Err(TextureError::LoadError(
                "No mip levels in texture".to_string(),
            ));
        }

        // Convert base level (u8 RGBA to f32 RGBA)
        let base = &oiio_tex.mip_levels[0];
        let pixels = convert_u8_to_f32_pixels(&base.data, oiio_tex.is_linear);

        // Convert additional mip levels
        let mip_levels: Vec<MipLevel> = oiio_tex
            .mip_levels
            .iter()
            .skip(1)
            .map(|mip| MipLevel {
                width: mip.width,
                height: mip.height,
                pixels: convert_u8_to_f32_pixels(&mip.data, oiio_tex.is_linear),
            })
            .collect();

        Ok(Texture::with_mips(
            oiio_tex.width,
            oiio_tex.height,
            pixels,
            mip_levels,
            path,
            oiio_tex.is_linear,
        ))
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

/// Load a texture from a file path (fallback when OIIO not available).
#[cfg(not(feature = "oiio"))]
fn load_texture_file(path: &Path) -> TextureResult<Texture> {
    let start = std::time::Instant::now();

    // Load image using the image crate
    let img = image::open(path).map_err(|e| {
        TextureError::LoadError(format!("Failed to open {}: {}", path.display(), e))
    })?;
    let decode_time = start.elapsed();

    let is_linear = is_linear_texture_path(path);

    if is_linear {
        // Load as linear float RGBA (EXR/HDR)
        let rgba = img.to_rgba32f();
        let (width, height) = rgba.dimensions();

        let pixels: Vec<[f32; 4]> = rgba.pixels().map(|p| [p[0], p[1], p[2], p[3]]).collect();

        let total_time = start.elapsed();
        log::info!(
            "Texture {} ({}x{}): decode={:.1}ms, total={:.1}ms",
            path.file_name().unwrap_or_default().to_string_lossy(),
            width,
            height,
            decode_time.as_secs_f32() * 1000.0,
            total_time.as_secs_f32() * 1000.0
        );

        Ok(Texture::new(
            width,
            height,
            pixels,
            path.to_string_lossy().to_string(),
        ))
    } else {
        // Convert to RGBA8
        let convert_start = std::time::Instant::now();
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        let convert_time = convert_start.elapsed();

        // Convert to linear float RGBA using lookup table for speed
        let linear_start = std::time::Instant::now();
        let lut = srgb_to_linear_lut();
        let pixels: Vec<[f32; 4]> = rgba
            .pixels()
            .map(|p| {
                [
                    lut[p[0] as usize],
                    lut[p[1] as usize],
                    lut[p[2] as usize],
                    p[3] as f32 / 255.0, // Alpha is linear
                ]
            })
            .collect();
        let linear_time = linear_start.elapsed();

        let total_time = start.elapsed();
        log::info!(
            "Texture {} ({}x{}): decode={:.1}ms, convert={:.1}ms, linear={:.1}ms, total={:.1}ms",
            path.file_name().unwrap_or_default().to_string_lossy(),
            width,
            height,
            decode_time.as_secs_f32() * 1000.0,
            convert_time.as_secs_f32() * 1000.0,
            linear_time.as_secs_f32() * 1000.0,
            total_time.as_secs_f32() * 1000.0
        );

        Ok(Texture::new(
            width,
            height,
            pixels,
            path.to_string_lossy().to_string(),
        ))
    }
}

/// Lookup table for sRGB to linear conversion (256 entries).
/// Lazily initialized on first use to avoid expensive powf() calls per pixel.
fn srgb_to_linear_lut() -> &'static [f32; 256] {
    use std::sync::OnceLock;
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = [0.0f32; 256];
        for (i, val) in lut.iter_mut().enumerate() {
            let v = i as f32 / 255.0;
            *val = if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            };
        }
        lut
    })
}

/// Convert sRGB byte value to linear float.
#[allow(dead_code)]
fn srgb_to_linear(value: u8) -> f32 {
    let v = value as f32 / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Load a texture treating all channels as linear data (no sRGB conversion).
fn load_texture_linear(path: &Path) -> TextureResult<Texture> {
    let img = image::open(path).map_err(|e| {
        TextureError::LoadError(format!("Failed to open {}: {}", path.display(), e))
    })?;

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    let pixels: Vec<[f32; 4]> = rgba
        .pixels()
        .map(|p| {
            [
                p[0] as f32 / 255.0,
                p[1] as f32 / 255.0,
                p[2] as f32 / 255.0,
                p[3] as f32 / 255.0,
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

/// Detect if a texture path should be treated as linear (HDR/EXR).
#[cfg(not(feature = "oiio"))]
fn is_linear_texture_path(path: &Path) -> bool {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => matches!(ext.to_ascii_lowercase().as_str(), "exr" | "hdr" | "tx"),
        None => false,
    }
}

/// Convert u8 RGBA pixels to f32 RGBA.
/// If source is sRGB, applies gamma correction. If linear, just normalizes.
#[cfg(feature = "oiio")]
fn convert_u8_to_f32_pixels(data: &[u8], is_linear: bool) -> Vec<[f32; 4]> {
    let pixel_count = data.len() / 4;
    let mut pixels = Vec::with_capacity(pixel_count);

    if is_linear {
        // Linear data - just normalize to 0-1
        for chunk in data.chunks_exact(4) {
            pixels.push([
                chunk[0] as f32 / 255.0,
                chunk[1] as f32 / 255.0,
                chunk[2] as f32 / 255.0,
                chunk[3] as f32 / 255.0,
            ]);
        }
    } else {
        // sRGB data - convert to linear
        let lut = srgb_to_linear_lut();
        for chunk in data.chunks_exact(4) {
            pixels.push([
                lut[chunk[0] as usize],
                lut[chunk[1] as usize],
                lut[chunk[2] as usize],
                chunk[3] as f32 / 255.0, // Alpha is linear
            ]);
        }
    }

    pixels
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
