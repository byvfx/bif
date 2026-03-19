//! Environment management for IBL and skybox rendering.
//!
//! Extracted from Renderer to reduce monolithic struct size.

use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;

use wgpu::Queue;

use crate::compute_ibl::{self, ComputeIbl};
use crate::environment::GpuEnvironment;
use crate::skybox;

/// Result from background IBL generation thread.
pub enum IblResult {
    Success {
        /// HDR pixels for GPU compute IBL (viewport)
        hdr_pixels: Vec<[f32; 3]>,
        hdr_width: u32,
        hdr_height: u32,
        /// Ivar CPU path tracer environment
        ivar_env: Arc<bif_renderer::HdriEnvironment>,
        source_path: String,
        load_path: String,
        rotation_rad: f32,
        intensity: f32,
        show_background: bool,
        load_secs: f64,
    },
    Error {
        source_path: String,
        load_path: String,
        message: String,
    },
}

/// Manages environment IBL state and skybox rendering.
pub struct EnvironmentManager {
    /// GPU environment resources (cubemaps, BRDF LUT, bind groups)
    pub gpu_env: GpuEnvironment,
    /// Whether to render the skybox background
    pub show_background: bool,
    /// Skybox render pipeline
    skybox_pipeline: wgpu::RenderPipeline,
    /// Skybox bind group (references gpu_env textures)
    skybox_bind_group: wgpu::BindGroup,
    /// GPU compute IBL pipelines
    compute_ibl: ComputeIbl,
    /// Async IBL generation receiver
    ibl_receiver: Option<mpsc::Receiver<IblResult>>,
    /// Async .tx conversion result receiver
    tx_receiver: Option<mpsc::Receiver<String>>,
    /// Whether an HDRI has been loaded (user or auto from DomeLight)
    pub hdri_loaded: bool,
}

impl EnvironmentManager {
    /// Create a new EnvironmentManager with default (black fallback) environment.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let gpu_env = GpuEnvironment::new_default(device, queue);

        let skybox_bind_group_layout = skybox::create_skybox_bind_group_layout(device);
        let skybox_pipeline = skybox::create_skybox_pipeline(
            device,
            camera_bind_group_layout,
            &skybox_bind_group_layout,
            surface_format,
        );
        let skybox_bind_group = skybox::create_skybox_bind_group(
            device,
            &skybox_bind_group_layout,
            &gpu_env.cubemap_view,
            &gpu_env.sampler,
            &gpu_env.params_buffer,
        );

        let compute_ibl = ComputeIbl::new(device);

