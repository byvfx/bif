// bif_qt viewport — minimal wgpu renderer embedded in the RenderWidget.
//
// Phase B scope: same triangle the spike drew, now inside the
// persistent bif_qt_shell. This proves the Phase 0 embedding pattern
// works within the cxx-qt-built shell. Phase B.2+ will swap in
// bif_renderer::Renderer for real USD rendering once input + event
// wiring is in place.
//
// Lifecycle (driven by Qt signals from cpp/render_widget.cpp):
//   Viewport::new(hwnd, hinstance, w, h) — called from showEvent
//   Viewport::resize(w, h)                — resizeEvent
//   Viewport::render()                    — paintEvent / 16ms tick
//
// ViewportCallbacks wraps `Option<Viewport>` so construction can
// defer until the HWND is real (post-showEvent). C++ holds a raw
// pointer to this Rust-owned struct for the lifetime of the event
// loop.

use anyhow::{anyhow, Result};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use std::num::NonZeroIsize;

pub struct Viewport {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
}

impl Viewport {
    /// # Safety
    /// Caller must guarantee that `hwnd` is a valid HWND belonging to a
    /// Qt widget that outlives this Viewport.
    pub unsafe fn new(hwnd: u64, hinstance: u64, width: u32, height: u32) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let raw_window = Self::build_raw_window_handle(hwnd, hinstance)?;
        let raw_display = RawDisplayHandle::Windows(WindowsDisplayHandle::new());

        let surface = instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: raw_display,
            raw_window_handle: raw_window,
        })?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| anyhow!("no wgpu adapter compatible with Qt surface"))?;

        log::info!("bif_qt viewport adapter: {:?}", adapter.get_info());

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("bif_qt viewport device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let pipeline = build_triangle_pipeline(&device, format);

        Ok(Self {
            _instance: instance,
            surface,
            device,
            queue,
            config,
            pipeline,
        })
    }

    fn build_raw_window_handle(hwnd: u64, hinstance: u64) -> Result<RawWindowHandle> {
        let hwnd = NonZeroIsize::new(hwnd as isize)
            .ok_or_else(|| anyhow!("HWND was null — widget not yet shown?"))?;
        let mut handle = Win32WindowHandle::new(hwnd);
        handle.hinstance = NonZeroIsize::new(hinstance as isize);
        Ok(RawWindowHandle::Win32(handle))
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self) -> Result<()> {
        let frame = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("bif_qt viewport encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bif_qt viewport pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Matches theme::BG_BASE (26,29,33) in linear sRGB.
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.010,
                            g: 0.013,
                            b: 0.017,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }
}

fn build_triangle_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bif_qt viewport triangle shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/triangle.wgsl").into()),
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bif_qt viewport pipeline layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bif_qt viewport triangle pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
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
    })
}

// ---------------------------------------------------------------------------
// ViewportCallbacks — Rust-opaque struct shared with C++ via the
// cxx-qt bridge. Holds the live Viewport so Qt signals
// (surfaceReady, resized, frameRequested) can drive render lifecycle.
// ---------------------------------------------------------------------------

pub struct ViewportCallbacks {
    viewport: Option<Viewport>,
}

impl ViewportCallbacks {
    pub fn new() -> Self {
        Self { viewport: None }
    }
}

impl Default for ViewportCallbacks {
    fn default() -> Self {
        Self::new()
    }
}

pub fn viewport_on_surface_ready(
    cb: &mut ViewportCallbacks,
    hwnd: u64,
    hinstance: u64,
    width: i32,
    height: i32,
) -> bool {
    let w = width.max(1) as u32;
    let h = height.max(1) as u32;
    // SAFETY: hwnd is a Qt-native HWND for a widget that outlives us.
    match unsafe { Viewport::new(hwnd, hinstance, w, h) } {
        Ok(v) => {
            log::info!("bif_qt viewport live — {w}x{h}");
            cb.viewport = Some(v);
            true
        }
        Err(e) => {
            log::error!("bif_qt viewport init failed: {e:#}");
            false
        }
    }
}

pub fn viewport_on_resize(cb: &mut ViewportCallbacks, width: i32, height: i32) {
    if let Some(v) = cb.viewport.as_mut() {
        v.resize(width.max(1) as u32, height.max(1) as u32);
    }
}

pub fn viewport_on_frame(cb: &mut ViewportCallbacks) {
    if let Some(v) = cb.viewport.as_mut() {
        if let Err(e) = v.render() {
            log::error!("bif_qt viewport render error: {e:#}");
        }
    }
}

pub fn viewport_on_shutdown(cb: &mut ViewportCallbacks) {
    log::info!("bif_qt viewport shutting down");
    cb.viewport = None;
}
