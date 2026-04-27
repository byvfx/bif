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
use bif_core::usd::layer::{
    LayerInfo, LayerOffset, LayerStack, OpinionSource, PayloadPolicy, PrimStackEntry,
};

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

/// Run `f` with read access to a `CompositeProvider` that merges the live
/// USD stage with the procedural prim cache. This is the same provider the
/// egui scene browser uses, so children of procedural prims and synthetic
/// `/BIF/` paths are visible (T0.2 fix — Qt previously saw only the raw
/// USD stage and dropped procedural / synthetic descendants).
///
/// Returns `None` when the viewport isn't ready. When no stage is loaded,
/// returns a USD-less composite (procedural-only).
fn with_scene_browser_provider<R>(
    f: impl FnOnce(&dyn bif_viewport::scene_browser::PrimDataProvider) -> R,
) -> Option<R> {
    use bif_viewport::scene_browser::{CompositeProvider, PrimDataProvider};
    with_viewport_mut(|vp| {
        let renderer = vp.renderer_mut();
        // Clone the Arc first so the MutexGuard borrows from a local,
        // not from `renderer.scene.usd_stage`. That frees `renderer` for
        // the immutable `cached_scene_graph()` call below (method calls
        // can't be split-borrowed across struct fields).
        let stage_arc = renderer.scene.usd_stage.clone();
        let cache = renderer.cached_scene_graph();
        let stage_guard = stage_arc.as_ref().and_then(|s| s.lock().ok());
        let composite = CompositeProvider::new(
            stage_guard.as_deref().map(|s| s as &dyn PrimDataProvider),
            cache,
        );
        f(&composite)
    })
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

/// Pick only layers USD says can accept authored edits. Muted layers
/// are still excluded even if writable — the shell should not author
/// new opinions into a muted layer.
fn is_writable_layer(info: &bif_core::usd::LayerInfo) -> bool {
    info.permission_to_edit && !info.is_muted
}

const DEFAULT_OUTLINE_COLOR_HEX: &str = "#FFA600";

fn srgb_u8_to_linear(component: u8) -> f32 {
    let srgb = component as f32 / 255.0;
    if srgb <= 0.04045 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}

fn parse_outline_color_hex(hex: &str) -> Option<[f32; 4]> {
    let bytes = hex.strip_prefix('#')?;
    if bytes.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&bytes[0..2], 16).ok()?;
    let g = u8::from_str_radix(&bytes[2..4], 16).ok()?;
    let b = u8::from_str_radix(&bytes[4..6], 16).ok()?;
    Some([
        srgb_u8_to_linear(r),
        srgb_u8_to_linear(g),
        srgb_u8_to_linear(b),
        1.0,
    ])
}

fn payload_policy_to_name(policy: PayloadPolicy) -> &'static str {
    match policy {
        PayloadPolicy::LoadAll => "LoadAll",
        PayloadPolicy::LoadNone => "LoadNone",
    }
}

fn parse_payload_policy_name(name: &str) -> Option<PayloadPolicy> {
    match name {
        "LoadAll" => Some(PayloadPolicy::LoadAll),
        "LoadNone" => Some(PayloadPolicy::LoadNone),
        _ => None,
    }
}

fn escape_html(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn layer_palette_hex(index: i32) -> &'static str {
    match index {
        0 => "#50BEB4",
        1 => "#B478DC",
        2 => "#E69646",
        3 => "#DCBE50",
        4 => "#E682B4",
        5 => "#5A96E6",
        6 => "#78C864",
        7 => "#DC6464",
        _ => "#8C919B",
    }
}

fn format_attr_opinion_tooltip(
    attr_name: &str,
    opinions: &[OpinionSource],
    state: &BifShellStateRust,
) -> String {
    let escaped_name = escape_html(attr_name);
    if opinions.is_empty() {
        return format!(
            "<b>{escaped_name}</b><br/><span style=\"color:#8C919B;\">No authored opinions.</span>"
        );
    }

    let mut html = format!("<b>{escaped_name}</b><br/><br/>");
    for opinion in opinions {
        let color = layer_palette_hex(color_index_for_layer(state, &opinion.layer_identifier));
        let label = if opinion.is_winning {
            " <span style=\"color:#F0D67A;\">(winning)</span>"
        } else {
            ""
        };
        html.push_str(&format!(
            "<span style=\"color:{color};\">&#9679;</span> \
             <b>{layer}</b>{label}<br/>\
             <span style=\"padding-left:14px;\"><code>{value}</code> \
             <span style=\"color:#8C919B;\">{value_type}</span></span><br/><br/>",
            layer = escape_html(&opinion.layer_identifier),
            value = escape_html(&opinion.value_display),
            value_type = escape_html(&opinion.value_type),
        ));
    }
    html
}

