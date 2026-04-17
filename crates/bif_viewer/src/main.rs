// Phase F: bif_viewer is now a thin shim over bif_qt::run().
//
// The Qt shell (bif_qt) is the primary UI as of v0.15. The previous
// winit + egui event loop, --usd / --usda CLI autoload, and per-event
// camera handling all moved into the Qt code path or are deferred.
//
// CLI autoload (--usd <path>) is currently NOT wired through bif_qt::run();
// open via File → Open Stage. Tracked in BUGLIST.

fn main() {
    env_logger::init();
    match bif_qt::run() {
        Ok(rc) => std::process::exit(rc),
        Err(e) => {
            eprintln!("bif_viewer failed: {e:#}");
            std::process::exit(1);
        }
    }
}
