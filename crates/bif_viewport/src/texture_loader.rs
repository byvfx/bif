//! Texture loading utilities for viewport rendering.
//!
//! Provides functions for loading, converting, and uploading textures to GPU.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use wgpu::{Device, Queue};

use crate::gpu_types::{GpuTextureSet, MAX_VIEWPORT_TEXTURES};

/// Process-global "GPU is no longer usable" flag.
///
/// Flipped by the wgpu uncaptured-error / device-lost callbacks installed at
/// device creation (see `bif_qt::Viewport::new`). Once set, the texture poll
/// loop drops further uploads instead of pushing more work at a dead device.
static GPU_UNHEALTHY: AtomicBool = AtomicBool::new(false);

pub fn gpu_is_healthy() -> bool {
    !GPU_UNHEALTHY.load(Ordering::Acquire)
}

pub fn mark_gpu_unhealthy() {
    if !GPU_UNHEALTHY.swap(true, Ordering::AcqRel) {
        log::warn!("viewport: GPU device lost — stopping further texture uploads");
    }
}

/// Default per-scene VRAM cap for viewport textures: 1.5 GiB.
///
/// Picked to leave headroom on 4-6 GiB consumer GPUs after Qt compositor,
/// swapchain, geometry buffers, IBL, etc. Production shots with 1000+
/// textures will hit this and fall back to placeholders for the overflow.
pub const DEFAULT_VRAM_BUDGET_BYTES: u64 = 1_500 * 1024 * 1024;

/// Running tally of bytes uploaded to GPU texture slots.
///
/// `try_charge` returns false once the budget is consumed, so the caller
/// can skip the upload and use a placeholder instead. Logs exactly once on
/// the first overflow so production shots don't spam the terminal.
#[derive(Debug)]
pub struct TextureBudget {
    used_bytes: u64,
    max_bytes: u64,
    warned: bool,
}

impl TextureBudget {
    pub fn new(max_bytes: u64) -> Self {
        Self {
            used_bytes: 0,
            max_bytes,
            warned: false,
        }
    }

    pub fn used_bytes(&self) -> u64 {
        self.used_bytes
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Try to reserve VRAM for a texture of the given size (RGBA8 + optional
    /// mip chain). Returns true on success and commits the charge. Returns
    /// false once the budget would be exceeded; the caller should skip the
    /// upload.
    pub fn try_charge(&mut self, width: u32, height: u32, mip_count: u32) -> bool {
        let bytes = estimate_texture_bytes(width, height, mip_count);
        if self.used_bytes.saturating_add(bytes) > self.max_bytes {
            if !self.warned {
                self.warned = true;
                log::warn!(
                    "viewport: VRAM budget {} MiB exhausted ({} MiB used, refused {}x{} ~{} KiB) — remaining textures render as placeholders",
                    self.max_bytes / (1024 * 1024),
                    self.used_bytes / (1024 * 1024),
                    width,
                    height,
                    bytes / 1024,
                );
            }
            return false;
        }
        self.used_bytes += bytes;
        true
    }
}

impl Default for TextureBudget {
    fn default() -> Self {
        Self::new(DEFAULT_VRAM_BUDGET_BYTES)
    }
}

/// Approximate VRAM cost of a 2D RGBA8 texture with `mip_count` levels.
/// 4 bytes per pixel; full mip chain converges on ~4/3 of base level bytes.
fn estimate_texture_bytes(width: u32, height: u32, mip_count: u32) -> u64 {
    let base = u64::from(width) * u64::from(height) * 4;
    if mip_count <= 1 {
        base
    } else {
        // Sum of geometric series 1 + 1/4 + 1/16 + ... bounded by 4/3.
        base * 4 / 3
    }
}

/// Message sent from background texture loading thread.
pub struct TextureLoadMessage {
    /// Resolved texture path (matches key in GpuTextureSet.index_map)
    pub path: String,
    /// Raw u8 RGBA pixel data
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub is_linear: bool,
}

/// Default maximum texture dimension for viewport rendering.
/// Textures larger than this are downscaled during CPU load.
/// Full-res textures are only loaded for batch render.
pub const DEFAULT_MAX_VIEWPORT_TEXTURE_SIZE: u32 = 2048;

/// Raw u8 texture data for direct GPU upload (no f32 intermediate).
struct RawTexture {
    width: u32,
    height: u32,
    /// RGBA u8 pixel data
    data: Vec<u8>,
    is_linear: bool,
    path: String,
}

/// Minimum texture size (in either dimension) to use GPU mipmaps.
/// Textures smaller than this skip mipmap generation (dispatch overhead not worth it).
const GPU_MIPMAP_MIN_SIZE: u32 = 64;

/// GPU compute mipmap generator.
///
/// Uses a box-filter downsample compute shader to generate mipmaps on the GPU,
/// avoiding expensive CPU mipmap generation.
pub struct MipmapGenerator {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl MipmapGenerator {
    /// Create the mipmap compute pipeline.
    pub fn new(device: &Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Mipmap Downsample Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/mipmap_downsample.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Mipmap BGL"),
            entries: &[
                // Source mip level (read via textureLoad)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Destination mip level (storage write)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Mipmap Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Mipmap Downsample Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
        }
    }

    /// Generate mipmaps for a texture using GPU compute.
    ///
    /// The texture must have been created with:
    /// - `STORAGE_BINDING` usage
    /// - `mip_level_count > 1`
    /// - Base level (mip 0) already uploaded
    ///
    /// Wraps the submit in a wgpu error scope so a device-lost (e.g. VRAM
    /// exhausted by a 1000+ texture scene) is captured and flipped into the
    /// process-global `mark_gpu_unhealthy` flag instead of fatal-panicking
    /// the main thread via `__fastfail`.
    pub fn generate(
        &self,
        device: &Device,
        queue: &Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
        mip_count: u32,
    ) {
        if mip_count <= 1 {
            return;
        }
        if !gpu_is_healthy() {
            return;
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Mipmap Generation"),
        });

        for mip in 1..mip_count {
            // Both views must use Rgba8Unorm: storage textures don't support sRGB,
            // and textureLoad reads raw values regardless of sRGB anyway.
            let src_view = texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(wgpu::TextureFormat::Rgba8Unorm),
                base_mip_level: mip - 1,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let dst_view = texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(wgpu::TextureFormat::Rgba8Unorm),
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Mipmap Bind Group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dst_view),
                    },
                ],
            });

            let mip_width = (width >> mip).max(1);
            let mip_height = (height >> mip).max(1);

            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Mipmap Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(mip_width.div_ceil(8), mip_height.div_ceil(8), 1);
        }

        // wgpu's `Queue::submit` routes errors through `handle_error_fatal!`
        // which panics unconditionally — `push_error_scope` does NOT catch
        // it. The only reliable way to survive a device-lost (e.g. VRAM
        // exhausted by a 1000+ texture production shot) is to catch the
        // unwinding panic itself. `panic = "unwind"` is the default for
        // dev/release in this workspace, so `catch_unwind` works.
        let _ = device;
        let submitted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            queue.submit(std::iter::once(encoder.finish()));
        }));
        if submitted.is_err() {
            log::error!(
                "mipmap submit panicked ({width}x{height}, {mip_count} mips) — GPU is dead, marking unhealthy"
            );
            mark_gpu_unhealthy();
        }
    }
}

