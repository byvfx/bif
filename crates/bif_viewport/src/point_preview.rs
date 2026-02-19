//! Point preview renderer for scatter point clouds.
//!
//! Draws point cloud positions as billboard quads with circle masking.
//! Uses a storage buffer for positions and instanced TriangleStrip quads.

use bif_math::Vec3;
use wgpu::util::DeviceExt;

/// GPU-aligned point position (16 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuPoint {
    x: f32,
    y: f32,
    z: f32,
    _pad: f32,
}

/// GPU-aligned point preview parameters.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PointParams {
    color: [f32; 4],
    point_size: f32,
    viewport_width: f32,
    viewport_height: f32,
    _pad: f32,
}

/// Renders point cloud positions as billboard quad circles.
pub struct PointPreviewRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    point_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    point_count: u32,
    /// Whether to show the point preview.
    pub visible: bool,
    /// Point color (RGBA).
    pub color: [f32; 4],
    /// Point size in pixels.
    pub point_size: f32,
}

impl PointPreviewRenderer {
    /// Create a new point preview renderer.
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Point Preview Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/point_preview.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Point Preview BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Point Preview Pipeline Layout"),
            bind_group_layouts: &[camera_bind_group_layout, &bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Point Preview Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let default_color = [0.0, 0.9, 0.9, 0.8]; // Cyan
        let default_point_size = 5.0;
        let params = PointParams {
            color: default_color,
            point_size: default_point_size,
            viewport_width: 1920.0,
            viewport_height: 1080.0,
            _pad: 0.0,
        };
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Point Preview Params"),
            contents: bytemuck::cast_slice(&[params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Dummy point buffer (1 point)
        let dummy = GpuPoint {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            _pad: 0.0,
        };
        let point_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Point Preview Points"),
            contents: bytemuck::cast_slice(&[dummy]),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Point Preview BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: point_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            pipeline,
            bind_group_layout,
            params_buffer,
            point_buffer,
            bind_group,
            point_count: 0,
            visible: false,
            color: default_color,
            point_size: default_point_size,
        }
    }

    /// Upload new point positions from scene point clouds.
    ///
    /// Upload point positions to the GPU storage buffer.
    ///
    /// Reuses the existing GPU buffer when it fits; only recreates
    /// the buffer and bind group when the data exceeds current capacity.
    ///
    /// Does NOT write params (color, size, viewport). Callers must set
    /// `point_preview_params_dirty = true` so the centralized dirty-flag
    /// block writes params exactly once before the render pass.
    pub fn upload_points(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        positions: &[Vec3],
    ) {
        if positions.is_empty() {
            self.point_count = 0;
            return;
        }

        let gpu_points: Vec<GpuPoint> = positions
            .iter()
            .map(|p| GpuPoint {
                x: p.x,
                y: p.y,
                z: p.z,
                _pad: 0.0,
            })
            .collect();

        let byte_size = (gpu_points.len() * std::mem::size_of::<GpuPoint>()) as u64;

        if byte_size <= self.point_buffer.size() {
            // Reuse existing buffer
            queue.write_buffer(&self.point_buffer, 0, bytemuck::cast_slice(&gpu_points));
        } else {
            // Allocate larger buffer + new bind group
            self.point_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Point Preview Points"),
                contents: bytemuck::cast_slice(&gpu_points),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });

            self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Point Preview BG"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.params_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.point_buffer.as_entire_binding(),
                    },
                ],
            });
        }

        self.point_count = positions.len() as u32;
    }

    /// Write current params (color, size, viewport) to GPU without re-uploading points.
    pub fn update_params(&self, queue: &wgpu::Queue, viewport_width: f32, viewport_height: f32) {
        let params = PointParams {
            color: self.color,
            point_size: self.point_size,
            viewport_width,
            viewport_height,
            _pad: 0.0,
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[params]));
    }

    /// Render points as billboard quads. Call after setting viewport/scissor.
    pub fn render<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        camera_bind_group: &'a wgpu::BindGroup,
    ) {
        if !self.visible || self.point_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, camera_bind_group, &[]);
        render_pass.set_bind_group(1, &self.bind_group, &[]);
        // 4 vertices per quad (triangle strip), one instance per point
        render_pass.draw(0..4, 0..self.point_count);
    }
}
