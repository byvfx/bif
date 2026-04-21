// Phase F: bif_viewer is now a thin shim over bif_qt::run().
//
// The Qt shell (bif_qt) is the primary UI as of v0.15. The previous
// winit + egui event loop, --usd / --usda CLI autoload, and per-event
// camera handling all moved into the Qt code path or are deferred.
//
// CLI autoload (--usd <path>) is currently NOT wired through bif_qt::run();
// open via File → Open Stage. Tracked in BUGLIST.

fn main() {
    // NB: do NOT call `env_logger::init()` here — `bif_qt::run()` runs
    // its own `try_init()` with a tuned filter
    // ("info,wgpu_core=warn,wgpu_hal=error,naga=warn"). A bare `init()`
    // here wins first and shadows that filter, so the shipping binary
    // would see wgpu log spam at 60 FPS.
    match bif_qt::run() {
        Ok(rc) => std::process::exit(rc),
        Err(e) => {
            eprintln!("bif_viewer failed: {e:#}");
            std::process::exit(1);
        }
    }
}
