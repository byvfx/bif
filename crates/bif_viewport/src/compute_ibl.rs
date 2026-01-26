//! GPU compute pipeline for IBL prefiltering.
//!
//! Replaces CPU-side IBL generation with GPU compute shaders:
//! 1. Equirect → cubemap conversion
//! 2. Irradiance convolution (diffuse IBL)
//! 3. GGX prefiltered specular (per mip level)

use wgpu::{util::DeviceExt, Device, Queue};

/// Cubemap face size for the base environment map.
pub const CUBEMAP_SIZE: u32 = 256;
/// Irradiance cubemap size.
pub const IRRADIANCE_SIZE: u32 = 32;
/// Prefiltered specular base size (halved per mip).
pub const PREFILTER_SIZE: u32 = 128;
/// Number of prefilter mip levels.
pub const PREFILTER_MIP_COUNT: u32 = 5;
/// Sample count for GGX prefilter.
const PREFILTER_SAMPLES: u32 = 1024;

/// Prefilter params uniform (matches WGSL struct).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct PrefilterParams {
    roughness: f32,
    sample_count: u32,
    face_size: u32,
    _pad: u32,
}

/// GPU compute IBL pipeline resources.
pub struct ComputeIbl {
    equirect_pipeline: wgpu::ComputePipeline,
    equirect_bind_group_layout: wgpu::BindGroupLayout,
    irradiance_pipeline: wgpu::ComputePipeline,
    irradiance_bind_group_layout: wgpu::BindGroupLayout,
    prefilter_pipeline: wgpu::ComputePipeline,
    prefilter_bind_group_layout: wgpu::BindGroupLayout,
    linear_sampler: wgpu::Sampler,
}

/// Output textures from GPU IBL generation.
pub struct ComputeIblOutput {
    /// Base cubemap (CUBEMAP_SIZE, 6 faces, 1 mip).
    pub cubemap: wgpu::Texture,
    /// Irradiance cubemap (IRRADIANCE_SIZE, 6 faces, 1 mip).
    pub irradiance: wgpu::Texture,
    /// Prefiltered specular cubemap (PREFILTER_SIZE, 6 faces, PREFILTER_MIP_COUNT mips).
    pub prefiltered: wgpu::Texture,
}

impl ComputeIbl {
    /// Create compute IBL pipelines.
    pub fn new(device: &Device) -> Self {
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Compute IBL Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let (equirect_pipeline, equirect_bind_group_layout) =
            Self::create_equirect_pipeline(device);
        let (irradiance_pipeline, irradiance_bind_group_layout) =
            Self::create_irradiance_pipeline(device);
        let (prefilter_pipeline, prefilter_bind_group_layout) =
            Self::create_prefilter_pipeline(device);

        Self {
            equirect_pipeline,
            equirect_bind_group_layout,
            irradiance_pipeline,
            irradiance_bind_group_layout,
            prefilter_pipeline,
            prefilter_bind_group_layout,
            linear_sampler,
        }
    }

    /// Run full IBL generation pipeline from equirectangular HDR pixels.
    ///
    /// `pixels` should be `width * height` RGB f32 values.
    pub fn generate(
        &self,
        device: &Device,
        queue: &Queue,
        width: u32,
        height: u32,
        pixels: &[[f32; 3]],
    ) -> ComputeIblOutput {
        // 1. Upload equirect as Rgba32Float 2D texture
        let equirect_texture = self.upload_equirect(device, queue, width, height, pixels);

        // 2. Create output cubemap textures (with STORAGE_BINDING)
        let cubemap = create_storage_cubemap(device, CUBEMAP_SIZE, 1, "Compute Cubemap");
        let irradiance = create_storage_cubemap(device, IRRADIANCE_SIZE, 1, "Compute Irradiance");
        let prefiltered = create_storage_cubemap(
            device,
            PREFILTER_SIZE,
            PREFILTER_MIP_COUNT,
            "Compute Prefiltered",
        );

        // 3. Dispatch equirect → cubemap
        self.dispatch_equirect(device, queue, &equirect_texture, &cubemap);

        // Create cubemap view for sampling in subsequent passes
        let cubemap_view = cubemap.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        // 4. Dispatch irradiance convolution
        self.dispatch_irradiance(device, queue, &cubemap_view, &irradiance);

        // 5. Dispatch prefilter per mip
        self.dispatch_prefilter(device, queue, &cubemap_view, &prefiltered);

        ComputeIblOutput {
            cubemap,
            irradiance,
            prefiltered,
        }
    }

