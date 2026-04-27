//! wgpu overlay for the selected prim translate gizmo.

use wgpu::util::DeviceExt;

use crate::gizmo::{axis_direction, project_to_screen, GizmoAxis, GizmoState};

const MAX_GIZMO_VERTICES: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct TransformGizmoVertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl TransformGizmoVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TransformGizmoVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub(crate) struct TransformGizmoRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
}

impl TransformGizmoRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        _camera_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Transform Gizmo Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/transform_gizmo.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Transform Gizmo Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Transform Gizmo Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[TransformGizmoVertex::desc()],
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
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let dummy_vertices = [TransformGizmoVertex {
            position: [0.0, 0.0, 0.0],
            color: [1.0, 1.0, 1.0],
        }; MAX_GIZMO_VERTICES];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Transform Gizmo Vertices"),
            contents: bytemuck::cast_slice(&dummy_vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            pipeline,
            vertex_buffer,
            vertex_count: 0,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.vertex_count = 0;
    }

    pub(crate) fn update(
        &mut self,
        queue: &wgpu::Queue,
        origin: bif_math::Vec3,
        camera: &bif_math::Camera,
        viewport_rect: (f32, f32, f32, f32),
        state: &GizmoState,
    ) {
        let camera_vec = camera.position - origin;
        let camera_dist = camera_vec.length().max(1.0);
        let axis_length = camera_dist * 0.14;
        let mut vertices = Vec::with_capacity(MAX_GIZMO_VERTICES);
        let view_proj = camera.view_projection_matrix();
        let Some(origin_screen) = project_to_screen(origin, &view_proj, viewport_rect) else {
            self.vertex_count = 0;
            return;
        };

        for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
            let Some(dir) = axis_direction(axis) else {
                continue;
            };
            let Some(end_screen) =
                project_to_screen(origin + dir * axis_length, &view_proj, viewport_rect)
            else {
                continue;
            };
            let color = axis_color(axis, state);
            let width = if state.active_axis == axis && state.is_dragging {
                10.0
            } else if state.hovered_axis == axis {
                8.0
            } else {
                6.0
            };
            push_screen_segment(
                &mut vertices,
                origin_screen,
                end_screen,
                viewport_rect,
                width,
                color,
            );
        }
        push_screen_square(
            &mut vertices,
            origin_screen,
            viewport_rect,
            7.0,
            [1.0, 0.95, 0.55],
        );

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        self.vertex_count = vertices.len() as u32;
    }

    pub(crate) fn render<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        _camera_bind_group: &'a wgpu::BindGroup,
    ) {
        if self.vertex_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}

fn push_screen_segment(
    out: &mut Vec<TransformGizmoVertex>,
    start: (f32, f32),
    end: (f32, f32),
    viewport_rect: (f32, f32, f32, f32),
    width_px: f32,
    color: [f32; 3],
) {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let nx = -dy / len * width_px * 0.5;
    let ny = dx / len * width_px * 0.5;
    let a = (start.0 - nx, start.1 - ny);
    let b = (start.0 + nx, start.1 + ny);
    let c = (end.0 + nx, end.1 + ny);
    let d = (end.0 - nx, end.1 - ny);
    push_tri(out, a, b, c, viewport_rect, color);
    push_tri(out, a, c, d, viewport_rect, color);
}

fn push_screen_square(
    out: &mut Vec<TransformGizmoVertex>,
    center: (f32, f32),
    viewport_rect: (f32, f32, f32, f32),
    half_size: f32,
    color: [f32; 3],
) {
    let a = (center.0 - half_size, center.1 - half_size);
    let b = (center.0 + half_size, center.1 - half_size);
    let c = (center.0 + half_size, center.1 + half_size);
    let d = (center.0 - half_size, center.1 + half_size);
    push_tri(out, a, b, c, viewport_rect, color);
    push_tri(out, a, c, d, viewport_rect, color);
}

fn push_tri(
    out: &mut Vec<TransformGizmoVertex>,
    a: (f32, f32),
    b: (f32, f32),
    c: (f32, f32),
    viewport_rect: (f32, f32, f32, f32),
    color: [f32; 3],
) {
    out.push(TransformGizmoVertex {
        position: screen_to_clip(a, viewport_rect),
        color,
    });
    out.push(TransformGizmoVertex {
        position: screen_to_clip(b, viewport_rect),
        color,
    });
    out.push(TransformGizmoVertex {
        position: screen_to_clip(c, viewport_rect),
        color,
    });
}

fn screen_to_clip(point: (f32, f32), viewport_rect: (f32, f32, f32, f32)) -> [f32; 3] {
    let (vp_x, vp_y, vp_w, vp_h) = viewport_rect;
    let x = ((point.0 - vp_x) / vp_w) * 2.0 - 1.0;
    let y = 1.0 - ((point.1 - vp_y) / vp_h) * 2.0;
    [x, y, 0.0]
}

fn axis_color(axis: GizmoAxis, state: &GizmoState) -> [f32; 3] {
    let base = match axis {
        GizmoAxis::X => [1.0, 0.18, 0.14],
        GizmoAxis::Y => [0.2, 0.95, 0.28],
        GizmoAxis::Z => [0.24, 0.48, 1.0],
        GizmoAxis::None => [1.0, 1.0, 1.0],
    };
    if state.active_axis == axis && state.is_dragging {
        [1.0, 0.95, 0.35]
    } else if state.hovered_axis == axis {
        [
            (base[0] + 0.25_f32).min(1.0_f32),
            (base[1] + 0.25_f32).min(1.0_f32),
            (base[2] + 0.25_f32).min(1.0_f32),
        ]
    } else {
        base
    }
}
