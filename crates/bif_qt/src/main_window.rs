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

use bif_core::scene_layer_state::SceneLayerState;
use bif_core::usd::layer::{LayerInfo, LayerOffset, LayerStack, PayloadPolicy};

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
        // Bumped on every scene_layer_state mutation. C++ models
        // connect to the auto-generated `layer_state_revisionChanged`
        // signal to trigger a reset/refresh.
        #[qproperty(i32, layer_state_revision)]
        // Currently-selected prim path (driven by scene browser
        // click). Empty = no selection. Property inspector listens
        // to the auto-generated `selected_prim_pathChanged` signal.
        #[qproperty(QString, selected_prim_path)]
        #[qproperty(QString, selected_prim_type)]
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

        // ---- Layer Stack state surface (Phase C.1) ----
        //
        // Invokables read and mutate `BifShellStateRust::scene_layer_state`.
        // Mutators bump `layer_state_revision` which auto-emits a
        // `layer_state_revisionChanged` signal that panel models listen to.

        /// Seeds a 3-layer demo `SceneLayerState` (shot / anim / root
        /// mimicking `test_assets/layers/root.usda`) for Phase C.1
        /// visual testing until real USD loading lands in Phase E.
        #[qinvokable]
        fn seed_demo_layer_stack(self: Pin<&mut BifShellState>);

        /// Number of layers in the stack. 0 when no stage is loaded.
        #[qinvokable]
        fn layer_count(self: &BifShellState) -> i32;

        /// Display name of the layer at `index`. Empty on OOB.
        #[qinvokable]
        fn layer_name_at(self: &BifShellState, index: i32) -> QString;

        /// Full identifier (path or anonymous) of the layer at `index`.
        #[qinvokable]
        fn layer_identifier_at(self: &BifShellState, index: i32) -> QString;

        /// Depth in the sublayer tree (0 = root). Drives indent in the
        /// panel. -1 on OOB.
        #[qinvokable]
        fn layer_depth_at(self: &BifShellState, index: i32) -> i32;

        /// Palette index (mod 8) for the layer color dot. -1 on OOB.
        #[qinvokable]
        fn layer_color_index_at(self: &BifShellState, index: i32) -> i32;

        /// True if the layer at `index` is currently muted.
        #[qinvokable]
        fn layer_muted_at(self: &BifShellState, index: i32) -> bool;

        /// True if the layer at `index` is the current working layer.
        #[qinvokable]
        fn layer_is_working(self: &BifShellState, index: i32) -> bool;

        /// Global isolation-mode flag.
        #[qinvokable]
        fn isolation_mode_active(self: &BifShellState) -> bool;

        /// Toggle mute on the layer at `index` (Phase C.1: state-only;
        /// Phase E wires through `UsdStage::set_layer_muted` and
        /// geometry re-extraction). Bumps `layer_state_revision`.
        #[qinvokable]
        fn set_layer_muted(self: Pin<&mut BifShellState>, index: i32, muted: bool);

        /// Set the working layer (Phase C.1: state-only; Phase E wires
        /// to the edit target). Bumps `layer_state_revision`.
        #[qinvokable]
        fn set_working_layer(self: Pin<&mut BifShellState>, index: i32);

        /// Toggle isolation mode. Bumps `layer_state_revision`.
        #[qinvokable]
        fn toggle_isolation_mode(self: Pin<&mut BifShellState>);
    }
}

/// Rust-side storage for `BifShellState` QObject properties.
/// cxx-qt generates getters/setters for fields matching `#[qproperty]`
/// declarations above. Fields without `#[qproperty]` are plain Rust
/// state accessed via `self.rust() / rust_mut()` inside invokables.
pub struct BifShellStateRust {
    pub title: cxx_qt_lib::QString,
    pub status_message: cxx_qt_lib::QString,
    /// Active workspace preset — one of "assembly", "lighting",
    /// "materials", "render". Empty on first launch (C++ side
    /// initializes from QSettings or falls back to "assembly").
    pub current_workspace: cxx_qt_lib::QString,
    /// Monotonic counter bumped on every scene_layer_state mutation.
    /// Auto-emits `layer_state_revisionChanged` for panel models.
    pub layer_state_revision: i32,
    /// Layer stack + mute set + working layer + isolation flag.
    /// None until a stage is loaded (Phase E) or demo data seeded.
    pub scene_layer_state: Option<SceneLayerState>,
    /// Current prim selection — driven by scene browser clicks.
    pub selected_prim_path: cxx_qt_lib::QString,
    /// Type name of the selected prim (e.g. "Mesh", "Xform").
    pub selected_prim_type: cxx_qt_lib::QString,
}

