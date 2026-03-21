//! Curve preview renderer for UsdGeomBasisCurves.
//!
//! Draws curves as line segments using LineList topology.
//! Uses a storage buffer for vertex positions.

use bif_math::Vec3;
use wgpu::util::DeviceExt;

/// Tessellation segments per cubic curve span. Higher = smoother, more verts.
const TESS_SEGMENTS: usize = 4;

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

    /// Upload curve data as tessellated line segments.
    ///
    /// Linear curves connect consecutive control points. Cubic curves
    /// (Bezier, B-spline, Catmull-Rom) are tessellated into smooth segments.
    /// Uses a grow-only buffer to avoid frame hitches on re-upload.
    pub fn upload_curves(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        curves: &[bif_core::scene::CurvesPrim],
        points_prims: &[bif_core::scene::PointsPrim],
    ) {
        let mut gpu_verts: Vec<GpuVertex> = Vec::new();

        for curve in curves {
            let xform = curve.transform;
            let pts = &curve.points;
            let mut offset = 0usize;

            for &count in &curve.curve_vertex_counts {
                let n = count as usize;
                match curve.curve_type {
                    bif_core::usd::CurveType::Linear => {
                        // Connect consecutive control points directly
                        for i in 0..n.saturating_sub(1) {
                            push_line_seg(&mut gpu_verts, &xform, pts, offset + i, offset + i + 1);
                        }
                    }
                    bif_core::usd::CurveType::Cubic => {
                        tessellate_cubic(&mut gpu_verts, &xform, pts, offset, n, curve.basis);
                    }
                }
                offset += n;
            }
        }

        // Points prims as small cross-hair line segments (6 verts per point)
        for pts in points_prims {
            let xform = pts.transform;
            let half = 0.02_f32;
            for &pos in &pts.positions {
                let wp = xform.transform_point3(pos);
                for axis in 0..3 {
                    let mut a = wp;
                    let mut b = wp;
                    match axis {
                        0 => {
                            a.x -= half;
                            b.x += half;
                        }
                        1 => {
                            a.y -= half;
                            b.y += half;
                        }
                        _ => {
                            a.z -= half;
                            b.z += half;
                        }
                    }
                    gpu_verts.push(gpu_vert(a));
                    gpu_verts.push(gpu_vert(b));
                }
            }
        }

        if gpu_verts.is_empty() {
            self.vertex_count = 0;
            return;
        }

        let byte_size = (gpu_verts.len() * std::mem::size_of::<GpuVertex>()) as u64;

        if byte_size <= self.vertex_buffer.size() {
            // Reuse existing buffer (grow-only: never shrink)
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&gpu_verts));
        } else {
            // Grow to 2x needed size to reduce future reallocations
            let alloc_size = byte_size * 2;
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Curve Preview Vertices"),
                size: alloc_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&gpu_verts));
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

// ============================================================================
// Curve tessellation helpers
// ============================================================================

fn gpu_vert(p: Vec3) -> GpuVertex {
    GpuVertex {
        x: p.x,
        y: p.y,
        z: p.z,
        _pad: 0.0,
    }
}

/// Push a single line segment (2 verts) from transformed control points.
fn push_line_seg(
    out: &mut Vec<GpuVertex>,
    xform: &bif_math::Mat4,
    pts: &[Vec3],
    idx_a: usize,
    idx_b: usize,
) {
    if let (Some(&a), Some(&b)) = (pts.get(idx_a), pts.get(idx_b)) {
        out.push(gpu_vert(xform.transform_point3(a)));
        out.push(gpu_vert(xform.transform_point3(b)));
    }
}

/// Tessellate cubic curve spans into line segments.
///
/// Bezier: (n-1)/3 spans of 4 CVs each.
/// BSpline/CatmullRom: n-3 spans, sliding window of 4 CVs.
fn tessellate_cubic(
    out: &mut Vec<GpuVertex>,
    xform: &bif_math::Mat4,
    pts: &[Vec3],
    offset: usize,
    n: usize,
    basis: bif_core::usd::CurveBasis,
) {
    if n < 4 {
        // Not enough CVs for cubic — fall back to linear
        for i in 0..n.saturating_sub(1) {
            push_line_seg(out, xform, pts, offset + i, offset + i + 1);
        }
        return;
    }

    let eval_fn: fn(Vec3, Vec3, Vec3, Vec3, f32) -> Vec3 = match basis {
        bif_core::usd::CurveBasis::Bezier => eval_bezier,
        bif_core::usd::CurveBasis::Bspline => eval_bspline,
        bif_core::usd::CurveBasis::CatmullRom => eval_catmull_rom,
    };

    let (num_spans, stride) = match basis {
        bif_core::usd::CurveBasis::Bezier => ((n - 1) / 3, 3),
        bif_core::usd::CurveBasis::Bspline | bif_core::usd::CurveBasis::CatmullRom => (n - 3, 1),
    };

    for span in 0..num_spans {
        let base = offset + span * stride;
        let (Some(&p0), Some(&p1), Some(&p2), Some(&p3)) = (
            pts.get(base),
            pts.get(base + 1),
            pts.get(base + 2),
            pts.get(base + 3),
        ) else {
            continue;
        };

        let mut prev = xform.transform_point3(eval_fn(p0, p1, p2, p3, 0.0));
        for seg in 1..=TESS_SEGMENTS {
            let t = seg as f32 / TESS_SEGMENTS as f32;
            let curr = xform.transform_point3(eval_fn(p0, p1, p2, p3, t));
            out.push(gpu_vert(prev));
            out.push(gpu_vert(curr));
            prev = curr;
        }
    }
}

/// Evaluate cubic Bezier at t ∈ [0, 1].
fn eval_bezier(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let u = 1.0 - t;
    let u2 = u * u;
    let t2 = t * t;
    p0 * (u2 * u) + p1 * (3.0 * u2 * t) + p2 * (3.0 * u * t2) + p3 * (t2 * t)
}

/// Evaluate uniform cubic B-spline at t ∈ [0, 1].
fn eval_bspline(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;
    let c0 = (1.0 - 3.0 * t + 3.0 * t2 - t3) / 6.0;
    let c1 = (4.0 - 6.0 * t2 + 3.0 * t3) / 6.0;
    let c2 = (1.0 + 3.0 * t + 3.0 * t2 - 3.0 * t3) / 6.0;
    let c3 = t3 / 6.0;
    p0 * c0 + p1 * c1 + p2 * c2 + p3 * c3
}

/// Evaluate Catmull-Rom spline at t ∈ [0, 1].
fn eval_catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;
    let c0 = (-t + 2.0 * t2 - t3) * 0.5;
    let c1 = (2.0 - 5.0 * t2 + 3.0 * t3) * 0.5;
    let c2 = (t + 4.0 * t2 - 3.0 * t3) * 0.5;
    let c3 = (-t2 + t3) * 0.5;
    p0 * c0 + p1 * c1 + p2 * c2 + p3 * c3
}
