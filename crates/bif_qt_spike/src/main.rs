// Phase 0 spike — wgpu into a Qt 6 QWidget.
//
// Launches a QMainWindow with an embedded RenderWidget whose HWND
// feeds wgpu::Surface. If you see a dark-grey background and a
// three-colored triangle that resizes with the window, the gate is
// PASSED and Phase A can proceed.
//
// See C:\Users\brandon\.claude\plans\iridescent-soaring-hamster.md
// for the gate criteria and fallback options.
//
// Run:
//   . .\setup_qt_env.ps1
//   cargo run -p bif_qt_spike

mod bridge;
mod renderer;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("bif_qt_spike starting — Phase 0 gate");

    let mut callbacks = bridge::SpikeCallbacks::new();
    // SAFETY: qt_spike_run blocks until QApplication::exec returns.
    // &mut callbacks stays live for the entire call. C++ only derefs
    // the pointer on the Qt main thread (same thread we're on).
    let rc = unsafe { bridge::ffi::qt_spike_run(&mut callbacks as *mut bridge::SpikeCallbacks) };
    log::info!("bif_qt_spike exiting rc={rc}");
    std::process::exit(rc);
}