/// Calculate the number of mip levels for given dimensions.
fn calculate_mip_count(width: u32, height: u32) -> u32 {
    let max_dim = width.max(height);
    (max_dim as f32).log2().floor() as u32 + 1
}

/// Check if a .tx file exists and is valid (newer than source).
/// Returns the .tx path if it should be used, None otherwise.
#[cfg(feature = "oiio")]
fn resolve_tx_path(source_path: &str) -> Option<String> {
    let tx_path = bif_core::oiio::get_tx_path(source_path);
    if bif_core::oiio::tx_is_valid(source_path, &tx_path) {
        Some(tx_path.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Check if texture path indicates linear color space (EXR, HDR).
/// Note: .tx files are NOT assumed linear — they preserve the source colorspace.
/// OIIO reports the actual is_linear flag from .tx metadata.
pub fn is_linear_texture_path(path: &str) -> bool {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some(ext) => matches!(ext.to_ascii_lowercase().as_str(), "exr" | "hdr"),
        None => false,
    }
}

/// Convert linear float value to sRGB byte.
pub fn linear_to_srgb_byte(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    let v = value.clamp(0.0, 1.0);
    let srgb = if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0 + 0.5) as u8
}

/// Convert linear float value to byte (no gamma correction).
pub fn linear_to_byte(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    let v = value.clamp(0.0, 1.0);
    (v * 255.0 + 0.5) as u8
}

/// Convert float RGBA pixels to byte RGBA.
pub fn texture_to_rgba8(width: u32, height: u32, pixels: &[[f32; 4]], is_linear: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((width * height * 4) as usize);
    for pixel in pixels {
        if is_linear {
            bytes.push(linear_to_byte(pixel[0]));
            bytes.push(linear_to_byte(pixel[1]));
            bytes.push(linear_to_byte(pixel[2]));
            bytes.push(linear_to_byte(pixel[3]));
        } else {
            bytes.push(linear_to_srgb_byte(pixel[0]));
            bytes.push(linear_to_srgb_byte(pixel[1]));
            bytes.push(linear_to_srgb_byte(pixel[2]));
            bytes.push(linear_to_byte(pixel[3]));
        }
    }
    bytes
}

/// Downscale texture using nearest-neighbor sampling.
pub fn downscale_texture_nearest(
    texture: &bif_core::texture::Texture,
    max_dimension: u32,
) -> (u32, u32, Vec<[f32; 4]>) {
    if texture.width <= max_dimension && texture.height <= max_dimension {
        return (texture.width, texture.height, texture.pixels.clone());
    }

    let scale = (texture.width as f32 / max_dimension as f32)
        .max(texture.height as f32 / max_dimension as f32);
    let new_width = ((texture.width as f32 / scale).floor() as u32).max(1);
    let new_height = ((texture.height as f32 / scale).floor() as u32).max(1);
    let mut pixels = vec![[0.0; 4]; (new_width * new_height) as usize];

    for y in 0..new_height {
        let src_y = ((y as f32) * scale).floor() as u32;
        let src_y = src_y.min(texture.height - 1);
        for x in 0..new_width {
            let src_x = ((x as f32) * scale).floor() as u32;
            let src_x = src_x.min(texture.width - 1);
            let src_index = (src_y * texture.width + src_x) as usize;
            let dst_index = (y * new_width + x) as usize;
            pixels[dst_index] = texture.pixels[src_index];
        }
    }

    (new_width, new_height, pixels)
}

/// Downscale raw u8 RGBA texture using nearest-neighbor sampling.
fn downscale_raw_nearest(
    width: u32,
    height: u32,
    data: &[u8],
    max_dimension: u32,
) -> (u32, u32, Vec<u8>) {
    if width <= max_dimension && height <= max_dimension {
        return (width, height, data.to_vec());
    }

    let scale = (width as f32 / max_dimension as f32).max(height as f32 / max_dimension as f32);
    let new_width = ((width as f32 / scale).floor() as u32).max(1);
    let new_height = ((height as f32 / scale).floor() as u32).max(1);
    let mut out = vec![0u8; (new_width * new_height * 4) as usize];

    for y in 0..new_height {
        let src_y = ((y as f32) * scale).floor() as u32;
        let src_y = src_y.min(height - 1);
        for x in 0..new_width {
            let src_x = ((x as f32) * scale).floor() as u32;
            let src_x = src_x.min(width - 1);
            let src_off = (src_y * width + src_x) as usize * 4;
            let dst_off = (y * new_width + x) as usize * 4;
            out[dst_off..dst_off + 4].copy_from_slice(&data[src_off..src_off + 4]);
        }
    }

    (new_width, new_height, out)
}

/// Load texture as raw u8 RGBA for direct GPU upload (OIIO path).
///
/// Skips the f32 intermediate — raw sRGB bytes go straight to GPU.
/// Handles UDIM patterns by stitching tile atlases.
#[cfg(feature = "oiio")]
fn load_raw_texture(path: &str, max_tile_size: u32) -> Option<RawTexture> {
    load_raw_texture_with_depth(path, 0, max_tile_size)
}

/// Inner loader with recursion depth tracking (OIIO path).
/// UDIM patterns should be expanded before calling — this loads individual tiles only.
#[cfg(feature = "oiio")]
fn load_raw_texture_with_depth(path: &str, _depth: u32, _max_tile_size: u32) -> Option<RawTexture> {
    // UDIM patterns should never reach here — they're expanded by the caller
    if is_udim_path(path) {
        log::warn!(
            "UDIM pattern reached load_raw_texture — should be expanded: {}",
            path
        );
        return None;
    }

    // Prefer .tx cache if valid and newer than source
    let tx_path = resolve_tx_path(path);
    let load_path = tx_path.as_deref().unwrap_or(path);
    if tx_path.is_some() {
        log::debug!("Using .tx cache: {}", load_path);
    }
    let is_linear = is_linear_texture_path(load_path);

    // Use OIIO to load as u8 directly (no mips for viewport)
    match bif_core::oiio::load_texture(load_path) {
        Ok(oiio_tex) => {
            if oiio_tex.mip_levels.is_empty() {
                log::warn!("OIIO returned no mip levels for {}", load_path);
                return None;
            }
            let base = &oiio_tex.mip_levels[0];
            Some(RawTexture {
                width: oiio_tex.width,
                height: oiio_tex.height,
                data: base.data.clone(),
                is_linear: oiio_tex.is_linear || is_linear,
                path: path.to_string(),
            })
        }
        Err(e) => {
            // If .tx load failed, retry with original source
            if tx_path.is_some() {
                log::warn!("Failed .tx load, retrying source: {}", path);
                let is_linear_src = is_linear_texture_path(path);
                match bif_core::oiio::load_texture(path) {
                    Ok(oiio_tex) => {
                        if oiio_tex.mip_levels.is_empty() {
                            return None;
                        }
                        let base = &oiio_tex.mip_levels[0];
                        return Some(RawTexture {
                            width: oiio_tex.width,
                            height: oiio_tex.height,
                            data: base.data.clone(),
                            is_linear: oiio_tex.is_linear || is_linear_src,
                            path: path.to_string(),
                        });
                    }
                    Err(e2) => {
                        log::warn!("Failed to load texture {}: {}", path, e2);
                        return None;
                    }
                }
            }
            log::warn!("Failed to load texture {}: {}", path, e);
            None
        }
    }
}

/// Load texture as raw u8 RGBA for direct GPU upload (non-OIIO fallback).
///
/// Falls back to TextureCache which loads via the `image` crate as f32,
/// then converts to u8 for upload. The f32 overhead is acceptable here
/// since the `image` crate path is already slower than OIIO.
/// Handles UDIM patterns by stitching tile atlases.
#[cfg(not(feature = "oiio"))]
fn load_raw_texture(path: &str, max_tile_size: u32) -> Option<RawTexture> {
    load_raw_texture_with_depth(path, 0, max_tile_size)
}

/// Inner loader (non-OIIO path).
/// UDIM patterns should be expanded before calling — this loads individual tiles only.
#[cfg(not(feature = "oiio"))]
fn load_raw_texture_with_depth(path: &str, _depth: u32, _max_tile_size: u32) -> Option<RawTexture> {
    if is_udim_path(path) {
        log::warn!(
            "UDIM pattern reached load_raw_texture — should be expanded: {}",
            path
        );
        return None;
    }

    use bif_core::texture::TextureCache;

    let mut cache = TextureCache::new();
    match cache.load(path) {
        Ok(tex) => {
            let is_linear = tex.is_linear || is_linear_texture_path(path);
            let data = texture_to_rgba8(tex.width, tex.height, &tex.pixels, is_linear);
            Some(RawTexture {
                width: tex.width,
                height: tex.height,
                data,
                is_linear,
                path: path.to_string(),
            })
        }
        Err(e) => {
            log::warn!("Failed to load texture {}: {}", path, e);
            None
        }
    }
}

/// Upload raw u8 RGBA texture to GPU. No f32 conversion.
///
/// Uses `Rgba8UnormSrgb` for sRGB textures (hardware decode),
/// `Rgba8Unorm` for linear textures.
///
/// If `mipmap_gen` is provided, generates GPU mipmaps for textures >= 64px.
fn upload_raw_texture(
    device: &Device,
    queue: &Queue,
    tex: &RawTexture,
    label: &str,
    max_dimension: u32,
    viewport_limit: u32,
    mipmap_gen: Option<&MipmapGenerator>,
) -> (wgpu::Texture, wgpu::TextureView) {
    // Apply viewport size limit (min of GPU max and configured limit)
    let effective_limit = max_dimension.min(viewport_limit);
    let needs_downscale = tex.width > effective_limit || tex.height > effective_limit;
    let downscaled = if needs_downscale {
        let (w, h, d) = downscale_raw_nearest(tex.width, tex.height, &tex.data, effective_limit);
        log::info!(
            "Viewport downscale {} from {}x{} to {}x{} (limit {})",
            tex.path,
            tex.width,
            tex.height,
            w,
            h,
            effective_limit
        );
        Some((w, h, d))
    } else {
        None
    };
    let (width, height) = downscaled
        .as_ref()
        .map(|(w, h, _)| (*w, *h))
        .unwrap_or((tex.width, tex.height));
    let data: &[u8] = match downscaled {
        Some((_, _, ref d)) => d,
        None => &tex.data,
    };

    // Determine mip count: use GPU mipmaps if generator available and texture is large enough
    let use_gpu_mips =
        mipmap_gen.is_some() && width >= GPU_MIPMAP_MIN_SIZE && height >= GPU_MIPMAP_MIN_SIZE;
    let mip_count = if use_gpu_mips {
        calculate_mip_count(width, height)
    } else {
        1
    };

    // STORAGE_BINDING requires a storage-compatible format. Rgba8UnormSrgb does NOT
    // support storage, so for sRGB textures that need GPU mipmaps we create the texture
    // as Rgba8Unorm and list Rgba8UnormSrgb as a view_format for the final sampling view.
    let is_srgb_with_mips = use_gpu_mips && !tex.is_linear;
    let format = if tex.is_linear || is_srgb_with_mips {
        // Rgba8Unorm: either linear data, or sRGB data that needs storage writes
        wgpu::TextureFormat::Rgba8Unorm
    } else {
        // sRGB without GPU mips — use native Srgb format
        wgpu::TextureFormat::Rgba8UnormSrgb
    };

    let usage = if use_gpu_mips {
        wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::STORAGE_BINDING
    } else {
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST
    };

    let gpu_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        // For sRGB textures with GPU mips: native format is Rgba8Unorm (for storage),
        // but we need Rgba8UnormSrgb views for correct sRGB sampling in render pass.
        view_formats: if is_srgb_with_mips {
            &[wgpu::TextureFormat::Rgba8UnormSrgb]
        } else {
            &[]
        },
    });

    // Upload base level (mip 0)
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &gpu_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    // Generate remaining mip levels on GPU
    if use_gpu_mips {
        if let Some(gen) = mipmap_gen {
            gen.generate(device, queue, &gpu_texture, width, height, mip_count);
        }
    }

    // Create view with correct format — sRGB textures with GPU mips need an explicit
    // Rgba8UnormSrgb view since the native texture format is Rgba8Unorm (for storage).
    let view = gpu_texture.create_view(&wgpu::TextureViewDescriptor {
        format: if is_srgb_with_mips {
            Some(wgpu::TextureFormat::Rgba8UnormSrgb)
        } else {
            None // default matches native format
        },
        ..Default::default()
    });

    (gpu_texture, view)
}

