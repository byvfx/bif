//! Texture loading utilities for viewport rendering.
//!
//! Provides functions for loading, converting, and uploading textures to GPU.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc;

use wgpu::{Device, Queue};

use crate::gpu_types::{GpuTextureSet, MAX_VIEWPORT_TEXTURES};

/// Message sent from background texture loading thread.
pub struct TextureLoadMessage {
    /// Resolved texture path (matches key in GpuTextureSet.index_map)
    pub path: String,
    /// Raw u8 RGBA pixel data
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub is_linear: bool,
    /// UDIM atlas grid dimensions (0 = not a UDIM texture)
    pub udim_grid_cols: u32,
    pub udim_grid_rows: u32,
    pub udim_min_col: u32,
    pub udim_min_row: u32,
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
    /// UDIM atlas grid dimensions (0 = not a UDIM texture)
    udim_grid_cols: u32,
    udim_grid_rows: u32,
    udim_min_col: u32,
    udim_min_row: u32,
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

        queue.submit(std::iter::once(encoder.finish()));
    }
}

/// Calculate the number of mip levels for given dimensions.
fn calculate_mip_count(width: u32, height: u32) -> u32 {
    let max_dim = width.max(height);
    (max_dim as f32).log2().floor() as u32 + 1
}

/// Check if texture path indicates linear color space (EXR, HDR).
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
fn load_raw_texture(path: &str) -> Option<RawTexture> {
    load_raw_texture_with_depth(path, 0)
}

