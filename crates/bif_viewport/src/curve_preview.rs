//! Curve preview renderer for UsdGeomBasisCurves.
//!
//! Draws curves as line segments using LineList topology.
//! Uses a storage buffer for vertex positions.

use wgpu::util::DeviceExt;

/// GPU-aligned vertex position (16 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    x: f32,
    y: f32,
    z: f32,
    _pad: f32,
}

/// GPU-aligned curve params.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct CurveParams {
    color: [f32; 4],
}

/// Renders curves as line segments.
pub struct CurvePreviewRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertex_count: u32,
    /// Whether to show the curve preview.
    pub visible: bool,
    /// Line color (RGBA).
    pub color: [f32; 4],
}

impl CurvePreviewRenderer {
    /// Create a new curve preview renderer.
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Curve Preview Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/curve_preview.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Curve Preview BGL"),
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
            label: Some("Curve Preview Pipeline Layout"),
            bind_group_layouts: &[camera_bind_group_layout, &bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Curve Preview Pipeline"),
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
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

        let default_color = [1.0, 0.6, 0.0, 0.9]; // Orange
        let params = CurveParams {
            color: default_color,
        };
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Curve Preview Params"),
            contents: bytemuck::cast_slice(&[params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let dummy = GpuVertex {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            _pad: 0.0,
        };
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Curve Preview Vertices"),
            contents: bytemuck::cast_slice(&[dummy]),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Curve Preview BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: vertex_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            pipeline,
            bind_group_layout,
            params_buffer,
            vertex_buffer,
            bind_group,
            vertex_count: 0,
            visible: true,
            color: default_color,
        }
    }

    /// Upload curve data as line segments.
    ///
    /// Converts curves (control points + vertex counts) into pairs of
    /// line segment endpoints for LineList rendering.
    pub fn upload_curves(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        curves: &[bif_core::scene::CurvesPrim],
        points_prims: &[bif_core::scene::PointsPrim],
    ) {
        let mut gpu_verts: Vec<GpuVertex> = Vec::new();

        // Convert curves to line segments
        for curve in curves {
            let xform = curve.transform;
            let mut offset = 0usize;
            for &count in &curve.curve_vertex_counts {
                let n = count as usize;
                for i in 0..n.saturating_sub(1) {
                    let idx_a = offset + i;
                    let idx_b = offset + i + 1;
                    if let (Some(&a), Some(&b)) = (curve.points.get(idx_a), curve.points.get(idx_b))
                    {
                        let wa = xform.transform_point3(a);
                        let wb = xform.transform_point3(b);
                        gpu_verts.push(GpuVertex {
                            x: wa.x,
                            y: wa.y,
                            z: wa.z,
                            _pad: 0.0,
                        });
                        gpu_verts.push(GpuVertex {
                            x: wb.x,
                            y: wb.y,
                            z: wb.z,
                            _pad: 0.0,
                        });
                    }
                }
                offset += n;
            }
        }

        // Render points prims as small cross-hair line segments (6 verts per point)
        for pts in points_prims {
            let xform = pts.transform;
            let half = 0.02_f32; // Cross size in world units
            for &pos in &pts.positions {
                let wp = xform.transform_point3(pos);
                // X axis
                gpu_verts.push(GpuVertex {
                    x: wp.x - half,
                    y: wp.y,
                    z: wp.z,
                    _pad: 0.0,
                });
                gpu_verts.push(GpuVertex {
                    x: wp.x + half,
                    y: wp.y,
                    z: wp.z,
                    _pad: 0.0,
                });
                // Y axis
                gpu_verts.push(GpuVertex {
                    x: wp.x,
                    y: wp.y - half,
                    z: wp.z,
                    _pad: 0.0,
                });
                gpu_verts.push(GpuVertex {
                    x: wp.x,
                    y: wp.y + half,
                    z: wp.z,
                    _pad: 0.0,
                });
                // Z axis
                gpu_verts.push(GpuVertex {
                    x: wp.x,
                    y: wp.y,
                    z: wp.z - half,
                    _pad: 0.0,
                });
                gpu_verts.push(GpuVertex {
                    x: wp.x,
                    y: wp.y,
                    z: wp.z + half,
                    _pad: 0.0,
                });
            }
        }

        if gpu_verts.is_empty() {
            self.vertex_count = 0;
            return;
        }

        let byte_size = (gpu_verts.len() * std::mem::size_of::<GpuVertex>()) as u64;

        if byte_size <= self.vertex_buffer.size() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&gpu_verts));
        } else {
            self.vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Curve Preview Vertices"),
                contents: bytemuck::cast_slice(&gpu_verts),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
            self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Curve Preview BG"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.params_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.vertex_buffer.as_entire_binding(),
                    },
                ],
            });
        }

        self.vertex_count = gpu_verts.len() as u32;
    }

    /// Write current params to GPU.
    pub fn update_params(&self, queue: &wgpu::Queue) {
        let params = CurveParams { color: self.color };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[params]));
    }

    /// Render line segments. Call after setting viewport/scissor.
    pub fn render<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        camera_bind_group: &'a wgpu::BindGroup,
    ) {
        if !self.visible || self.vertex_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, camera_bind_group, &[]);
        render_pass.set_bind_group(1, &self.bind_group, &[]);
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}