/// Create a GPU texture from a bif_core texture.
pub fn create_gpu_texture(
    device: &Device,
    queue: &Queue,
    texture: &bif_core::texture::Texture,
    is_linear: bool,
    label: &str,
    max_dimension: u32,
) -> wgpu::Texture {
    // Use texture's is_linear field if available, otherwise fall back to parameter
    let is_linear = texture.is_linear || is_linear;
    let format = if is_linear {
        wgpu::TextureFormat::Rgba8Unorm
    } else {
        wgpu::TextureFormat::Rgba8UnormSrgb
    };

    // Check if we need to downscale (affects mipmaps too)
    let needs_downscale = texture.width > max_dimension || texture.height > max_dimension;

    if needs_downscale {
        // Downscale path - no mipmaps (would need regeneration)
        let (width, height, pixels) = downscale_texture_nearest(texture, max_dimension);
        log::warn!(
            "Downscaled texture {} from {}x{} to {}x{} (limit {}). Mipmaps disabled.",
            texture.path,
            texture.width,
            texture.height,
            width,
            height,
            max_dimension
        );

        let gpu_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let rgba = texture_to_rgba8(width, height, &pixels, is_linear);
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &gpu_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        return gpu_texture;
    }

    // Normal path - upload with mipmaps if available
    let mip_count = texture.mip_count();

    let gpu_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    // Upload base level (mip 0)
    let rgba = texture_to_rgba8(texture.width, texture.height, &texture.pixels, is_linear);
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &gpu_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * texture.width),
            rows_per_image: Some(texture.height),
        },
        wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
    );

    // Upload additional mip levels if present
    for (mip_index, mip_level) in texture.mip_levels.iter().enumerate() {
        let mip_rgba = texture_to_rgba8(
            mip_level.width,
            mip_level.height,
            &mip_level.pixels,
            is_linear,
        );
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &gpu_texture,
                mip_level: (mip_index + 1) as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &mip_rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * mip_level.width),
                rows_per_image: Some(mip_level.height),
            },
            wgpu::Extent3d {
                width: mip_level.width,
                height: mip_level.height,
                depth_or_array_layers: 1,
            },
        );
    }

    if mip_count > 1 {
        log::debug!("Uploaded texture {} with {} mip levels", label, mip_count);
    }

    gpu_texture
}

