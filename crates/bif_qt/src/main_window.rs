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
        /// QMainWindow + BifShellState internally.
        ///
        /// # Safety
        ///
        /// Must be called exactly once, on the main thread, before
        /// any other Qt code runs in the process. The function takes
        /// over the main thread for the Qt event loop and does not
        /// return until the window is closed.
        unsafe fn bif_qt_run_shell() -> i32;
    }

    extern "RustQt" {
        /// Phase A placeholder QObject — holds app-level shell state
        /// (window title, last-loaded stage path, status message).
        /// Phase B expands this into the real shell model.
        #[qobject]
        #[qproperty(QString, title)]
        #[qproperty(QString, status_message)]
        type BifShellState = super::BifShellStateRust;

        /// Phase A smoke-test invokable — verifies Rust↔C++ round-trip.
        #[qinvokable]
        fn describe(self: Pin<&mut BifShellState>) -> QString;
    }
}

/// Rust-side storage for `BifShellState` QObject properties.
/// cxx-qt generates getters/setters for fields matching `#[qproperty]`
/// declarations above.
pub struct BifShellStateRust {
    pub title: cxx_qt_lib::QString,
    pub status_message: cxx_qt_lib::QString,
}

impl Default for BifShellStateRust {
    fn default() -> Self {
        Self {
            title: cxx_qt_lib::QString::from("BIF — USD Orchestration (Qt)"),
            status_message: cxx_qt_lib::QString::from("Ready."),
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
}