/// Pick the strongest writable sublayer as the edit target. Walks the
/// flattened stack in natural (strength) order — `SceneLayerState::from_stage`
/// pushes the root first then sublayers depth-first, so index 0 is the
/// strongest opinion source. Returns `None` if nothing qualifies (the
/// caller should keep the previous working_layer).
fn pick_strongest_writable_sublayer(state: &bif_core::SceneLayerState) -> Option<usize> {
    state
        .stack
        .layers
        .iter()
        .enumerate()
        .find(|(_, info)| is_writable_layer(info))
        .map(|(idx, _)| idx)
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
        #[qproperty(bool, can_undo)]
        #[qproperty(bool, can_redo)]
        #[qproperty(f64, outline_width)]
        #[qproperty(QString, outline_color_hex)]
        #[qproperty(QString, ivar_status)]
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
        /// Bumped on stage load/close so the camera picker repopulates.
        #[qproperty(i32, camera_list_revision)]
        /// Viewport — when true (default), the built-in box-LOD system
        /// swaps full geometry for AABB boxes past the per-instance
        /// distance threshold. Bound to View → Toggle LOD. Mirrors
        /// `Renderer::display_settings.lod_enabled` after a successful
        /// toggle; viewport owns the behavior, qprop owns the UI bind.
        #[qproperty(bool, lod_enabled)]
        type BifShellState = super::BifShellStateRust;

        /// Smoke-test invokable — verifies Rust↔C++ round-trip.
        #[qinvokable]
        fn describe(self: Pin<&mut BifShellState>) -> QString;

        /// File/New Stage (Ctrl+N). Phase B stub.
        #[qinvokable]
        fn on_new_stage(self: Pin<&mut BifShellState>);

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

        /// Edit → Undo (Ctrl+Z). Mirrors `Viewport::undo_last_command`.
        #[qinvokable]
        fn on_undo(self: Pin<&mut BifShellState>);

        /// Edit → Redo (Ctrl+Shift+Z). Mirrors `Viewport::redo_last_command`.
        #[qinvokable]
        fn on_redo(self: Pin<&mut BifShellState>);

        /// Poll the live viewport undo stack and mirror it onto
        /// `can_undo` / `can_redo` for C++ action enable state.
        #[qinvokable]
        fn sync_undo_redo_state(self: Pin<&mut BifShellState>);

        /// Update selection-outline width and mirror it onto the live viewport.
        #[qinvokable]
        fn on_set_outline_width(self: Pin<&mut BifShellState>, width: f64);

        /// Update selection-outline color from a `#RRGGBB` string and mirror
        /// it onto the live viewport as linear RGBA.
        #[qinvokable]
        fn on_set_outline_color(self: Pin<&mut BifShellState>, color_hex: QString);

        /// Trigger an Ivar preview render from the Qt shell.
        #[qinvokable]
        fn on_start_ivar_render(self: Pin<&mut BifShellState>);

        /// Poll the live renderer's Ivar state and mirror it onto `ivar_status`.
        #[qinvokable]
        fn sync_ivar_status(self: Pin<&mut BifShellState>);

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

        /// Viewport mouse move / primary drag for the translate gizmo.
        #[qinvokable]
        fn on_transform_gizmo_move(self: Pin<&mut BifShellState>, x: i32, y: i32) -> bool;

        /// Viewport primary-button release for translate gizmo commit.
        #[qinvokable]
        fn on_transform_gizmo_release(self: Pin<&mut BifShellState>, x: i32, y: i32) -> bool;

        /// Scene-browser tree selection change. Routes through the
        /// renderer so viewport gizmo + outline highlight sync to the
        /// clicked row. Also mirrors path/type into shell qprops so the
        /// property inspector (bound to `selected_prim_pathChanged`)
        /// updates in the same invocation.
        #[qinvokable]
        fn on_tree_prim_selected(self: Pin<&mut BifShellState>, path: QString, type_name: QString);

        /// View → Toggle LOD. Updates the `lod_enabled` qprop and
        /// mirrors the new value onto `Renderer::display_settings.lod_enabled`
        /// so the next culling pass honors it. Default-on matches the
        /// pre-Qt egui behavior; artists can disable when the box-LOD
        /// swap is confusing selection or debugging geometry.
        #[qinvokable]
        fn on_set_lod_enabled(self: Pin<&mut BifShellState>, enabled: bool);

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

        /// Whether a real USD stage is currently loaded.
        #[qinvokable]
        fn has_loaded_stage(self: &BifShellState) -> bool;

        /// Current payload-policy mode as `LoadAll` or `LoadNone`.
        #[qinvokable]
        fn payload_policy_name(self: &BifShellState) -> QString;

        /// Update payload loading policy. Reloads the current stage when one
        /// is open so workspace switches take effect immediately.
        #[qinvokable]
        fn on_set_payload_policy(self: Pin<&mut BifShellState>, policy_name: QString) -> bool;

        /// Tier 1 edit-target surface. All 4 read `scene_layer_state
        /// .working_layer`; pill / status chip / viewport edge tint /
        /// breadcrumb layer segment all refresh on
        /// `layer_state_revisionChanged`.

        /// `true` when a stage is loaded and an edit target is set.
        #[qinvokable]
        fn active_edit_target_is_set(self: &BifShellState) -> bool;

        /// Display name of the active edit-target layer (the `working_layer`).
        /// Empty when no stage is loaded.
        #[qinvokable]
        fn active_edit_target_name(self: &BifShellState) -> QString;

        /// Full identifier of the active edit-target layer (file path or
        /// anonymous layer tag). Empty when no stage is loaded.
        #[qinvokable]
        fn active_edit_target_identifier(self: &BifShellState) -> QString;

        /// Palette index (mod 8) for the active edit-target layer's
        /// color dot. -1 when no stage is loaded.
        #[qinvokable]
        fn active_edit_target_color_index(self: &BifShellState) -> i32;

        /// Display string for the currently-loaded stage file (leaf
        /// name + parent folder if short). Empty when no stage loaded.
        /// Breadcrumb + title-bar surfaces read this.
        #[qinvokable]
        fn current_stage_display(self: &BifShellState) -> QString;

        /// Friendly label for a USD attribute name (`xformOp:translate`
        /// → `"Position"`). Unknown names pass through unchanged.
        /// Property inspector consumes this as the primary column text
        /// with the raw USD name surfaced in the tooltip.
        #[qinvokable]
        fn friendly_attribute_name(self: &BifShellState, raw: QString) -> QString;

        /// Friendly label for a USD prim type name (`Xform` →
        /// `"Transform"`, `PointInstancer` → `"Point Instancer"`).
        /// Unknown names pass through unchanged.
        #[qinvokable]
        fn friendly_prim_type(self: &BifShellState, raw: QString) -> QString;

        /// Compose the main-window title from stage + edit target +
        /// dirty state. Format: `BIF — stage.usda[*] · Editing: layer`.
        /// Refreshed on `layer_state_revisionChanged` + stage load.
        #[qinvokable]
        fn compose_title(self: &BifShellState) -> QString;

        // ---- Timeline state surface (Phase D.1) ----

        /// Toggle play/pause. Phase D.1 is UI-only; Phase E wires a
        /// QTimer that advances current_frame.
        #[qinvokable]
        fn toggle_playback(self: Pin<&mut BifShellState>);

        /// Push `frame` through to the renderer's animation evaluation
        /// path (`Renderer::set_time`) so `AnimatedTransform` /
        /// vertex-animation / skinning channels update the next paint.
        /// Connected from `current_frameChanged` in the Qt window so
        /// both QTimer-driven playback and scrubber drags propagate.
        #[qinvokable]
        fn on_frame_changed(self: Pin<&mut BifShellState>, frame: i32);

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

        /// Kind metadata (e.g. "component", "assembly", "group") for the
        /// prim at `path`. Empty when unset or stage unloaded. (Same TODO
        /// as the egui browser — composed-stage kind isn't surfaced from
        /// the C++ bridge yet.)
        #[qinvokable]
        fn prim_kind_at(self: &BifShellState, path: QString) -> QString;

        /// Computed visibility (inherited) for the prim at `path`.
        /// `true` when prim not found so empty trees aren't all-hidden.
        #[qinvokable]
        fn prim_is_visible_at(self: &BifShellState, path: QString) -> bool;

        /// Active flag for the prim at `path`. Inactive prims still
        /// appear in the tree (dimmed) per egui parity.
        #[qinvokable]
        fn prim_is_active_at(self: &BifShellState, path: QString) -> bool;

        /// Palette index (mod 8) for the layer dot next to the prim
        /// in the scene browser. Sources from `SceneLayerState::
        /// layer_for_prim` (prim_path → strongest opinion source
        /// layer). Returns -1 when no mapping exists (procedural
        /// prim with no USD layer, or stage unloaded).
        #[qinvokable]
        fn prim_color_index_at(self: &BifShellState, path: QString) -> i32;

        // ---- Property Inspector surface (Phase E.2 move 8) ----
        //
        // All read from the currently-selected prim (`selected_prim_path`).
        // Attribute rows query live stage data; prim-stack rows use a cache
        // mirrored on selection changes and layer-state revisions so the C++
        // property inspector doesn't trigger O(N*M) stack walks on rebuild.

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

        /// Palette color index (mod 8) for the winning opinion on attribute
        /// `attr_index` of the selected prim. -1 when unresolvable.
        #[qinvokable]
        fn selected_prim_attr_color_index_at(self: &BifShellState, attr_index: i32) -> i32;

        /// Rich-HTML tooltip enumerating the full opinion stack for the selected
        /// prim attribute at `attr_index`.
        #[qinvokable]
        fn selected_prim_attr_tooltip_at(self: &BifShellState, attr_index: i32) -> QString;

        // ---- Camera picker surface ----

        /// Number of UsdGeomCamera prims in the loaded stage. 0 when no stage.
        #[qinvokable]
        fn usd_camera_count(self: &BifShellState) -> i32;

        /// Full prim path of USD camera at `index`. Empty on OOB.
        #[qinvokable]
        fn usd_camera_path_at(self: &BifShellState, index: i32) -> QString;

        /// Active camera source key: "free" | "ortho:Top" | "usd:/path".
        #[qinvokable]
        fn active_camera_name(self: &BifShellState) -> QString;

        /// Switch camera. `source` is "free", "ortho:<Preset>", or "usd:<path>".
        #[qinvokable]
        fn on_select_camera(self: Pin<&mut BifShellState>, source: QString);

        /// Author a working-layer visibility opinion for `path`.
        /// Routes through `Renderer::dispatch_visibility` → C4a
        /// `EditOperation::Visibility` so the toggle is one undo step
        /// and saves cleanly through Ctrl+S. C4b-Carry-1.
        #[qinvokable]
        fn on_set_visibility(self: Pin<&mut BifShellState>, path: QString, visible: bool);

        // ---- Material Sheet surface (C4b-1) ----
        //
        // The Material Sheet C++ panel queries cached input rows from
        // these invokables (count + per-row name/type/value) and
        // dispatches edits via `on_set_material_param` /
        // `on_bind_material`. Cache refreshes on selection change.

        /// Number of shader inputs on the bound material's surface
        /// shader for the selected prim. 0 when nothing bound.
        #[qinvokable]
        fn selected_prim_material_input_count(self: &BifShellState) -> i32;

        /// Input name at `index` (e.g. "base_color").
        #[qinvokable]
        fn selected_prim_material_input_name_at(self: &BifShellState, index: i32) -> QString;

        /// USD type token at `index` (e.g. "float", "color3f").
        #[qinvokable]
        fn selected_prim_material_input_type_at(self: &BifShellState, index: i32) -> QString;

        /// Stringified current value at `index`.
        #[qinvokable]
        fn selected_prim_material_input_value_at(self: &BifShellState, index: i32) -> QString;

        /// Surface shader prim path for the bound material on the
        /// selected prim. Empty when no binding.
        #[qinvokable]
        fn selected_prim_material_shader_path(self: &BifShellState) -> QString;

        /// Author a working-layer shader-input override on the
        /// surface shader of the bound material. `value_str` is
        /// type-encoded ("0.5", "1,0,0" for color3f, "true"/"false"
        /// for bool). Records one undo step. C4b-1.
        #[qinvokable]
        fn on_set_material_param(
            self: Pin<&mut BifShellState>,
            input_name: QString,
            type_name: QString,
            value_str: QString,
        );

        /// Bind material `material_path` to the selected prim on the
        /// working layer. C4b-1.
        #[qinvokable]
        fn on_bind_material(self: Pin<&mut BifShellState>, material_path: QString);
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
    /// "materials", "review". Empty on first launch (C++ side
    /// initializes from QSettings or falls back to "assembly").
    pub current_workspace: cxx_qt_lib::QString,
    /// Edit menu enable state — mirrored from the live viewport undo stack.
    pub can_undo: bool,
    /// Edit menu enable state — mirrored from the live viewport redo stack.
    pub can_redo: bool,
    /// Selection-outline width mirrored onto `Renderer::display_settings`.
    pub outline_width: f64,
    /// Outline color as an sRGB `#RRGGBB` string for Qt controls.
    pub outline_color_hex: cxx_qt_lib::QString,
    /// Live Ivar progress/status string for Render Settings + status bar.
    pub ivar_status: cxx_qt_lib::QString,
    /// Monotonic counter bumped on every scene_layer_state mutation.
    /// Auto-emits `layer_state_revisionChanged` for panel models.
    pub layer_state_revision: i32,
    /// Monotonic counter bumped on stage load / close. Auto-emits
    /// `scene_browser_revisionChanged` for the Scene Browser panel.
    pub scene_browser_revision: i32,
    /// Layer stack + mute set + working layer + isolation flag.
    /// None until a stage is loaded (Phase E) or demo data seeded.
    pub scene_layer_state: Option<SceneLayerState>,
    /// Workspace-driven payload policy used for the next stage load/reload.
    pub payload_policy: PayloadPolicy,
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
    /// Monotonic counter bumped on stage load/close so the camera picker
    /// QComboBox knows to repopulate its USD camera entries.
    pub camera_list_revision: i32,
    /// UsdGeomCamera prim paths cached from the last successful stage load.
    pub usd_camera_paths: Vec<String>,
    /// Active camera source key: "free" | "ortho:<Preset>" | "usd:<path>".
    pub active_camera_source: String,
    /// Viewport — built-in box-LOD toggle. Default `true` matches
    /// `DisplaySettings::default()` in bif_viewport. Mirrored onto
    /// `Renderer::display_settings.lod_enabled` by `on_set_lod_enabled`.
    pub lod_enabled: bool,
    /// Cached prim-stack snapshot for the property inspector's composition arcs.
    /// Refreshed on selected-prim changes and layer-state revision bumps.
    pub selected_prim_stack_cache: Vec<PrimStackEntry>,
    /// Cached bound-material shader inputs for the Material Sheet tab.
    /// Refreshed on selection / layer revision bumps. C4b-1.
    pub selected_material_inputs_cache: Vec<bif_core::usd::BoundMaterialInput>,
    /// Surface shader prim path for the bound material on the selected
    /// prim. Empty when nothing bound. C4b-1.
    pub selected_material_shader_path: String,
}