/// Create default white GPU texture set.
pub fn create_default_gpu_textures(device: &Device, queue: &Queue) -> GpuTextureSet {
    let default_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Default White Texture"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &default_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255u8, 255u8, 255u8, 255u8],
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );

    let textures = vec![default_texture];
    let mut views = Vec::with_capacity(MAX_VIEWPORT_TEXTURES);
    for _ in 0..MAX_VIEWPORT_TEXTURES {
        views.push(textures[0].create_view(&wgpu::TextureViewDescriptor::default()));
    }

    GpuTextureSet {
        textures,
        views,
        index_map: HashMap::new(),
        udim_map: HashMap::new(),
        texture_budget: TextureBudget::default(),
    }
}

/// Check if a texture path contains a UDIM token (`<UDIM>`).
pub fn is_udim_path(path: &str) -> bool {
    bif_core::texture::is_udim_path(path)
}

// normalize_path and find_udim_tiles removed — using bif_core::texture equivalents

// ── UDIM Atlas Disk Cache (u8 viewport path) ──────────────────────

/// Resolve a texture path against a per-material `source_dir`, falling back
/// to `fallback_base` if the material has no `source_dir`.
///
/// Returns the resolved path as a `String` suitable for `TextureCache::load`.
fn resolve_texture_path(
    tex_path: &str,
    material_source_dir: Option<&std::path::Path>,
    fallback_base: Option<&std::path::Path>,
) -> String {
    let p = Path::new(tex_path);
    // UNC paths with forward slashes aren't recognized as absolute on Windows
    if p.is_absolute() || tex_path.starts_with("//") {
        return tex_path.to_string();
    }
    // Resolve against material-level dir first, then fallback.
    // No filesystem check — TextureCache handles missing files gracefully.
    if let Some(dir) = material_source_dir.or(fallback_base) {
        return dir.join(p).to_string_lossy().into_owned();
    }
    tex_path.to_string()
}

