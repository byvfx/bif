// App runner — invokes the C++ `bif_qt_run_shell` entry point
// declared in the cxx-qt bridge at src/main_window.rs.
//
// Phase A keeps lifecycle C++-side (matching the spike) because
// cxx-qt-lib's QtWidgets coverage is thin. Phase B/C may promote
// QApplication construction into Rust once panel QObjects have
// absorbed enough Rust ownership that it makes sense.

use anyhow::Result;

/// Run the BIF Qt shell event loop. Returns the QApplication exit
/// code. Blocks the caller.
pub fn run() -> Result<i32> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();

    log::info!("bif_qt {} — starting Qt shell", crate::BIF_QT_VERSION);
    // SAFETY: QApplication + QMainWindow lifecycle is entirely C++-
    // managed. The function blocks until the event loop exits.
    let rc = unsafe { crate::main_window::qobject::bif_qt_run_shell() };
    log::info!("bif_qt shell exited rc={rc}");
    Ok(rc)
}