impl Default for BifShellStateRust {
    fn default() -> Self {
        Self {
            title: cxx_qt_lib::QString::from("BIF — USD Orchestration (Qt)"),
            status_message: cxx_qt_lib::QString::from("Ready."),
            current_workspace: cxx_qt_lib::QString::from(""),
            can_undo: false,
            can_redo: false,
            outline_width: 0.004,
            outline_color_hex: cxx_qt_lib::QString::from(DEFAULT_OUTLINE_COLOR_HEX),
            ivar_status: cxx_qt_lib::QString::from(""),
            layer_state_revision: 0,
            scene_browser_revision: 0,
            scene_layer_state: None,
            payload_policy: PayloadPolicy::LoadAll,
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
            camera_list_revision: 0,
            usd_camera_paths: Vec::new(),
            active_camera_source: "free".to_string(),
            lod_enabled: true,
            selected_prim_stack_cache: Vec::new(),
            selected_material_inputs_cache: Vec::new(),
            selected_material_shader_path: String::new(),
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

    /// Phase B stub — Phase C (actually v0.16) wires to save logic.
    /// Tier 1 item #4: gives clearer feedback about why nothing
    /// happened and flags the no-edit-target case explicitly.
    fn on_save(mut self: Pin<&mut Self>) {
        log::info!("action: File/Save");
        let Some(layer_id) = self
            .as_ref()
            .rust()
            .scene_layer_state
            .as_ref()
            .and_then(|s| s.stack.layers.get(s.working_layer))
            .map(|l| l.identifier.clone())
        else {
            self.as_mut().set_status_message(cxx_qt_lib::QString::from(
                "Save: no stage loaded — open a stage first",
            ));
            return;
        };

        let msg = match with_stage(|stage| stage.save_layer(&layer_id)) {
            Some(Ok(())) => {
                if let Some(state) = self.as_mut().rust_mut().scene_layer_state.as_mut() {
                    state.mark_working_layer_dirty(false);
                }
                with_viewport_mut(|vp| {
                    if let Some(state) = vp.renderer_mut().scene.layer_state.as_mut() {
                        state.mark_working_layer_dirty(false);
                    }
                });
                bump_revision(self.as_mut());
                format!("Saved {layer_id}")
            }
            Some(Err(e)) => format!("Save failed: {e}"),
            None => "Save failed: viewport not ready".to_string(),
        };
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&msg));
    }

