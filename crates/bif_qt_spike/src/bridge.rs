// FFI bridge for the Phase 0 spike.
//
// C++ side owns QApplication / QMainWindow / signal wiring entirely
// (see cpp/app_main.cpp). Rust side owns the wgpu Renderer and
// exposes four callbacks C++ calls on Qt events:
//
//   spike_on_surface_ready(cb, hwnd, hinst, w, h) -> bool
//   spike_on_resize(cb, w, h)
//   spike_on_frame(cb)
//   spike_on_shutdown(cb)
//
// `SpikeCallbacks` is a Rust-opaque type (see cxx docs §"opaque Rust
// types") — C++ holds a raw pointer to it for the lifetime of the
// QApplication event loop.
//
// Using plain #[cxx::bridge] (not #[cxx_qt::bridge]) — cxx-qt is a
// Phase A concern when we introduce Rust-side QObjects. Phase 0 only
// needs Qt detection (qt-build-utils in build.rs) + cxx FFI.

#[cxx::bridge]
pub mod ffi {
    extern "Rust" {
        type SpikeCallbacks;

        fn spike_on_surface_ready(
            cb: &mut SpikeCallbacks,
            hwnd: u64,
            hinstance: u64,
            width: i32,
            height: i32,
        ) -> bool;

        fn spike_on_resize(cb: &mut SpikeCallbacks, width: i32, height: i32);
        fn spike_on_frame(cb: &mut SpikeCallbacks);
        fn spike_on_shutdown(cb: &mut SpikeCallbacks);
    }

    unsafe extern "C++" {
        include!("app_main.h");

        // Blocks until QApplication::exec returns.
        unsafe fn qt_spike_run(cb: *mut SpikeCallbacks) -> i32;
    }
}

use crate::renderer::Renderer;

/// Rust-side state handed to C++ as an opaque pointer.
/// C++ calls the `spike_on_*` free functions above with this as the
/// first argument.
pub struct SpikeCallbacks {
    renderer: Option<Renderer>,
}

impl SpikeCallbacks {
    pub fn new() -> Self {
        Self { renderer: None }
    }
}

impl Default for SpikeCallbacks {
    fn default() -> Self {
        Self::new()
    }
}

pub fn spike_on_surface_ready(
    cb: &mut SpikeCallbacks,
    hwnd: u64,
    hinstance: u64,
    width: i32,
    height: i32,
) -> bool {
    let w = width.max(1) as u32;
    let h = height.max(1) as u32;
    // SAFETY: hwnd is a Qt-native HWND for a widget that outlives us.
    match unsafe { Renderer::new(hwnd, hinstance, w, h) } {
        Ok(r) => {
            log::info!("spike: wgpu surface ready {w}x{h}");
            cb.renderer = Some(r);
            true
        }
        Err(e) => {
            log::error!("spike: wgpu surface init failed: {e:#}");
            false
        }
    }
}

pub fn spike_on_resize(cb: &mut SpikeCallbacks, width: i32, height: i32) {
    if let Some(r) = cb.renderer.as_mut() {
        r.resize(width.max(1) as u32, height.max(1) as u32);
    }
}

pub fn spike_on_frame(cb: &mut SpikeCallbacks) {
    if let Some(r) = cb.renderer.as_mut() {
        if let Err(e) = r.render() {
            log::error!("spike: render error: {e:#}");
        }
    }
}

pub fn spike_on_shutdown(cb: &mut SpikeCallbacks) {
    log::info!("spike: shutting down");
    cb.renderer = None;
}