/// Inner loader with UDIM recursion depth tracking (OIIO path).
#[cfg(feature = "oiio")]
fn load_raw_texture_with_depth(path: &str, udim_depth: u32) -> Option<RawTexture> {
    // Handle UDIM textures
    if is_udim_path(path) {
        return load_udim_atlas_inner(path, udim_depth);
    }

    let is_linear = is_linear_texture_path(path);

    // Use OIIO to load as u8 directly (no mips for viewport)
    match bif_core::oiio::load_texture(path) {
        Ok(oiio_tex) => {
            if oiio_tex.mip_levels.is_empty() {
                log::warn!("OIIO returned no mip levels for {}", path);
                return None;
            }
            let base = &oiio_tex.mip_levels[0];
            Some(RawTexture {
                width: oiio_tex.width,
                height: oiio_tex.height,
                data: base.data.clone(),
                is_linear: oiio_tex.is_linear || is_linear,
                path: path.to_string(),
                udim_grid_cols: 0,
                udim_grid_rows: 0,
                udim_min_col: 0,
                udim_min_row: 0,
            })
        }
        Err(e) => {
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
fn load_raw_texture(path: &str) -> Option<RawTexture> {
    load_raw_texture_with_depth(path, 0)
}

/// Inner loader with UDIM recursion depth tracking (non-OIIO path).
#[cfg(not(feature = "oiio"))]
fn load_raw_texture_with_depth(path: &str, udim_depth: u32) -> Option<RawTexture> {
    // Handle UDIM textures
    if is_udim_path(path) {
        return load_udim_atlas_inner(path, udim_depth);
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
                udim_grid_cols: 0,
                udim_grid_rows: 0,
                udim_min_col: 0,
                udim_min_row: 0,
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
        udim_grid: HashMap::new(),
    }
}

/// Check if a texture path contains a UDIM token (`<UDIM>`).
pub fn is_udim_path(path: &str) -> bool {
    bif_core::texture::is_udim_path(path)
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

/// Scan filesystem for existing UDIM tiles matching the pattern.
/// Returns a vec of (udim_id, path) sorted by UDIM ID.
fn find_udim_tiles(pattern: &str) -> Vec<(u32, String)> {
    let mut tiles = Vec::new();
    // UDIM range: 1001..=1100 (10 columns x 10 rows)
    for udim in 1001..=1100 {
        let tile_path = pattern.replace("<UDIM>", &udim.to_string());
        let normalized = normalize_path(&tile_path);
        if Path::new(&normalized).exists() {
            tiles.push((udim, normalized));
        }
    }
    tiles.sort_by_key(|(id, _)| *id);
    log::debug!(
        "UDIM tile scan (viewport): pattern={}, found {} tiles",
        pattern,
        tiles.len()
    );
    tiles
}

/// Maximum recursion depth for UDIM atlas loading.
/// Prevents infinite recursion if UDIM tile paths themselves contain `<UDIM>`.
const MAX_UDIM_RECURSION_DEPTH: u32 = 2;

/// Resolve UDIM texture: load all tiles and stitch into a single atlas.
/// Returns the stitched atlas as raw RGBA bytes + grid dimensions.
/// Includes recursion depth guard to prevent infinite recursion.
fn load_udim_atlas_inner(pattern: &str, depth: u32) -> Option<RawTexture> {
    if depth >= MAX_UDIM_RECURSION_DEPTH {
        log::error!(
            "UDIM atlas recursion depth exceeded (max {}) for: {}",
            MAX_UDIM_RECURSION_DEPTH,
            pattern
        );
        return None;
    }
    let tiles = find_udim_tiles(pattern);
    if tiles.is_empty() {
        log::warn!("No UDIM tiles found for pattern: {}", pattern);
        return None;
    }

    // Load all tiles
    let mut loaded_tiles: Vec<(u32, RawTexture)> = Vec::new();
    for (udim, tile_path) in &tiles {
        if let Some(tex) = load_raw_texture_with_depth(tile_path, depth + 1) {
            loaded_tiles.push((*udim, tex));
        } else {
            log::warn!("Failed to load UDIM tile {}: {}", udim, tile_path);
        }
    }

    if loaded_tiles.is_empty() {
        return None;
    }

    // Determine grid bounds from UDIM IDs
    // UDIM = 1000 + col + row*10, col in 1..=10, row in 0..=9
    let mut min_col = u32::MAX;
    let mut max_col = 0u32;
    let mut min_row = u32::MAX;
    let mut max_row = 0u32;
    for (udim, _) in &loaded_tiles {
        let col = (udim - 1001) % 10;
        let row = (udim - 1001) / 10;
        min_col = min_col.min(col);
        max_col = max_col.max(col);
        min_row = min_row.min(row);
        max_row = max_row.max(row);
    }
    let num_cols = max_col - min_col + 1;
    let num_rows = max_row - min_row + 1;

    // Find max tile dimensions (handle mixed resolutions)
    let max_tile_w = loaded_tiles.iter().map(|(_, t)| t.width).max().unwrap_or(1);
    let max_tile_h = loaded_tiles
        .iter()
        .map(|(_, t)| t.height)
        .max()
        .unwrap_or(1);

    let atlas_w = num_cols * max_tile_w;
    let atlas_h = num_rows * max_tile_h;
    let mut atlas_data = vec![0u8; (atlas_w * atlas_h * 4) as usize];

    // Stitch tiles into atlas
    for (udim, tile) in &loaded_tiles {
        let col = (udim - 1001) % 10 - min_col;
        let row = (udim - 1001) / 10 - min_row;
        // USD UDIM: row 0 is bottom, but in atlas pixel space row 0 is top.
        // Flip row so UDIM row 0 maps to the bottom of the atlas.
        let flipped_row = (num_rows - 1) - row;

        // Resize tile if smaller than max tile size
        let (tw, th, tdata) = if tile.width != max_tile_w || tile.height != max_tile_h {
            downscale_raw_nearest(
                tile.width,
                tile.height,
                &tile.data,
                max_tile_w.max(max_tile_h),
            )
        } else {
            (tile.width, tile.height, tile.data.clone())
        };

        let dest_x = col * max_tile_w;
        let dest_y = flipped_row * max_tile_h;
        for y in 0..th.min(max_tile_h) {
            let src_off = (y * tw * 4) as usize;
            let dst_off = ((dest_y + y) * atlas_w + dest_x) as usize * 4;
            let copy_bytes = (tw.min(max_tile_w) * 4) as usize;
            if src_off + copy_bytes <= tdata.len() && dst_off + copy_bytes <= atlas_data.len() {
                atlas_data[dst_off..dst_off + copy_bytes]
                    .copy_from_slice(&tdata[src_off..src_off + copy_bytes]);
            }
        }
    }

    let is_linear = loaded_tiles
        .first()
        .map(|(_, t)| t.is_linear)
        .unwrap_or(false);

    log::info!(
        "UDIM atlas: {} tiles -> {}x{} ({}x{} grid, tile {}x{})",
        loaded_tiles.len(),
        atlas_w,
        atlas_h,
        num_cols,
        num_rows,
        max_tile_w,
        max_tile_h
    );

    Some(RawTexture {
        width: atlas_w,
        height: atlas_h,
        data: atlas_data,
        is_linear,
        path: pattern.to_string(),
        udim_grid_cols: num_cols,
        udim_grid_rows: num_rows,
        udim_min_col: min_col,
        udim_min_row: min_row,
    })
}

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

/// Create GPU textures for all materials in a scene.
///
/// Uses raw u8 loading to skip the f32 intermediate conversion.
/// Textures are loaded in parallel and uploaded directly as u8 to the GPU.
///
/// `base_dir` is a legacy fallback for materials that don't have `source_dir`
/// set (e.g. single-file loads). For multi-USD workflows each material carries
/// its own `source_dir` which takes precedence.
pub fn create_gpu_textures_for_scene(
    device: &Device,
    queue: &Queue,
    scene: &bif_core::Scene,
    base_dir: Option<&Path>,
) -> GpuTextureSet {
    use rayon::prelude::*;

    let mut texture_set = create_default_gpu_textures(device, queue);
    let max_dimension = device.limits().max_texture_dimension_2d;

    let texture_paths = collect_scene_texture_paths(scene, base_dir);
    let paths_to_load: Vec<_> = texture_paths
        .into_iter()
        .take(MAX_VIEWPORT_TEXTURES - 1) // Leave slot 0 for default
        .collect();

    if paths_to_load.len() >= MAX_VIEWPORT_TEXTURES - 1 {
        log::warn!(
            "Texture count exceeds GPU limit {}. Extra textures will be skipped.",
            MAX_VIEWPORT_TEXTURES - 1
        );
    }

    // Load textures in parallel as raw u8 (no f32 conversion)
    let load_start = std::time::Instant::now();
    let loaded_textures: Vec<_> = paths_to_load
        .par_iter()
        .map(|path| load_raw_texture(path).map(|tex| (path.clone(), tex)))
        .collect();
    let load_time = load_start.elapsed();
    log::info!(
        "Loaded {} textures (raw u8): {:.1}ms",
        loaded_textures.iter().filter(|t| t.is_some()).count(),
        load_time.as_secs_f32() * 1000.0
    );

    // Upload to GPU (must be sequential — wgpu API requirement)
    let upload_start = std::time::Instant::now();
    for item in loaded_textures.into_iter().flatten() {
        let (path, raw_tex) = item;
        let label = format!("Viewport Texture: {}", path);
        let (gpu_texture, view) = upload_raw_texture(
            device,
            queue,
            &raw_tex,
            &label,
            max_dimension,
            DEFAULT_MAX_VIEWPORT_TEXTURE_SIZE,
            None, // No GPU mipmaps in sync path
        );
        let index = texture_set.textures.len() as u32;

        texture_set.textures.push(gpu_texture);
        texture_set.views[index as usize] = view;

        // Store UDIM grid info if this is a UDIM atlas
        if raw_tex.udim_grid_cols > 0 {
            texture_set.udim_grid.insert(
                index,
                [
                    raw_tex.udim_grid_cols,
                    raw_tex.udim_grid_rows,
                    raw_tex.udim_min_col,
                    raw_tex.udim_min_row,
                ],
            );
        }

        texture_set.index_map.insert(path, index);
    }
    let upload_time = upload_start.elapsed();
    log::info!(
        "Uploaded {} textures to GPU: {:.1}ms",
        texture_set.textures.len() - 1,
        upload_time.as_secs_f32() * 1000.0
    );

    texture_set
}

/// Create GPU texture set with placeholder (white) textures for all paths.
///
/// Pre-allocates texture indices so materials can reference them immediately.
/// Actual textures stream in asynchronously via `start_texture_loading_async`.
pub fn prepare_texture_placeholders(
    device: &Device,
    queue: &Queue,
    scene: &bif_core::Scene,
    base_dir: Option<&Path>,
) -> GpuTextureSet {
    let mut texture_set = create_default_gpu_textures(device, queue);

    let texture_paths = collect_scene_texture_paths(scene, base_dir);
    let paths_to_load: Vec<_> = texture_paths
        .into_iter()
        .take(MAX_VIEWPORT_TEXTURES - 1)
        .collect();

    // Pre-allocate indices — views point to default white texture for now
    for path in &paths_to_load {
        let index = texture_set.textures.len() as u32;
        // No new GPU texture yet — view stays as default white (from create_default_gpu_textures)
        texture_set.index_map.insert(path.clone(), index);
        // Push a dummy reference to default texture to keep indices contiguous
        texture_set
            .textures
            .push(device.create_texture(&wgpu::TextureDescriptor {
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
            }));
    }

    log::info!(
        "Prepared {} placeholder textures for async loading",
        paths_to_load.len()
    );

    texture_set
}

/// Start loading textures on a background thread.
///
/// Returns a receiver that delivers `TextureLoadMessage` for each loaded texture.
/// Call `poll_texture_loads()` each frame to upload completed textures to GPU.
pub fn start_texture_loading_async(
    scene: &bif_core::Scene,
    base_dir: Option<&Path>,
) -> mpsc::Receiver<TextureLoadMessage> {
    // Bounded channel: background thread blocks when 32 decoded textures are buffered,
    // preventing unbounded RAM growth on large scenes (500+ textures).
    let (tx, rx) = mpsc::sync_channel(32);

    let texture_paths = collect_scene_texture_paths(scene, base_dir);
    let total_textures = texture_paths.len();
    let max_slots = MAX_VIEWPORT_TEXTURES - 1;
    if total_textures > max_slots {
        log::warn!(
            "Scene has {} textures, viewport limit is {} — {} textures will be missing",
            total_textures,
            max_slots,
            total_textures - max_slots
        );
    }
    let paths_to_load: Vec<_> = texture_paths.into_iter().take(max_slots).collect();

    std::thread::spawn(move || {
        use rayon::prelude::*;

        // Load textures in chunks to limit peak RAM (16 textures at a time).
        // Without chunking, rayon decodes all textures simultaneously → OOM on 500+ textures.
        const CHUNK_SIZE: usize = 16;
        for chunk in paths_to_load.chunks(CHUNK_SIZE) {
            let results: Vec<_> = chunk
                .par_iter()
                .filter_map(|path| load_raw_texture(path).map(|tex| (path.clone(), tex)))
                .collect();

            for (path, raw_tex) in results {
                if tx
                    .send(TextureLoadMessage {
                        path,
                        width: raw_tex.width,
                        height: raw_tex.height,
                        data: raw_tex.data,
                        is_linear: raw_tex.is_linear,
                        udim_grid_cols: raw_tex.udim_grid_cols,
                        udim_grid_rows: raw_tex.udim_grid_rows,
                        udim_min_col: raw_tex.udim_min_col,
                        udim_min_row: raw_tex.udim_min_row,
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
    let Some(&index) = texture_set.index_map.get(&msg.path) else {
        log::warn!("Streamed texture {} has no pre-allocated index", msg.path);
        return false;
    };

    let label = format!("Viewport Texture: {}", msg.path);
    let raw = RawTexture {
        width: msg.width,
        height: msg.height,
        data: msg.data,
        is_linear: msg.is_linear,
        path: msg.path,
        udim_grid_cols: msg.udim_grid_cols,
        udim_grid_rows: msg.udim_grid_rows,
        udim_min_col: msg.udim_min_col,
        udim_min_row: msg.udim_min_row,
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

    // Store UDIM grid info if this is a UDIM atlas
    if raw.udim_grid_cols > 0 {
        texture_set.udim_grid.insert(
            index,
            [
                raw.udim_grid_cols,
                raw.udim_grid_rows,
                raw.udim_min_col,
                raw.udim_min_row,
            ],
        );
    }

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
}
