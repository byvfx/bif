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
use std::cell::Cell;

use bif_core::scene_layer_state::SceneLayerState;
use bif_core::usd::layer::{LayerInfo, LayerOffset, LayerStack, PayloadPolicy};
use bif_viewport::SceneManager;

use crate::viewport::{
    viewport_on_frame, viewport_on_resize, viewport_on_shutdown, viewport_on_surface_ready,
    Viewport, ViewportCallbacks,
};

// ---------------------------------------------------------------------------
// BifShellState ↔ ViewportCallbacks bridge (ADR-007).
//
// BifShellState invokables need to reach ViewportCallbacks::viewport to drive
// scene loads / camera ops / frame-selected / etc. The two Rust singletons
// live in disjoint scopes (app.rs runtime vs cxx-qt QObject lifecycle), so
// we bridge through a thread_local raw pointer installed at app startup.
//
// Safety invariant: ViewportCallbacks lives on app.rs's stack for the entire
// Qt event loop. install_viewport_callbacks is called before bif_qt_run_shell
// enters the event loop — no invokable can fire before install. Qt UI is
// single-threaded, so thread_local + Cell<*mut _> is sound.
//
// See ADR-007 in wiki/architecture/adr/ for the full rationale + alternatives.
// ---------------------------------------------------------------------------

thread_local! {
    /// Installed by `app.rs` before entering the Qt event loop. Read by
    /// `with_viewport_mut` from `BifShellState` invokables.
    static VIEWPORT_CALLBACKS: Cell<*mut ViewportCallbacks> =
        const { Cell::new(std::ptr::null_mut()) };
}

/// Install the `ViewportCallbacks` pointer for this thread. Must be called
/// once, on the main (Qt UI) thread, before `bif_qt_run_shell`. Pointer
/// must remain valid for the full duration of the Qt event loop.
///
/// # Safety
///
/// Caller guarantees `cb` points to a `ViewportCallbacks` that outlives the
/// Qt event loop (typically stack-allocated in `app.rs::run`).
pub unsafe fn install_viewport_callbacks(cb: *mut ViewportCallbacks) {
    VIEWPORT_CALLBACKS.with(|slot| slot.set(cb));
}

/// Run `f` with read access to the live `UsdStage` via
/// `SceneManager::usd_stage`. Returns `None` when no stage is loaded,
/// the viewport isn't ready, or the stage mutex is poisoned.
///
/// The `Arc` is cloned out of the viewport first so the `with_viewport_mut`
/// re-entrant guard doesn't hold across the stage lock.
fn with_stage<R>(f: impl FnOnce(&bif_core::usd::UsdStage) -> R) -> Option<R> {
    let stage_arc = with_viewport_mut(|vp| vp.renderer_mut().scene.usd_stage.clone()).flatten()?;
    let guard = stage_arc.lock().ok()?;
    Some(f(&guard))
}

/// Map a layer identifier to the palette color index used by the
/// Layer Stack panel (modulo 8). Returns -1 when the identifier isn't
/// present in the currently-loaded scene layer state.
fn color_index_for_layer(state: &BifShellStateRust, identifier: &str) -> i32 {
    let Some(scene) = state.scene_layer_state.as_ref() else {
        return -1;
    };
    scene
        .stack
        .layers
        .iter()
        .position(|l| l.identifier == identifier)
        .map(|i| (i as i32) % 8)
        .unwrap_or(-1)
}

