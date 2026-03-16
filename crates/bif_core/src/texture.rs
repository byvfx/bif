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

    /// UDIM atlas grid columns (0 = not a UDIM texture)
    pub udim_grid_cols: u32,
    /// UDIM atlas grid rows
    pub udim_grid_rows: u32,
    /// Minimum column index in the UDIM grid
    pub udim_min_col: u32,
    /// Minimum row index in the UDIM grid
    pub udim_min_row: u32,
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
            udim_grid_cols: 0,
            udim_grid_rows: 0,
            udim_min_col: 0,
            udim_min_row: 0,
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
            udim_grid_cols: 0,
            udim_grid_rows: 0,
            udim_min_col: 0,
            udim_min_row: 0,
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
            udim_grid_cols: 0,
            udim_grid_rows: 0,
            udim_min_col: 0,
            udim_min_row: 0,
        }
    }

    /// Get the number of mip levels (including base).
    pub fn mip_count(&self) -> u32 {
        1 + self.mip_levels.len() as u32
    }

    /// Whether this texture is a UDIM atlas.
    pub fn is_udim(&self) -> bool {
        self.udim_grid_cols > 0
    }

    /// Check if this texture has mipmaps.
    pub fn has_mipmaps(&self) -> bool {
        !self.mip_levels.is_empty()
    }

    /// Transform UV coordinates for UDIM atlas lookup.
    ///
    /// For non-UDIM textures, wraps UVs to [0, 1].
    /// For UDIM atlases, maps tile-space UVs to atlas-space UVs
    /// using the same math as `basic.wgsl` lines 281-305.
    ///
    /// Note on V-axis: this function returns UVs in atlas pixel space where
    /// row 0 is at image top (flipped_row + inverted sub_v). Then `sample()`
    /// applies `1.0 - v` to convert back to bottom-left origin. The two
    /// flips cancel correctly — this is intentional, not a bug.
    #[inline]
    fn transform_uv(&self, u: f32, v: f32) -> (f32, f32) {
        if !self.is_udim() {
            return (u.rem_euclid(1.0), v.rem_euclid(1.0));
        }
        let raw_col = u.floor() as i32 - self.udim_min_col as i32;
        let raw_row = v.floor() as i32 - self.udim_min_row as i32;
        let col = raw_col.clamp(0, self.udim_grid_cols as i32 - 1) as u32;
        let row = raw_row.clamp(0, self.udim_grid_rows as i32 - 1) as u32;
        let sub_u = u.fract().rem_euclid(1.0);
        let sub_v = v.fract().rem_euclid(1.0);
        let flipped_row = self.udim_grid_rows - 1 - row;
        let atlas_u = (col as f32 + sub_u) / self.udim_grid_cols as f32;
        let atlas_v = (flipped_row as f32 + (1.0 - sub_v)) / self.udim_grid_rows as f32;
        (atlas_u, atlas_v)
    }

    /// Sample the texture at UV coordinates (bilinear filtering).
    ///
    /// UV coordinates are in [0, 1] range, with (0, 0) at bottom-left.
    #[must_use]
    pub fn sample(&self, u: f32, v: f32) -> Vec3 {
        // Guard against 0-size textures to avoid modulo/index panics
        if self.width == 0 || self.height == 0 {
            return Vec3::new(1.0, 0.0, 1.0); // Magenta debug color
        }

        let (u, v) = self.transform_uv(u, v);

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
    #[must_use]
    pub fn sample_channel(&self, u: f32, v: f32, channel: usize) -> f32 {
        let (u, v) = self.transform_uv(u, v);
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
    #[must_use]
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
    #[must_use]
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

        // UDIM atlas: stitch tiles into single texture
        if is_udim_path(path) {
            let texture = Arc::new(self.load_udim_atlas(path, false)?);
            self.textures.insert(path.to_string(), texture.clone());
            return Ok(texture);
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

        // UDIM atlas: stitch tiles into single texture (linear)
        if is_udim_path(path) {
            let texture = Arc::new(self.load_udim_atlas(path, true)?);
            self.textures.insert(cache_key, texture.clone());
            return Ok(texture);
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

    /// Load a UDIM texture atlas from tiled files.
    ///
    /// Two-pass approach to limit peak memory:
    ///   1. Probe tile dimensions via header read (no pixel decode)
    ///   2. Load each tile one at a time, downscale, stitch into atlas, drop
    ///
    /// Tiles larger than `MAX_IVAR_UDIM_TILE_SIZE` are downscaled before
    /// stitching so the atlas stays within reasonable memory bounds.
    fn load_udim_atlas(&self, pattern: &str, linear: bool) -> TextureResult<Texture> {
        let resolved_pattern = self.resolve_path(pattern).to_string_lossy().into_owned();
        let tiles = find_udim_tiles(&resolved_pattern);
        if tiles.is_empty() {
            return Err(TextureError::LoadError(format!(
                "No UDIM tiles found for pattern: {}",
                pattern
            )));
        }

        // Phase 1: Probe dimensions from headers (no pixel decode)
        struct TileInfo {
            udim: u32,
            path: String,
        }
        let mut tile_infos = Vec::new();
        let mut min_col = u32::MAX;
        let mut max_col = 0u32;
        let mut min_row = u32::MAX;
        let mut max_row = 0u32;
        let mut raw_max_w = 0u32;
        let mut raw_max_h = 0u32;

        for (udim, tile_path) in &tiles {
            let (w, h) = image::image_dimensions(tile_path).map_err(|e| {
                TextureError::LoadError(format!("Can't probe {}: {}", tile_path, e))
            })?;
            let col = (udim - 1001) % 10;
            let row = (udim - 1001) / 10;
            min_col = min_col.min(col);
            max_col = max_col.max(col);
            min_row = min_row.min(row);
            max_row = max_row.max(row);
            raw_max_w = raw_max_w.max(w);
            raw_max_h = raw_max_h.max(h);
            tile_infos.push(TileInfo {
                udim: *udim,
                path: tile_path.clone(),
            });
        }

        let num_cols = max_col - min_col + 1;
        let num_rows = max_row - min_row + 1;

        // Cap per-tile dimensions to limit memory
        let mut target_w = raw_max_w;
        let mut target_h = raw_max_h;
        if target_w > MAX_IVAR_UDIM_TILE_SIZE || target_h > MAX_IVAR_UDIM_TILE_SIZE {
            let scale = MAX_IVAR_UDIM_TILE_SIZE as f32 / target_w.max(target_h) as f32;
            target_w = ((target_w as f32 * scale) as u32).max(1);
            target_h = ((target_h as f32 * scale) as u32).max(1);
        }

        // Cap total atlas to MAX_IVAR_ATLAS_SIZE (tiles * grid can still exceed)
        let atlas_w_raw = num_cols * target_w;
        let atlas_h_raw = num_rows * target_h;
        if atlas_w_raw > MAX_IVAR_ATLAS_SIZE || atlas_h_raw > MAX_IVAR_ATLAS_SIZE {
            let scale = MAX_IVAR_ATLAS_SIZE as f32 / atlas_w_raw.max(atlas_h_raw) as f32;
            target_w = ((target_w as f32 * scale) as u32).max(1);
            target_h = ((target_h as f32 * scale) as u32).max(1);
        }

        if target_w != raw_max_w || target_h != raw_max_h {
            log::info!(
                "UDIM tiles capped {}x{} -> {}x{} for Ivar ({}x{} grid)",
                raw_max_w,
                raw_max_h,
                target_w,
                target_h,
                num_cols,
                num_rows
            );
        }

        let atlas_w = num_cols * target_w;
        let atlas_h = num_rows * target_h;

        // Guard against u32 overflow and cap at MAX_IVAR_ATLAS_PIXELS
        let total_pixels = (atlas_w as u64) * (atlas_h as u64);
        if total_pixels > MAX_IVAR_ATLAS_PIXELS {
            return Err(TextureError::LoadError(format!(
                "UDIM atlas too large: {}x{} ({} MB)",
                atlas_w,
                atlas_h,
                total_pixels * 16 / 1_048_576
            )));
        }

        let mut atlas_pixels = vec![[0.0f32; 4]; (atlas_w * atlas_h) as usize];

        // Phase 2: Load each tile, downscale, stitch, drop (one at a time)
        let mut tiles_loaded = 0u32;
        for info in &tile_infos {
            let tile_full = Path::new(&info.path);
            let tex = if linear {
                load_texture_linear(tile_full)?
            } else {
                #[cfg(feature = "oiio")]
                {
                    self.load_with_oiio(tile_full, &info.path)?
                }
                #[cfg(not(feature = "oiio"))]
                {
                    load_texture_file(tile_full)?
                }
            };

            let col = (info.udim - 1001) % 10 - min_col;
            let row = (info.udim - 1001) / 10 - min_row;
            let flipped_row = (num_rows - 1) - row;

            // Downscale to target tile size if needed
            let tile_pixels = if tex.width != target_w || tex.height != target_h {
                scale_pixels_box(&tex.pixels, tex.width, tex.height, target_w, target_h)
            } else {
                tex.pixels
            };
            // tex dropped here — only one tile in memory at a time

            let dest_x = col * target_w;
            let dest_y = flipped_row * target_h;
            for y in 0..target_h {
                for x in 0..target_w {
                    let src_idx = (y * target_w + x) as usize;
                    let dst_idx = ((dest_y + y) * atlas_w + dest_x + x) as usize;
                    atlas_pixels[dst_idx] = tile_pixels[src_idx];
                }
            }
            tiles_loaded += 1;
        }

        log::info!(
            "UDIM atlas (Ivar): {} tiles -> {}x{} ({}x{} grid, tile {}x{})",
            tiles_loaded,
            atlas_w,
            atlas_h,
            num_cols,
            num_rows,
            target_w,
            target_h
        );

        Ok(Texture {
            width: atlas_w,
            height: atlas_h,
            pixels: atlas_pixels,
            mip_levels: Vec::new(),
            path: pattern.to_string(),
            is_linear: linear,
            udim_grid_cols: num_cols,
            udim_grid_rows: num_rows,
            udim_min_col: min_col,
            udim_min_row: min_row,
        })
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

/// Maximum per-tile dimension for Ivar UDIM atlases.
/// Tiles larger than this are downscaled before stitching to limit memory.
const MAX_IVAR_UDIM_TILE_SIZE: u32 = 2048;

/// Maximum atlas dimension (either axis) for Ivar UDIM atlases.
/// Prevents multi-tile grids from creating huge f32 buffers.
/// 8192x8192 * 16 bytes = 1 GB — the hard upper bound.
const MAX_IVAR_ATLAS_SIZE: u32 = 8192;

/// Maximum total pixel count for UDIM atlases (memory budget).
/// 16M pixels * 16 bytes/pixel = 256 MB.
const MAX_IVAR_ATLAS_PIXELS: u64 = 16_777_216;

/// Check if a texture path contains a UDIM token (`<UDIM>`).
pub fn is_udim_path(path: &str) -> bool {
    path.contains("<UDIM>")
}

/// Scan filesystem for existing UDIM tiles matching the pattern.
/// Returns a vec of (udim_id, path) sorted by UDIM ID.
fn find_udim_tiles(pattern: &str) -> Vec<(u32, String)> {
    let mut tiles = Vec::new();
    for udim in 1001..=1200 {
        let tile_path = pattern.replace("<UDIM>", &udim.to_string());
        if Path::new(&tile_path).exists() {
            tiles.push((udim, tile_path));
        }
    }
    tiles
}

/// Scale f32 pixel data to target dimensions using box filter (area average).
///
/// Averages all source pixels that map to each destination pixel.
/// Prevents moire/aliasing on high-frequency textures (wood grain, fabric)
/// that nearest-neighbor would produce when downscaling UDIM tiles.
fn scale_pixels_box(
    src: &[[f32; 4]],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0; 4]; (dst_w * dst_h) as usize];
    let scale_x = src_w as f32 / dst_w as f32;
    let scale_y = src_h as f32 / dst_h as f32;
    for y in 0..dst_h {
        let sy0 = (y as f32 * scale_y) as u32;
        let sy1 = (((y + 1) as f32 * scale_y).ceil() as u32).min(src_h);
        for x in 0..dst_w {
            let sx0 = (x as f32 * scale_x) as u32;
            let sx1 = (((x + 1) as f32 * scale_x).ceil() as u32).min(src_w);
            let mut acc = [0.0f32; 4];
            let mut count = 0u32;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let p = src[(sy * src_w + sx) as usize];
                    for (a, &s) in acc.iter_mut().zip(p.iter()) {
                        *a += s;
                    }
                    count += 1;
                }
            }
            if count > 0 {
                let inv = 1.0 / count as f32;
                for a in &mut acc {
                    *a *= inv;
                }
            }
            out[(y * dst_w + x) as usize] = acc;
        }
    }
    out
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
    fn test_is_udim_path() {
        assert!(is_udim_path("color_<UDIM>.exr"));
        assert!(is_udim_path("/textures/diffuse_<UDIM>.png"));
        assert!(!is_udim_path("color_1001.exr"));
        assert!(!is_udim_path("diffuse.png"));
    }

    #[test]
    fn test_transform_uv_non_udim() {
        let tex = Texture::solid_color(Vec3::new(1.0, 0.0, 0.0));
        // Non-UDIM wraps to [0,1]
        let (u, v) = tex.transform_uv(1.5, -0.3);
        assert!((u - 0.5).abs() < 1e-5);
        assert!((v - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_transform_uv_2x2_grid() {
        // Simulate a 2x2 UDIM grid (tiles 1001, 1002, 1011, 1012)
        let mut tex = Texture::solid_color(Vec3::new(0.0, 0.0, 0.0));
        tex.udim_grid_cols = 2;
        tex.udim_grid_rows = 2;
        tex.udim_min_col = 0;
        tex.udim_min_row = 0;

        // Tile 1001 (col=0, row=0): UV (0.5, 0.5) → atlas bottom-left
        let (au, av) = tex.transform_uv(0.5, 0.5);
        // col=0, row=0, flipped_row=1
        // atlas_u = (0 + 0.5) / 2 = 0.25
        // atlas_v = (1 + 0.5) / 2 = 0.75
        assert!((au - 0.25).abs() < 1e-5, "tile 1001 u: got {}", au);
        assert!((av - 0.75).abs() < 1e-5, "tile 1001 v: got {}", av);

        // Tile 1002 (col=1, row=0): UV (1.5, 0.5) → atlas bottom-right
        let (au, av) = tex.transform_uv(1.5, 0.5);
        assert!((au - 0.75).abs() < 1e-5, "tile 1002 u: got {}", au);
        assert!((av - 0.75).abs() < 1e-5, "tile 1002 v: got {}", av);

        // Tile 1011 (col=0, row=1): UV (0.5, 1.5) → atlas top-left
        let (au, av) = tex.transform_uv(0.5, 1.5);
        assert!((au - 0.25).abs() < 1e-5, "tile 1011 u: got {}", au);
        assert!((av - 0.25).abs() < 1e-5, "tile 1011 v: got {}", av);

        // Tile 1012 (col=1, row=1): UV (1.5, 1.5) → atlas top-right
        let (au, av) = tex.transform_uv(1.5, 1.5);
        assert!((au - 0.75).abs() < 1e-5, "tile 1012 u: got {}", au);
        assert!((av - 0.25).abs() < 1e-5, "tile 1012 v: got {}", av);
    }

    #[test]
    fn test_udim_sample_tile_color() {
        // Build a 2x1 UDIM atlas: left tile=red, right tile=blue
        // Each tile is 2px wide so bilinear sampling stays within tile at center
        let pixels = vec![
            [1.0, 0.0, 0.0, 1.0], // tile 1001 px 0
            [1.0, 0.0, 0.0, 1.0], // tile 1001 px 1
            [0.0, 0.0, 1.0, 1.0], // tile 1002 px 0
            [0.0, 0.0, 1.0, 1.0], // tile 1002 px 1
        ];
        let mut tex = Texture::new(4, 1, pixels, "<udim_test>");
        tex.udim_grid_cols = 2;
        tex.udim_grid_rows = 1;
        tex.udim_min_col = 0;
        tex.udim_min_row = 0;

        // Sample tile 1001 center (u=0.5, v=0.5) → should be red
        let color = tex.sample(0.5, 0.5);
        assert!(
            (color.x - 1.0).abs() < 0.01,
            "expected red, got {:?}",
            color
        );
        assert!(color.z < 0.01, "expected no blue, got {:?}", color);

        // Sample tile 1002 center (u=1.5, v=0.5) → should be blue
        let color = tex.sample(1.5, 0.5);
        assert!(color.x < 0.01, "expected no red, got {:?}", color);
        assert!(
            (color.z - 1.0).abs() < 0.01,
            "expected blue, got {:?}",
            color
        );
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