/// Collect unique *resolved* texture paths from scene materials.
///
/// Each texture path is resolved against its material's `source_dir` (if set),
/// falling back to `fallback_base`. This ensures multi-USD scenes with
/// different base directories produce correct paths.
pub fn collect_scene_texture_paths(
    scene: &bif_core::Scene,
    fallback_base: Option<&std::path::Path>,
) -> Vec<String> {
    let mut unique_paths = HashSet::new();
    let mut paths = Vec::new();

    for material in &scene.materials {
        let mat = material.as_ref();
        let src_dir = mat.source_dir.as_deref();
        let candidate_paths = [
            mat.base_color_texture.as_deref(),
            mat.specular_roughness_texture.as_deref(),
            mat.base_metalness_texture.as_deref(),
            mat.normal_texture.as_deref(),
            mat.emission_texture.as_deref(),
        ];

        for raw_path in candidate_paths.into_iter().flatten() {
            let resolved = resolve_texture_path(raw_path, src_dir, fallback_base);
            if unique_paths.insert(resolved.clone()) {
                paths.push(resolved);
            }
        }
    }

    if !paths.is_empty() {
        log::info!("Collected {} texture paths from materials", paths.len());
    }
    paths
}

/// Create GPU texture set with placeholder (white) textures for all paths.
///
/// Pre-allocates texture indices so materials can reference them immediately.
/// Returns `(GpuTextureSet, expanded_tile_paths)` — pass the tile paths to
/// `start_texture_loading_async` to avoid redundant UDIM tile discovery.
pub fn prepare_texture_placeholders(
    device: &Device,
    queue: &Queue,
    scene: &bif_core::Scene,
    base_dir: Option<&Path>,
) -> (GpuTextureSet, Vec<String>) {
    use bif_core::texture::{find_udim_tiles, UdimGridLayout};

    let mut texture_set = create_default_gpu_textures(device, queue);

    let texture_paths = collect_scene_texture_paths(scene, base_dir);

    let make_placeholder = |device: &Device| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    };

    let mut total_slots = 0usize;
    let max_slots = MAX_VIEWPORT_TEXTURES - 1; // slot 0 = default white
    let mut expanded_paths: Vec<String> = Vec::new();

    for path in &texture_paths {
        if is_udim_path(path) {
            let tiles = find_udim_tiles(path);
            if let Some(layout) = UdimGridLayout::from_tiles(tiles) {
                let slots = layout.grid_slots() as usize;

                // I1: Check capacity BEFORE allocating — skip sets that won't fit
                if total_slots + slots > max_slots {
                    log::warn!(
                        "UDIM '{}' needs {} slots, only {} remain — skipping",
                        path,
                        slots,
                        max_slots - total_slots
                    );
                    continue;
                }

                let base_index = texture_set.textures.len() as u32;

                texture_set.index_map.insert(path.clone(), base_index);
                texture_set.udim_map.insert(
                    path.clone(),
                    crate::gpu_types::UdimGpuMapping {
                        base_index,
                        num_cols: layout.num_cols,
                        num_rows: layout.num_rows,
                        min_col: layout.min_col,
                        min_row: layout.min_row,
                    },
                );

                for info in &layout.tiles {
                    let col = info.col - layout.min_col;
                    let row = info.row - layout.min_row;
                    let idx = row * layout.num_cols + col;
                    texture_set
                        .index_map
                        .insert(info.path.clone(), base_index + idx);
                    expanded_paths.push(info.path.clone());
                }

                for _ in 0..slots {
                    texture_set.textures.push(make_placeholder(device));
                }
                total_slots += slots;
            }
        } else {
            if total_slots >= max_slots {
                log::warn!(
                    "Texture slots at GPU limit ({}), skipping remaining",
                    max_slots
                );
                break;
            }
            let index = texture_set.textures.len() as u32;
            texture_set.index_map.insert(path.clone(), index);
            texture_set.textures.push(make_placeholder(device));
            expanded_paths.push(path.clone());
            total_slots += 1;
        }
    }

    // C1: Sync views to cover all allocated texture slots.
    // views was pre-allocated to MAX_VIEWPORT_TEXTURES in create_default_gpu_textures,
    // all pointing to the default white texture. textures.len() should never exceed
    // MAX_VIEWPORT_TEXTURES due to the capacity checks above, but guard defensively.
    if texture_set.textures.len() > texture_set.views.len() {
        log::error!(
            "BUG: textures ({}) > views ({}), padding views to prevent panic",
            texture_set.textures.len(),
            texture_set.views.len()
        );
        while texture_set.views.len() < texture_set.textures.len() {
            texture_set
                .views
                .push(texture_set.textures[0].create_view(&wgpu::TextureViewDescriptor::default()));
        }
    }

    log::info!(
        "Prepared {} texture slots for async loading ({} UDIM groups)",
        total_slots,
        texture_set.udim_map.len()
    );

    (texture_set, expanded_paths)
}