    fn upload_equirect(
        &self,
        device: &Device,
        queue: &Queue,
        width: u32,
        height: u32,
        pixels: &[[f32; 3]],
    ) -> wgpu::Texture {
        // Convert RGB to RGBA f32
        let rgba: Vec<f32> = pixels
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 1.0])
            .collect();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Equirect HDR"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&rgba),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * 16), // 4 * f32 (4 bytes each)
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        texture
    }

    fn dispatch_equirect(
        &self,
        device: &Device,
        queue: &Queue,
        equirect: &wgpu::Texture,
        cubemap: &wgpu::Texture,
    ) {
        let equirect_view = equirect.create_view(&Default::default());
        let cubemap_storage_view = cubemap.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Equirect to Cube Bind Group"),
            layout: &self.equirect_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&equirect_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&cubemap_storage_view),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Equirect to Cube"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Equirect to Cube Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.equirect_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let wg = CUBEMAP_SIZE.div_ceil(8);
            pass.dispatch_workgroups(wg, wg, 6);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    fn dispatch_irradiance(
        &self,
        device: &Device,
        queue: &Queue,
        cubemap_view: &wgpu::TextureView,
        irradiance: &wgpu::Texture,
    ) {
        let irradiance_storage_view = irradiance.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Irradiance Bind Group"),
            layout: &self.irradiance_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(cubemap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&irradiance_storage_view),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Irradiance Convolution"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Irradiance Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.irradiance_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let wg = IRRADIANCE_SIZE.div_ceil(8);
            pass.dispatch_workgroups(wg, wg, 6);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    fn dispatch_prefilter(
        &self,
        device: &Device,
        queue: &Queue,
        cubemap_view: &wgpu::TextureView,
        prefiltered: &wgpu::Texture,
    ) {
        for mip in 0..PREFILTER_MIP_COUNT {
            let mip_size = PREFILTER_SIZE >> mip;
            let roughness = mip as f32 / (PREFILTER_MIP_COUNT - 1) as f32;

            let params = PrefilterParams {
                roughness,
                sample_count: PREFILTER_SAMPLES,
                face_size: mip_size,
                _pad: 0,
            };
            let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Prefilter Params"),
                contents: bytemuck::cast_slice(&[params]),
                usage: wgpu::BufferUsages::UNIFORM,
            });

            // Create view for this specific mip level
            let mip_view = prefiltered.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Prefilter Bind Group"),
                layout: &self.prefilter_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(cubemap_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&mip_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: params_buffer.as_entire_binding(),
                    },
                ],
            });

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some(&format!("Prefilter Mip {}", mip)),
            });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Prefilter Pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.prefilter_pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                let wg = mip_size.div_ceil(8);
                pass.dispatch_workgroups(wg, wg, 6);
            }
            queue.submit(std::iter::once(encoder.finish()));
        }
    }

    fn create_equirect_pipeline(device: &Device) -> (wgpu::ComputePipeline, wgpu::BindGroupLayout) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Equirect to Cube Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/equirect_to_cube.wgsl").into()),
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Equirect BGL"),
            entries: &[
                // Input equirect texture (non-filterable, used with textureLoad)
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
                // Output cubemap (storage)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Equirect Pipeline Layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Equirect to Cube Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });

        (pipeline, layout)
    }

    fn create_irradiance_pipeline(
        device: &Device,
    ) -> (wgpu::ComputePipeline, wgpu::BindGroupLayout) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Irradiance Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shaders/irradiance_compute.wgsl").into(),
            ),
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Irradiance BGL"),
            entries: &[
                // Input cubemap
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Output irradiance (storage)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Irradiance Pipeline Layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Irradiance Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });

        (pipeline, layout)
    }

    fn create_prefilter_pipeline(
        device: &Device,
    ) -> (wgpu::ComputePipeline, wgpu::BindGroupLayout) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Prefilter Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/prefilter_compute.wgsl").into()),
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Prefilter BGL"),
            entries: &[
                // Input cubemap
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Output prefiltered mip (storage)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                // Params uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Prefilter Pipeline Layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Prefilter Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });

        (pipeline, layout)
    }
}

/// Create a cubemap texture with STORAGE_BINDING for compute writes.
fn create_storage_cubemap(
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
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    })
}
