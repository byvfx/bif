// App runner — allocates ViewportCallbacks (Rust-owned wgpu
// renderer state), hands its raw pointer to C++, and invokes the
// QApplication::exec entry point.
//
// Lifetime: ViewportCallbacks sits on the stack for the entire
// QApplication lifetime. C++ uses the raw pointer in Qt signal
// lambdas (all on the main thread), never escapes it.

use anyhow::Result;

use crate::viewport::ViewportCallbacks;

/// Run the BIF Qt shell event loop. Returns the QApplication exit
/// code. Blocks the caller.
pub fn run() -> Result<i32> {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .try_init();

    log::info!("bif_qt {} — starting Qt shell", crate::BIF_QT_VERSION);

    let mut viewport_cb = ViewportCallbacks::new();
    let stylesheet = crate::theme::qt_stylesheet();

    // Install the ViewportCallbacks pointer for BifShellState invokables to
    // reach the live Renderer (ADR-007). SAFETY: `viewport_cb` stays on this
    // stack frame for the entire `bif_qt_run_shell` call — pointer valid
    // for the full Qt event loop. Install BEFORE entering the event loop
    // so no invokable can fire against a null pointer.
    // SAFETY: see ADR-007.
    unsafe {
        crate::main_window::install_viewport_callbacks(&mut viewport_cb as *mut ViewportCallbacks);
    }

    // SAFETY: bif_qt_run_shell blocks until QApplication::exec
    // returns. `&mut viewport_cb` stays live for the entire call.
    // `stylesheet` lives on the stack; rust::Str views into it for
    // the single QString::fromUtf8 copy at startup.
    let rc = unsafe {
        crate::main_window::qobject::bif_qt_run_shell(
            &mut viewport_cb as *mut ViewportCallbacks,
            &stylesheet,
        )
    };
    log::info!("bif_qt shell exited rc={rc}");
    Ok(rc)
}