    fn on_save_as(mut self: Pin<&mut Self>) {
        log::info!("action: File/Save As");
        let msg = if !self.as_ref().active_edit_target_is_set() {
            "Save As: no stage loaded — open a stage first"
        } else {
            "Save As: write path not yet wired (v0.16)"
        };
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(msg));
    }

    /// Tier 1 item #4: compose the window title from stage path +
    /// edit-target + dirty state. Format:
    ///   `BIF — <stage.usda>[*] · Editing: <layer>`
    /// Dirty asterisk is wired via `LayerInfo::is_dirty` which flips
    /// only on real USD writes — latent until v0.16 save lands.
    fn compose_title(&self) -> cxx_qt_lib::QString {
        let stage = self
            .rust()
            .current_stage_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned());
        let (edit_target, dirty) = self
            .rust()
            .scene_layer_state
            .as_ref()
            .map(|s| {
                let dirty = s.stack.layers.iter().any(|l| l.is_dirty);
                let target = s
                    .stack
                    .layers
                    .get(s.working_layer)
                    .map(|l| l.display_name.clone())
                    .unwrap_or_default();
                (target, dirty)
            })
            .unwrap_or((String::new(), false));

        let title = match stage {
            None => "BIF — No stage".to_string(),
            Some(name) => {
                let asterisk = if dirty { "*" } else { "" };
                if edit_target.is_empty() {
                    format!("BIF — {name}{asterisk}")
                } else {
                    format!("BIF — {name}{asterisk} · Editing: {edit_target}")
                }
            }
        };
        cxx_qt_lib::QString::from(&title)
    }

    fn on_about(mut self: Pin<&mut Self>) {
        log::info!("action: Help/About");
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&format!(
                "BIF — USD Orchestration Tool — bif_qt {}",
                crate::BIF_QT_VERSION,
            )));
    }

    fn on_stage_path_opened(mut self: Pin<&mut Self>, path: cxx_qt_lib::QString) {
        let path_str: String = (&path).into();
        let path_buf = std::path::PathBuf::from(&path_str);
        log::info!("stage open requested: {path_str}");

        // Stop any playback carried over from the prior stage. Leaving
        // `is_playing=true` would start the timer immediately against a
        // `current_frame` that belongs to the previous stage's range;
        // `detect_timeline_from_stage` will clamp the frame afterwards.
        self.as_mut().set_is_playing(false);

        // 2026-04-17 fix: when a stage is already loaded, prims from the
        // previous load lingered visually (viewport) and in the scene
        // browser tree (procedural cache survived the SceneManager swap)
        // because `load_usd_scene` doesn't fully evict prior state.
        // `Renderer::reset_scene_state` is the single source of truth for
        // "evict the previous stage entirely" — clears both halves of
        // the CompositeProvider source (USD stage + node-graph cache).
        // Skipped on first open (nothing to drain yet).
        let had_prior_stage = self.as_ref().rust().current_stage_path.is_some();
        if had_prior_stage {
            with_viewport_mut(|vp| vp.renderer_mut().reset_scene_state());
            // Clear shell-state mirrors so the new load doesn't see stale
            // `scene_layer_state` / `selected_prim_*` from the prior stage.
            {
                let mut r = self.as_mut().rust_mut();
                r.scene_layer_state = None;
            }
            self.as_mut()
                .set_selected_prim_path(cxx_qt_lib::QString::from(""));
            self.as_mut()
                .set_selected_prim_type(cxx_qt_lib::QString::from(""));
            refresh_selected_prim_stack_cache(self.as_mut());
            // Bump scene browser revision so the model rebuilds against the
            // empty state before the new stage's data lands — prevents a
            // brief frame where old + new prims merge in the tree.
            bump_scene_browser_revision(self.as_mut());
            bump_revision(self.as_mut());
        }

        // Phase E.2 move 4: record the path for `detect_timeline_from_stage`.
        self.as_mut().rust_mut().current_stage_path = Some(path_buf.clone());

        // Phase E.2 move 2: drive the real scene load through the renderer
        // via the ADR-007 bridge. Synchronous — SceneManager::load_usd_scene
        // blocks on the C++ bridge + GPU buffer uploads. Matches bif_viewer's
        // startup-load behavior; acceptable for now. Async path is a future
        // optimization (scene_manager.rs already has `load_usd_scene_async`).
        let payload_policy = self.as_ref().rust().payload_policy;
        let load_result = with_viewport_mut(|vp| {
            vp.renderer_mut()
                .load_usd_scene_with_policy(&path_buf, payload_policy)
        });

        match load_result {
            None => {
                log::warn!("stage open: viewport not ready yet (surface not created?)");
                refresh_undo_redo_qprops(self.as_mut());
                refresh_ivar_status_qprop(self.as_mut());
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!(
                        "Viewport not ready — cannot load {path_str}",
                    )));
            }
            Some(Err(e)) => {
                log::error!("stage load failed: {e:?}");
                refresh_undo_redo_qprops(self.as_mut());
                refresh_ivar_status_qprop(self.as_mut());
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

                // Tier 1: auto-pick the strongest writable sublayer as edit
                // target. `SceneLayerState::from_stage` already defaulted
                // `working_layer` to the root; re-pick so anonymous / muted
                // roots skip to the next candidate instead of silently
                // authoring into a non-persistent layer.
                let (edit_target_name, loaded_policy, picked_idx) = {
                    let mut r = self.as_mut().rust_mut();
                    if let Some(state) = r.scene_layer_state.as_mut() {
                        let picked_idx = pick_strongest_writable_sublayer(state);
                        if let Some(idx) = picked_idx {
                            state.set_working_layer(idx);
                        }
                        (
                            state
                                .stack
                                .layers
                                .get(state.working_layer)
                                .map(|l| l.display_name.clone())
                                .unwrap_or_default(),
                            Some(state.payload_policy),
                            picked_idx,
                        )
                    } else {
                        (String::new(), None, None)
                    }
                };
                if let Some(idx) = picked_idx {
                    with_viewport_mut(|vp| {
                        let renderer = vp.renderer_mut();
                        let stage_arc = renderer.scene.usd_stage.clone();
                        if let Some(state) = renderer.scene.layer_state.as_mut() {
                            if let Some(stage_arc) = stage_arc {
                                if let Ok(stage) = stage_arc.lock() {
                                    if let Err(e) = state.set_edit_target(idx, &stage) {
                                        log::warn!("edit target sync failed: {e}");
                                    }
                                }
                            } else {
                                state.set_working_layer(idx);
                            }
                        }
                    });
                }
                if let Some(policy) = loaded_policy {
                    self.as_mut().rust_mut().payload_policy = policy;
                }

                bump_revision(self.as_mut());
                bump_scene_browser_revision(self.as_mut());

                // Populate camera picker: collect UsdGeomCamera prim paths.
                let camera_paths =
                    with_stage(|stage| stage.list_camera_prims().unwrap_or_default())
                        .unwrap_or_default();
                {
                    let mut r = self.as_mut().rust_mut();
                    r.usd_camera_paths = camera_paths;
                    r.active_camera_source = "free".to_string();
                }
                bump_camera_list_revision(self.as_mut());
                refresh_undo_redo_qprops(self.as_mut());
                refresh_ivar_status_qprop(self.as_mut());

                log::info!("stage loaded: {path_str}");
                let msg = if edit_target_name.is_empty() {
                    format!("Loaded: {path_str}")
                } else {
                    format!("Loaded: {path_str}  •  Edit target: {edit_target_name}")
                };
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&msg));
            }
        }
    }

    fn close_stage(mut self: Pin<&mut Self>) {
        log::info!("action: File/Close Stage");

        // Stop playback before tearing the stage down. The timer in
        // window_builder.cpp stops on `is_playing=false`; otherwise it
        // keeps advancing `current_frame` onto the first-launch screen
        // and leaks into the next stage load (reopen inherits stale
        // frame state). Reset frame to start for the same reason.
        let start_frame = *self.as_ref().start_frame();
        self.as_mut().set_is_playing(false);
        self.as_mut().set_current_frame(start_frame);

        // `Renderer::reset_scene_state` drains GPU then evicts both
        // halves of the CompositeProvider source (USD stage in
        // `scene` + procedural prim cache in `nodes`) so the scene
        // browser tree clears with the viewport. `wait_for_gpu` is
        // mandatory before dropping textures/buffers (same root cause
        // as the D3D12 shutdown crash — commit d290c9d).
        let drained = with_viewport_mut(|vp| vp.renderer_mut().reset_scene_state());

        if drained.is_none() {
            log::warn!("close_stage: viewport not ready — only clearing shell state");
        }

        // Clear shell state that mirrors scene/selection.
        {
            let mut r = self.as_mut().rust_mut();
            r.scene_layer_state = None;
            r.current_stage_path = None;
            r.usd_camera_paths = Vec::new();
            r.active_camera_source = "free".to_string();
        }
        bump_camera_list_revision(self.as_mut());
        self.as_mut()
            .set_selected_prim_path(cxx_qt_lib::QString::from(""));
        self.as_mut()
            .set_selected_prim_type(cxx_qt_lib::QString::from(""));
        refresh_selected_prim_stack_cache(self.as_mut());
        bump_revision(self.as_mut());
        bump_scene_browser_revision(self.as_mut());
        refresh_undo_redo_qprops(self.as_mut());
        refresh_ivar_status_qprop(self.as_mut());

        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from("Stage closed."));
    }

    fn on_undo(mut self: Pin<&mut Self>) {
        log::info!("action: Edit/Undo");
        let outcome = with_viewport_mut(|vp| vp.renderer_mut().undo());
        let message = match outcome {
            Some(Some(desc)) => format!("Undo: {desc}"),
            Some(None) => "Undo: nothing to undo".to_string(),
            None => "Undo failed — viewport not ready".to_string(),
        };
        refresh_undo_redo_qprops(self.as_mut());
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&message));
    }

    fn on_redo(mut self: Pin<&mut Self>) {
        log::info!("action: Edit/Redo");
        let outcome = with_viewport_mut(|vp| vp.renderer_mut().redo());
        let message = match outcome {
            Some(Some(desc)) => format!("Redo: {desc}"),
            Some(None) => "Redo: nothing to redo".to_string(),
            None => "Redo failed — viewport not ready".to_string(),
        };
        refresh_undo_redo_qprops(self.as_mut());
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&message));
    }

    fn sync_undo_redo_state(mut self: Pin<&mut Self>) {
        refresh_undo_redo_qprops(self.as_mut());
    }

    fn on_set_outline_width(mut self: Pin<&mut Self>, width: f64) {
        let clamped = width.clamp(0.001, 0.05);
        self.as_mut().set_outline_width(clamped);
        let applied = with_viewport_mut(|vp| {
            let renderer = vp.renderer_mut();
            renderer.display_settings.outline_width = clamped as f32;
            renderer.update_camera();
        })
        .is_some();
        let msg = if applied {
            format!("Outline width: {:.3}", clamped)
        } else {
            format!("Outline width queued: {:.3} (viewport not ready)", clamped)
        };
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&msg));
    }

    fn on_set_outline_color(mut self: Pin<&mut Self>, color_hex: cxx_qt_lib::QString) {
        let raw: String = (&color_hex).into();
        let mut normalized = raw.trim().to_ascii_uppercase();
        if !normalized.starts_with('#') {
            normalized.insert(0, '#');
        }
        let Some(linear) = parse_outline_color_hex(&normalized) else {
            self.as_mut().set_status_message(cxx_qt_lib::QString::from(
                "Outline color must be a #RRGGBB value",
            ));
            return;
        };
        self.as_mut()
            .set_outline_color_hex(cxx_qt_lib::QString::from(&normalized));
        let applied = with_viewport_mut(|vp| {
            let renderer = vp.renderer_mut();
            renderer.display_settings.outline_color = linear;
            renderer.update_camera();
        })
        .is_some();
        let msg = if applied {
            format!("Outline color: {normalized}")
        } else {
            format!("Outline color queued: {normalized} (viewport not ready)")
        };
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&msg));
    }

    fn on_start_ivar_render(mut self: Pin<&mut Self>) {
        let outcome = with_viewport_mut(|vp| {
            let renderer = vp.renderer_mut();
            let started = renderer.trigger_ivar_render();
            let status = renderer.ivar_status_line();
            (started, status)
        });
        refresh_ivar_status_qprop(self.as_mut());
        let message = match outcome {
            Some((true, status)) if !status.is_empty() => status,
            Some((true, _)) => "Ivar render started".to_string(),
            Some((false, _)) => "Ivar render unavailable — load a stage first".to_string(),
            None => "Ivar render deferred — viewport not ready".to_string(),
        };
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&message));
    }

    fn sync_ivar_status(mut self: Pin<&mut Self>) {
        refresh_ivar_status_qprop(self.as_mut());
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
        let gizmo_axis = with_viewport_mut(|vp| {
            vp.renderer_mut()
                .begin_transform_gizmo_drag(x as f32, y as f32)
        })
        .flatten();
        if let Some(axis) = gizmo_axis {
            self.as_mut()
                .set_status_message(cxx_qt_lib::QString::from(&format!("Move {axis} axis")));
            return;
        }

        // Route through the renderer's unified selection path so viewport
        // outline + gizmo state stay in sync with tree/property panels.
        // `select_at_screen` ray-casts into the pick BVH, updates
        // `selection.selected_instance_index` + `selected_prim_path`,
        // and clears gizmo state. We read the result back here to mirror
        // path/type into shell qprops (property inspector listens on
        // `selected_prim_pathChanged`).
        let pick_result = with_viewport_mut(|vp| {
            let r = vp.renderer_mut();
            r.select_at_screen(x as f32, y as f32);
            let idx = r.selection.selected_instance_index?;
            // Pick indices align with `scene.instances.prim_paths` — the
            // same collection `select_at_screen` uses internally for path
            // resolution (bif_viewport/src/lib.rs:1378). Reading from
            // `working_scene.instances()` yields empty paths for some
            // prototype-sourced instances on post-composition scenes,
            // which is why the status bar was showing "Selected: " with
            // no path after a successful hit.
            let raw = r.scene.instances.prim_paths.get(idx).cloned()?;
            if raw.is_empty() {
                log::warn!(
                    "pick idx={idx} resolved to empty prim_path at \
                     scene.instances.prim_paths[{idx}] — stage/pick desync?"
                );
                return None;
            }
            let path = denormalize_synthetic_path(&raw);
            let stage_arc = r.scene.usd_stage.clone();
            let type_name = stage_arc
                .and_then(|arc| {
                    arc.lock()
                        .ok()
                        .and_then(|stage| stage.get_prim_info_by_path(&path).ok())
                        .map(|info| info.type_name)
                })
                .unwrap_or_default();
            log::debug!("pick idx={idx} raw={raw} path={path} type={type_name}");
            let has_gizmo = r.has_transform_gizmo();
            Some((path, type_name, has_gizmo))
        })
        .flatten();

        match pick_result {
            Some((path, type_name, has_gizmo)) => {
                log::info!("pick hit: path={path} type={type_name}");
                self.as_mut()
                    .set_selected_prim_path(cxx_qt_lib::QString::from(&path));
                self.as_mut()
                    .set_selected_prim_type(cxx_qt_lib::QString::from(&type_name));
                refresh_selected_prim_stack_cache(self.as_mut());
                let suffix = if has_gizmo {
                    " — move handles ready"
                } else {
                    " — no movable viewport instance"
                };
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!(
                        "Selected: {path}{suffix}"
                    )));
            }
            None => {
                self.as_mut()
                    .set_selected_prim_path(cxx_qt_lib::QString::from(""));
                self.as_mut()
                    .set_selected_prim_type(cxx_qt_lib::QString::from(""));
                refresh_selected_prim_stack_cache(self.as_mut());
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from("Selection cleared."));
            }
        }
    }

    fn on_transform_gizmo_move(self: Pin<&mut Self>, x: i32, y: i32) -> bool {
        with_viewport_mut(|vp| {
            vp.renderer_mut()
                .update_transform_gizmo_drag(x as f32, y as f32)
        })
        .unwrap_or(false)
    }

    fn on_transform_gizmo_release(mut self: Pin<&mut Self>, x: i32, y: i32) -> bool {
        let moved = with_viewport_mut(|vp| {
            vp.renderer_mut()
                .end_transform_gizmo_drag(x as f32, y as f32)
        })
        .unwrap_or(false);
        if moved {
            self.as_mut()
                .set_status_message(cxx_qt_lib::QString::from("Transform moved"));
        }
        moved
    }

    fn on_tree_prim_selected(
        mut self: Pin<&mut Self>,
        path: cxx_qt_lib::QString,
        type_name: cxx_qt_lib::QString,
    ) {
        let path_str: String = (&path).into();
        if path_str.is_empty() {
            return;
        }
        // Drive renderer-side selection so the viewport gizmo + outline
        // highlight follow the tree click. `select_prim_by_path` resolves
        // the prim path back to an instance index when available.
        let has_gizmo = with_viewport_mut(|vp| {
            let renderer = vp.renderer_mut();
            renderer.select_prim_by_path(&path_str);
            renderer.has_transform_gizmo()
        })
        .unwrap_or(false);
        // Mirror path/type into shell qprops so the property inspector
        // (which binds to `selected_prim_pathChanged`) updates too.
        self.as_mut().set_selected_prim_path(path);
        self.as_mut().set_selected_prim_type(type_name);
        refresh_selected_prim_stack_cache(self.as_mut());
        let suffix = if has_gizmo {
            " — move handles ready"
        } else {
            " — no movable viewport instance"
        };
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&format!(
                "Selected: {path_str}{suffix}"
            )));
    }

    fn on_set_lod_enabled(mut self: Pin<&mut Self>, enabled: bool) {
        // Update the qprop first so the menu checkbox reflects the new
        // state even if the viewport isn't ready yet (pre-surfaceReady).
        self.as_mut().set_lod_enabled(enabled);
        let applied = with_viewport_mut(|vp| {
            vp.renderer_mut().display_settings.lod_enabled = enabled;
        })
        .is_some();
        let msg = if applied {
            if enabled {
                "LOD: enabled"
            } else {
                "LOD: disabled"
            }
        } else {
            "LOD toggle deferred — viewport not ready"
        };
        log::info!("set_lod_enabled({enabled}) applied={applied}");
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(msg));
    }

    // -----------------------------------------------------------------
    // Layer Stack state surface (Phase C.1)
    // -----------------------------------------------------------------

    fn seed_demo_layer_stack(mut self: Pin<&mut Self>) {
        // Mimic `test_assets/layers/root.usda` — three layers with
        // shot overriding anim overriding root. Fake identifiers so
        // we don't need a real USD load for Phase C.1 visual testing.
        let payload_policy = self.as_ref().rust().payload_policy;
        let layers = vec![
            LayerInfo {
                identifier: "G:/demo/root.usda".into(),
                display_name: "root.usda".into(),
                depth: 0,
                parent_index: None,
                is_muted: false,
                is_anonymous: false,
                is_dirty: false,
                permission_to_edit: true,
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
                permission_to_edit: true,
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
                permission_to_edit: true,
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
            payload_policy,
            layer_for_prim: Default::default(),
            edit_history: bif_core::usd::EditHistory::with_working_layer("root.usda"),
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

        // Update the shell mirror immediately so the checkbox reflects
        // the new state even if the reload path below fails or there's
        // no live stage (demo-only layer stack).
        if let Some(state) = self.as_mut().rust_mut().scene_layer_state.as_mut() {
            state.set_muted(&identifier, muted);
        }
        bump_revision(self.as_mut());
        log::info!("layer {index} muted={muted}");

        // For real stages, push the mute through the renderer. The loader
        // (scene_loader.rs:1496-1508) snapshots `scene.layer_state.muted`
        // at entry and replays it via `load_usd_with_stage_muted` BEFORE
        // payloads are fetched — so geometry is extracted under the muted
        // composition. Empty scenes (e.g. muting a def-providing layer)
        // are handled by load_usd_scene itself: viewport clears, Layer
        // Stack panel stays active for unmute. No new renderer API is
        // required; we just mutate the authoritative mute set on the
        // renderer's `scene.layer_state` and re-run the load.
        let Some(path) = self.as_ref().rust().current_stage_path.clone() else {
            // Demo-only stack (no live stage): UI toggle updates the
            // mirror but there's nothing to recompose.
            return;
        };

        // NB: skip `reset_scene_state` on purpose — it wipes
        // `scene.layer_state`, which would erase the mute snapshot the
        // loader reads on re-entry. `load_usd_scene` / `finalize_usd_scene`
        // overwrite the USD halves of `SceneManager` internally and call
        // `reload_working_scene` for the GPU rebuild, which is what we
        // need here. Post-Phase-F node restoration may want a "reset
        // node caches but preserve layer_state" path for hybrid setups.
        let reload_result = with_viewport_mut(|vp| {
            let r = vp.renderer_mut();
            if let Some(state) = r.scene.layer_state.as_mut() {
                state.set_muted(&identifier, muted);
            }
            r.load_usd_scene(&path)
        });

        match reload_result {
            Some(Ok(())) => {
                // Refresh the shell mirror from the freshly-composed
                // stage (muted layers now report `is_muted=true`, and
                // per-prim strongest-layer assignments may have shifted).
                let fresh_state =
                    with_viewport_mut(|vp| vp.renderer_mut().scene.layer_state.clone()).flatten();
                let fresh_policy = fresh_state.as_ref().map(|s| s.payload_policy);
                {
                    let mut r = self.as_mut().rust_mut();
                    r.scene_layer_state = fresh_state;
                    if let Some(policy) = fresh_policy {
                        r.payload_policy = policy;
                    }
                }
                bump_revision(self.as_mut());
                bump_scene_browser_revision(self.as_mut());
                let verb = if muted { "muted" } else { "unmuted" };
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!(
                        "Layer {verb}: {identifier}",
                    )));
            }
            Some(Err(e)) => {
                log::error!("layer mute reload failed: {e:?}");
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!("Mute failed: {e:?}")));
            }
            None => {
                log::warn!("layer mute: viewport not ready — shell mirror updated only");
            }
        }
    }

    fn set_working_layer(mut self: Pin<&mut Self>, index: i32) {
        if index < 0 {
            return;
        }
        let idx = index as usize;
        if let Some(state) = self.as_mut().rust_mut().scene_layer_state.as_mut() {
            state.set_working_layer(idx);
        }
        with_viewport_mut(|vp| {
            let renderer = vp.renderer_mut();
            let stage_arc = renderer.scene.usd_stage.clone();
            if let Some(state) = renderer.scene.layer_state.as_mut() {
                if let Some(stage_arc) = stage_arc {
                    if let Ok(stage) = stage_arc.lock() {
                        if let Err(e) = state.set_edit_target(idx, &stage) {
                            log::warn!("working layer edit-target sync failed: {e}");
                        }
                    }
                } else {
                    state.set_working_layer(idx);
                }
            }
        });
        bump_revision(self.as_mut());
        log::info!("working layer → {index}");
    }

    fn toggle_isolation_mode(mut self: Pin<&mut Self>) {
        if let Some(state) = self.as_mut().rust_mut().scene_layer_state.as_mut() {
            state.isolation_mode = !state.isolation_mode;
        }
        bump_revision(self.as_mut());
    }

    fn has_loaded_stage(&self) -> bool {
        self.rust().current_stage_path.is_some()
    }

    fn payload_policy_name(&self) -> cxx_qt_lib::QString {
        cxx_qt_lib::QString::from(payload_policy_to_name(self.rust().payload_policy))
    }

    fn on_set_payload_policy(mut self: Pin<&mut Self>, policy_name: cxx_qt_lib::QString) -> bool {
        let policy_name: String = (&policy_name).into();
        let Some(policy) = parse_payload_policy_name(&policy_name) else {
            let msg = format!("Unknown payload policy: {policy_name}");
            log::warn!("{msg}");
            self.as_mut()
                .set_status_message(cxx_qt_lib::QString::from(&msg));
            return false;
        };

        if policy == self.as_ref().rust().payload_policy {
            return true;
        }

        let status_line = format!("Payload policy: {}", payload_policy_to_name(policy));
        let Some(path) = self.as_ref().rust().current_stage_path.clone() else {
            {
                let mut r = self.as_mut().rust_mut();
                r.payload_policy = policy;
                if let Some(state) = r.scene_layer_state.as_mut() {
                    state.payload_policy = policy;
                }
            }
            bump_revision(self.as_mut());
            self.as_mut()
                .set_status_message(cxx_qt_lib::QString::from(&status_line));
            return true;
        };

        let reload_result =
            with_viewport_mut(|vp| vp.renderer_mut().load_usd_scene_with_policy(&path, policy));

        match reload_result {
            Some(Ok(())) => {
                let fresh_state =
                    with_viewport_mut(|vp| vp.renderer_mut().scene.layer_state.clone()).flatten();
                let camera_paths =
                    with_stage(|stage| stage.list_camera_prims().unwrap_or_default())
                        .unwrap_or_default();
                let edit_target_name = {
                    let mut r = self.as_mut().rust_mut();
                    r.payload_policy = policy;
                    r.scene_layer_state = fresh_state;
                    r.usd_camera_paths = camera_paths;
                    r.active_camera_source = "free".to_string();
                    if let Some(state) = r.scene_layer_state.as_mut() {
                        if let Some(idx) = pick_strongest_writable_sublayer(state) {
                            state.working_layer = idx;
                        }
                        state
                            .stack
                            .layers
                            .get(state.working_layer)
                            .map(|l| l.display_name.clone())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                };
                bump_revision(self.as_mut());
                bump_scene_browser_revision(self.as_mut());
                bump_camera_list_revision(self.as_mut());
                refresh_undo_redo_qprops(self.as_mut());
                refresh_ivar_status_qprop(self.as_mut());

                let msg = if edit_target_name.is_empty() {
                    status_line
                } else {
                    format!("{status_line}  •  Edit target: {edit_target_name}")
                };
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&msg));
                true
            }
            Some(Err(e)) => {
                log::error!("payload policy reload failed: {e:?}");
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!(
                        "Payload reload failed: {e:?}"
                    )));
                false
            }
            None => {
                log::warn!("payload policy change: viewport not ready");
                self.as_mut().set_status_message(cxx_qt_lib::QString::from(
                    "Payload policy deferred — viewport not ready",
                ));
                false
            }
        }
    }

    // -----------------------------------------------------------------
    // Edit-target surface (Tier 1)
    // -----------------------------------------------------------------
    //
    // Each reader walks `scene_layer_state.stack.layers[working_layer]`.
    // Safe over empty state (returns false / empty / -1). All C++ consumers
    // (pill, status chip, viewport edge tint, breadcrumb segment) refresh
    // on `layer_state_revisionChanged`.

    fn active_edit_target_is_set(&self) -> bool {
        self.rust()
            .scene_layer_state
            .as_ref()
            .map(|s| s.stack.layers.get(s.working_layer).is_some())
            .unwrap_or(false)
    }

    fn active_edit_target_name(&self) -> cxx_qt_lib::QString {
        let name = self
            .rust()
            .scene_layer_state
            .as_ref()
            .and_then(|s| s.stack.layers.get(s.working_layer))
            .map(|l| l.display_name.clone())
            .unwrap_or_default();
        cxx_qt_lib::QString::from(&name)
    }

    fn active_edit_target_identifier(&self) -> cxx_qt_lib::QString {
        let ident = self
            .rust()
            .scene_layer_state
            .as_ref()
            .and_then(|s| s.stack.layers.get(s.working_layer))
            .map(|l| l.identifier.clone())
            .unwrap_or_default();
        cxx_qt_lib::QString::from(&ident)
    }

    fn active_edit_target_color_index(&self) -> i32 {
        let Some(state) = self.rust().scene_layer_state.as_ref() else {
            return -1;
        };
        if state.stack.layers.get(state.working_layer).is_none() {
            return -1;
        }
        (state.working_layer as i32) % 8
    }

    fn current_stage_display(&self) -> cxx_qt_lib::QString {
        let display = self
            .rust()
            .current_stage_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        cxx_qt_lib::QString::from(&display)
    }

    fn friendly_attribute_name(&self, raw: cxx_qt_lib::QString) -> cxx_qt_lib::QString {
        let r: String = (&raw).into();
        let friendly = crate::schema_labels::friendly_attribute_name(&r);
        cxx_qt_lib::QString::from(friendly)
    }

    fn friendly_prim_type(&self, raw: cxx_qt_lib::QString) -> cxx_qt_lib::QString {
        let r: String = (&raw).into();
        let friendly = crate::schema_labels::friendly_prim_type(&r);
        cxx_qt_lib::QString::from(friendly)
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

    /// Connected from `current_frameChanged` — drives `Renderer::set_time`
    /// so animation re-evaluates on both QTimer-driven playback and
    /// scrubber drags. Safe no-op when viewport isn't ready.
    fn on_frame_changed(self: Pin<&mut Self>, frame: i32) {
        let _ = &self; // reserve `self: Pin<&mut Self>` signature for cxx-qt
        with_viewport_mut(|vp| {
            vp.renderer_mut().set_time(frame as f64);
        });
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
        // Clamp `current_frame` into the newly-detected range. The spin-
        // box widget clamps its display, but the underlying qproperty
        // drives the playback timer (window_builder.cpp) and render
        // evaluation — if it stays out of range, playback advances from
        // the wrong frame until the user interacts with the UI.
        let cur = *self.as_ref().current_frame();
        let clamped = cur.clamp(start, end);
        if clamped != cur {
            self.as_mut().set_current_frame(clamped);
        }
        self.as_mut()
            .set_status_message(cxx_qt_lib::QString::from(&format!(
                "Timeline detected: {start}-{end} @ {fps} fps",
            )));
    }

    // -----------------------------------------------------------------
    // Scene Browser surface (Phase E.2 move 7)
    // -----------------------------------------------------------------

    fn root_prim_count(&self) -> i32 {
        with_scene_browser_provider(|provider| provider.root_paths().len() as i32).unwrap_or(0)
    }

    fn root_prim_path_at(&self, index: i32) -> cxx_qt_lib::QString {
        let path = with_scene_browser_provider(|provider| {
            provider
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
        with_scene_browser_provider(|provider| provider.get_children(&p).len() as i32).unwrap_or(0)
    }

    fn child_prim_path_at(
        &self,
        parent_path: cxx_qt_lib::QString,
        index: i32,
    ) -> cxx_qt_lib::QString {
        let parent: String = (&parent_path).into();
        let child = with_scene_browser_provider(|provider| {
            provider
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
        // Composite provider so procedural prim type names ("Mesh",
        // "PointInstancer", "Scope") resolve too — not just USD prims.
        let type_name = with_scene_browser_provider(|provider| {
            provider
                .get_prim_info(&p)
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

    fn prim_kind_at(&self, path: cxx_qt_lib::QString) -> cxx_qt_lib::QString {
        let p: String = (&path).into();
        let kind = with_scene_browser_provider(|provider| {
            provider
                .get_prim_info(&p)
                .map(|i| i.kind)
                .unwrap_or_default()
        })
        .unwrap_or_default();
        cxx_qt_lib::QString::from(&kind)
    }

    fn prim_is_visible_at(&self, path: cxx_qt_lib::QString) -> bool {
        let p: String = (&path).into();
        with_scene_browser_provider(|provider| {
            provider
                .get_prim_info(&p)
                .map(|i| i.is_visible)
                .unwrap_or(true)
        })
        .unwrap_or(true)
    }

    fn prim_is_active_at(&self, path: cxx_qt_lib::QString) -> bool {
        let p: String = (&path).into();
        with_scene_browser_provider(|provider| {
            provider
                .get_prim_info(&p)
                .map(|i| i.is_active)
                .unwrap_or(true)
        })
        .unwrap_or(true)
    }

    fn prim_color_index_at(&self, path: cxx_qt_lib::QString) -> i32 {
        let p: String = (&path).into();
        let Some(state) = self.rust().scene_layer_state.as_ref() else {
            return -1;
        };
        state
            .layer_for_prim
            .get(&p)
            .map(|idx| (*idx as i32) % 8)
            .unwrap_or(-1)
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
        self.rust().selected_prim_stack_cache.len() as i32
    }

    fn selected_prim_stack_layer_at(&self, index: i32) -> cxx_qt_lib::QString {
        let id = self
            .rust()
            .selected_prim_stack_cache
            .get(index as usize)
            .map(|e| e.layer_identifier.clone())
            .unwrap_or_default();
        cxx_qt_lib::QString::from(&id)
    }

    fn selected_prim_stack_specifier_at(&self, index: i32) -> cxx_qt_lib::QString {
        let label = self
            .rust()
            .selected_prim_stack_cache
            .get(index as usize)
            .map(|e| e.specifier.as_str())
            .unwrap_or("");
        cxx_qt_lib::QString::from(label)
    }

    fn selected_prim_stack_has_opinion_at(&self, index: i32) -> bool {
        self.rust()
            .selected_prim_stack_cache
            .get(index as usize)
            .map(|e| e.has_authored_opinions)
            .unwrap_or(false)
    }

    fn selected_prim_stack_color_index_at(&self, index: i32) -> i32 {
        let identifier = self
            .rust()
            .selected_prim_stack_cache
            .get(index as usize)
            .map(|e| e.layer_identifier.clone());
        match identifier {
            Some(id) => color_index_for_layer(self.rust(), &id),
            None => -1,
        }
    }

    fn selected_prim_attr_color_index_at(&self, attr_index: i32) -> i32 {
        let prim_path: String = (&self.rust().selected_prim_path).into();
        if prim_path.is_empty() {
            return -1;
        }
        let attr_name = with_stage(|stage| {
            stage
                .get_prim_attributes(&prim_path)
                .ok()
                .and_then(|v| v.get(attr_index as usize).map(|a| a.name.clone()))
        })
        .flatten();
        let Some(name) = attr_name else { return -1 };
        let winning_id = with_stage(|stage| {
            stage
                .get_attribute_opinions(&prim_path, &name)
                .ok()
                .and_then(|v| {
                    v.into_iter()
                        .find(|o| o.is_winning)
                        .map(|o| o.layer_identifier)
                })
        })
        .flatten();
        match winning_id {
            Some(id) => color_index_for_layer(self.rust(), &id),
            None => -1,
        }
    }

    fn selected_prim_attr_tooltip_at(&self, attr_index: i32) -> cxx_qt_lib::QString {
        let prim_path: String = (&self.rust().selected_prim_path).into();
        if prim_path.is_empty() {
            return cxx_qt_lib::QString::from("");
        }
        let tooltip = with_stage(|stage| {
            let Some(attr_name) = stage
                .get_prim_attributes(&prim_path)
                .ok()
                .and_then(|v| v.get(attr_index as usize).map(|a| a.name.clone()))
            else {
                return String::new();
            };
            let opinions = stage
                .get_attribute_opinions(&prim_path, &attr_name)
                .unwrap_or_default();
            format_attr_opinion_tooltip(&attr_name, &opinions, self.rust())
        })
        .unwrap_or_default();
        cxx_qt_lib::QString::from(&tooltip)
    }

    // -----------------------------------------------------------------
    // Camera picker surface
    // -----------------------------------------------------------------

    fn usd_camera_count(&self) -> i32 {
        self.rust().usd_camera_paths.len() as i32
    }

    fn usd_camera_path_at(&self, index: i32) -> cxx_qt_lib::QString {
        self.rust()
            .usd_camera_paths
            .get(index as usize)
            .map(|s| cxx_qt_lib::QString::from(s.as_str()))
            .unwrap_or_default()
    }

    fn active_camera_name(&self) -> cxx_qt_lib::QString {
        cxx_qt_lib::QString::from(self.rust().active_camera_source.as_str())
    }

    fn on_set_visibility(mut self: Pin<&mut Self>, path: cxx_qt_lib::QString, visible: bool) {
        let path_str: String = (&path).into();
        if path_str.is_empty() {
            return;
        }
        let result =
            with_viewport_mut(|vp| vp.renderer_mut().dispatch_visibility(&path_str, visible));
        match result {
            Some(Ok(_desc)) => {
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!(
                        "Visibility {} for {}",
                        if visible { "shown" } else { "hidden" },
                        path_str
                    )));
                refresh_undo_redo_qprops(self.as_mut());
                bump_revision(self.as_mut());
            }
            Some(Err(e)) => {
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!(
                        "Visibility edit failed: {e}"
                    )));
            }
            None => {}
        }
    }

    fn selected_prim_material_input_count(&self) -> i32 {
        self.rust().selected_material_inputs_cache.len() as i32
    }

    fn selected_prim_material_input_name_at(&self, index: i32) -> cxx_qt_lib::QString {
        let idx = if index < 0 { 0 } else { index as usize };
        self.rust()
            .selected_material_inputs_cache
            .get(idx)
            .map(|i| cxx_qt_lib::QString::from(i.name.as_str()))
            .unwrap_or_default()
    }

    fn selected_prim_material_input_type_at(&self, index: i32) -> cxx_qt_lib::QString {
        let idx = if index < 0 { 0 } else { index as usize };
        self.rust()
            .selected_material_inputs_cache
            .get(idx)
            .map(|i| cxx_qt_lib::QString::from(i.type_name.as_str()))
            .unwrap_or_default()
    }

    fn selected_prim_material_input_value_at(&self, index: i32) -> cxx_qt_lib::QString {
        let idx = if index < 0 { 0 } else { index as usize };
        self.rust()
            .selected_material_inputs_cache
            .get(idx)
            .map(|i| cxx_qt_lib::QString::from(i.value.as_str()))
            .unwrap_or_default()
    }

    fn selected_prim_material_shader_path(&self) -> cxx_qt_lib::QString {
        cxx_qt_lib::QString::from(self.rust().selected_material_shader_path.as_str())
    }

    fn on_set_material_param(
        mut self: Pin<&mut Self>,
        input_name: cxx_qt_lib::QString,
        type_name: cxx_qt_lib::QString,
        value_str: cxx_qt_lib::QString,
    ) {
        let name: String = (&input_name).into();
        let ty: String = (&type_name).into();
        let value: String = (&value_str).into();
        let shader_path = self.as_ref().rust().selected_material_shader_path.clone();
        if shader_path.is_empty() || name.is_empty() || ty.is_empty() {
            return;
        }
        // Capture before-value from the cached snapshot for the inverse.
        let before = self
            .as_ref()
            .rust()
            .selected_material_inputs_cache
            .iter()
            .find(|i| i.name == name)
            .and_then(|i| parse_shader_value(&ty, &i.value));
        let Some(after) = parse_shader_value(&ty, &value) else {
            self.as_mut()
                .set_status_message(cxx_qt_lib::QString::from(&format!(
                    "Material edit: unsupported type/value {ty}={value}"
                )));
            return;
        };
        let result = with_viewport_mut(|vp| {
            vp.renderer_mut()
                .dispatch_material_param_override(&shader_path, &name, before, after)
        });
        match result {
            Some(Ok(_desc)) => {
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!(
                        "Material {name}={value}"
                    )));
                refresh_undo_redo_qprops(self.as_mut());
                bump_revision(self.as_mut());
            }
            Some(Err(e)) => {
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!(
                        "Material edit failed: {e}"
                    )));
            }
            None => {}
        }
    }

    fn on_bind_material(mut self: Pin<&mut Self>, material_path: cxx_qt_lib::QString) {
        let prim_path: String = (&self.as_ref().rust().selected_prim_path).into();
        let mat: String = (&material_path).into();
        if prim_path.is_empty() || mat.is_empty() {
            return;
        }
        let result =
            with_viewport_mut(|vp| vp.renderer_mut().dispatch_material_assign(&prim_path, &mat));
        match result {
            Some(Ok(_desc)) => {
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!(
                        "Bound {prim_path} → {mat}"
                    )));
                refresh_undo_redo_qprops(self.as_mut());
                bump_revision(self.as_mut());
            }
            Some(Err(e)) => {
                self.as_mut()
                    .set_status_message(cxx_qt_lib::QString::from(&format!(
                        "Bind material failed: {e}"
                    )));
            }
            None => {}
        }
    }

    fn on_select_camera(mut self: Pin<&mut Self>, source: cxx_qt_lib::QString) {
        let source_str: String = (&source).into();
        self.as_mut().rust_mut().active_camera_source = source_str.clone();

        if source_str == "free" {
            with_viewport_mut(|vp| vp.renderer_mut().apply_free_fly());
        } else if let Some(preset_str) = source_str.strip_prefix("ortho:") {
            let preset = match preset_str {
                "Top" => Some(bif_viewport::OrthoPreset::Top),
                "Bottom" => Some(bif_viewport::OrthoPreset::Bottom),
                "Front" => Some(bif_viewport::OrthoPreset::Front),
                "Back" => Some(bif_viewport::OrthoPreset::Back),
                "Right" => Some(bif_viewport::OrthoPreset::Right),
                "Left" => Some(bif_viewport::OrthoPreset::Left),
                _ => None,
            };
            if let Some(preset) = preset {
                with_viewport_mut(|vp| vp.renderer_mut().apply_ortho_view(preset));
            }
        } else if let Some(path) = source_str.strip_prefix("usd:") {
            let path = path.to_string();
            with_viewport_mut(|vp| vp.renderer_mut().apply_usd_camera(&path));
        }
    }
}