/// Run `f` with mutable access to the live `Viewport`. Returns `None` when:
///   - `install_viewport_callbacks` was never called (pointer null), or
///   - the viewport hasn't been constructed yet (surfaceReady not fired), or
///   - the viewport has been torn down.
///
/// Callers must handle `None` gracefully.
fn with_viewport_mut<R>(f: impl FnOnce(&mut Viewport) -> R) -> Option<R> {
    VIEWPORT_CALLBACKS.with(|slot| {
        let ptr = slot.get();
        if ptr.is_null() {
            return None;
        }
        // SAFETY: Qt UI is single-threaded; pointer installed before the
        // event loop starts and valid for its full duration; no other site
        // holds a conflicting &mut to the same ViewportCallbacks concurrently
        // (viewport_on_* callbacks run via cxx-qt `extern "Rust"`, not
        // through this thread_local).
        let cb = unsafe { &mut *ptr };
        cb.viewport_mut().map(f)
    })
}

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
            scale_factor: f32,
        ) -> bool;
        fn viewport_on_resize(
            cb: &mut ViewportCallbacks,
            width: i32,
            height: i32,
            scale_factor: f32,
        );
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
        // Bumped on every stage load / close. SceneBrowserModel
        // listens to `scene_browser_revisionChanged` to
        // beginResetModel/endResetModel and rebuild from the live
        // UsdStage via prim-tree invokables.
        #[qproperty(i32, scene_browser_revision)]
        // Currently-selected prim path (driven by scene browser
        // click). Empty = no selection. Property inspector listens
        // to the auto-generated `selected_prim_pathChanged` signal.
        #[qproperty(QString, selected_prim_path)]
        #[qproperty(QString, selected_prim_type)]
        // Timeline state (Phase D.1 / E.1). Phase E.2 wires the
        // auto-detect from bif_core::TimelineState + USD metadata.
        #[qproperty(i32, current_frame)]
        #[qproperty(i32, start_frame)]
        #[qproperty(i32, end_frame)]
        #[qproperty(i32, playback_fps)]
        /// When true, playback paces to `playback_fps` (real clock).
        /// When false, playback advances as fast as possible — useful
        /// for non-realtime scenes where each frame's render cost
        /// dominates.
        #[qproperty(bool, realtime_playback)]
        /// When true, playback wraps back to `start_frame` at
        /// `end_frame`. When false, playback stops at `end_frame`.
        /// Future: upgrade to an enum (Repeat / Bounce / Stop /
        /// Continue) à la Nuke.
        #[qproperty(bool, loop_playback)]
        #[qproperty(bool, is_playing)]
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

        /// Record an opened stage path in the status bar + recents
        /// (QSettings-side). Phase E.2 parses + loads the stage;
        /// Phase E.1 records only.
        #[qinvokable]
        fn on_stage_path_opened(self: Pin<&mut BifShellState>, path: QString);

        /// File → Close Stage. Drains GPU, replaces the renderer's
        /// `SceneManager` with a fresh one, rebuilds the pick BVH,
        /// clears shell state (layer stack, selection, timeline path),
        /// and bumps `layer_state_revision` so panel models reset.
        /// C++ side swaps the central stack back to the first-launch
        /// screen (index 0).
        #[qinvokable]
        fn close_stage(self: Pin<&mut BifShellState>);

        /// Camera orbit delta forwarded from RenderWidget::cameraOrbit.
        /// Phase E.1 just updates status; Phase E.2 dispatches a
        /// real AppEvent::CameraOrbit to bif_renderer::Renderer.
        #[qinvokable]
        fn on_camera_orbit(self: Pin<&mut BifShellState>, dx: i32, dy: i32);

        /// Camera pan delta.
        #[qinvokable]
        fn on_camera_pan(self: Pin<&mut BifShellState>, dx: i32, dy: i32);

        /// Camera wheel zoom.
        #[qinvokable]
        fn on_camera_zoom(self: Pin<&mut BifShellState>, angle_delta: i32);

        /// Frame the currently-selected prim (F key). Phase E.2
        /// dispatches AppEvent::FrameSelected; Phase E.1 stub status.
        #[qinvokable]
        fn on_frame_selected(self: Pin<&mut BifShellState>);

        /// Viewport LMB click (unmodified). `x` and `y` are framebuffer
        /// (physical-pixel) coords — `RenderWidget::mousePressEvent`
        /// multiplies by `devicePixelRatioF()` before emitting. Ray-
        /// casts against the live pick BVH and sets
        /// `selected_prim_path` / `selected_prim_type` on hit, or
        /// clears them on miss. Uses the ADR-007 β bridge.
        #[qinvokable]
        fn on_prim_pick(self: Pin<&mut BifShellState>, x: i32, y: i32);

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

        // ---- Timeline state surface (Phase D.1) ----

        /// Toggle play/pause. Phase D.1 is UI-only; Phase E wires a
        /// QTimer that advances current_frame.
        #[qinvokable]
        fn toggle_playback(self: Pin<&mut BifShellState>);

        /// Step current frame by `delta`, clamped to [start_frame, end_frame].
        #[qinvokable]
        fn step_frame(self: Pin<&mut BifShellState>, delta: i32);

        /// Jump to the previous keyframe before `current_frame`, or
        /// clamp to `start_frame` if none. Stops playback.
        #[qinvokable]
        fn jump_to_prev_keyframe(self: Pin<&mut BifShellState>);

        /// Jump to the next keyframe after `current_frame`, or clamp
        /// to `end_frame` if none. Stops playback.
        #[qinvokable]
        fn jump_to_next_keyframe(self: Pin<&mut BifShellState>);

        /// Number of keyframes the demo timeline exposes.
        #[qinvokable]
        fn keyframe_count(self: &BifShellState) -> i32;

        /// Frame of the keyframe at `index`. -1 on OOB.
        #[qinvokable]
        fn keyframe_at(self: &BifShellState, index: i32) -> i32;

        /// Re-detect timeline range + fps from the currently-loaded
        /// USD stage's `startTimeCode` / `endTimeCode` /
        /// `timeCodesPerSecond`. Phase E.1 is a status-only stub;
        /// Phase E.2 does the real read after stage load lands.
        #[qinvokable]
        fn detect_timeline_from_stage(self: Pin<&mut BifShellState>);

        // ---- Scene Browser surface (Phase E.2 move 7) ----
        //
        // Invokables read the live UsdStage via the ADR-007 bridge +
        // `scene.usd_stage: Arc<Mutex<UsdStage>>`. SceneBrowserModel
        // walks the tree by calling these repeatedly, mirroring the
        // two-call `count` / `at(i)` pattern used by layer_stack.

        /// Top-level prim count (children of pseudo-root).
        #[qinvokable]
        fn root_prim_count(self: &BifShellState) -> i32;

        /// Path of root-level prim at `index`. Empty on OOB.
        #[qinvokable]
        fn root_prim_path_at(self: &BifShellState, index: i32) -> QString;

        /// Number of child prims under `parent_path`. 0 if path not
        /// found or stage unloaded.
        #[qinvokable]
        fn child_prim_count(self: &BifShellState, parent_path: QString) -> i32;

        /// Path of the `index`th child of `parent_path`. Empty on OOB.
        #[qinvokable]
        fn child_prim_path_at(self: &BifShellState, parent_path: QString, index: i32) -> QString;

        /// USD type name for the prim at `path` (e.g. "Mesh", "Xform").
        /// Empty if prim not found.
        #[qinvokable]
        fn prim_type_name_at(self: &BifShellState, path: QString) -> QString;

        /// Display (leaf) name for the prim at `path`.
        #[qinvokable]
        fn prim_display_name_at(self: &BifShellState, path: QString) -> QString;

        // ---- Property Inspector surface (Phase E.2 move 8) ----
        //
        // All read from the currently-selected prim (`selected_prim_path`)
        // via a fresh `UsdStage::get_prim_attributes` / `get_prim_stack`
        // call. C++ side rebuilds on `selected_prim_pathChanged`.

        /// Number of authored attributes on the selected prim.
        #[qinvokable]
        fn selected_prim_attribute_count(self: &BifShellState) -> i32;

        /// Attribute name at `index`.
        #[qinvokable]
        fn selected_prim_attribute_name_at(self: &BifShellState, index: i32) -> QString;

        /// Attribute typeName at `index`.
        #[qinvokable]
        fn selected_prim_attribute_type_at(self: &BifShellState, index: i32) -> QString;

        /// Stringified attribute value at `index`.
        #[qinvokable]
        fn selected_prim_attribute_value_at(self: &BifShellState, index: i32) -> QString;

        /// Number of entries in the selected prim's composition stack.
        #[qinvokable]
        fn selected_prim_stack_count(self: &BifShellState) -> i32;

        /// Layer identifier for stack entry at `index`.
        #[qinvokable]
        fn selected_prim_stack_layer_at(self: &BifShellState, index: i32) -> QString;

        /// Specifier ("def", "over", "class") for stack entry at `index`.
        #[qinvokable]
        fn selected_prim_stack_specifier_at(self: &BifShellState, index: i32) -> QString;

        /// Whether stack entry at `index` has authored opinions.
        #[qinvokable]
        fn selected_prim_stack_has_opinion_at(self: &BifShellState, index: i32) -> bool;

        /// Layer palette color index (mod 8) for stack entry at `index`.
        /// -1 on OOB or when the entry's layer isn't in the loaded stack.
        #[qinvokable]
        fn selected_prim_stack_color_index_at(self: &BifShellState, index: i32) -> i32;
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
    /// Monotonic counter bumped on stage load / close. Auto-emits
    /// `scene_browser_revisionChanged` for the Scene Browser panel.
    pub scene_browser_revision: i32,
    /// Layer stack + mute set + working layer + isolation flag.
    /// None until a stage is loaded (Phase E) or demo data seeded.
    pub scene_layer_state: Option<SceneLayerState>,
    /// Current prim selection — driven by scene browser clicks.
    pub selected_prim_path: cxx_qt_lib::QString,
    /// Type name of the selected prim (e.g. "Mesh", "Xform").
    pub selected_prim_type: cxx_qt_lib::QString,
    /// Timeline — current playhead frame.
    pub current_frame: i32,
    /// Timeline — inclusive start frame.
    pub start_frame: i32,
    /// Timeline — inclusive end frame.
    pub end_frame: i32,
    /// Timeline — frames-per-second used for the playback QTimer.
    /// Phase E.2 defaults to the loaded stage's `timeCodesPerSecond`;
    /// user can override via the timeline FPS spinbox.
    pub playback_fps: i32,
    /// Timeline — when true (default), the playback timer ticks at
    /// 1000/fps ms so playback runs at real wallclock speed. When
    /// false the timer ticks as fast as possible — useful for non-
    /// realtime scenes where rendering dominates frame time.
    pub realtime_playback: bool,
    /// Timeline — loop playback at end_frame. When false, playback
    /// stops. Future work: expand to an enum with Bounce/Continue.
    pub loop_playback: bool,
    /// Timeline — true when playback is active (Phase E wires the timer).
    pub is_playing: bool,
    /// Path of the stage the user last opened via File → Open. Stored
    /// without further interpretation — `detect_timeline_from_stage`
    /// opens a throwaway `UsdStage` from this path to read time
    /// metadata. `None` until a stage is opened.
    ///
    /// This is a stopgap for Phase E.2 move 4 (2026-04-15). Move 2
    /// will replace it with a proper stage handle shared between
    /// `BifShellState` and `ViewportCallbacks::viewport.renderer.scene`.
    pub current_stage_path: Option<std::path::PathBuf>,
}