/// Start loading textures on a background thread.
///
/// `tile_paths` should come from `prepare_texture_placeholders` (already expanded,
/// avoids redundant UDIM tile discovery).
///
/// Returns a receiver that delivers `TextureLoadMessage` for each loaded texture.
/// Call `poll_texture_loads()` each frame to upload completed textures to GPU.
pub fn start_texture_loading_async(tile_paths: Vec<String>) -> mpsc::Receiver<TextureLoadMessage> {
    // Bounded channel: background thread blocks when 32 decoded textures are buffered,
    // preventing unbounded RAM growth on large scenes (500+ textures).
    let (tx, rx) = mpsc::sync_channel(32);

    let total_textures = tile_paths.len();
    let max_slots = MAX_VIEWPORT_TEXTURES - 1;
    if total_textures > max_slots {
        log::warn!(
            "Scene has {} texture tiles, viewport limit is {} — {} will be missing",
            total_textures,
            max_slots,
            total_textures - max_slots
        );
    }
    // Adaptive texture size: auto-downsample for large scenes to save RAM
    let adaptive_tex_size = if total_textures > 200 {
        log::info!(
            "Large scene ({} tiles) — auto-downscaling viewport textures to 512px",
            total_textures
        );
        512u32
    } else if total_textures > 50 {
        1024u32
    } else {
        DEFAULT_MAX_VIEWPORT_TEXTURE_SIZE
    };
    let paths_to_load: Vec<_> = tile_paths.into_iter().take(max_slots).collect();
    let load_count = paths_to_load.len();

    std::thread::spawn(move || {
        use rayon::prelude::*;

        // Load textures in chunks to limit peak RAM (16 textures at a time).
        const CHUNK_SIZE: usize = 16;
        let mut loaded = 0usize;
        for chunk in paths_to_load.chunks(CHUNK_SIZE) {
            let results: Vec<_> = chunk
                .par_iter()
                .filter_map(|path| {
                    load_raw_texture(path, adaptive_tex_size).map(|tex| (path.clone(), tex))
                })
                .collect();

            for (path, mut raw_tex) in results {
                if raw_tex.width > adaptive_tex_size || raw_tex.height > adaptive_tex_size {
                    let (w, h, d) = downscale_raw_nearest(
                        raw_tex.width,
                        raw_tex.height,
                        &raw_tex.data,
                        adaptive_tex_size,
                    );
                    raw_tex.width = w;
                    raw_tex.height = h;
                    raw_tex.data = d;
                }

                loaded += 1;
                if loaded.is_multiple_of(64) || loaded == load_count {
                    log::info!("Texture streaming: {}/{} loaded", loaded, load_count);
                }

                if tx
                    .send(TextureLoadMessage {
                        path,
                        width: raw_tex.width,
                        height: raw_tex.height,
                        data: raw_tex.data,
                        is_linear: raw_tex.is_linear,
                    })
                    .is_err()
                {
                    return; // Receiver dropped — scene was unloaded
                }
            }
        }
    });

    rx
}