/// Increment `layer_state_revision` to trigger the cxx-qt-generated
/// `layer_state_revisionChanged` signal. C++ panel models listen for
/// this and refresh.
fn bump_revision(mut state: Pin<&mut qobject::BifShellState>) {
    refresh_selected_prim_stack_cache(state.as_mut());
    refresh_selected_material_inputs_cache(state.as_mut());
    let next = state.as_ref().rust().layer_state_revision.wrapping_add(1);
    state.as_mut().set_layer_state_revision(next);
}

fn refresh_selected_material_inputs_cache(mut state: Pin<&mut qobject::BifShellState>) {
    let path: String = (&state.as_ref().rust().selected_prim_path).into();
    if path.is_empty() {
        let mut r = state.as_mut().rust_mut();
        r.selected_material_inputs_cache.clear();
        r.selected_material_shader_path.clear();
        return;
    }
    let snapshot = with_stage(|stage| {
        stage
            .get_bound_material_inputs(&path)
            .ok()
            .unwrap_or_else(|| (String::new(), Vec::new()))
    })
    .unwrap_or_else(|| (String::new(), Vec::new()));
    let mut r = state.as_mut().rust_mut();
    r.selected_material_shader_path = snapshot.0;
    r.selected_material_inputs_cache = snapshot.1;
}

