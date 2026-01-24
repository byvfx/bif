//! GPU environment map management for IBL rendering.
//!
//! Handles cubemap texture creation, upload from CPU-generated IBL maps,
//! and bind group management for the environment lighting pipeline.

use wgpu::{Device, Queue};

use bif_core::ibl::BrdfLut;

use crate::gpu_types::EnvironmentParamsUniform;

/// GPU-side environment resources for IBL.
pub struct GpuEnvironment {
    /// Irradiance cubemap for diffuse IBL.
    pub irradiance_texture: wgpu::Texture,
    pub irradiance_view: wgpu::TextureView,
    /// Prefiltered specular cubemap with mip levels.
    pub prefiltered_texture: wgpu::Texture,
    pub prefiltered_view: wgpu::TextureView,
    /// BRDF integration LUT.
    pub brdf_lut_texture: wgpu::Texture,
    pub brdf_lut_view: wgpu::TextureView,
    /// Cubemap sampler.
    pub sampler: wgpu::Sampler,
    /// Environment parameters uniform buffer.
    pub params_buffer: wgpu::Buffer,
    pub params: EnvironmentParamsUniform,
    /// Bind group for environment (group 3).
    pub bind_group: wgpu::BindGroup,
    /// Bind group layout.
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl GpuEnvironment {
    /// Create default (black fallback) environment resources.
    pub fn new_default(device: &Device, queue: &Queue) -> Self {
        let bind_group_layout = Self::create_bind_group_layout(device);

        // Create 1x1 black fallback cubemaps
        let irradiance_texture = create_cubemap_texture(device, 1, 1, "Irradiance Fallback");
        let prefiltered_texture = create_cubemap_texture(device, 1, 1, "Prefiltered Fallback");

        // Upload black pixels for cubemaps
        let black_face = [[0.0f32, 0.0, 0.0, 1.0]; 1];
        for face in 0..6u32 {
            upload_cubemap_face(queue, &irradiance_texture, 1, face, 0, &black_face);
            upload_cubemap_face(queue, &prefiltered_texture, 1, face, 0, &black_face);
        }

        // Pre-generate and upload BRDF LUT (environment-independent, only needs computing once)
        let brdf_lut = bif_core::ibl::generate_brdf_lut(bif_core::ibl::BRDF_LUT_SIZE);
        let brdf_lut_texture = create_lut_texture(device, brdf_lut.size, "BRDF LUT");
        upload_brdf_lut(queue, &brdf_lut_texture, &brdf_lut);

        let irradiance_view = irradiance_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let prefiltered_view = prefiltered_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let brdf_lut_view = brdf_lut_texture.create_view(&Default::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Environment Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let params = EnvironmentParamsUniform::new();
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Environment Params"),
            contents: bytemuck::cast_slice(&[params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = Self::create_bind_group(
            device,
            &bind_group_layout,
            &irradiance_view,
            &prefiltered_view,
            &brdf_lut_view,
            &sampler,
            &params_buffer,
        );

        Self {
            irradiance_texture,
            irradiance_view,
            prefiltered_texture,
            prefiltered_view,
            brdf_lut_texture,
            brdf_lut_view,
            sampler,
            params_buffer,
            params,
            bind_group,
            bind_group_layout,
        }
    }

    /// Load environment from GPU compute output textures (no CPU upload needed).
    pub fn load_from_compute(
        &mut self,
        device: &Device,
        queue: &Queue,
        output: crate::compute_ibl::ComputeIblOutput,
        mip_count: u32,
    ) {
        self.irradiance_texture = output.irradiance;
        self.prefiltered_texture = output.prefiltered;

        self.irradiance_view = self
            .irradiance_texture
            .create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            });
        self.prefiltered_view =
            self.prefiltered_texture
                .create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::Cube),
                    mip_level_count: Some(mip_count),
                    ..Default::default()
                });

        self.params.has_environment = 1;
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[self.params]));

        self.bind_group = Self::create_bind_group(
            device,
            &self.bind_group_layout,
            &self.irradiance_view,
            &self.prefiltered_view,
            &self.brdf_lut_view,
            &self.sampler,
            &self.params_buffer,
        );

        log::info!("Environment maps loaded from GPU compute output");
    }

    /// Update just the environment parameters (rotation, intensity, background toggle).
    pub fn update_params(&mut self, queue: &Queue) {
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[self.params]));
    }

    /// Create the bind group layout for environment (group 3).
    pub fn create_bind_group_layout(device: &Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Environment Bind Group Layout"),
            entries: &[
                // Irradiance cubemap
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                // Prefiltered cubemap
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                // BRDF LUT
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Environment params
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    fn create_bind_group(
        device: &Device,
        layout: &wgpu::BindGroupLayout,
        irradiance_view: &wgpu::TextureView,
        prefiltered_view: &wgpu::TextureView,
        brdf_lut_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        params_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Environment Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(irradiance_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(prefiltered_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(brdf_lut_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        })
    }
}

/// Create a cubemap texture (6-layer 2D array with cube-compatible flag).
fn create_cubemap_texture(
    device: &Device,
    size: u32,
    mip_count: u32,
    label: &str,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Create a 2D LUT texture.
fn create_lut_texture(device: &Device, size: u32, label: &str) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Upload a single cubemap face to a mip level.
fn upload_cubemap_face(
    queue: &Queue,
    texture: &wgpu::Texture,
    size: u32,
    face_index: u32,
    mip_level: u32,
    pixels: &[[f32; 4]],
) {
    // Convert f32 RGBA to f16 RGBA for Rgba16Float format
    let f16_data: Vec<u8> = pixels
        .iter()
        .flat_map(|p| {
            [
                half::f16::from_f32(p[0]).to_le_bytes(),
                half::f16::from_f32(p[1]).to_le_bytes(),
                half::f16::from_f32(p[2]).to_le_bytes(),
                half::f16::from_f32(p[3]).to_le_bytes(),
            ]
            .into_iter()
            .flatten()
        })
        .collect();

    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture,
            mip_level,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: face_index,
            },
            aspect: wgpu::TextureAspect::All,
        },
        &f16_data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(size * 8), // 4 channels * 2 bytes (f16)
            rows_per_image: Some(size),
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
    );
}

/// Upload the BRDF LUT (RG16Float format).
fn upload_brdf_lut(queue: &Queue, texture: &wgpu::Texture, lut: &BrdfLut) {
    let f16_data: Vec<u8> = lut
        .pixels
        .iter()
        .flat_map(|p| {
            [
                half::f16::from_f32(p[0]).to_le_bytes(),
                half::f16::from_f32(p[1]).to_le_bytes(),
            ]
            .into_iter()
            .flatten()
        })
        .collect();

    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &f16_data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(lut.size * 4), // 2 channels * 2 bytes (f16)
            rows_per_image: Some(lut.size),
        },
        wgpu::Extent3d {
            width: lut.size,
            height: lut.size,
            depth_or_array_layers: 1,
        },
    );
}


use wgpu::util::DeviceExt;