impl Default for BifShellStateRust {
    fn default() -> Self {
        Self {
            title: cxx_qt_lib::QString::from("BIF — USD Orchestration (Qt)"),
            status_message: cxx_qt_lib::QString::from("Ready."),
            current_workspace: cxx_qt_lib::QString::from(""),
            layer_state_revision: 0,
            scene_browser_revision: 0,
            scene_layer_state: None,
            selected_prim_path: cxx_qt_lib::QString::from(""),
            selected_prim_type: cxx_qt_lib::QString::from(""),
            current_frame: 0,
            start_frame: 0,
            end_frame: 120,
            playback_fps: 24,
            realtime_playback: true,
            loop_playback: true,
            is_playing: false,
            current_stage_path: None,
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

    fn on_stage_path_opened(mut self: Pin<&mut Self>, path: cxx_qt_lib::QString) {
        let path_str: String = (&path).into();
        let path_buf = std::path::PathBuf::from(&path_str);
        log::info!("stage open requested: {path_str}");

        // Phase E.2 move 4: record the path for `detect_timeline_from_stage`.
        self.as_mut().rust_mut().current_stage_path = Some(path_buf.clone());

        // Phase E.2 move 2: drive the real scene load through the renderer
        // via the ADR-007 bridge. Synchronous — SceneManager::load_usd_scene
        // blocks on the C++ bridge + GPU buffer uploads. Matches bif_viewer's
        // startup-load behavior; acceptable for now. Async path is a future
        // optimization (scene_manager.rs already has `load_usd_scene_async`).
        let load_result = with_viewport_mut(|vp| vp.renderer_mut().load_usd_scene(&path_buf));

        match load_result {
            None => {
                log::warn!("stage open: viewport not ready yet (surface not created?)");
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!(
                        "Viewport not ready — cannot load {path_str}",
                    )));
            }
            Some(Err(e)) => {
                log::error!("stage load failed: {e:?}");
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!("Load failed: {e:?}",)));
            }
            Some(Ok(())) => {
                // Pull the freshly-populated layer state out of the renderer
                // and mirror it on self so the Layer Stack panel (and the
                // Phase C property inspector's composition-arcs mirror) see
                // real data on the next layer_state_revision bump.
                let layer_state_clone =
                    with_viewport_mut(|vp| vp.renderer_mut().scene.layer_state.clone()).flatten();
                self.as_mut().rust_mut().scene_layer_state = layer_state_clone;
                bump_revision(self.as_mut());
                bump_scene_browser_revision(self.as_mut());

                log::info!("stage loaded: {path_str}");
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!("Loaded: {path_str}",)));
            }
        }
    }

    fn close_stage(mut self: Pin<&mut Self>) {
        log::info!("action: File/Close Stage");

        // Drain GPU + reset renderer scene to a fresh SceneManager.
        // `wait_for_gpu` is mandatory before dropping textures/buffers
        // (same root cause as the D3D12 shutdown crash — commit d290c9d).
        let drained = with_viewport_mut(|vp| {
            vp.renderer_mut().wait_for_gpu();
            vp.renderer_mut().scene = SceneManager::new();
            vp.renderer_mut().rebuild_pick_scene();
        });

        if drained.is_none() {
            log::warn!("close_stage: viewport not ready — only clearing shell state");
        }

        // Clear shell state that mirrors scene/selection.
        {
            let mut r = self.as_mut().rust_mut();
            r.scene_layer_state = None;
            r.current_stage_path = None;
        }
        self.as_mut()
            .set_selected_prim_path(cxx_qt_lib::QString::from(""));
        self.as_mut()
            .set_selected_prim_type(cxx_qt_lib::QString::from(""));
        bump_revision(self.as_mut());
        bump_scene_browser_revision(self.as_mut());

        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from("Stage closed."));
    }

    fn on_camera_orbit(self: Pin<&mut Self>, dx: i32, dy: i32) {
        // Sensitivity matches bif_viewer's ORBIT_SENSITIVITY (0.005).
        const ORBIT_SENSITIVITY: f32 = 0.005;
        with_viewport_mut(|vp| {
            let r = vp.renderer_mut();
            r.cam
                .camera
                .orbit(dx as f32 * ORBIT_SENSITIVITY, dy as f32 * ORBIT_SENSITIVITY);
            r.update_camera();
        });
    }

    fn on_camera_pan(self: Pin<&mut Self>, dx: i32, dy: i32) {
        // Scale pan speed by camera-to-target distance so panning feels
        // proportional regardless of zoom level. Matches bif_viewer's
        // PAN_SENSITIVITY (0.1) + PAN_DISTANCE_SCALE (0.0001).
        const PAN_BASE: f32 = 0.01;
        with_viewport_mut(|vp| {
            let r = vp.renderer_mut();
            let dist = (r.cam.camera.position - r.cam.camera.target).length();
            let scale = PAN_BASE * dist.max(0.1);
            r.cam
                .camera
                .pan(-dx as f32 * scale, dy as f32 * scale, 0.0, 1.0);
            r.update_camera();
        });
    }

    fn on_camera_zoom(self: Pin<&mut Self>, angle_delta: i32) {
        // Qt angleDelta is in eighths-of-a-degree; 120 = one notch.
        // Convert to a dolly distance. Matches bif_viewer's scroll
        // handling (SCROLL_DOLLY_SCALE * lines * distance).
        const DOLLY_SCALE: f32 = 0.001;
        with_viewport_mut(|vp| {
            let r = vp.renderer_mut();
            let dist = (r.cam.camera.position - r.cam.camera.target).length();
            let dolly = angle_delta as f32 * DOLLY_SCALE * dist.max(0.1);
            r.cam.camera.dolly(dolly);
            r.update_camera();
        });
    }

    fn on_frame_selected(mut self: Pin<&mut Self>) {
        let path_qs = self.as_ref().rust().selected_prim_path.clone();
        let path: String = (&path_qs).into();
        let msg = if path.is_empty() {
            "Frame: no prim selected".to_string()
        } else {
            format!("Frame: {path}  (Phase E.2 wires bounds calc)")
        };
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&msg));
    }

    fn on_prim_pick(mut self: Pin<&mut Self>, x: i32, y: i32) {
        // Ray-cast into the live pick BVH. Pixel coords are already
        // framebuffer-space (DPR-multiplied in render_widget.cpp).
        let hit = with_viewport_mut(|vp| vp.renderer_mut().pick_instance_at(x as f32, y as f32))
            .flatten();

        let Some(idx) = hit else {
            // Miss — clear selection + status.
            self.as_mut()
                .set_selected_prim_path(cxx_qt_lib::QString::from(""));
            self.as_mut()
                .set_selected_prim_type(cxx_qt_lib::QString::from(""));
            self.as_mut()
                .set_status_message(cxx_qt_lib::QString::from("Selection cleared."));
            return;
        };

        // Look up the prim path on the hit instance. Strip the `/BIF/...`
        // synthetic prefix the USD loader applies to prototype-sourced
        // instances — user-facing paths should match stage authoring.
        let (path, type_name) = with_viewport_mut(|vp| {
            let raw = vp
                .renderer_mut()
                .scene
                .working_scene
                .instances()
                .get(idx)
                .map(|inst| (*inst.prim_path).to_string())?;
            let path = denormalize_synthetic_path(&raw);
            // Prim type: consult the usd_stage if available; otherwise
            // empty (keeps the property inspector resilient).
            let stage_arc = vp.renderer_mut().scene.usd_stage.clone();
            let type_name = stage_arc
                .and_then(|arc| {
                    arc.lock()
                        .ok()
                        .and_then(|stage| stage.get_prim_info_by_path(&path).ok())
                        .map(|info| info.type_name)
                })
                .unwrap_or_default();
            Some((path, type_name))
        })
        .flatten()
        .unwrap_or_default();

        log::info!("pick hit: idx={idx} path={path} type={type_name}");
        self.as_mut()
            .set_selected_prim_path(cxx_qt_lib::QString::from(&path));
        self.as_mut()
            .set_selected_prim_type(cxx_qt_lib::QString::from(&type_name));
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&format!("Selected: {path}")));
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

    // -----------------------------------------------------------------
    // Timeline surface (Phase D.1)
    // -----------------------------------------------------------------

    fn toggle_playback(mut self: Pin<&mut Self>) {
        let next = !self.as_ref().rust().is_playing;
        self.as_mut().set_is_playing(next);
        log::info!(
            "timeline playback → {}",
            if next { "play" } else { "pause" }
        );
    }

    fn step_frame(mut self: Pin<&mut Self>, delta: i32) {
        // Bind the Pin temp so it outlives the .rust() borrow.
        let pin_ref = self.as_ref();
        let r = pin_ref.rust();
        let cur = r.current_frame;
        let start = r.start_frame;
        let end = r.end_frame;
        let next = (cur + delta).clamp(start, end);
        self.as_mut().set_current_frame(next);
    }

    fn jump_to_prev_keyframe(mut self: Pin<&mut Self>) {
        let (target, was_playing) = {
            let pin_ref = self.as_ref();
            let r = pin_ref.rust();
            let cur = r.current_frame;
            let start = r.start_frame;
            let kfs = selected_prim_keyframes(&r.selected_prim_path);
            // Largest keyframe strictly less than current; fallback to start.
            let target = kfs
                .iter()
                .copied()
                .filter(|&k| k < cur)
                .max()
                .unwrap_or(start);
            (target, r.is_playing)
        };
        if was_playing {
            self.as_mut().set_is_playing(false);
        }
        self.as_mut().set_current_frame(target);
    }

    fn jump_to_next_keyframe(mut self: Pin<&mut Self>) {
        let (target, was_playing) = {
            let pin_ref = self.as_ref();
            let r = pin_ref.rust();
            let cur = r.current_frame;
            let end = r.end_frame;
            let kfs = selected_prim_keyframes(&r.selected_prim_path);
            // Smallest keyframe strictly greater than current; fallback to end.
            let target = kfs
                .iter()
                .copied()
                .filter(|&k| k > cur)
                .min()
                .unwrap_or(end);
            (target, r.is_playing)
        };
        if was_playing {
            self.as_mut().set_is_playing(false);
        }
        self.as_mut().set_current_frame(target);
    }

    fn keyframe_count(&self) -> i32 {
        selected_prim_keyframes(&self.rust().selected_prim_path).len() as i32
    }

    fn keyframe_at(&self, index: i32) -> i32 {
        selected_prim_keyframes(&self.rust().selected_prim_path)
            .get(index as usize)
            .copied()
            .unwrap_or(-1)
    }

    fn detect_timeline_from_stage(mut self: Pin<&mut Self>) {
        // Read timeline metadata from the live renderer's UsdStage
        // (loaded by on_stage_path_opened → Renderer::load_usd_scene).
        // Uses the ADR-007 bridge to reach the renderer's SceneManager.
        let timeline = with_viewport_mut(|vp| {
            let stage_arc = vp.renderer_mut().scene.usd_stage.as_ref()?;
            let stage = stage_arc.lock().ok()?;
            stage.get_timeline().ok()
        })
        .flatten();

        let Some(tl) = timeline else {
            log::info!("timeline: detect-from-stage — no stage loaded");
            self.as_mut().set_status_message(cxx_qt_lib::QString::from(
                "Detect timeline: no stage loaded (File \u{2192} Open first).",
            ));
            return;
        };

        if !tl.has_authored_time_range {
            log::info!(
                "timeline: no authored time range; using USD defaults \
                 (start={}, end={}, fps={})",
                tl.start_time_code,
                tl.end_time_code,
                tl.frames_per_second
            );
        }
        let start = tl.start_time_code.round() as i32;
        let end = tl.end_time_code.round() as i32;
        let fps = (tl.frames_per_second.round() as i32).max(1);
        self.as_mut().set_start_frame(start);
        self.as_mut().set_end_frame(end);
        self.as_mut().set_playback_fps(fps);
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&format!(
                "Timeline detected: {start}-{end} @ {fps} fps",
            )));
    }

    // -----------------------------------------------------------------
    // Scene Browser surface (Phase E.2 move 7)
    // -----------------------------------------------------------------

    fn root_prim_count(&self) -> i32 {
        with_stage(|stage| {
            use bif_viewport::scene_browser::PrimDataProvider;
            stage.root_paths().len() as i32
        })
        .unwrap_or(0)
    }

    fn root_prim_path_at(&self, index: i32) -> cxx_qt_lib::QString {
        let path = with_stage(|stage| {
            use bif_viewport::scene_browser::PrimDataProvider;
            stage
                .root_paths()
                .get(index as usize)
                .cloned()
                .unwrap_or_default()
        })
        .unwrap_or_default();
        cxx_qt_lib::QString::from(&path)
    }

    fn child_prim_count(&self, parent_path: cxx_qt_lib::QString) -> i32 {
        let p: String = (&parent_path).into();
        with_stage(|stage| {
            use bif_viewport::scene_browser::PrimDataProvider;
            stage.get_children(&p).len() as i32
        })
        .unwrap_or(0)
    }

    fn child_prim_path_at(
        &self,
        parent_path: cxx_qt_lib::QString,
        index: i32,
    ) -> cxx_qt_lib::QString {
        let parent: String = (&parent_path).into();
        let child = with_stage(|stage| {
            use bif_viewport::scene_browser::PrimDataProvider;
            stage
                .get_children(&parent)
                .get(index as usize)
                .cloned()
                .unwrap_or_default()
        })
        .unwrap_or_default();
        cxx_qt_lib::QString::from(&child)
    }

    fn prim_type_name_at(&self, path: cxx_qt_lib::QString) -> cxx_qt_lib::QString {
        let p: String = (&path).into();
        let type_name = with_stage(|stage| {
            stage
                .get_prim_info_by_path(&p)
                .map(|info| info.type_name)
                .unwrap_or_default()
        })
        .unwrap_or_default();
        cxx_qt_lib::QString::from(&type_name)
    }

    fn prim_display_name_at(&self, path: cxx_qt_lib::QString) -> cxx_qt_lib::QString {
        let p: String = (&path).into();
        // Leaf segment of the path is the USD prim name by convention.
        // Strips any numeric suffix safely — prim names don't collide
        // with path indices at the authoring level.
        let name = p.rsplit('/').next().unwrap_or("").to_string();
        cxx_qt_lib::QString::from(&name)
    }

    // -----------------------------------------------------------------
    // Property Inspector surface (Phase E.2 move 8)
    // -----------------------------------------------------------------

    fn selected_prim_attribute_count(&self) -> i32 {
        let path: String = (&self.rust().selected_prim_path).into();
        if path.is_empty() {
            return 0;
        }
        with_stage(|stage| {
            stage
                .get_prim_attributes(&path)
                .map(|v| v.len() as i32)
                .unwrap_or(0)
        })
        .unwrap_or(0)
    }

    fn selected_prim_attribute_name_at(&self, index: i32) -> cxx_qt_lib::QString {
        let path: String = (&self.rust().selected_prim_path).into();
        let name = with_stage(|stage| {
            stage
                .get_prim_attributes(&path)
                .ok()
                .and_then(|v| v.get(index as usize).map(|a| a.name.clone()))
                .unwrap_or_default()
        })
        .unwrap_or_default();
        cxx_qt_lib::QString::from(&name)
    }

    fn selected_prim_attribute_type_at(&self, index: i32) -> cxx_qt_lib::QString {
        let path: String = (&self.rust().selected_prim_path).into();
        let type_name = with_stage(|stage| {
            stage
                .get_prim_attributes(&path)
                .ok()
                .and_then(|v| v.get(index as usize).map(|a| a.type_name.clone()))
                .unwrap_or_default()
        })
        .unwrap_or_default();
        cxx_qt_lib::QString::from(&type_name)
    }

    fn selected_prim_attribute_value_at(&self, index: i32) -> cxx_qt_lib::QString {
        let path: String = (&self.rust().selected_prim_path).into();
        let value = with_stage(|stage| {
            stage
                .get_prim_attributes(&path)
                .ok()
                .and_then(|v| v.get(index as usize).map(|a| a.value.clone()))
                .unwrap_or_default()
        })
        .unwrap_or_default();
        cxx_qt_lib::QString::from(&value)
    }

    fn selected_prim_stack_count(&self) -> i32 {
        let path: String = (&self.rust().selected_prim_path).into();
        if path.is_empty() {
            return 0;
        }
        with_stage(|stage| {
            stage
                .get_prim_stack(&path)
                .map(|v| v.len() as i32)
                .unwrap_or(0)
        })
        .unwrap_or(0)
    }

    fn selected_prim_stack_layer_at(&self, index: i32) -> cxx_qt_lib::QString {
        let path: String = (&self.rust().selected_prim_path).into();
        let id = with_stage(|stage| {
            stage
                .get_prim_stack(&path)
                .ok()
                .and_then(|v| v.get(index as usize).map(|e| e.layer_identifier.clone()))
                .unwrap_or_default()
        })
        .unwrap_or_default();
        cxx_qt_lib::QString::from(&id)
    }

    fn selected_prim_stack_specifier_at(&self, index: i32) -> cxx_qt_lib::QString {
        use bif_core::usd::layer::PrimSpecifier;
        let path: String = (&self.rust().selected_prim_path).into();
        let spec = with_stage(|stage| {
            stage
                .get_prim_stack(&path)
                .ok()
                .and_then(|v| v.get(index as usize).map(|e| e.specifier))
        })
        .flatten();
        let label = match spec {
            Some(PrimSpecifier::Def) => "def",
            Some(PrimSpecifier::Over) => "over",
            Some(PrimSpecifier::Class) => "class",
            None => "",
        };
        cxx_qt_lib::QString::from(label)
    }

    fn selected_prim_stack_has_opinion_at(&self, index: i32) -> bool {
        let path: String = (&self.rust().selected_prim_path).into();
        with_stage(|stage| {
            stage
                .get_prim_stack(&path)
                .ok()
                .and_then(|v| v.get(index as usize).map(|e| e.has_authored_opinions))
                .unwrap_or(false)
        })
        .unwrap_or(false)
    }

    fn selected_prim_stack_color_index_at(&self, index: i32) -> i32 {
        let path: String = (&self.rust().selected_prim_path).into();
        let identifier = with_stage(|stage| {
            stage
                .get_prim_stack(&path)
                .ok()
                .and_then(|v| v.get(index as usize).map(|e| e.layer_identifier.clone()))
        })
        .flatten();
        match identifier {
            Some(id) => color_index_for_layer(self.rust(), &id),
            None => -1,
        }
    }
}