/// Parse a UI-supplied value string into a `ShaderValue` for the given
/// USD type token. Returns `None` for unsupported types or parse errors.
/// Color3f / float3 strings are comma-separated triples.
fn parse_shader_value(type_name: &str, value: &str) -> Option<bif_core::usd::ShaderValue> {
    use bif_core::usd::ShaderValue;
    match type_name {
        "float" => value.parse::<f32>().ok().map(ShaderValue::Float),
        "double" => value.parse::<f64>().ok().map(ShaderValue::Double),
        "int" => value.parse::<i32>().ok().map(ShaderValue::Int),
        "bool" => match value.to_ascii_lowercase().as_str() {
            "true" | "1" => Some(ShaderValue::Bool(true)),
            "false" | "0" => Some(ShaderValue::Bool(false)),
            _ => None,
        },
        "token" => Some(ShaderValue::Token(value.to_string())),
        "string" => Some(ShaderValue::String(value.to_string())),
        "color3f" | "float3" => {
            let parts: Vec<&str> = value.split(',').map(str::trim).collect();
            if parts.len() != 3 {
                return None;
            }
            let r = parts[0].parse::<f32>().ok()?;
            let g = parts[1].parse::<f32>().ok()?;
            let b = parts[2].parse::<f32>().ok()?;
            if type_name == "color3f" {
                Some(ShaderValue::Color3f([r, g, b]))
            } else {
                Some(ShaderValue::Vec3f([r, g, b]))
            }
        }
        _ => None,
    }
}