        Self {
            gpu_env,
            show_background: true,
            skybox_pipeline,
            skybox_bind_group,
            compute_ibl,
            ibl_receiver: None,
            tx_receiver: None,
            hdri_loaded: false,
        }
    }

    /// Get the environment bind group layout for pipeline creation.
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.gpu_env.bind_group_layout
    }

    /// Get the environment bind group for rendering.
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.gpu_env.bind_group
    }

    /// Start async HDRI load in background thread.
    pub fn start_hdri_load(&mut self, path: &Path, rotation: f32, intensity: f32, show_bg: bool) {
        self.hdri_loaded = true;
        let (tx, rx) = mpsc::channel();
        self.ibl_receiver = Some(rx);
        let rotation_rad = rotation.to_radians();
        let path_str = path.to_string_lossy().to_string();
        let source_path = path_str.clone();

        std::thread::spawn(move || {
            let load_start = std::time::Instant::now();
            // Skip .tx conversion for HDRIs - we load all pixels anyway so no benefit
            // .tx is useful for material textures (tiled access, mipmaps) but not for
            // environment maps where we need the full image for IBL prefiltering
            let load_path = path_str.clone();

            match bif_core::hdr::HdrImage::load(&load_path) {
                Ok(hdr) => {
                    let load_secs = load_start.elapsed().as_secs_f64();
                    log::info!(
                        "HDRI load finished in {:.2}s ({}x{}, path={})",
                        load_secs,
                        hdr.width,
                        hdr.height,
                        load_path
                    );
                    let hdr = if hdr.width.max(hdr.height) > bif_core::hdr::MAX_IBL_DIMENSION {
                        log::warn!(
                            "HDRI {}x{} exceeds max dimension {}, downscaling",
                            hdr.width,
                            hdr.height,
                            bif_core::hdr::MAX_IBL_DIMENSION
                        );
                        hdr.downscale_to_max_dim(bif_core::hdr::MAX_IBL_DIMENSION)
                    } else {
                        hdr
                    };
                    let hdr_pixels = hdr.pixels.clone();
                    let hdr_width = hdr.width;
                    let hdr_height = hdr.height;
                    let ivar_env = bif_renderer::HdriEnvironment::new(hdr, rotation_rad, intensity);
                    let _ = tx.send(IblResult::Success {
                        hdr_pixels,
                        hdr_width,
                        hdr_height,
                        ivar_env: Arc::new(ivar_env),
                        source_path,
                        load_path,
                        rotation_rad,
                        intensity,
                        show_background: show_bg,
                        load_secs,
                    });
                }
                Err(e) => {
                    let load_secs = load_start.elapsed().as_secs_f64();
                    log::error!(
                        "HDRI load failed in {:.2}s (path={}): {}",
                        load_secs,
                        load_path,
                        e
                    );
                    let _ = tx.send(IblResult::Error {
                        source_path,
                        load_path,
                        message: e.to_string(),
                    });
                }
            }
        });
    }

    /// Poll for completed async IBL generation.
    pub fn poll_ibl_result(&mut self) -> Option<IblResult> {
        let result = self.ibl_receiver.as_ref().and_then(|rx| rx.try_recv().ok());
        if result.is_some() {
            self.ibl_receiver = None;
        }
        result
    }

    /// Apply IBL result from background thread.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_ibl_result(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        hdr_pixels: &[[f32; 3]],
        hdr_width: u32,
        hdr_height: u32,
        rotation_rad: f32,
        intensity: f32,
        show_background: bool,
    ) -> f64 {
        let compute_start = std::time::Instant::now();
        // GPU compute IBL for viewport
        let output = self
            .compute_ibl
            .generate(device, queue, hdr_width, hdr_height, hdr_pixels);

        let mip_count = compute_ibl::PREFILTER_MIP_COUNT;
        self.gpu_env.params.max_mip = (mip_count - 1).max(1) as f32;
        self.gpu_env
            .load_from_compute(device, queue, output, mip_count);
        self.update_params(queue, intensity, rotation_rad, show_background);
        let compute_secs = compute_start.elapsed().as_secs_f64();
        log::info!(
            "HDRI compute IBL finished in {:.2}s ({}x{})",
            compute_secs,
            hdr_width,
            hdr_height
        );
        // Rebuild skybox bind group (use base cubemap for sharper skybox)
        let skybox_bgl = skybox::create_skybox_bind_group_layout(device);
        self.skybox_bind_group = skybox::create_skybox_bind_group(
            device,
            &skybox_bgl,
            &self.gpu_env.cubemap_view,
            &self.gpu_env.sampler,
            &self.gpu_env.params_buffer,
        );
        compute_secs
    }

    /// Update environment parameters without regenerating maps.
    pub fn update_params(&mut self, queue: &Queue, intensity: f32, rotation: f32, show_bg: bool) {
        self.gpu_env.params.intensity = intensity;
        self.gpu_env.params.rotation = rotation;
        self.show_background = show_bg;
        self.gpu_env.update_params(queue);
    }

    /// Check if environment has loaded IBL data.
    pub fn has_environment(&self) -> bool {
        self.gpu_env.params.has_environment != 0
    }

    /// Start async .tx texture conversion.
    #[cfg(feature = "oiio")]
    pub fn start_tx_conversion(
        &mut self,
        paths: Vec<String>,
        base_dir: Option<std::path::PathBuf>,
    ) {
        if paths.is_empty() {
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.tx_receiver = Some(rx);

        std::thread::spawn(move || {
            let cache = match base_dir {
                Some(dir) => bif_core::texture::TextureCache::with_base_dir(dir),
                None => bif_core::texture::TextureCache::new(),
            };
            let count = cache.convert_textures_to_tx(&paths);
            let status = if count > 0 {
                format!("{}/{} converted", count, paths.len())
            } else {
                "All up to date".into()
            };
            let _ = tx.send(status);
        });
    }

    /// Start async .tx texture conversion (stub when oiio not available).
    #[cfg(not(feature = "oiio"))]
    pub fn start_tx_conversion(
        &mut self,
        _paths: Vec<String>,
        _base_dir: Option<std::path::PathBuf>,
    ) {
        // oiio feature not available
    }

    /// Poll for completed .tx conversion result.
    pub fn poll_tx_result(&mut self) -> Option<String> {
        let result = self.tx_receiver.as_ref().and_then(|rx| rx.try_recv().ok());
        if result.is_some() {
            self.tx_receiver = None;
        }
        result
    }

    /// Render the skybox background.
    ///
    /// Call this before geometry rendering, with depth write disabled.
    pub fn render_skybox<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        camera_bind_group: &'a wgpu::BindGroup,
    ) {
        if !self.show_background || !self.has_environment() {
            return;
        }
        render_pass.set_pipeline(&self.skybox_pipeline);
        render_pass.set_bind_group(0, camera_bind_group, &[]);
        render_pass.set_bind_group(1, &self.skybox_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }

    /// Check if skybox should be rendered (for load op decisions).
    pub fn should_render_skybox(&self) -> bool {
        self.show_background && self.has_environment()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ibl_result_success_variant() {
        // Just a compile check for the enum
        use super::IblResult;
        let _result = IblResult::Error {
            source_path: "test.hdr".to_string(),
            load_path: "test.hdr".to_string(),
            message: "test error".to_string(),
        };
    }
}
