// First real #[cxx_qt::bridge] in BIF.
//
// Phase A scope: minimal cxx-qt bridge that compiles against Qt 6.8.3
// LTS + MSVC 2022 and is callable from C++ (which assembles the
// QMainWindow in cpp/window_builder.cpp). Subsequent phases expand
// this module with:
//   - panel models (QAbstractItemModel subclasses for scene browser,
//     layer stack, property inspector)
//   - invokable commands (open/save/frame/etc.)
//   - signals for AppEvent integration
//
// cxx-qt 0.7 convention: QT_NO_KEYWORDS is defined automatically so
// `slots` doesn't collide with Rust — C++ side uses Q_SIGNALS: /
// Q_EMIT / Q_SLOTS: instead of the unmacro'd forms.
//
// API note: in cxx-qt 0.7, `#[qinvokable]` functions are DECLARED
// inside `extern "RustQt"` and IMPLEMENTED in a regular impl block
// OUTSIDE the bridge module. Putting a non-empty impl block inside
// the bridge is a compile error.

use core::pin::Pin;
use cxx_qt::CxxQtType;

use crate::viewport::{
    viewport_on_frame, viewport_on_resize, viewport_on_shutdown, viewport_on_surface_ready,
    ViewportCallbacks,
};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "C++" {
        include!("window_builder.h");
        /// Blocks until QApplication::exec returns. Implemented in
        /// cpp/window_builder.cpp. Constructs QApplication +
        /// QMainWindow + BifShellState + RenderWidget internally
        /// and wires RenderWidget's signals to the viewport
        /// callbacks passed in. Applies `stylesheet` via
        /// QApplication::setStyleSheet at startup.
        ///
        /// # Safety
        ///
        /// Must be called exactly once, on the main thread, before
        /// any other Qt code runs in the process. The function takes
        /// over the main thread for the Qt event loop and does not
        /// return until the window is closed. `viewport_cb` must
        /// remain valid for the duration of the call.
        unsafe fn bif_qt_run_shell(viewport_cb: *mut ViewportCallbacks, stylesheet: &str) -> i32;
    }

    // Viewport callback plumbing — C++ RenderWidget signals call
    // these Rust functions, which forward to the wgpu Viewport
    // (see src/viewport.rs). `ViewportCallbacks` is resolved
    // against `super::ViewportCallbacks` — the parent module's
    // `use crate::viewport::ViewportCallbacks` below brings it
    // into scope.
    extern "Rust" {
        type ViewportCallbacks;

        fn viewport_on_surface_ready(
            cb: &mut ViewportCallbacks,
            hwnd: u64,
            hinstance: u64,
            width: i32,
            height: i32,
        ) -> bool;
        fn viewport_on_resize(cb: &mut ViewportCallbacks, width: i32, height: i32);
        fn viewport_on_frame(cb: &mut ViewportCallbacks);
        fn viewport_on_shutdown(cb: &mut ViewportCallbacks);
    }

    extern "RustQt" {
        /// Shell state QObject — window title, status message, and
        /// menu-action invokables. Phase B: stubs that update
        /// status_message. Phase C+ wires to real logic.
        #[qobject]
        #[qproperty(QString, title)]
        #[qproperty(QString, status_message)]
        #[qproperty(QString, current_workspace)]
        type BifShellState = super::BifShellStateRust;

        /// Smoke-test invokable — verifies Rust↔C++ round-trip.
        #[qinvokable]
        fn describe(self: Pin<&mut BifShellState>) -> QString;

        /// File/New Stage (Ctrl+N). Phase B stub.
        #[qinvokable]
        fn on_new_stage(self: Pin<&mut BifShellState>);

        /// File/Open Stage (Ctrl+O). Phase B stub.
        #[qinvokable]
        fn on_open_stage(self: Pin<&mut BifShellState>);

        /// File/Save (Ctrl+S). Phase B stub.
        #[qinvokable]
        fn on_save(self: Pin<&mut BifShellState>);

        /// File/Save As (Ctrl+Shift+S). Phase B stub.
        #[qinvokable]
        fn on_save_as(self: Pin<&mut BifShellState>);

        /// Help/About. Phase B stub.
        #[qinvokable]
        fn on_about(self: Pin<&mut BifShellState>);
    }
}

/// Rust-side storage for `BifShellState` QObject properties.
/// cxx-qt generates getters/setters for fields matching `#[qproperty]`
/// declarations above.
pub struct BifShellStateRust {
    pub title: cxx_qt_lib::QString,
    pub status_message: cxx_qt_lib::QString,
    /// Active workspace preset — one of "assembly", "lighting",
    /// "materials", "render". Empty on first launch (C++ side
    /// initializes from QSettings or falls back to "assembly").
    pub current_workspace: cxx_qt_lib::QString,
}

impl Default for BifShellStateRust {
    fn default() -> Self {
        Self {
            title: cxx_qt_lib::QString::from("BIF — USD Orchestration (Qt)"),
            status_message: cxx_qt_lib::QString::from("Ready."),
            current_workspace: cxx_qt_lib::QString::from(""),
        }
    }
}

impl qobject::BifShellState {
    /// Backing implementation of the `describe` invokable.
    fn describe(self: Pin<&mut Self>) -> cxx_qt_lib::QString {
        let rust = self.rust();
        cxx_qt_lib::QString::from(&format!(
            "BifShellState[title={}, status={}]",
            rust.title, rust.status_message
        ))
    }

    /// Phase B stub — logs, updates status. Phase C wires to
    /// Scene reset + new-stage creation via bif_core.
    fn on_new_stage(mut self: Pin<&mut Self>) {
        log::info!("action: File/New Stage");
        self.as_mut().set_status_message(cxx_qt_lib::QString::from(
            "New Stage — not yet implemented (v0.16)",
        ));
    }

    /// Phase B stub — Phase C wires to QFileDialog + UsdStage::Open
    /// via bif_core.
    fn on_open_stage(mut self: Pin<&mut Self>) {
        log::info!("action: File/Open Stage");
        self.as_mut().set_status_message(cxx_qt_lib::QString::from(
            "Open Stage — not yet implemented (Phase C)",
        ));
    }

    /// Phase B stub — Phase C (actually v0.16) wires to save logic.
    fn on_save(mut self: Pin<&mut Self>) {
        log::info!("action: File/Save");
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from("Save — deferred to v0.16"));
    }

    fn on_save_as(mut self: Pin<&mut Self>) {
        log::info!("action: File/Save As");
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from("Save As — deferred to v0.16"));
    }

    fn on_about(mut self: Pin<&mut Self>) {
        log::info!("action: Help/About");
        self.as_mut().set_status_message(cxx_qt_lib::QString::from(
            "BIF — USD Orchestration Tool — bif_qt v0.14.0 (Phase B shell)",
        ));
    }
}
