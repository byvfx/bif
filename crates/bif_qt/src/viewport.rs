// bif_qt viewport — hosts the real `bif_viewport::Renderer` inside a Qt
// `RenderWidget`. Phase E.2 move 1 (2026-04-15).
//
// Previously Phase B shipped a triangle demo here. Now that
// `bif_viewport::Renderer` is winit-free (Phase E.2-prep, 2026-04-15), we
// build the wgpu primitives from the Qt-native HWND and hand them to
// `Renderer::new(...)`. No egui is attached — the whole UI is Qt now.
//
// Lifecycle (driven by Qt signals from cpp/render_widget.cpp):
//   Viewport::new(hwnd, hinstance, w, h) — called from showEvent
//   Viewport::resize(w, h)                — resizeEvent
//   Viewport::render()                    — paintEvent / 16ms tick
//
// ViewportCallbacks wraps `Option<Viewport>` so construction can defer
// until the HWND is real (post-showEvent). C++ holds a raw pointer to this
// Rust-owned struct for the lifetime of the event loop.

use anyhow::{anyhow, Result};
use bif_viewport::Renderer;
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use std::num::NonZeroIsize;

/// Matches theme::BG_BASE (26,29,33) in linear sRGB. Used when the viewport
/// has nothing scene-ful to render.
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.010,
    g: 0.013,
    b: 0.017,
    a: 1.0,
};

pub struct Viewport {
    renderer: Renderer,
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
                required_features: bif_viewport::REQUIRED_FEATURES,
                required_limits: bif_viewport::required_limits(),
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

        // Phase E.2: default scale_factor to 1.0 until we wire
        // QScreen::devicePixelRatio() through the cxx-qt bridge. egui is not
        // attached — Qt owns the UI chrome.
        let renderer = Renderer::new(
            surface,
            device,
            queue,
            config,
            (width.max(1), height.max(1)),
            1.0,
        )?;

        // `adapter` and `instance` drop here; Renderer owns device/queue/surface.
        drop(adapter);
        drop(instance);

        Ok(Self { renderer })
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
        // scale_factor stays at 1.0 for now; hook through QScreen later.
        self.renderer
            .resize((width, height), self.renderer.scale_factor());
    }

    pub fn render(&mut self) -> Result<()> {
        // Headless path — no egui input, no PlatformOutput expected back.
        match self.renderer.render(CLEAR_COLOR, None) {
            Ok(_) => Ok(()),
            Err(e) => {
                // Recover from transient surface loss/outdated the same way
                // bif_viewer does: reconfigure at current size + scale.
                if let Some(surface_err) = e.downcast_ref::<wgpu::SurfaceError>() {
                    match surface_err {
                        wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
                            let size = self.renderer.size;
                            let scale = self.renderer.scale_factor();
                            self.renderer.resize(size, scale);
                            Ok(())
                        }
                        _ => Err(e),
                    }
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Access the underlying renderer (for scene-load invokables, etc).
    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }
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

    /// Mutable access to the live viewport (for scene-load invokables that
    /// need to reach into the renderer).
    pub fn viewport_mut(&mut self) -> Option<&mut Viewport> {
        self.viewport.as_mut()
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
