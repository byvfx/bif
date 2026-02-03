//! Lights management for viewport rendering.
//!
//! Extracted from Renderer to reduce monolithic struct size.

use wgpu::util::DeviceExt;

use crate::gpu_types::LightsUniform;

/// Manages viewport lighting state and GPU resources.
pub struct LightsManager {
    /// CPU-side uniform data
    pub uniform: LightsUniform,
    /// GPU buffer for lights uniform
    pub buffer: wgpu::Buffer,
    /// Bind group layout for lights
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group for lights
    pub bind_group: wgpu::BindGroup,
    /// Scene lights (from USD)
    pub scene_lights: Vec<bif_core::Light>,
}

impl LightsManager {
    /// Create a new LightsManager with default (empty) lights.
    pub fn new(device: &wgpu::Device) -> Self {
        let uniform = LightsUniform::new();

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Lights Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Lights Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Lights Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            uniform,
            buffer,
            bind_group_layout,
            bind_group,
            scene_lights: Vec::new(),
        }
    }

    /// Update lights from scene lights.
    pub fn update(&mut self, queue: &wgpu::Queue, lights: &[bif_core::Light]) {
        self.scene_lights = lights.to_vec();
        self.uniform = LightsUniform::from_scene_lights(lights);
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[self.uniform]));
    }

    /// Get the bind group layout for pipeline creation.
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }
}
