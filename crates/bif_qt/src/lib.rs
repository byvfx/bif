// bif_qt — Qt 6 UI crate for BIF v0.15.0+.
//
// Clippy allow: cxx-qt expands `unsafe extern "C++"` declarations
// into functions the lint can't see the docs for. We write safety
// docs on the declarations regardless (main_window.rs) but need the
// allow at crate scope for the generated forms.
#![allow(clippy::missing_safety_doc)]

//
//
// Replaces bif_viewport's egui panel layer. Dependencies:
//   - bif_core       (UI-agnostic data: Scene, SceneLayerState, events)
//   - bif_renderer   (wgpu viewport rendering)
//   - cxx-qt / Qt 6  (widgets, signals/slots)
//
// Entry point: `bif_qt::run()` constructs QApplication + main window
// and runs the Qt event loop. Binaries (bif_viewer in Phase F, the
// temporary bif_qt_shell binary for Phase A/B dogfooding) call this.

pub mod app;
pub mod main_window;
pub mod theme;
pub mod viewport;

pub use app::run;

/// Crate version surfaced on the About dialog and log banner.
pub const BIF_QT_VERSION: &str = env!("CARGO_PKG_VERSION");