/// Increment `layer_state_revision` to trigger the cxx-qt-generated
/// `layer_state_revisionChanged` signal. C++ panel models listen for
/// this and refresh.
fn bump_revision(mut state: Pin<&mut qobject::BifShellState>) {
    let next = state.as_ref().rust().layer_state_revision.wrapping_add(1);
    state.as_mut().set_layer_state_revision(next);
}

/// Increment `scene_browser_revision` to trigger
/// `scene_browser_revisionChanged`. SceneBrowserModel listens for
/// this and calls `beginResetModel/endResetModel`.
fn bump_scene_browser_revision(mut state: Pin<&mut qobject::BifShellState>) {
    let next = state.as_ref().rust().scene_browser_revision.wrapping_add(1);
    state.as_mut().set_scene_browser_revision(next);
}

/// Convert a synthetic `/BIF/{real_path}/{idx}` instance path back to
/// the real path. Mirrors `bif_viewport::denormalize_synthetic_path`
/// (private to that crate). Kept local so user-facing selection paths
/// match stage-authored prim paths.
fn denormalize_synthetic_path(prim_path: &str) -> String {
    if let Some(stripped) = prim_path.strip_prefix("/BIF/") {
        if let Some(last_slash) = stripped.rfind('/') {
            let suffix = &stripped[last_slash + 1..];
            if suffix.parse::<usize>().is_ok() {
                return stripped[..last_slash].to_string();
            }
        }
        return stripped.to_string();
    }
    prim_path.to_string()
}

