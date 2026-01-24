//! Texture loading utilities for viewport rendering.
//!
//! Provides functions for loading, converting, and uploading textures to GPU.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use wgpu::{Device, Queue};

use bif_core::texture::TextureCache;

use crate::gpu_types::{GpuTextureSet, MAX_VIEWPORT_TEXTURES};

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
    }
}

/// Collect unique texture paths from scene materials.
pub fn collect_scene_texture_paths(scene: &bif_core::Scene) -> Vec<String> {
    let mut unique_paths = HashSet::new();
    let mut paths = Vec::new();

    for material in &scene.materials {
        let material = material.as_ref();
        let candidate_paths = [
            material.diffuse_texture.as_deref(),
            material.roughness_texture.as_deref(),
            material.metallic_texture.as_deref(),
            material.normal_texture.as_deref(),
            material.emissive_texture.as_deref(),
        ];

        for path in candidate_paths.into_iter().flatten() {
            if unique_paths.insert(path.to_string()) {
                paths.push(path.to_string());
            }
        }
    }

    if !paths.is_empty() {
        log::info!("Collected {} texture paths from materials", paths.len());
    }
    paths
}

/// Create GPU textures for all materials in a scene.
pub fn create_gpu_textures_for_scene(
    device: &Device,
    queue: &Queue,
    scene: &bif_core::Scene,
    base_dir: Option<&Path>,
) -> GpuTextureSet {
    #[cfg(not(feature = "oiio"))]
    use rayon::prelude::*;

    let mut texture_set = create_default_gpu_textures(device, queue);
    let max_dimension = device.limits().max_texture_dimension_2d;

    let texture_paths = collect_scene_texture_paths(scene);
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

    // Load textures. OIIO is not thread-safe for concurrent make_tx/load,
    // so we serialize when using OIIO. Without OIIO, par_iter is safe.
    let load_start = std::time::Instant::now();
    let base_dir_owned = base_dir.map(|p| p.to_path_buf());

    let load_one = |path: &String| {
        let mut cache = if let Some(ref base) = base_dir_owned {
            TextureCache::with_base_dir(base)
        } else {
            TextureCache::new()
        };
        // Disable .tx conversion - OIIO's make_texture crashes on Windows.
        // Mipmaps are generated in-memory from the source file instead.
        #[cfg(feature = "oiio")]
        {
            cache.auto_convert_tx = false;
        }
        match cache.load(path) {
            Ok(tex) => Some((path.clone(), tex)),
            Err(e) => {
                log::warn!("Failed to load texture {}: {}", path, e);
                None
            }
        }
    };

    #[cfg(feature = "oiio")]
    let loaded_textures: Vec<_> = paths_to_load.iter().map(load_one).collect();

    #[cfg(not(feature = "oiio"))]
    let loaded_textures: Vec<_> = paths_to_load.par_iter().map(load_one).collect();

    let load_time = load_start.elapsed();
    log::info!(
        "Loaded {} textures: {:.1}ms",
        loaded_textures.iter().filter(|t| t.is_some()).count(),
        load_time.as_secs_f32() * 1000.0
    );

    // Upload to GPU (must be sequential - wgpu API requirement)
    let upload_start = std::time::Instant::now();
    for item in loaded_textures.into_iter().flatten() {
        let (path, texture) = item;
        let is_linear = is_linear_texture_path(&path);
        let label = format!("Viewport Texture: {}", path);
        let gpu_texture =
            create_gpu_texture(device, queue, &texture, is_linear, &label, max_dimension);
        let view = gpu_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let index = texture_set.textures.len() as u32;

        texture_set.textures.push(gpu_texture);
        texture_set.views[index as usize] = view;
        texture_set.index_map.insert(path, index);
    }
    let upload_time = upload_start.elapsed();
    log::info!(
        "Uploaded {} textures to GPU: {:.1}ms",
        texture_set.textures.len() - 1, // -1 for default texture
        upload_time.as_secs_f32() * 1000.0
    );

    texture_set
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
}