impl Default for BifShellStateRust {
    fn default() -> Self {
        Self {
            title: cxx_qt_lib::QString::from("BIF — USD Orchestration (Qt)"),
            status_message: cxx_qt_lib::QString::from("Ready."),
            current_workspace: cxx_qt_lib::QString::from(""),
            layer_state_revision: 0,
            scene_layer_state: None,
            selected_prim_path: cxx_qt_lib::QString::from(""),
            selected_prim_type: cxx_qt_lib::QString::from(""),
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

    // -----------------------------------------------------------------
    // Layer Stack state surface (Phase C.1)
    // -----------------------------------------------------------------

    fn seed_demo_layer_stack(mut self: Pin<&mut Self>) {
        // Mimic `test_assets/layers/root.usda` — three layers with
        // shot overriding anim overriding root. Fake identifiers so
        // we don't need a real USD load for Phase C.1 visual testing.
        let layers = vec![
            LayerInfo {
                identifier: "G:/demo/root.usda".into(),
                display_name: "root.usda".into(),
                depth: 0,
                parent_index: None,
                is_muted: false,
                is_anonymous: false,
                is_dirty: false,
                real_path: std::path::PathBuf::new(),
                offset: LayerOffset::default(),
            },
            LayerInfo {
                identifier: "G:/demo/shot.usda".into(),
                display_name: "shot.usda".into(),
                depth: 1,
                parent_index: Some(0),
                is_muted: false,
                is_anonymous: false,
                is_dirty: false,
                real_path: std::path::PathBuf::new(),
                offset: LayerOffset::default(),
            },
            LayerInfo {
                identifier: "G:/demo/anim.usda".into(),
                display_name: "anim.usda".into(),
                depth: 1,
                parent_index: Some(0),
                is_muted: false,
                is_anonymous: false,
                is_dirty: false,
                real_path: std::path::PathBuf::new(),
                offset: LayerOffset::default(),
            },
        ];
        let stack = LayerStack {
            layers,
            root_index: 0,
        };
        let state = SceneLayerState {
            stack,
            working_layer: 0,
            muted: Default::default(),
            isolation_mode: false,
            payload_policy: PayloadPolicy::LoadAll,
            layer_for_prim: Default::default(),
        };
        self.as_mut().rust_mut().scene_layer_state = Some(state);
        bump_revision(self.as_mut());
        log::info!("seeded demo layer stack (3 layers)");
    }

    fn layer_count(&self) -> i32 {
        self.rust()
            .scene_layer_state
            .as_ref()
            .map(|s| s.stack.layers.len() as i32)
            .unwrap_or(0)
    }

    fn layer_name_at(&self, index: i32) -> cxx_qt_lib::QString {
        self.rust()
            .scene_layer_state
            .as_ref()
            .and_then(|s| s.stack.layers.get(index as usize))
            .map(|l| cxx_qt_lib::QString::from(l.display_name.as_str()))
            .unwrap_or_default()
    }

    fn layer_identifier_at(&self, index: i32) -> cxx_qt_lib::QString {
        self.rust()
            .scene_layer_state
            .as_ref()
            .and_then(|s| s.stack.layers.get(index as usize))
            .map(|l| cxx_qt_lib::QString::from(l.identifier.as_str()))
            .unwrap_or_default()
    }

    fn layer_depth_at(&self, index: i32) -> i32 {
        self.rust()
            .scene_layer_state
            .as_ref()
            .and_then(|s| s.stack.layers.get(index as usize))
            .map(|l| l.depth as i32)
            .unwrap_or(-1)
    }

    fn layer_color_index_at(&self, index: i32) -> i32 {
        if self.layer_count() == 0 || index < 0 || index >= self.layer_count() {
            -1
        } else {
            index % 8
        }
    }

    fn layer_muted_at(&self, index: i32) -> bool {
        self.rust()
            .scene_layer_state
            .as_ref()
            .and_then(|s| s.stack.layers.get(index as usize))
            .map(|l| l.is_muted)
            .unwrap_or(false)
    }

    fn layer_is_working(&self, index: i32) -> bool {
        self.rust()
            .scene_layer_state
            .as_ref()
            .map(|s| s.working_layer == index as usize)
            .unwrap_or(false)
    }

    fn isolation_mode_active(&self) -> bool {
        self.rust()
            .scene_layer_state
            .as_ref()
            .map(|s| s.isolation_mode)
            .unwrap_or(false)
    }

    fn set_layer_muted(mut self: Pin<&mut Self>, index: i32, muted: bool) {
        let identifier = {
            let Some(state) = self.rust().scene_layer_state.as_ref() else {
                return;
            };
            let Some(layer) = state.stack.layers.get(index as usize) else {
                return;
            };
            layer.identifier.clone()
        };
        if let Some(state) = self.as_mut().rust_mut().scene_layer_state.as_mut() {
            state.set_muted(&identifier, muted);
        }
        bump_revision(self.as_mut());
        log::info!("layer {index} muted={muted}");
    }

    fn set_working_layer(mut self: Pin<&mut Self>, index: i32) {
        if let Some(state) = self.as_mut().rust_mut().scene_layer_state.as_mut() {
            state.set_working_layer(index as usize);
        }
        bump_revision(self.as_mut());
        log::info!("working layer → {index}");
    }

    fn toggle_isolation_mode(mut self: Pin<&mut Self>) {
        if let Some(state) = self.as_mut().rust_mut().scene_layer_state.as_mut() {
            state.isolation_mode = !state.isolation_mode;
        }
        bump_revision(self.as_mut());
    }
}

/// Increment `layer_state_revision` to trigger the cxx-qt-generated
/// `layer_state_revisionChanged` signal. C++ panel models listen for
/// this and refresh.
fn bump_revision(mut state: Pin<&mut qobject::BifShellState>) {
    let next = state.as_ref().rust().layer_state_revision.wrapping_add(1);
    state.as_mut().set_layer_state_revision(next);
}