fn selected_prim_stack_snapshot(path: &cxx_qt_lib::QString) -> Vec<PrimStackEntry> {
    let path: String = path.into();
    if path.is_empty() {
        return Vec::new();
    }
    with_stage(|stage| stage.get_prim_stack(&path).unwrap_or_default()).unwrap_or_default()
}

fn refresh_selected_prim_stack_cache(mut state: Pin<&mut qobject::BifShellState>) {
    let path = state.as_ref().rust().selected_prim_path.clone();
    let cache = selected_prim_stack_snapshot(&path);
    state.as_mut().rust_mut().selected_prim_stack_cache = cache;
    refresh_selected_material_inputs_cache(state.as_mut());
}

fn current_undo_redo_availability() -> (bool, bool) {
    with_viewport_mut(|vp| {
        let renderer = vp.renderer_mut();
        (renderer.can_undo(), renderer.can_redo())
    })
    .unwrap_or((false, false))
}

fn refresh_undo_redo_qprops(mut state: Pin<&mut qobject::BifShellState>) {
    let (can_undo, can_redo) = current_undo_redo_availability();
    let (prev_undo, prev_redo) = {
        let pin_ref = state.as_ref();
        let r = pin_ref.rust();
        (r.can_undo, r.can_redo)
    };
    if prev_undo != can_undo {
        state.as_mut().set_can_undo(can_undo);
    }
    if prev_redo != can_redo {
        state.as_mut().set_can_redo(can_redo);
    }
}