/// Return the selected prim's animation keyframe times (rounded to
/// integer frames), sorted ascending. Empty when no selection, no
/// matching instance, or no authored keyframes. Reads through the
/// ADR-007 β bridge via `with_viewport_mut`.
///
/// Path matching walks `scene.working_scene.instances()` looking for
/// an `Instance::prim_path` equal to (or synthetic-suffixed match of)
/// the selected path — handles the `/BIF/...` synthesis the USD loader
/// applies to prototype-sourced instances.
fn selected_prim_keyframes(selected: &cxx_qt_lib::QString) -> Vec<i32> {
    let target: String = selected.into();
    if target.is_empty() {
        return Vec::new();
    }
    with_viewport_mut(|vp| {
        let scene = &vp.renderer_mut().scene.working_scene;
        let instances = scene.instances();
        let animations = scene.instance_animations();
        // Find first instance whose prim_path matches the selection —
        // exact match first, then `/BIF/...` synthetic-path suffix.
        let idx = instances.iter().position(|inst| {
            let p: &str = &inst.prim_path;
            p == target
                || (p.starts_with("/BIF/")
                    && p.rsplit_once('/')
                        .map(|(_, leaf)| leaf == target.rsplit('/').next().unwrap_or(""))
                        .unwrap_or(false))
        });
        let Some(idx) = idx else { return Vec::new() };
        let anim = match animations.get(idx).and_then(|a| a.as_ref()) {
            Some(a) => a,
            None => return Vec::new(),
        };
        let Some(kfs) = anim.keyframes.as_ref() else {
            return Vec::new();
        };
        let mut times: Vec<i32> = kfs.iter().map(|k| k.time.round() as i32).collect();
        times.sort_unstable();
        times.dedup();
        times
    })
    .unwrap_or_default()
}