/// Upload a streamed texture to the GPU, replacing its placeholder.
///
/// If `mipmap_gen` is provided, generates GPU mipmaps for large textures.
/// Returns true if the texture was uploaded (index found in map).
pub fn upload_streamed_texture(
    device: &Device,
    queue: &Queue,
    texture_set: &mut GpuTextureSet,
    msg: TextureLoadMessage,
    max_dimension: u32,
    mipmap_gen: Option<&MipmapGenerator>,
) -> bool {
    if !gpu_is_healthy() {
        return false;
    }

    let Some(&index) = texture_set.index_map.get(&msg.path) else {
        log::warn!("Streamed texture {} has no pre-allocated index", msg.path);
        return false;
    };

    // C1: Bounds guard — prevent panic if index exceeds views/textures capacity
    let idx = index as usize;
    if idx >= texture_set.views.len() || idx >= texture_set.textures.len() {
        log::warn!(
            "Streamed texture {} index {} out of bounds (views={}, textures={})",
            msg.path,
            index,
            texture_set.views.len(),
            texture_set.textures.len()
        );
        return false;
    }

    // Predict the effective on-GPU size (after `upload_raw_texture`'s
    // viewport-size clamp) and reserve VRAM before doing any GPU work.
    // Once the budget is exhausted, leave the placeholder in place.
    let effective_limit = max_dimension.min(DEFAULT_MAX_VIEWPORT_TEXTURE_SIZE);
    let eff_width = msg.width.min(effective_limit);
    let eff_height = msg.height.min(effective_limit);
    let predicted_mip_count =
        if eff_width >= GPU_MIPMAP_MIN_SIZE && eff_height >= GPU_MIPMAP_MIN_SIZE {
            calculate_mip_count(eff_width, eff_height)
        } else {
            1
        };
    if !texture_set
        .texture_budget
        .try_charge(eff_width, eff_height, predicted_mip_count)
    {
        return false;
    }

    let label = format!("Viewport Texture: {}", msg.path);
    let raw = RawTexture {
        width: msg.width,
        height: msg.height,
        data: msg.data,
        is_linear: msg.is_linear,
        path: msg.path,
    };
    let (gpu_texture, view) = upload_raw_texture(
        device,
        queue,
        &raw,
        &label,
        max_dimension,
        DEFAULT_MAX_VIEWPORT_TEXTURE_SIZE,
        mipmap_gen,
    );

    // Replace placeholder
    texture_set.textures[index as usize] = gpu_texture;
    texture_set.views[index as usize] = view;

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_linear_texture_path() {
        assert!(is_linear_texture_path("foo.exr"));
        assert!(is_linear_texture_path("bar.EXR"));
        assert!(is_linear_texture_path("hdr.hdr"));
        // .tx preserves source colorspace — not assumed linear
        assert!(!is_linear_texture_path("diffuse.tx"));
        assert!(!is_linear_texture_path("normal.TX"));
        assert!(!is_linear_texture_path("diffuse.png"));
        assert!(!is_linear_texture_path("normal.jpg"));
        assert!(!is_linear_texture_path("noext"));
    }

    #[test]
    fn test_linear_to_srgb_byte() {
        assert_eq!(linear_to_srgb_byte(0.0), 0);
        assert_eq!(linear_to_srgb_byte(1.0), 255);
        assert_eq!(linear_to_srgb_byte(f32::NAN), 0);
        assert_eq!(linear_to_srgb_byte(f32::INFINITY), 0);
        // Mid-gray ~0.18 linear -> ~0.46 sRGB -> ~117 byte
        let mid = linear_to_srgb_byte(0.18);
        assert!(mid > 100 && mid < 140);
    }

    #[test]
    fn test_linear_to_byte() {
        assert_eq!(linear_to_byte(0.0), 0);
        assert_eq!(linear_to_byte(1.0), 255);
        assert_eq!(linear_to_byte(0.5), 128);
        assert_eq!(linear_to_byte(-1.0), 0); // clamped
        assert_eq!(linear_to_byte(2.0), 255); // clamped
    }

    #[test]
    fn test_texture_to_rgba8() {
        let pixels = vec![[0.0, 0.5, 1.0, 1.0]];
        let bytes = texture_to_rgba8(1, 1, &pixels, true); // linear
        assert_eq!(bytes, vec![0, 128, 255, 255]);
    }

    #[test]
    fn test_downscale_raw_nearest_no_op() {
        let data = vec![255u8; 4 * 4 * 4]; // 4x4 white
        let (w, h, out) = downscale_raw_nearest(4, 4, &data, 8);
        assert_eq!(w, 4);
        assert_eq!(h, 4);
        assert_eq!(out.len(), data.len());
    }

    #[test]
    fn test_downscale_raw_nearest_halves() {
        // 4x4 checkerboard: R=255 at (0,0), G=255 at (1,0), etc.
        let mut data = vec![0u8; 4 * 4 * 4];
        // Set all alpha to 255
        for i in 0..16 {
            data[i * 4 + 3] = 255;
        }
        // Pixel (0,0) = red
        data[0] = 255;
        let (w, h, _) = downscale_raw_nearest(4, 4, &data, 2);
        assert_eq!(w, 2);
        assert_eq!(h, 2);
    }

    #[test]
    fn test_calculate_mip_count() {
        assert_eq!(calculate_mip_count(1, 1), 1);
        assert_eq!(calculate_mip_count(2, 2), 2);
        assert_eq!(calculate_mip_count(4, 4), 3);
        assert_eq!(calculate_mip_count(1024, 1024), 11);
        assert_eq!(calculate_mip_count(512, 256), 10);
    }

    #[test]
    fn test_default_viewport_texture_size() {
        assert_eq!(DEFAULT_MAX_VIEWPORT_TEXTURE_SIZE, 2048);
    }

    #[test]
    fn test_estimate_texture_bytes_base() {
        assert_eq!(estimate_texture_bytes(1024, 1024, 1), 1024 * 1024 * 4);
    }

    #[test]
    fn test_estimate_texture_bytes_with_mips() {
        // 4/3 of base level
        let base = 1024u64 * 1024 * 4;
        assert_eq!(estimate_texture_bytes(1024, 1024, 11), base * 4 / 3);
    }

    #[test]
    fn test_budget_happy_path() {
        let mut b = TextureBudget::new(64 * 1024 * 1024);
        assert!(b.try_charge(1024, 1024, 1));
        assert!(b.try_charge(1024, 1024, 1));
        assert!(b.try_charge(1024, 1024, 1));
        assert_eq!(b.used_bytes(), 3 * 1024 * 1024 * 4);
    }

    #[test]
    fn test_budget_rejects_over_cap() {
        let mut b = TextureBudget::new(1024 * 1024 * 4 + 1); // 4 MiB + 1
        assert!(b.try_charge(1024, 1024, 1)); // exactly 4 MiB consumed
        assert!(!b.try_charge(1024, 1024, 1)); // would push over cap
        assert_eq!(b.used_bytes(), 1024 * 1024 * 4);
    }

    #[test]
    fn test_budget_warns_only_once() {
        let mut b = TextureBudget::new(1);
        assert!(!b.try_charge(1, 1, 1));
        assert!(b.warned);
        // Second rejection doesn't re-arm the warning flag — still true, no panic.
        assert!(!b.try_charge(1, 1, 1));
        assert!(b.warned);
    }

    #[test]
    fn test_default_vram_budget_is_one_and_a_half_gib() {
        assert_eq!(DEFAULT_VRAM_BUDGET_BYTES, 1_500 * 1024 * 1024);
    }
}