fn current_ivar_status_line() -> String {
    with_viewport_mut(|vp| vp.renderer_mut().ivar_status_line()).unwrap_or_default()
}

fn refresh_ivar_status_qprop(mut state: Pin<&mut qobject::BifShellState>) {
    let status = current_ivar_status_line();
    let previous: String = (&state.as_ref().rust().ivar_status).into();
    if previous != status {
        state
            .as_mut()
            .set_ivar_status(cxx_qt_lib::QString::from(&status));
    }
}

/// Increment `scene_browser_revision` to trigger
/// `scene_browser_revisionChanged`. SceneBrowserModel listens for
/// this and calls `beginResetModel/endResetModel`.
fn bump_scene_browser_revision(mut state: Pin<&mut qobject::BifShellState>) {
    let next = state.as_ref().rust().scene_browser_revision.wrapping_add(1);
    state.as_mut().set_scene_browser_revision(next);
}

/// Increment `camera_list_revision` to trigger `camera_list_revisionChanged`.
/// The camera picker QComboBox repopulates its USD camera entries on this signal.
fn bump_camera_list_revision(mut state: Pin<&mut qobject::BifShellState>) {
    let next = state.as_ref().rust().camera_list_revision.wrapping_add(1);
    state.as_mut().set_camera_list_revision(next);
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

#[cfg(test)]
mod tests {
    use super::*;
    use bif_core::scene_layer_state::SceneLayerState;

    fn make_layer(identifier: &str, permission_to_edit: bool, muted: bool) -> LayerInfo {
        LayerInfo {
            identifier: identifier.to_string(),
            display_name: identifier.to_string(),
            real_path: Default::default(),
            is_anonymous: false,
            is_dirty: false,
            is_muted: muted,
            permission_to_edit,
            offset: LayerOffset::default(),
            parent_index: None,
            depth: 0,
        }
    }

    #[test]
    fn pick_strongest_writable_sublayer_skips_locked_layers() {
        let state = SceneLayerState {
            stack: LayerStack {
                layers: vec![
                    make_layer("locked.usda", false, false),
                    make_layer("anim.usda", true, false),
                ],
                root_index: 0,
            },
            working_layer: 0,
            muted: Default::default(),
            isolation_mode: false,
            payload_policy: PayloadPolicy::LoadAll,
            layer_for_prim: Default::default(),
            edit_history: bif_core::usd::EditHistory::with_working_layer("locked.usda"),
        };

        assert_eq!(pick_strongest_writable_sublayer(&state), Some(1));
    }

    #[test]
    fn parse_outline_color_hex_converts_srgb_to_linear() {
        let color = parse_outline_color_hex("#FFA600").expect("valid outline color");
        assert!((color[0] - 1.0).abs() < 1e-6);
        assert!((color[1] - 0.381_326_02).abs() < 1e-6);
        assert!((color[2] - 0.0).abs() < 1e-6);
        assert!((color[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parse_payload_policy_name_accepts_known_values() {
        assert_eq!(
            parse_payload_policy_name("LoadAll"),
            Some(PayloadPolicy::LoadAll)
        );
        assert_eq!(
            parse_payload_policy_name("LoadNone"),
            Some(PayloadPolicy::LoadNone)
        );
        assert_eq!(parse_payload_policy_name("Review"), None);
    }

    #[test]
    fn format_attr_opinion_tooltip_marks_winning_and_escapes_html() {
        let mut state = BifShellStateRust::default();
        state.scene_layer_state = Some(SceneLayerState {
            stack: LayerStack {
                layers: vec![
                    make_layer("shot<&>.usda", true, false),
                    make_layer("anim.usda", true, false),
                ],
                root_index: 0,
            },
            working_layer: 0,
            muted: Default::default(),
            isolation_mode: false,
            payload_policy: PayloadPolicy::LoadAll,
            layer_for_prim: Default::default(),
            edit_history: bif_core::usd::EditHistory::with_working_layer("shot<&>.usda"),
        });
        let opinions = vec![
            OpinionSource {
                layer_identifier: "shot<&>.usda".to_string(),
                value_display: "\"<rough>\"".to_string(),
                value_type: "token&".to_string(),
                is_winning: true,
            },
            OpinionSource {
                layer_identifier: "anim.usda".to_string(),
                value_display: "0.15".to_string(),
                value_type: "float".to_string(),
                is_winning: false,
            },
        ];

        let html = format_attr_opinion_tooltip("inputs:roughness<1>", &opinions, &state);

        assert!(html.contains("inputs:roughness&lt;1&gt;"));
        assert!(html.contains("shot&lt;&amp;&gt;.usda"));
        assert!(html.contains("&quot;&lt;rough&gt;&quot;"));
        assert!(html.contains("token&amp;"));
        assert!(html.contains("(winning)"));
        assert!(!html.contains("shot<&>.usda"));
        assert!(!html.contains("\"<rough>\""));
    }
}
