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

    /// Wrap UV coordinates to [0, 1] for texture sampling.
    #[inline]
    fn wrap_uv(&self, u: f32, v: f32) -> (f32, f32) {
        (u.rem_euclid(1.0), v.rem_euclid(1.0))
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
        if !u.is_finite() || !v.is_finite() {
            return Vec3::new(1.0, 0.0, 1.0); // Magenta debug color
        }

        let (u, v) = self.wrap_uv(u, v);

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
        if self.width == 0 || self.height == 0 {
            return 0.0;
        }
        if !u.is_finite() || !v.is_finite() {
            return 0.0;
        }
        let (u, v) = self.wrap_uv(u, v);
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

// ── UDIM Per-Tile Types ─────────────────────────────────────────

/// Info about a single UDIM tile on disk.
#[derive(Clone, Debug)]
pub struct UdimTileInfo {
    /// UDIM ID (1001-1200)
    pub udim_id: u32,
    /// Column in UDIM grid: `(udim - 1001) % 10`
    pub col: u32,
    /// Row in UDIM grid: `(udim - 1001) / 10`
    pub row: u32,
    /// Resolved file path
    pub path: String,
}

/// Grid layout computed from discovered UDIM tiles.
#[derive(Clone, Debug)]
pub struct UdimGridLayout {
    pub tiles: Vec<UdimTileInfo>,
    pub num_cols: u32,
    pub num_rows: u32,
    pub min_col: u32,
    pub min_row: u32,
}

impl UdimGridLayout {
    /// Compute grid layout from `(udim_id, path)` pairs.
    /// Returns `None` if tiles is empty.
    pub fn from_tiles(tiles: Vec<(u32, String)>) -> Option<Self> {
        if tiles.is_empty() {
            return None;
        }
        let mut min_col = u32::MAX;
        let mut max_col = 0u32;
        let mut min_row = u32::MAX;
        let mut max_row = 0u32;
        let mut infos = Vec::with_capacity(tiles.len());
        for (udim, path) in tiles {
            if !(1001..=1200).contains(&udim) {
                log::warn!("Skipping invalid UDIM ID {} (expected 1001-1200)", udim);
                continue;
            }
            let col = (udim - 1001) % 10;
            let row = (udim - 1001) / 10;
            min_col = min_col.min(col);
            max_col = max_col.max(col);
            min_row = min_row.min(row);
            max_row = max_row.max(row);
            infos.push(UdimTileInfo {
                udim_id: udim,
                col,
                row,
                path,
            });
        }
        if infos.is_empty() {
            return None;
        }
        Some(Self {
            tiles: infos,
            num_cols: max_col - min_col + 1,
            num_rows: max_row - min_row + 1,
            min_col,
            min_row,
        })
    }

    /// Total grid slots (num_cols * num_rows). Some may be empty (sparse UDIM).
    #[must_use]
    pub fn grid_slots(&self) -> u32 {
        self.num_cols * self.num_rows
    }
}

/// A set of individually-loaded UDIM tiles (no atlas stitching).
///
/// Each tile is an independent `Texture`. Sampling resolves the tile from
/// UV coordinates, then samples within the tile using fractional UVs.
/// Missing tiles in sparse layouts return magenta.
#[derive(Clone, Debug)]
pub struct UdimTileSet {
    pub layout: UdimGridLayout,
    /// Tiles indexed by `(row - min_row) * num_cols + (col - min_col)`.
    /// `None` = missing tile (sparse UDIM).
    pub tiles: Vec<Option<Arc<Texture>>>,
    pub is_linear: bool,
    pub pattern: String,
}

impl UdimTileSet {
    /// Sample the tile set at UV coordinates (bilinear filtering).
    ///
    /// Resolves tile from `floor(u)`, `floor(v)`, then samples within
    /// the tile using `fract(u)`, `fract(v)`.
    #[must_use]
    pub fn sample(&self, u: f32, v: f32) -> Vec3 {
        if !u.is_finite() || !v.is_finite() {
            return Vec3::new(1.0, 0.0, 1.0);
        }
        match self.resolve_tile(u, v) {
            Some(tile) => {
                let su = u.fract().rem_euclid(1.0);
                let sv = v.fract().rem_euclid(1.0);
                tile.sample(su, sv)
            }
            None => Vec3::new(1.0, 0.0, 1.0), // magenta = missing tile
        }
    }

    /// Sample a single channel with bilinear filtering.
    #[must_use]
    pub fn sample_channel(&self, u: f32, v: f32, channel: usize) -> f32 {
        if !u.is_finite() || !v.is_finite() {
            return 0.0;
        }
        match self.resolve_tile(u, v) {
            Some(tile) => {
                let su = u.fract().rem_euclid(1.0);
                let sv = v.fract().rem_euclid(1.0);
                tile.sample_channel(su, sv, channel)
            }
            None => 0.0,
        }
    }

    /// Resolve which tile a UV coordinate maps to.
    /// Out-of-range UVs clamp to the grid boundary (matches shader behavior).
    /// This silently shows the edge tile instead of a diagnostic color —
    /// acceptable for viewport preview, but a future debug mode could return None.
    fn resolve_tile(&self, u: f32, v: f32) -> Option<&Texture> {
        let raw_col = u.floor() as i32 - self.layout.min_col as i32;
        let raw_row = v.floor() as i32 - self.layout.min_row as i32;
        let col = raw_col.clamp(0, self.layout.num_cols as i32 - 1) as u32;
        let row = raw_row.clamp(0, self.layout.num_rows as i32 - 1) as u32;
        let idx = (row * self.layout.num_cols + col) as usize;
        self.tiles.get(idx).and_then(|t| t.as_deref())
    }

    /// Number of non-empty tiles.
    pub fn tile_count(&self) -> usize {
        self.tiles.iter().filter(|t| t.is_some()).count()
    }

    /// Get a specific tile by grid position (for material editor preview).
    pub fn get_tile(&self, col: u32, row: u32) -> Option<&Arc<Texture>> {
        let idx = (row * self.layout.num_cols + col) as usize;
        self.tiles.get(idx).and_then(|t| t.as_ref())
    }
}

/// Cache for loaded textures.
///
/// Textures are loaded on-demand and cached for reuse.
/// When the `oiio` feature is enabled, textures can be loaded with mipmaps
/// and automatically converted to .tx format.
#[derive(Clone)]
pub struct TextureCache {
    /// Cached textures by file path
    textures: HashMap<String, Arc<Texture>>,

    /// Cached UDIM tile sets by UDIM pattern path
    udim_tilesets: HashMap<String, Arc<UdimTileSet>>,

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
            udim_tilesets: HashMap::new(),
            base_dir: None,
            #[cfg(feature = "oiio")]
            prefer_tx: true,
            #[cfg(feature = "oiio")]
            generate_mipmaps: true,
        }
    }

    /// Create a texture cache with a base directory for relative paths.
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            textures: HashMap::new(),
            udim_tilesets: HashMap::new(),
            base_dir: Some(base_dir.into()),
            #[cfg(feature = "oiio")]
            prefer_tx: true,
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

        // UDIM paths should use load_udim_tileset() instead
        if is_udim_path(path) {
            return Err(TextureError::LoadError(
                "Use load_udim_tileset() for UDIM textures".to_string(),
            ));
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

        // UDIM paths should use load_udim_tileset() instead
        if is_udim_path(path) {
            return Err(TextureError::LoadError(
                "Use load_udim_tileset() for UDIM textures".to_string(),
            ));
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

    /// Load a UDIM texture as a per-tile set (no atlas stitching).
    ///
    /// Each tile is loaded individually and stored in a `UdimTileSet`.
    /// This is much faster than atlas stitching since no pixel copying
    /// or downscaling is needed.
    pub fn load_udim_tileset(
        &mut self,
        path: &str,
        linear: bool,
    ) -> TextureResult<Arc<UdimTileSet>> {
        let cache_key = if linear {
            format!("{}_linear", path)
        } else {
            path.to_string()
        };
        if let Some(ts) = self.udim_tilesets.get(&cache_key) {
            return Ok(ts.clone());
        }

        let resolved_pattern = self.resolve_path(path).to_string_lossy().into_owned();
        let raw_tiles = find_udim_tiles(&resolved_pattern);
        if raw_tiles.is_empty() {
            return Err(TextureError::LoadError(format!(
                "No UDIM tiles found for pattern: {}",
                path
            )));
        }

        let layout = UdimGridLayout::from_tiles(raw_tiles).ok_or_else(|| {
            TextureError::LoadError("Failed to compute UDIM grid layout".to_string())
        })?;

        let start = std::time::Instant::now();

        // Allocate grid slots (None = missing tile)
        let slot_count = (layout.num_cols * layout.num_rows) as usize;
        let mut tiles: Vec<Option<Arc<Texture>>> = vec![None; slot_count];

        // Load each tile individually, reusing the single-texture cache
        for info in &layout.tiles {
            let col = info.col - layout.min_col;
            let row = info.row - layout.min_row;
            let idx = (row * layout.num_cols + col) as usize;

            let tile = if linear {
                load_texture_linear(Path::new(&info.path))?
            } else {
                #[cfg(feature = "oiio")]
                {
                    self.load_with_oiio(Path::new(&info.path), &info.path)?
                }
                #[cfg(not(feature = "oiio"))]
                {
                    load_texture_file(Path::new(&info.path))?
                }
            };
            tiles[idx] = Some(Arc::new(tile));
        }

        let elapsed = start.elapsed();
        log::info!(
            "UDIM tileset: {} tiles ({}x{} grid) loaded in {:.1}ms — {}",
            layout.tiles.len(),
            layout.num_cols,
            layout.num_rows,
            elapsed.as_secs_f64() * 1000.0,
            path
        );

        let tileset = Arc::new(UdimTileSet {
            layout,
            tiles,
            is_linear: linear,
            pattern: path.to_string(),
        });
        self.udim_tilesets.insert(cache_key, tileset.clone());
        Ok(tileset)
    }

    /// Get a cached UDIM tile set.
    pub fn get_udim_tileset(&self, path: &str) -> Option<Arc<UdimTileSet>> {
        self.udim_tilesets.get(path).cloned()
    }

    /// Pre-convert a list of texture paths to .tx via subprocess.
    /// Returns number of successful conversions.
    /// Call this from GUI before rendering to pre-generate .tx files.
    #[cfg(feature = "oiio")]
    pub fn convert_textures_to_tx(&self, paths: &[String]) -> usize {
        use rayon::prelude::*;
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Pre-filter to find which paths need conversion (serial, fast)
        // Skip UDIM patterns — individual tiles are converted separately
        let to_convert: Vec<_> = paths
            .iter()
            .filter(|p| !is_udim_path(p))
            .filter_map(|path| {
                let full_path = self.resolve_path(path);
                let tx_path = oiio::get_tx_path(&full_path);
                if oiio::tx_is_valid(&full_path, &tx_path) {
                    None
                } else {
                    Some((full_path, tx_path))
                }
            })
            .collect();

        if to_convert.is_empty() {
            return 0;
        }

        log::info!("Converting {} textures to .tx (parallel)", to_convert.len());
        let converted = AtomicUsize::new(0);

        to_convert.par_iter().for_each(|(full_path, tx_path)| {
            log::info!("Converting to .tx: {}", full_path.display());
            if Self::make_tx_subprocess(full_path, tx_path) {
                converted.fetch_add(1, Ordering::Relaxed);
            }
        });

        let count = converted.load(Ordering::Relaxed);
        if count > 0 {
            log::info!("Converted {}/{} textures to .tx", count, to_convert.len());
        }
        count
    }

    /// Delete .tx cache files for the given source paths.
    /// Returns number of .tx files successfully removed.
    #[cfg(feature = "oiio")]
    pub fn clear_tx_cache(&self, paths: &[String]) -> usize {
        let mut removed = 0;
        for path in paths {
            let full_path = self.resolve_path(path);
            let tx_path = oiio::get_tx_path(&full_path);
            if tx_path.exists() && std::fs::remove_file(&tx_path).is_ok() {
                removed += 1;
            }
        }
        if removed > 0 {
            log::info!("Cleared {} .tx cache files", removed);
        }
        removed
    }

    /// Pre-warm the texture cache by loading all textures in parallel.
    /// Textures already in cache are skipped. After this call,
    /// subsequent `load()` calls will hit the in-memory cache.
    #[cfg(feature = "oiio")]
    pub fn pre_warm_parallel(&mut self, paths: &[String]) {
        use rayon::prelude::*;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let to_load: Vec<_> = paths
            .iter()
            .filter(|p| !is_udim_path(p) && !self.textures.contains_key(p.as_str()))
            .cloned()
            .collect();

        if to_load.is_empty() {
            return;
        }

        log::info!("Pre-warming {} textures in parallel", to_load.len());
        let start = std::time::Instant::now();

        let base_dir = self.base_dir.clone();
        let prefer_tx = self.prefer_tx;
        let generate_mipmaps = self.generate_mipmaps;
        let failed = AtomicUsize::new(0);

        let results: Vec<_> = to_load
            .par_iter()
            .filter_map(|path| {
                // Resolve path (same logic as resolve_path but standalone)
                let normalized = normalize_path(path);
                let norm_path = std::path::Path::new(&normalized);
                let full_path = if norm_path.is_absolute() {
                    norm_path.to_path_buf()
                } else if let Some(base) = &base_dir {
                    base.join(norm_path)
                } else {
                    norm_path.to_path_buf()
                };

                // Prefer .tx if valid
                let load_path = if prefer_tx {
                    let tx_path = oiio::get_tx_path(&full_path);
                    if oiio::tx_is_valid(&full_path, &tx_path) {
                        tx_path
                    } else {
                        full_path.clone()
                    }
                } else {
                    full_path.clone()
                };

                // Load via OIIO
                let oiio_result = if generate_mipmaps {
                    oiio::load_texture_with_mips(&load_path)
                } else {
                    oiio::load_texture(&load_path)
                };

                match oiio_result {
                    Ok(oiio_tex) => {
                        if oiio_tex.mip_levels.is_empty() {
                            failed.fetch_add(1, Ordering::Relaxed);
                            return None;
                        }
                        let base = &oiio_tex.mip_levels[0];
                        let pixels = convert_u8_to_f32_pixels(&base.data, oiio_tex.is_linear);
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
                        let texture = Texture::with_mips(
                            oiio_tex.width,
                            oiio_tex.height,
                            pixels,
                            mip_levels,
                            path.as_str(),
                            oiio_tex.is_linear,
                        );
                        Some((path.clone(), Arc::new(texture)))
                    }
                    Err(e) => {
                        log::warn!("Pre-warm failed {}: {}", path, e);
                        failed.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                }
            })
            .collect();

        let loaded = results.len();
        for (key, tex) in results {
            self.textures.insert(key, tex);
        }

        let elapsed = start.elapsed();
        let fail_count = failed.load(Ordering::Relaxed);
        if fail_count > 0 {
            log::info!(
                "Pre-warmed {} textures ({} failed) in {:.1}ms",
                loaded,
                fail_count,
                elapsed.as_secs_f64() * 1000.0
            );
        } else {
            log::info!(
                "Pre-warmed {} textures in {:.1}ms",
                loaded,
                elapsed.as_secs_f64() * 1000.0
            );
        }
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
                log::debug!("Loading .tx: {}", tx_path.display());
                tx_path
            } else {
                log::debug!("No .tx found: {}", full_path.display());
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

    /// Clear all cached textures and UDIM tile sets.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.udim_tilesets.clear();
    }

    /// Remove textures only held by the cache (strong_count == 1).
    /// Call after rebuilding materials to free orphaned texture RAM.
    pub fn sweep_unreferenced(&mut self) -> usize {
        let before = self.textures.len();
        self.textures.retain(|_, arc| Arc::strong_count(arc) > 1);
        let freed = before - self.textures.len();
        if freed > 0 {
            log::info!(
                "TextureCache: swept {} unreferenced textures ({} remain)",
                freed,
                self.textures.len()
            );
        }
        freed
    }

    /// Get total memory usage of cached textures.
    pub fn total_size_bytes(&self) -> usize {
        self.textures.values().map(|t| t.size_bytes()).sum()
    }

    /// Resolve a path relative to the base directory.
    /// Normalizes forward-slash UNC paths on Windows before checking.
    fn resolve_path(&self, path: &str) -> PathBuf {
        let normalized = normalize_path(path);
        let path = Path::new(&normalized);

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

/// Normalize path for OS filesystem access.
/// On Windows, converts forward-slash UNC paths (`//server/share/...`)
/// to backslash UNC paths (`\\server\share\...`) that Windows APIs expect.
fn normalize_path(path: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        path.replace('/', "\\")
    }
    #[cfg(not(target_os = "windows"))]
    {
        path.to_string()
    }
}

/// Check if a texture path contains a UDIM token (`<UDIM>`).
pub fn is_udim_path(path: &str) -> bool {
    path.contains("<UDIM>")
}

/// Scan filesystem for existing UDIM tiles matching the pattern.
/// Returns a vec of (udim_id, path) sorted by UDIM ID.
pub fn find_udim_tiles(pattern: &str) -> Vec<(u32, String)> {
    let mut tiles = Vec::new();
    for udim in 1001..=1200 {
        let tile_path = pattern.replace("<UDIM>", &udim.to_string());
        let normalized = normalize_path(&tile_path);
        if Path::new(&normalized).exists() {
            tiles.push((udim, normalized));
        }
    }
    log::debug!(
        "UDIM tile scan: pattern={}, found {} tiles",
        pattern,
        tiles.len()
    );
    tiles
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
    fn test_wrap_uv() {
        let tex = Texture::solid_color(Vec3::new(1.0, 0.0, 0.0));
        // Wraps to [0,1]
        let (u, v) = tex.wrap_uv(1.5, -0.3);
        assert!((u - 0.5).abs() < 1e-5);
        assert!((v - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_sweep_unreferenced() {
        let mut cache = TextureCache::new();

        // Insert two textures
        let tex_a = Arc::new(Texture::solid_color(Vec3::new(1.0, 0.0, 0.0)));
        let tex_b = Arc::new(Texture::solid_color(Vec3::new(0.0, 1.0, 0.0)));

        cache.textures.insert("a".to_string(), tex_a.clone());
        cache.textures.insert("b".to_string(), tex_b.clone());
        assert_eq!(cache.len(), 2);

        // Both have external refs (strong_count > 1) — sweep should free nothing
        assert_eq!(cache.sweep_unreferenced(), 0);
        assert_eq!(cache.len(), 2);

        // Drop external ref to tex_b — now only cache holds it (strong_count == 1)
        drop(tex_b);
        assert_eq!(cache.sweep_unreferenced(), 1);
        assert_eq!(cache.len(), 1);
        assert!(cache.is_cached("a"));
        assert!(!cache.is_cached("b"));

        // Drop external ref to tex_a — sweep should free it too
        drop(tex_a);
        assert_eq!(cache.sweep_unreferenced(), 1);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_normalize_path_unc() {
        let result = normalize_path("//server/share/textures/diffuse.png");
        #[cfg(target_os = "windows")]
        assert_eq!(result, r"\\server\share\textures\diffuse.png");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(result, "//server/share/textures/diffuse.png");
    }

    #[test]
    fn test_normalize_path_regular() {
        let result = normalize_path("textures/diffuse.png");
        #[cfg(target_os = "windows")]
        assert_eq!(result, r"textures\diffuse.png");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(result, "textures/diffuse.png");
    }

    #[test]
    fn test_normalize_path_already_backslash() {
        let result = normalize_path(r"\\server\share\diffuse.png");
        #[cfg(target_os = "windows")]
        assert_eq!(result, r"\\server\share\diffuse.png");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(result, r"\\server\share\diffuse.png");
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

    #[test]
    fn test_udim_grid_layout_from_tiles() {
        // 2x2 grid: tiles 1001, 1002, 1011, 1012
        let tiles = vec![
            (1001, "a_1001.exr".into()),
            (1002, "a_1002.exr".into()),
            (1011, "a_1011.exr".into()),
            (1012, "a_1012.exr".into()),
        ];
        let layout = UdimGridLayout::from_tiles(tiles).unwrap();
        assert_eq!(layout.num_cols, 2);
        assert_eq!(layout.num_rows, 2);
        assert_eq!(layout.min_col, 0);
        assert_eq!(layout.min_row, 0);
        assert_eq!(layout.grid_slots(), 4);
    }

    #[test]
    fn test_udim_grid_layout_sparse() {
        // Sparse: tiles 1001, 1003, 1011 (missing 1002, 1012, 1013)
        let tiles = vec![
            (1001, "a_1001.exr".into()),
            (1003, "a_1003.exr".into()),
            (1011, "a_1011.exr".into()),
        ];
        let layout = UdimGridLayout::from_tiles(tiles).unwrap();
        assert_eq!(layout.num_cols, 3); // cols 0,1,2
        assert_eq!(layout.num_rows, 2); // rows 0,1
        assert_eq!(layout.min_col, 0);
        assert_eq!(layout.min_row, 0);
    }

    #[test]
    fn test_udim_grid_layout_empty() {
        assert!(UdimGridLayout::from_tiles(vec![]).is_none());
    }

    #[test]
    fn test_udim_tileset_sample_2x1() {
        // 2 tiles: tile 1001 = red (2x2), tile 1002 = blue (2x2)
        let red = Arc::new(Texture::new(2, 2, vec![[1.0, 0.0, 0.0, 1.0]; 4], "red"));
        let blue = Arc::new(Texture::new(2, 2, vec![[0.0, 0.0, 1.0, 1.0]; 4], "blue"));
        let layout = UdimGridLayout::from_tiles(vec![
            (1001, "a_1001.exr".into()),
            (1002, "a_1002.exr".into()),
        ])
        .unwrap();
        let ts = UdimTileSet {
            layout,
            tiles: vec![Some(red), Some(blue)],
            is_linear: false,
            pattern: "a_<UDIM>.exr".into(),
        };

        // Tile 1001 center → red
        let c = ts.sample(0.5, 0.5);
        assert!((c.x - 1.0).abs() < 0.01, "1001 should be red: {:?}", c);
        assert!(c.z < 0.01);

        // Tile 1002 center → blue
        let c = ts.sample(1.5, 0.5);
        assert!((c.z - 1.0).abs() < 0.01, "1002 should be blue: {:?}", c);
        assert!(c.x < 0.01);
    }

    #[test]
    fn test_udim_tileset_missing_tile() {
        // 2x1 grid, only tile 1001 present (1002 missing)
        let red = Arc::new(Texture::new(2, 2, vec![[1.0, 0.0, 0.0, 1.0]; 4], "red"));
        let layout = UdimGridLayout::from_tiles(vec![(1001, "a_1001.exr".into())]).unwrap();
        let ts = UdimTileSet {
            layout,
            tiles: vec![Some(red)],
            is_linear: false,
            pattern: "a_<UDIM>.exr".into(),
        };

        // Tile 1001 → red
        let c = ts.sample(0.5, 0.5);
        assert!((c.x - 1.0).abs() < 0.01);

        // Out of grid → clamped to tile 1001 (only 1 col)
        let c = ts.sample(1.5, 0.5);
        assert!((c.x - 1.0).abs() < 0.01, "clamped to tile 0: {:?}", c);
    }

    #[test]
    fn test_udim_tileset_2x2_grid() {
        // 4 tiles: 1001=red, 1002=green, 1011=blue, 1012=white
        let mk = |r, g, b| Arc::new(Texture::new(2, 2, vec![[r, g, b, 1.0]; 4], "t"));
        let layout = UdimGridLayout::from_tiles(vec![
            (1001, "a_1001.exr".into()),
            (1002, "a_1002.exr".into()),
            (1011, "a_1011.exr".into()),
            (1012, "a_1012.exr".into()),
        ])
        .unwrap();
        let ts = UdimTileSet {
            layout,
            tiles: vec![
                Some(mk(1.0, 0.0, 0.0)), // col=0,row=0
                Some(mk(0.0, 1.0, 0.0)), // col=1,row=0
                Some(mk(0.0, 0.0, 1.0)), // col=0,row=1
                Some(mk(1.0, 1.0, 1.0)), // col=1,row=1
            ],
            is_linear: false,
            pattern: "a_<UDIM>.exr".into(),
        };

        let c = ts.sample(0.5, 0.5);
        assert!((c.x - 1.0).abs() < 0.1, "1001=red: {:?}", c);

        let c = ts.sample(1.5, 0.5);
        assert!((c.y - 1.0).abs() < 0.1, "1002=green: {:?}", c);

        let c = ts.sample(0.5, 1.5);
        assert!((c.z - 1.0).abs() < 0.1, "1011=blue: {:?}", c);

        let c = ts.sample(1.5, 1.5);
        assert!(
            (c.x - 1.0).abs() < 0.1 && (c.y - 1.0).abs() < 0.1 && (c.z - 1.0).abs() < 0.1,
            "1012=white: {:?}",
            c
        );
    }

    #[test]
    fn test_udim_tileset_sample_channel() {
        let tex = Arc::new(Texture::new(1, 1, vec![[0.2, 0.5, 0.8, 1.0]], "t"));
        let layout = UdimGridLayout::from_tiles(vec![(1001, "a_1001.exr".into())]).unwrap();
        let ts = UdimTileSet {
            layout,
            tiles: vec![Some(tex)],
            is_linear: false,
            pattern: "a_<UDIM>.exr".into(),
        };
        assert!((ts.sample_channel(0.5, 0.5, 0) - 0.2).abs() < 0.01);
        assert!((ts.sample_channel(0.5, 0.5, 1) - 0.5).abs() < 0.01);
        assert!((ts.sample_channel(0.5, 0.5, 2) - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_udim_tileset_negative_uv() {
        // Negative UVs should clamp to tile (0,0) via resolve_tile
        let red = Arc::new(Texture::new(
            2, 2,
            vec![[1.0, 0.0, 0.0, 1.0]; 4],
            "red",
        ));
        let layout = UdimGridLayout::from_tiles(vec![
            (1001, "a_1001.exr".into()),
            (1002, "a_1002.exr".into()),
        ]).unwrap();
        let ts = UdimTileSet {
            layout,
            tiles: vec![Some(red), None],
            is_linear: false,
            pattern: "a_<UDIM>.exr".into(),
        };

        // Negative UV clamps to col=0 (tile 1001 = red)
        let c = ts.sample(-0.5, 0.5);
        assert!((c.x - 1.0).abs() < 0.01, "negative u should clamp to tile 0: {:?}", c);

        // Negative v also clamps
        let c = ts.sample(0.5, -0.5);
        assert!((c.x - 1.0).abs() < 0.01, "negative v should clamp to tile 0: {:?}", c);
    }

    #[test]
    fn test_udim_grid_layout_invalid_id() {
        // UDIM IDs outside 1001-1200 should be skipped
        let tiles = vec![
            (999, "bad.exr".into()),
            (1001, "a_1001.exr".into()),
        ];
        let layout = UdimGridLayout::from_tiles(tiles).unwrap();
        assert_eq!(layout.tiles.len(), 1);
        assert_eq!(layout.tiles[0].udim_id, 1001);
    }
}
