# Changelog

All notable changes to BIF will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **v0.15.0 camera-picker — UsdGeomCamera + ortho presets + free-fly toggle** (2026-04-19). Viewport no longer locked to free-fly; artists can look through any `UsdGeomCamera` prim or snap to 6 standard orthographic views.
  - `UsdStage::list_camera_prims()` — filters `all_prims()` by `type_name == "Camera"` (no new FFI; pure Rust-side filter).
  - `Renderer::apply_usd_camera` / `apply_ortho_view` / `apply_free_fly` — three new helpers on `bif_viewport::Renderer`. Ortho view positions camera along the preset axis, sets `ProjectionMode::Orthographic { ortho_size }` derived from current camera distance; existing `sync_viewport_to_usd_camera` reused. `pub use bif_math::OrthoPreset` re-exported from `bif_viewport` so `bif_qt` doesn't need a direct `bif_math` dep.
  - 4 new invokables + `camera_list_revision` qproperty on `BifShellState` — `usd_camera_count`, `usd_camera_path_at`, `active_camera_name`, `on_select_camera`. `on_select_camera` dispatches `"free"` / `"ortho:Top"` / `"usd:/path"` string keys. `on_stage_path_opened` caches camera paths + bumps revision; `close_stage` clears them.
  - `QComboBox` camera picker in breadcrumb row (`window_builder.cpp`) — "Perspective" first, then 6 ortho presets, then USD camera leaf names (if any). Repopulates on `camera_list_revisionChanged`. Animated USD cameras work automatically — existing `animation.rs` frame-change path already re-syncs `selected_usd_camera` each tick.

- **v0.15.0 Tier 1 — Edit-target visibility + schema labels + save-flow polish** (2026-04-17). Persistent "where am I editing?" signal across 4 surfaces + friendly attribute/prim names.
  - **Edit-target pill + status-bar chip + viewport 2px edge tint** — all three read 4 new cxx-qt invokables (`active_edit_target_{is_set,name,identifier,color_index}`) that resolve `scene_layer_state.working_layer`; refresh on `layer_state_revisionChanged`. Pill lives on the right of the breadcrumb row, chip as a permanent widget on the status bar, tint as a colored `QFrame` wrapping the viewport.
  - **Auto-pick strongest writable sublayer on stage load** — new `pick_strongest_writable_sublayer` helper walks `stack.layers` for the first `!is_anonymous && !is_muted` candidate and sets it as the edit target; status bar shows `"Loaded: <path>  •  Edit target: <layer>"`. Real `SdfLayer::PermissionToEdit()` FFI deferred to Tier 1.5.
  - **Breadcrumb layer segment** — breadcrumb now shows `stage.usda › layer (edit) › prim › path`, refreshing on both `selected_prim_pathChanged` and `layer_state_revisionChanged`. New `current_stage_display` invokable feeds the stage-name segment.
  - **Window title binding** — `compose_title` invokable computes `BIF — stage.usda[*] · Editing: layer` from stage + edit target + `LayerInfo::is_dirty`; wired via `layer_state_revisionChanged`. Dirty asterisk is latent until v0.16 write path lands.
  - **Friendly schema labels** — new `crates/bif_qt/src/schema_labels.rs` (~180 attribute entries across `xformOp:*`, `primvars:*`, UsdGeom, UsdLux, UsdGeomCamera; ~30 prim type entries). Property inspector calls `friendly_attribute_name` and `friendly_prim_type` invokables; raw USD name surfaces in each row's tooltip. 4/4 unit tests pass. Unknown names pass through unchanged so obscure attrs stay debuggable.
  - **Save-flow feedback** — `on_save` / `on_save_as` now branch on `active_edit_target_is_set` so the status message distinguishes "no stage loaded" from "write path not yet wired (v0.16)".
  - **Viewport toolbar** (item #6.5) + **first-opinion guard** (item #5) deferred — toolbar gets its own session (M effort + FEATURES.md expanded scope to 9 display modes); first-opinion guard depends on Tier 1.5 per-attr opinion FFI.

- **v0.15.0 Tier 0 — Qt Scene Browser parity** (2026-04-16). Closes the egui-vs-Qt fidelity gap caught in the UX audit before egui deletion. Workspace builds clean.
  - **CompositeProvider routing.** New `with_scene_browser_provider` helper in `bif_qt::main_window` builds a `bif_viewport::scene_browser::CompositeProvider` (USD stage + procedural prim cache + synthetic `/BIF/` paths) per call. The 4 existing tree invokables (`root_prim_count`, `root_prim_path_at`, `child_prim_count`, `child_prim_path_at`) and `prim_type_name_at` now route through it instead of raw `UsdStage::child_prim_paths` — same data path egui uses. New `Renderer::cached_scene_graph()` accessor on `bif_viewport` exposes the cache without widening the private `nodes` field.
  - **Three new invokables.** `prim_kind_at`, `prim_is_visible_at`, `prim_is_active_at` — sourced from `PrimDisplayInfo`. Specifier dropped from scope (not surfaced from composed stage; egui doesn't show it either).
  - **4-column scene browser.** `SceneBrowserModel::columnCount()` 1→4 with new `Columns` enum (Name/Type/Children/Kind). New `PrimRoles`: `KindRole`, `IsVisibleRole`, `IsActiveRole`, `ChildrenCountRole`. `PrimNode` extended with `kind`, `is_visible`, `is_active`. `headerData` + `data()` cover all columns; header now visible with section resize modes (Stretch / ResizeToContents). `populate_subtree` + `rebuild_from_state` pre-fetch the new fields per row.
  - **Row chrome — eye glyph + inactive dimming.** `PrimRowDelegate` extended: column 0 reserves space for an eye glyph (filled when visible, struck-through when hidden) before the existing layer color dot; inactive prims get reduced-alpha text across all columns via palette override. Read-only for now (toggle interactivity is Tier 1+).

- **v0.15.0 Phase F — egui bridge deletion from `bif_viewport` + `bif_viewer`** (2026-04-16). bif_viewer is now a thin shim over `bif_qt::run()`. Workspace builds + clippy clean.
  - **Renderer egui surface deleted.** Removed `egui_ctx` / `egui_state` / `egui_renderer` fields, `attach_egui` / `egui_state_mut` / `egui_ctx` / `reset_property_inspector_cache` methods. `Renderer::render` simplified to `render(&mut self, clear_color: wgpu::Color) -> Result<()>` — `raw_input` param + `Option<PlatformOutput>` return both gone. Internal `run_egui_frame` (~870 LOC) and the egui paint pass in `submit_gpu_frame` deleted.
  - **`render_ui.rs` deleted** (~729 LOC of pure egui chrome assembly).
  - **`egui-wgpu` + `egui-winit` dropped from `bif_viewport/Cargo.toml`.** Only the bridge needed them. `egui` + `egui-snarl` stay (panel modules `property_inspector` / `layer_stack_panel` / `node_graph` / `theme` remain in-tree as dead code for future cannibalization — feature-flag gating deferred until Qt replacements land).
  - **`bif_viewer` retargeted at `bif_qt::run`.** 905-line winit + egui event loop replaced with a 14-line shim. Dropped `wgpu`, `winit`, `egui`, `egui-wgpu`, `egui-winit`, `pollster` from `bif_viewer/Cargo.toml`. CLI autoload (`--usd <path>`) regression filed in BUGLIST.
  - **`bif_qt` fallout — single line.** `viewport.rs::Viewport::render` updated to drop the `None` raw_input arg.
  - **Plan adaptation.** Original Phase F also called for an `egui_panels_legacy` feature flag gating panel modules; gating cleanly required also gating `NodeGraphContext` (egui-snarl typed) — large refactor for a "future cannibalization" benefit only. Skipped as not pulling weight; revisit when those modules actually get rewritten.

- **v0.15.0 Phase E.2 finish — moves 5 / 7 / 8 / 9 + Close Stage + HiDPI** (2026-04-16, single session). Closes the remaining Phase E.2 deliverables; `bif_qt_shell` is now a live USD viewer end-to-end (pending dogfood). Build + clippy clean.
  - **Move 9 — Real timeline keyframes from `AnimatedTransform`.** Dropped `demo_keyframes: Vec<i32>`. `keyframe_count` / `keyframe_at` / `jump_to_prev_keyframe` / `jump_to_next_keyframe` now derive from the selected prim's animation via a new `selected_prim_keyframes` helper that walks `scene.working_scene.instances()` matching on `prim_path` (exact or `/BIF/...` synthetic-suffix fallback), pulls `scene.instance_animations()[idx].keyframes`, rounds to i32, sorts, dedups. `TimelineRuler` now re-paints on `selected_prim_pathChanged`.
  - **Move 5 — Gizmo pick on LMB.** New `on_prim_pick(x, y)` invokable → `Renderer::pick_instance_at` (f32 framebuffer pixels) → `scene.working_scene.instances()[idx].prim_path` → local `denormalize_synthetic_path` (inlined from bif_viewport, kept crate-private over there) → sets `selected_prim_path` + `selected_prim_type` (the type via `UsdStage::get_prim_info_by_path`). Replaces the Phase E.2 status-bar stub at `window_builder.cpp:506`. `RenderWidget::mousePressEvent` multiplies pick coords by `devicePixelRatioF()` before emit so the ray matches the DPR-scaled framebuffer.
  - **Move 7 — Scene Browser real prim tree.** New `#[qproperty(i32, scene_browser_revision)]` + `bump_scene_browser_revision` helper; bumped on stage load success + in `close_stage`. New `with_stage(|stage| ...)` helper clones the `Arc<Mutex<UsdStage>>` out of the renderer then locks (no re-entrant guard hazard). 6 new invokables: `root_prim_count`, `root_prim_path_at`, `child_prim_count`, `child_prim_path_at`, `prim_type_name_at`, `prim_display_name_at` — backed by `PrimDataProvider::root_paths` / `get_children` on `UsdStage` + inherent `get_prim_info_by_path` for type names. `SceneBrowserModel` ctor now takes `BifShellState*`; connects `scene_browser_revisionChanged` → `beginResetModel/endResetModel` (mirrors `LayerStackModel` pattern). `rebuild_from_state()` recursively walks the tree with a depth cap of 64. Falls back to demo tree when no stage loaded.
  - **Move 8 — Property Inspector real attributes + arcs.** 9 new invokables: `selected_prim_attribute_count/name_at/type_at/value_at` (4) via `UsdStage::get_prim_attributes`, plus `selected_prim_stack_count/layer_at/specifier_at/has_opinion_at/color_index_at` (5) via `UsdStage::get_prim_stack`. Specifier maps `PrimSpecifier::{Def, Over, Class}` to their UsdPrim spec strings. Color index derives from matching `PrimStackEntry::layer_identifier` against `SceneLayerState::stack.layers` indices (mod 8 for the 8-color palette). `PropertyInspectorWidget::populate_attributes` + `populate_composition_arcs` rewritten to loop real data; deleted ~55 lines of `FakeAttr` tables + `attrs_for_type`. Arc rendering gains `(inherited)` marker when an entry has no authored opinion.
  - **File → Close Stage (Ctrl+W).** New `close_stage` invokable on `BifShellState` drains GPU (`wait_for_gpu`), replaces `Renderer::scene` with `SceneManager::new()`, calls `rebuild_pick_scene()`, clears `scene_layer_state` + `current_stage_path`, resets `selected_prim_path` + `selected_prim_type`, bumps both revisions. C++ side adds a `close_stage: QAction*` to `MenuActions` (shortcut `QKeySequence::Close`), wires it in `wire_shell_actions` with a paint-tick pause around the call + swap to `central_stack` index 0. Command palette gets a `File: Close Stage` entry.
  - **HiDPI.** `viewport_on_surface_ready` + `viewport_on_resize` bridge signatures now take `scale_factor: f32`. `Viewport::new` / `Viewport::resize` pass it through to `Renderer`. Captured inside the existing `window_builder.cpp` lambdas via `viewport->devicePixelRatioF()` — no Qt signal-signature change. Combines cleanly with Move 5's DPR-aware pick coords so the ray-cast matches the DPR-scaled framebuffer.
  - **New helpers on main_window.rs.** `with_stage(|stage| ...) -> Option<R>` (stage access), `color_index_for_layer(&state, &identifier) -> i32` (layer-identifier → palette index), `denormalize_synthetic_path` (inlined copy), `bump_scene_browser_revision`.

- **v0.15.0 Phase E.2 move 4 optimization + move 6 + camera wire + polish** (2026-04-15, same session).
  - **Move 4 optimization** — `detect_timeline_from_stage` now reads from the live renderer's `UsdStage` (via the ADR-007 bridge) instead of reopening a throwaway stage per click. Removed the `bif_core::usd::UsdStage` import.
  - **Move 6 — breadcrumb wire** — `window_builder.cpp` connects `BifShellState::selected_prim_pathChanged` → `breadcrumb_set_path` with `QString::split('/', Qt::SkipEmptyParts)` for the segment trail.
  - **Camera orbit / pan / zoom wired** — `on_camera_orbit / on_camera_pan / on_camera_zoom` invokables now drive the real `Renderer.cam.camera` via `with_viewport_mut`. Orbit uses 0.005 sensitivity (matches bif_viewer); pan + zoom scale by camera-to-target distance.
  - **GPU cleanup on exit** — new `Renderer::wait_for_gpu()` blocks until all in-flight GPU submissions drain. Called from `viewport_on_shutdown` before dropping the `Viewport` — prevents D3D12 `OBJECT_DELETED_WHILE_STILL_IN_USE` corruption errors on close.
  - **Double-init guard** — `viewport_on_surface_ready` skips re-creation when `cb.viewport.is_some()`. Fixes the crash where `window->setVisible(false/true)` around `QFileDialog` re-triggered `showEvent` → a second `Renderer::new` dropped the in-flight first Renderer.
  - **Shared Open Stage flow** — new `trigger_open_stage()` helper in `window_builder.cpp` unifies File → Open menu, first-launch "Open USD" button, and recent-stage clicks through a single code path. First-launch screen and recents now actually load the USD (previously Phase-B status-echo stubs).
  - **File dialog UX — paint-tick pause** — instead of hiding the main window around `QFileDialog::getOpenFileName` (which made the whole app briefly vanish), `RenderWidget` now exposes `pausePainting` / `resumePainting` (the 16ms QTimer becomes a member variable). `trigger_open_stage` pauses the tick around the file picker so the native dialog stops fighting the wgpu paint loop for z-order.
  - **Log filter** — wgpu_core/wgpu_hal/naga filtered to WARN/ERROR in the env_logger init. Stops `Device::maintain: waiting for submission index …` INFO spam at 60 FPS, plus noisy Vulkan layer warnings from Galaxy Overlay / Rockstar Social Club.

- **v0.15.0 Phase E.2 move 2 — real stage load wired through `BifShellState` invokable** (2026-04-15). `on_stage_path_opened` (fired by File → Open's `QFileDialog`) now drives the full scene load through the renderer, replacing its Phase E.1 status-echo stub. Uses the ADR-007 bridge: `with_viewport_mut(|vp| vp.renderer_mut().load_usd_scene(&path))` runs `bif_viewport::Renderer::load_usd_scene` synchronously (C++ bridge + GPU upload, matches `bif_viewer`'s behavior). On success, clones `renderer.scene.layer_state` onto `BifShellState::scene_layer_state` and bumps `layer_state_revision` — the Layer Stack panel now shows real layers instead of the demo seed. Three failure paths with distinct status messages (viewport not ready, stage load error, null-pointer safety fallback).
- **ADR-007 — `BifShellState` ↔ `ViewportCallbacks` bridge** (`wiki/architecture/adr/007-shell-state-to-viewport-bridge.md`). Locks in the β pattern (thread-local raw pointer) for cxx-qt invokables to reach the live `Viewport` / `Renderer`. Rationale: Qt UI single-threaded → `thread_local!<Cell<*mut _>>` is sound + lock-free. Encapsulated in a `with_viewport_mut(|vp| ...)` helper with one small `unsafe` block and a documented lifetime invariant (`ViewportCallbacks` lives on `app.rs`'s stack for the full event loop; install runs before the loop starts). Reversible — swap internals for an `Arc<Mutex<_>>` without changing call-site APIs if threading is ever needed.
- **`bif_qt::main_window::install_viewport_callbacks` + `with_viewport_mut`** (new). The ADR-007 implementation. `app.rs::run` calls `install_viewport_callbacks(&mut viewport_cb)` before `bif_qt_run_shell`. Invokables call `with_viewport_mut(|vp| ...)` which returns `Option<R>` — gracefully no-ops when the pointer is null or the viewport's surface isn't yet ready / already torn down.

- **v0.15.0 Phase E.2 move 4 — real timeline detection from USD stage** (2026-04-15). Timeline toolbar's ⇅ detect-from-stage button now reads actual USD time metadata. `BifShellState::on_stage_path_opened` stores the path in a new Rust-side `current_stage_path: Option<PathBuf>` field. `detect_timeline_from_stage` opens a throwaway `UsdStage` from that path, calls the existing `UsdStage::get_timeline()` (which wraps `GetStartTimeCode` / `GetEndTimeCode` / `GetTimeCodesPerSecond` via `cpp_bridge::usd_bridge_get_timeline`), and writes `start_frame` / `end_frame` / `playback_fps` qproperties. Logs when the stage has no authored time range. Status messages on success and both failure modes (path not set, stage open fails, get_timeline fails). Standalone implementation — no shared stage handle yet. Inefficient (reopens the stage per click) but gets the UI correct; move 2 + the architecture decision that follows will replace it with a shared stage driving the viewport.

### Changed

- **v0.15.0 Phase E.2 move 1 — `bif_qt::Viewport` hosts real `bif_viewport::Renderer`** (2026-04-15). Replaces the triangle demo. `bif_qt` now depends on `bif_viewport`. `Viewport::new` builds wgpu primitives from the Qt-native HWND, requests a device with `bif_viewport::REQUIRED_FEATURES` + `required_limits()`, and hands everything to `Renderer::new(surface, device, queue, config, size, scale_factor)`. Frame ticks render via `renderer.render(clear_color, None)` (no egui — Qt owns the UI). Resize + surface-lost recovery thread through `renderer.resize(size, scale_factor)`. `Viewport::renderer_mut()` + `ViewportCallbacks::viewport_mut()` expose the renderer to future scene-load invokables (moves 2+4). `scale_factor` hardcoded to 1.0 until `QScreen::devicePixelRatio()` is wired. Removed the inline triangle pipeline, shader file (`crates/bif_qt/shaders/triangle.wgsl`), and `build_triangle_pipeline` helper.

- **v0.15.0 Phase E.2-prep — `bif_viewport::Renderer` decoupled from winit** (2026-04-15). Foundation refactor that unblocks Phase E.2 move 1 (pulling Renderer into `bif_qt` from a raw HWND). `Renderer::new` now takes `(surface, device, queue, config, size, scale_factor)` — pure wgpu primitives, synchronous; the winit-coupled `Arc<winit::Window>` dep is gone. New optional-overlay API: `attach_egui(egui_state)`, `egui_state_mut()`, `egui_ctx()`, `set_dialog_focus_hook(Fn(bool) + 'static)`. `render()` now takes `Option<egui::RawInput>` and returns `Result<Option<egui::PlatformOutput>>` — headless when `None`, egui overlay when `Some`. `resize()` signature adds `scale_factor: f32`. `REQUIRED_FEATURES` + `required_limits()` constants exposed so callers request the right wgpu device. Zero direct `winit::*` imports in `bif_viewport/src/lib.rs` + `render.rs` code (doc comments + `egui_winit::State` transitive type only). `bif_viewer` keeps working via a new `create_renderer(window)` helper in `main.rs` that owns the wgpu/egui setup and installs the dialog-focus hook. Removed: `handle_egui_event` (callers now drive `egui_state.on_window_event` directly), old `resize((u32, u32))` signature, internal `window` field.

### Fixed

- **Animation playback now updates the viewport** (2026-04-17). `Renderer::update_animation` only ran from the deleted egui frame loop; in Qt mode the QTimer advanced `current_frame` but nothing pushed it through to the renderer. Extracted the eval loop into `apply_animation_at_current_frame()`, added `Renderer::set_time(frame: f64)` that snaps the timeline + delegates to the helper, and wired a Qt `current_frameChanged` → new `on_frame_changed` invokable → `Renderer::set_time`. Covers both QTimer-driven playback and scrubber drags. Closes BUGLIST entry from 2026-04-17.
- **Loading a second USD now evicts the first** (2026-04-17). Both the viewport and scene browser tree showed leftover prims from the previous stage on a second open. Added `Renderer::reset_scene_state()` that drains GPU + replaces `SceneManager` + clears all `nodes.*` caches (cached_scene_graph, instancer_results, node_proto/cloud maps, prim counts) + rebuilds pick BVH. `bif_qt::on_stage_path_opened` calls it before loading when a prior stage is set; `bif_qt::close_stage` (Ctrl+W) collapsed onto the same helper, closing a latent procedural-cache leak in the close path too.
- **Property inspector per-attribute opinion color** (2026-04-19, Tier 1.5). Each attribute row in the property inspector now shows the color of its own winning opinion source instead of the same prim-level color for every row. `selected_prim_attr_color_index_at(i)` calls `get_attribute_opinions(prim, attr)` → finds the entry where `is_winning == true` → maps its layer identifier to the palette index. Replaces the previous approach that used `selected_prim_stack_color_index_at(0)` for every attribute row.

- **Scene browser layer color dots now appear next to prims** (2026-04-17). `populate_subtree` and `rebuild_from_state` were passing `color_index = -1` for every row, so `PrimRowDelegate` never painted the layer dot. Added `prim_color_index_at(path)` invokable that reads `SceneLayerState::layer_for_prim` (prim_path → strongest opinion source layer) and returns the palette index mod 8. Wired into both row builders.

### Added

- **v0.15.0 Phase E.1 — Input + event wiring (stubs)** — interactivity layer without real USD yet. Phase E.2 wires bif_renderer + scene_loader on top.
  - **Viewport input** — RenderWidget now overrides mousePress/Move/Release + wheel. LMB (no mods) → `primPickRequested(x, y)` signal (Phase E.2 ray-casts). Alt+LMB drag → `cameraOrbit(dx, dy)`. MMB drag → `cameraPan(dx, dy)`. Wheel → `cameraZoom(angle)`. 4 new `#[qinvokable]`s on `BifShellState` (`on_camera_orbit/pan/zoom`, `on_frame_selected`) forward status echoes until E.2 dispatches to a real Renderer.
  - **Keyboard shortcuts** — F = frame-selected (ApplicationShortcut). Space = play/pause. Left/Right = step ±1 frame. Shift+Left/Right = prev/next keyframe. All routed through a new `ShortcutRegistry` that reads `QSettings("shortcuts/<id>")` for user overrides with compiled-in fallbacks — v0.16 Preferences dialog will plug in without callsite changes.
  - **Real QFileDialog on File → Open** — selects file, stores path in `QSettings("recent_stages")`, flips central stack to viewport. Phase E.2 wires `bif_core::scene_loader::load_usd_scene`.
  - **Timeline QTimer-driven playback** — advances `current_frame` while `is_playing`. Interval retunes on fps / realtime change. Loop vs stop-at-end controlled by new `loop_playback` qproperty.
  - **Nuke-inspired timeline toolbar** — 3-zone layout via expanding QWidget spacers: LEFT (fps spinbox, RT toggle, Loop toggle), CENTER (⏮ ◀ ▶Play ▶ ⏭ + visually-dominant orange frame counter), RIGHT (Start/End spinboxes, ⇅ detect-from-stage). 5 new qproperties on `BifShellState` (`playback_fps`, `realtime_playback`, `loop_playback`, new invokables `jump_to_prev/next_keyframe` + `detect_timeline_from_stage` stub).
- **`cpp/shortcut_registry.{h,cpp}`** — centralized `QKeySequence` lookup with string-ID action registry (`kTimelinePrevFrame`, `kCameraFrameSelected`, etc.) and QSettings override path. `lookup(id, default)` for callers, `set_override(id, seq)` for a future Preferences UI.

### Added

- **v0.15.0 Phase D — Secondary panels (Timeline, Node Graph, Render Settings)** — replaces the last 3 placeholders.
  - **D.1 Timeline (`cpp/timeline_widget.{h,cpp}`)** — Toolbar (Prev/Play/Next/frame spinbox/range) + `TimelineRuler : QWidget` with custom `paintEvent`: frame range ticks (major every 20, minor every 10), orange keyframe diamonds, blue playhead triangle + vertical line. Scrub via mouse drag/press. 4 new qproperties (`current_frame`, `start_frame`, `end_frame`, `is_playing`) + 2 invokables (`toggle_playback`, `step_frame`) + demo keyframes via `keyframe_count`/`keyframe_at`. Phase E wires the timer-driven frame advance + real `AnimatedTransform` keyframes.
  - **D.2 Node Graph (`cpp/node_graph_widget.{h,cpp}`)** — Replaces egui-snarl. `QGraphicsScene` + `NodeGraphView : QGraphicsView` subclass with wheel-zoom (`AnchorUnderMouse`) + middle-mouse pan (translates to `ScrollHandDrag`). `BifNodeGraphicsItem : QGraphicsObject` per node: rounded body + category-colored header (blue=Composition, orange=Operation, green=Render, purple=Environment), input/output pin lollipops with type-coded colors (green=scene, orange=image, blue=env), pin labels. `BifNodeWire : QGraphicsPathItem` — bezier wires that refresh on `moved()` signal from endpoint nodes. Hardcoded 5-node demo graph (Hero/UsdRead → Scatter100 → Offset/Xform → Beauty/IvarRender, with Studio/HdriEnvironment into IvarRender's env input) until Phase E wires `NodeGraphContext` + `SceneNode` enum.
  - **D.3 Render Settings (`cpp/render_settings_widget.{h,cpp}`)** — `QFormLayout` inside two styled `QGroupBox`es (Path Tracer: spp, max depth, SHARC; Post-Processing: exposure, gamma, OIDN denoising). Values stored locally; Phase E wires `bif_renderer::RenderConfig`.
- **Bottom dock now tabified** — Node Graph + Timeline share the bottom tab area. Render Settings tabified with Property Inspector on the right. Workspace presets updated so each preset targets a sensible panel subset.

### Added

- **v0.15.0 Phase C — Core panels (Layer Stack, Scene Browser, Property Inspector)** — first real panels on the Qt shell.
  - **C.1 Layer Stack (`cpp/layer_stack_{model,widget}.{h,cpp}`)** — `QListView` backed by `LayerStackModel : QAbstractListModel`. Reads through 9 new `#[qinvokable]` methods on `BifShellState` (`layer_count`, `layer_name_at`, `layer_depth_at`, `layer_color_index_at`, `layer_muted_at`, `layer_is_working`, `layer_identifier_at`, `isolation_mode_active`, + 3 mutators `set_layer_muted`/`set_working_layer`/`toggle_isolation_mode`). Model listens to the auto-generated `layer_state_revisionChanged` signal (new `#[qproperty(i32, layer_state_revision)]` bumped on every mutation). `LayerRowDelegate` paints layer-color dot + indent, with `editorEvent` override so checkbox hit-testing aligns with the visually-shifted checkbox. Double-click row → set as working layer (bolds). Isolation toolbar toggle wires through `toggle_isolation_mode`. `seed_demo_layer_stack` invokable constructs a 3-layer fake stack (root/shot/anim) mirroring `test_assets/layers/root.usda` for Phase C.1 visual testing.
  - **C.2 Scene Browser (`cpp/scene_browser_{model,widget}.{h,cpp}`)** — `QTreeView` backed by `SceneBrowserModel : QAbstractItemModel` (real tree: `index`/`parent`/`rowCount`). Each `PrimNode` has name, type, path, color index, parent pointer, `std::vector<std::unique_ptr<PrimNode>> children` (Qt's QList/QVector can't hold move-only types). Hardcoded 10-prim demo tree (World / Hero / Geom / Body / Head / Skel / Skeleton / Sky / Sun / Dome / Ground) until Phase E. Custom `PrimRowDelegate` paints a 6px color dot before the branch decoration. `HierarchicalFilter : QSortFilterProxyModel` keeps parents visible when a descendant matches the search filter. Selection routes through `BifShellState::setSelected_prim_path / setSelected_prim_type`.
  - **C.3 Property Inspector (`cpp/property_inspector_widget.{h,cpp}`)** — Listens to the new `#[qproperty(QString, selected_prim_path)]` + `selected_prim_type` signals on `BifShellState`. Layout: path header + blue type tag, collapsible `Composition Arcs` group (currently mirrors the layer stack strongest-first with def/over tags — Phase E wires real `UsdStage::get_prim_stack`), `QTabWidget` with Attributes (QTableView + QStandardItemModel + `OpinionDotDelegate` painting winning-layer color on the Name column) and Relationships (placeholder for C.4). Fake attribute sets per prim type (Mesh → points/faceVertexCounts/normals/extent/displayColor, Xform → translate/rotate/scale/xformOpOrder, Light → intensity/exposure/color/angle, Skeleton → joints/bindTransforms, Scope/SkelRoot → visibility/purpose).
- New `BifShellState` qproperties: `layer_state_revision: i32`, `selected_prim_path: QString`, `selected_prim_type: QString`. Rust-side field `scene_layer_state: Option<SceneLayerState>` (bif_core type) wrapped by invokables.

### Added

- **v0.15.0 Phase B (slices 5–8) — workspaces, first-launch, breadcrumb, command palette** — Phase B complete.
  - **B.5 Workspace switcher** — 4 presets (Assembly / Lighting / Materials / Render) persist `QMainWindow::saveState()` byte arrays in `QSettings`. Switching saves the outgoing preset's current layout, then restores the incoming one (or applies a hardcoded default for that preset on first switch). Last-active preset persisted in `workspaces/last_active`. New `current_workspace` qproperty on `BifShellState`. Defaults visible: Assembly = all docks; Lighting = Layer Stack + Node Graph hidden; Materials = only Property Inspector + Node Graph; Render = only Property Inspector.
  - **B.6 First-launch screen** — new `FirstLaunchWidget` (`cpp/first_launch_widget.{h,cpp}`) with "Welcome to BIF" title + tagline + two card buttons (📄 New Stage, 📂 Open Stage) + Recent Stages list pulled from `QSettings("recent_stages")`. Cards emit `newStageClicked` / `openStageClicked` signals; window builder routes to `BifShellState::on_*` invokables and flips the central QStackedWidget to viewport.
  - **B.7 Breadcrumb bar** — `QToolBar` ("breadcrumb_bar") above the central stack. Phase B stub shows "(no stage)" segment; `breadcrumb_set_path(QStringList)` helper rebuilds segments with `›` separators (Phase C calls it on prim selection).
  - **B.8 Command palette (Ctrl+P)** — borderless modal `CommandPalette : QDialog` (`cpp/command_palette.{h,cpp}`) with `QLineEdit` + `QListView` + `QSortFilterProxyModel` over a `QStringListModel`. 11 commands aggregated from `MenuActions`. Substring case-insensitive filter (Phase B); fuzzy scorer deferred to v0.16. Up/Down navigates without leaving the search field; Enter triggers + closes; Esc cancels. Centered overlay positioned over the main window.
- **Layout restructure** — central widget is now a `QWidget` container with `QVBoxLayout`(breadcrumb `QToolBar` + `QStackedWidget`(first-launch | viewport)) instead of the bare `RenderWidget`.

### Added

- **v0.15.0 Phase B (slices 1–4) — Qt shell functional** — `bif_qt_shell` now a live shell with wgpu viewport, styled panels, menu actions, and zen mode. Slices landed in one commit:
  - **B.1 Viewport** — `cpp/render_widget.{h,cpp}` + `shaders/triangle.wgsl` + `src/viewport.rs` ported from `bif_qt_spike`. RenderWidget is the central widget; wgpu surface feeds directly into the Qt-native HWND. Rust-opaque `ViewportCallbacks` wraps `Option<Viewport>` and plugs into the cxx-qt bridge through an `extern "Rust"` block. C++ wires `RenderWidget::{surfaceReady, resized, frameRequested}` signals to cxx-generated trampolines that forward to `viewport_on_*` free functions.
  - **B.2 Stylesheet** — `theme::qt_stylesheet()` now applied via `QApplication::setStyleSheet` at shell startup. `bif_qt_run_shell(viewport_cb, stylesheet)` takes a `rust::Str` arg (cxx's `&str`), C++ converts with `QString::fromUtf8` and installs.
  - **B.3 Menu actions** — `build_menu_bar` now returns a `MenuActions` struct with pointers to all `QAction`s. New `wire_shell_actions` helper connects each trigger to a cxx-qt invokable on `BifShellState`. 5 new `#[qinvokable]` methods (`on_new_stage`, `on_open_stage`, `on_save`, `on_save_as`, `on_about`) — all Phase B stubs that update `status_message`. Keyboard shortcuts via `QKeySequence::New / Open / Save / SaveAs` + `Ctrl+Q` for Exit (hardcoded because `QKeySequence::Quit` is empty on Windows) + `Ctrl+1..4` for workspaces + `Ctrl+\` for zen mode.
  - **B.4 Zen mode** — checkable `Zen Mode` action; `QAction::toggled` fires a lambda that iterates `QDockWidget` children via `findChildren` and toggles their visibility. Menu bar + status bar stay visible.
- **v0.15.0 Phase A — `crates/bif_qt/` scaffolding** — new Qt UI crate replacing bif_viewport's egui panel layer. First real `#[cxx_qt::bridge]` in BIF: `BifShellState` QObject with two `#[qproperty(QString)]` fields (`title`, `status_message`) + one `#[qinvokable]` method (`describe`). C++ side (`cpp/window_builder.cpp`) assembles QMainWindow with menu bar (File/View/Help + Ctrl+N/O/S/Q/1-4/\\ accelerators), 4 movable/floatable QDockWidgets (Scene Browser / Layer Stack / Property Inspector / Node Graph — all placeholders for Phase B), central viewport placeholder, and status bar. Standalone `bif_qt_shell` binary for Phase A/B dogfooding (deleted at Phase F when bif_viewer switches over). `src/theme.rs` ports 34 color constants from `bif_viewport/src/theme.rs` as UI-agnostic `Color(u8, u8, u8, u8)` + `qt_stylesheet()` generator covering QMainWindow, QDockWidget, QMenuBar, QMenu, QStatusBar, QPushButton, QLineEdit, QTreeView, QTableView, QListView, QHeaderView, QTabWidget, QTabBar, QScrollBar. 4 unit tests on theme constants + stylesheet generation.
- **v0.15.0 Phase 0 spike — wgpu into Qt 6 QWidget (gate PASSED 2026-04-13)** — new `crates/bif_qt_spike/` crate proves end-to-end that wgpu renders into a Qt-native HWND with MSVC 2022 + Qt 6.8.3 LTS. Stack: `qt-build-utils 0.7` finds Qt → `cxx 1.0` + `cxx_build` bridges Rust↔C++ → custom `RenderWidget : QWidget` with `WA_NativeWindow` + `WA_PaintOnScreen` + `paintEngine() == nullptr` yields a raw HWND → `wgpu::Surface` configured in sRGB + HighPerf adapter → triangle draws cleanly + resizes. 6 C++/Rust files + `shaders/triangle.wgsl` + `setup_qt_env.ps1`. Spike crate scheduled for deletion at Phase H.
- **ADR-006 — Qt 6 via cxx-qt** — `wiki/architecture/adr/006-qt-via-cxx-qt.md` records the binding decision (cxx-qt 0.7 + Qt 6.8 LTS + LGPL dynamic linking), the wgpu-QWidget embedding recipe, migration strategy (shell-first on `v0.15-qt` branch, merge at Phase H), and 4 captured gotchas (MSVC `/Zc:__cplusplus`, `QT_NO_KEYWORDS` trade-off, moc driving, argc lifetime).
- **`setup_qt_env.ps1`** — companion to `setup_usd_env.ps1`. Sets `Qt6_DIR`, `CMAKE_PREFIX_PATH`, `QT_PLUGIN_PATH`, `QML2_IMPORT_PATH`, and prepends Qt bin + CMake_64 + Ninja to `PATH`.
- Workspace-level Qt/FFI dependencies: `raw-window-handle 0.6`, `cxx 1.0`, `cxx-qt 0.7`, `cxx-qt-lib 0.7` (qt_full feature), `cxx-qt-build 0.7`, `qt-build-utils 0.7`.

### Fixed

- **Muting the def-providing layer clears the viewport cleanly** — previously `load_usd_with_stage_muted` bailed with `LoadError::NoGeometry` whenever composition resolved to zero prims (e.g. the user muted `anim.usda` which carried the `def Cube` in the test fixture), the reload failed, and the viewport kept showing the pre-mute cube — UI desynced from mute state. Split the empty-scene gate: the strict `load_usd_with_stage` still returns `NoGeometry` (first-load callers need geometry), but the permissive muted variant returns the empty scene successfully. `finalize_usd_scene` in `bif_viewport` now recognises the empty case, installs the stage handle + re-seeds `SceneLayerState` so the Layer Stack panel can still unmute, clears `working_scene` + `mesh_data`, and calls `reload_working_scene` to flush GPU buffers.
- **Layer mute now updates the viewport geometry** — previously, `UsdStage::set_layer_muted` recomposed the stage but the C++ bridge's cached mesh data and the GPU vertex buffers were still pre-mute, so toggling mute had no visible effect. Fix has three parts: (1) new `load_usd_with_stage_muted(path, muted_identifiers)` in `bif_core::usd::loader` applies layer mutes immediately after opening the stage and before payloads load, so the bridge's first cache pass composes under the mutes; (2) `scene_loader::load_usd_scene` snapshots `SceneLayerState.muted` and feeds it to the muted variant — the user's mute set survives the implicit stage reopen that every reload triggers; (3) the `AppEvent::LayerMuteToggled` dispatch arm routes the reload through `handle_node_graph_event(LoadUsdFile)` instead of calling `load_usd_scene` directly, so the UsdRead node's `node_proto_map` is cleaned up via `remove_and_reindex_prototype` + `compact_materials` before the reload — otherwise `working_scene.prototypes` accumulates new geometry on top of stale entries and the viewport renders two cubes where one is expected.
- **Layer Stack panel mute/radio clicks now propagate** — `run_egui_frame` takes the `EventBus` out of `Renderer` via `std::mem::take` at frame start; existing UI code emits to the local binding. The `LayerStackPanel` mount site was passing `&mut self.event_bus` (the empty placeholder) so every `LayerSelected` / `LayerMuteToggled` / `WorkingLayerChanged` / `IsolationModeToggled` vanished before `dispatch_events` ran. Route to the local `event_bus` like every other panel. Also swaps the empty-label radio/checkbox for single-char `W` and `M` labels so the hit area is usable.

### Changed

- **`test_assets/layers/` fixture now demos muting visibly in the viewport** — original fixture authored `/Hero.xformOp:translate` in all three layers with root winning, so muting any sublayer produced no visible change (USD forbids muting the root layer). Rewrote: root carries no prim opinions, shot authors translate=(3,0,0) + red, anim defines a unit cube at origin + white. Muting shot now snaps the cube back to origin and flips it white; muting anim removes it entirely. Updated 4 integration tests: attribute-opinion count 3 → 2, prim-stack count 3 → 2, mute asserts shot → 1 opinion from anim. New demo matrix documented in `root.usda` doc string.

## [0.14.0] - 2026-04-13

### Added

- **v0.14.0 Phase A (in progress)** — layer-aware USD FFI surface: `SdfLayer` stack walk, `UsdPrim::GetPrimStack`, `UsdAttribute::GetPropertyStack` (opinion sources), layer mute/unmute, layer offset read, explicit `PayloadPolicy::{LoadAll, LoadNone}` stage open. 9 new C types + 8 extern "C" fns + 4 destructors in `cpp/usd_bridge/`, mirrored `#[repr(C)]` types in `crates/bif_core/src/usd/ffi_raw.rs`. Values are rendered as display strings via `TfStringify` (read-only; editing lands in v0.16).
- **v0.14.0 Phase B (in progress)** — safe Rust wrappers on top of Phase A. New `crates/bif_core/src/usd/layer.rs` module with UI-agnostic types: `LayerStack` (with `find_by_identifier` + `children_of` tree helpers), `LayerInfo`, `LayerOffset`, `PrimStackEntry`, `PrimSpecifier`, `OpinionSource`, `EditTarget`, `PayloadPolicy`. Seven new `UsdStage` methods: `open_with_policy`, `get_layer_stack`, `get_edit_target`, `set_layer_muted`, `get_layer_offset`, `get_prim_stack`, `get_attribute_opinions`. Eight new `convert_*` fns in `ffi_convert.rs` + 13 unit tests covering struct conversion, winning-index flag propagation, specifier mapping, null-pointer paths, and `LayerStack` tree navigation — all run without USD env.
- **v0.14.0 Phase C (in progress)** — `SceneLayerState` attached to `bif_core::Scene` as `layer_state: Option<SceneLayerState>`. Tracks the sublayer stack, working-layer index, muted set, isolation-mode flag, payload policy, and a `prim_path → strongest-layer-index` map for scene-browser color dots. Constructor `SceneLayerState::from_stage` seeds state from a loaded `UsdStage`; `populate_layer_for_prim` walks prim paths and records each prim's authoring layer. 6 new unit tests. LRU opinion cache planned in the Phase C scope was deferred — keeping the struct pure-data preserves `Scene: Clone` and leaves caching as a UI-layer concern if needed later.
- **v0.14.0 Phase D scaffolding (in progress)** — UI infrastructure for layer-aware panels: `LAYER_COLORS: [Color32; 8]` HSL-spaced palette + `layer_color(index)` helper in `bif_viewport/src/theme.rs`; five new `AppEvent` variants (`LayerSelected`, `LayerMuteToggled`, `WorkingLayerChanged`, `PayloadPolicyChanged`, `IsolationModeToggled`) in `app_event.rs`; stub dispatch arms in `Renderer::dispatch_events` that log incoming layer events until the panels come online.
- **v0.14.0 scene loader wiring (in progress)** — `scene_loader.rs::finalize_usd_scene` now seeds `scene.layer_state` via `SceneLayerState::from_stage` and populates `layer_for_prim` by walking `stage.all_prims()`. USD-loaded scenes carry the sublayer tree + color-dot map end-to-end; procedural-only scenes still have `layer_state: None`. Logs counts (layers / muted / prims mapped) at load time for diagnostics.
- **v0.14.0 Layer Stack panel (in progress)** — new `crates/bif_viewport/src/layer_stack_panel.rs`: egui panel that reads `&SceneLayerState` and emits `AppEvent`s (layer select, mute toggle, working-layer change, isolation toggle). Renders the sublayer tree as an indented list with layer color dot, working-layer radio, mute checkbox, layer-name label (strikethrough when muted, bold when working), and authored offset label when non-identity. 4 new unit tests incl. headless egui render smoke tests.
- **v0.14.0 Layer Stack panel wiring (in progress)** — `LayerStackPanel` now lives on `Renderer` and renders inside a collapsing header at the top of the left sidebar above the scene browser. Real dispatch handlers replace the log stubs: `LayerMuteToggled` calls `UsdStage::set_layer_muted` (resolves layer index → identifier, locks the `Arc<Mutex<UsdStage>>`) and mirrors the change into `SceneLayerState::set_muted`; `WorkingLayerChanged` updates `SceneLayerState::working_layer`; `IsolationModeToggled` updates the UI-hint flag; `PayloadPolicyChanged` records the user's choice and defers the stage reopen to v0.14.5.
- **v0.14.0 Scene browser color dots (in progress)** — `scene_browser::render_scene_browser` + `render_prim_row` accept an optional `layer_for_prim: &HashMap<String, usize>` threaded from `SceneLayerState`. Each prim row now shows a 3 px color dot between the type icon and the prim name, colored via `theme::layer_color(index)`. Missing when no stage is loaded or the prim isn't in the USD stage (procedural-only prims), so the UI gracefully degrades.
- **v0.14.0 Composition arcs in property inspector (in progress)** — `PrimProperties` gained a `composition_arcs: Vec<PrimStackEntry>` field populated from `UsdStage::get_prim_stack` on prim selection. A new `Composition Arcs (N)` collapsing header at the top of the Attributes tab lists every layer contributing an opinion on the selected prim, ordered strongest-first. Each row shows the specifier (`def` / `over` / `class`), the layer identifier, and the strongest opinion is bold; others dimmed.
- **v0.14.0 Per-attribute opinion inspector (in progress)** — `PrimProperties` gained an `opinion_traces: HashMap<String, (Vec<OpinionSource>, Option<usize>)>` map populated from `UsdStage::get_attribute_opinions` for each authored attribute with multi-layer opinions. In the Attributes Grid, such attributes get a 3 px color dot prefix matching the winning layer's palette color. Hovering the dot opens a tooltip listing every contributing layer (winning marked `▶` + bold), each paired with its authored display value. Single-opinion / unauthored attributes render unchanged, keeping the tabular layout clean.
- **v0.14.0 Phase E — layer-aware fixture + integration tests (in progress)** — new `test_assets/layers/root.usda`, `shot.usda`, `anim.usda` — a 3-layer sublayer stack where `/Hero.xformOp:translate` has a three-opinion stack (root=100,200,300 → shot=10,20,30 → anim=1,2,3). 4 new integration tests in `crates/bif_core/src/usd/cpp_bridge.rs`: `test_get_layer_stack_on_multilayer_fixture` asserts depth + parent_index shape; `test_attribute_opinions_winning_layer` asserts strongest-first ordering with the authored (100,200,300) winning; `test_mute_layer_recomposes_opinions` mutes `shot.usda` → reasserts the opinion stack shrinks → unmutes → reasserts restored (captured a USD semantic: `RequestLayerMuting` forbids muting the root layer, so the test mutes a sublayer instead); `test_prim_stack_lists_all_layer_opinions` covers `UsdStage::get_prim_stack` with specifier mapping for the `def Xform` from `anim.usda`.

## [0.13.6] - 2026-04-12

### Added

- **v0.13.6 UsdSkelBlendShape — CPU morph target deformation** — blend shapes load from USD, evaluate per-frame, and compose with skinning (shapes applied before LBS).
  - **C++ FFI** — `UsdBridgeBlendShapeTarget` + `UsdBridgeBlendShapeBindingData` structs. `usd_bridge_get_blend_shape_binding_count/get/compute_weights` functions. Dense-expanded in C++ (zero-padded, `pointIndices` scattered at load), shape-order remap built per-mesh vs `UsdSkelAnimation::blendShapes`. `UsdSkelAnimQuery` cached per skeleton for weight eval.
  - **Rust FFI** — `RawBlendShapeTarget`/`RawBlendShapeBindingData` in `ffi_raw.rs`, safe wrappers in `cpp_bridge.rs` (`blend_shape_binding_count`, `get_blend_shape_binding`, `compute_blend_shape_weights`), conversion in `ffi_convert.rs`.
  - **`Mesh::blend_shapes` + `Mesh::bind_normals`** — `BlendShapeTarget` (name, dense offsets, optional normal offsets) and `BlendShapeBinding` (targets + FFI binding index). `bind_normals` snapshot parallels `bind_positions`.
  - **`skinning::apply_blend_shapes()`** — linear delta accumulation: `out[i] += target.offsets[i] * weight` per target. Weights unclamped (USD spec — exaggeration/anti-shapes legal). Normals not renormalized (downstream `skin_normals` handles it).
  - **Pipeline order** — blend shapes applied to `bind_positions` → scratch buffer → fed as input to `skin_positions`/`skin_normals`. Both inline and multi-draw playback paths updated.
  - **Loader** — walks bridge blend shape bindings, matches by mesh path, attaches `BlendShapeBinding`, snapshots `bind_normals`.
  - **6 new unit tests** — passthrough, single@1.0, two@0.5, normal deltas, unclamped weights, shapes+skin composition order.
  - **Test asset** — `test_assets/skel/two_bone_arm.usda` extended with 2 BlendShape prims (`squash`, `twist`) + animated `blendShapeWeights` over frames 0-36.
  - **GPU path stub** — `bif_renderer::gpu_blend_shapes::GpuBlendShapeLayout` reserves data layout for future GPU skinning.
  - **Known limits:** `UsdSkelInbetweenShape` deferred. Normal deltas required on the BlendShape prim for accurate shading — when absent, skinning uses bind-pose normals (documented). GPU path CPU-only (stubs only).

### Changed

- **Architecture refactor campaign closed** — `ARCHITECTURE_REFACTORS.md` and `ARCHITECTURE_REVIEW.md` updated to reflect that all 5 phases (FFI split, cross-platform, node graph eval engine, scene pipeline, renderer hub decomposition) and 7 of 8 prioritized review items have shipped across v0.13.0 → v0.13.5. §10 table now carries a Status column with commit references. `scene_loader.rs` shrinkage logged as a deferred Phase 4.5 follow-up pending v0.14.0 layer-aware rewrite.
- **Node graph extension checklist** — `wiki/architecture/node-graph-system.md` now has a concrete 10-step "Adding a New Node Type" reference card covering the eval engine wiring, persistence round-trip, and `node_dispatch.rs` event handler.

### Fixed

- **Rigid-skinned mesh offset on multi-joint rigid bindings** — `SkinKind::Rigid` compression in `crates/bif_core/src/usd/loader.rs` assumed `element_size == 1, weight == 1.0`, but USD's `UsdSkelSkinningQuery::IsRigidlyDeformed()` also covers meshes with uniform per-prim multi-bone influence. Taking only `joint_indices[0]` + `joint_weights[0]` for hair (3 head/neck bones at w=0.333 each) and fingernails (2 tip bones at w=0.5 each) collapsed every vertex by the fractional weight, visually shrinking the mesh toward its first bone's origin. Loader now gates the compact `SkinKind::Rigid` encoding on `element_size == 1` only; multi-joint rigid meshes broadcast their single authored influence block across post-split vertices and flow through `SkinKind::PerVertex`. Affected: HumanFemale hair, fingernails on accessory meshes. New regression test `skinning::tests::rigid_matches_pervertex_single_influence` locks `SkinKind::Rigid{J, 1.0}` to match equivalent `SkinKind::PerVertex{[J;N],[1.0;N], 1}`. Bug was pre-existing since v0.13.5.2.
- **`persistence.rs` path-relativization tests now cross-platform** — `path_relativization_*` tests used to hardcode `D:\\projects\\...` literals. Rewrote with `#[cfg(windows)]` / `#[cfg(not(windows))]` constants so the tests exercise the same logic on both targets. `sample_project()` `file_path` dropped its `D:\\` prefix (round-trip serde test doesn't hit the filesystem).

## [0.13.5] - 2026-04-10

### Added

- **v0.13.5 UsdSkel import + CPU linear blend skinning** — skinned characters load, render at bind pose, and deform per-frame when scrubbing the timeline.
  - **C++ bridge refactor** — `cache_skeleton_data()` now uses `UsdSkelCache` + `UsdSkelSkeletonQuery` + `UsdSkelSkinningQuery` via `UsdSkelRoot::ComputeSkelBindings`, replacing raw attribute reads. Persistent `skel_cache` member on `UsdBridgeStage` enables per-time-code eval without re-populating. Fixed a latent SSO-related UAF in `joint_path_ptrs` fixup.
  - **New FFI** — `usd_bridge_compute_skel_skin_xforms(stage, skel_idx, time_code, out, capacity)` evaluates cached SkeletonQuery at a time code and writes joint-skel transforms into a caller-allocated buffer. Rust wrapper `UsdStage::compute_skel_xforms(skel_idx, t)`.
  - **`bif_core::skinning` module** — `compute_skin_matrices` assembles the palette (`joint_skel * inv_bind * geom_bind`). `skin_positions` does weighted blend per vertex. `skin_normals` uses inverse-transpose 3×3 so non-uniform-scale joints produce correct normals. Out-of-range joint indices are skipped (no panic) on malformed skins.
  - **`Mesh::skin` + `Mesh::bind_positions`** — new `SkinBinding` struct stores skeleton path, joint indices/weights, element size, geom-bind-transform, and pre-inverted bind matrices. Loader populates these via `stage.get_skin_binding(mesh_idx)` + precomputed inv-binds per skeleton.
  - **Viewport `update_skinning(frame)`** — extends `update_animation` hot path. For each `SkinnedMeshEntry`, evaluates joint xforms, runs CPU LBS, writes positions into `mesh_data.vertices` (single-mesh or combined-buffer mode) and re-uploads via `queue.write_buffer`. Multi-draw mode flagged as v0.13.5 limitation (one-time warning, deferred).
  - **Scene registration** — scene loader scans loaded prototypes and registers skinned ones in `SceneManager::skinned_meshes`, snapshotting `bind_positions`, `SkinBinding`, and a per-entry scratch buffer so the hot path is allocation-free.
  - **Test fixture** — `test_assets/skel/two_bone_arm.usda`: 2-joint rig, 8-vertex box, deterministic bind pose (identity + T(0,1,0)), used by cpp_bridge, loader, and skinning round-trip tests. `usdchecker` clean.
  - **14 new tests** — 4 cpp_bridge (`test_load_two_bone_arm_skel`, `test_compute_skel_xforms_at_time`, `test_compute_skel_xforms_animated_character` with HumanFemale), 2 loader (`test_load_two_bone_arm_mesh_skin`), 8 skinning unit tests.
  - **Validation asset** — Pixar's HumanFemale UsdSkel example at `assets/UsdSkelExamples/HumanFemale/HumanFemale.walk.usd`. Anim eval test asserts joint motion over the authored 101-129 time range (max delta 39.5).
  - **Multi-draw skinning** — `MultiDrawState::update_skinning` writes skinned positions to per-prototype GPU vertex buffers (the path used for scenes with >1 prototype). Mirrors `update_vertex_animation`'s structure: build palette, run LBS into a per-entry scratch, write each prototype's `vertex_buffer`. Required to make characters with many parts (e.g. HumanFemale's 77 prototypes) actually deform.
  - **Per-mesh joint-order remap** — when a skinned mesh authors a custom `skel:joints` primvar (subset/reordering of the skeleton's joint order), `ComputeJointInfluences` returns indices into the mesh's local order, not the skeleton's. The bridge now builds a mesh-local→skel-global lookup map and remaps each influence index. Without this, body parts pull transforms from the wrong joints and the mesh explodes.
  - **UV-seam joint expansion** — subdivision meshes with `faceVarying` UVs duplicate vertices across UV seams in `mesh.positions` (e.g. 32890→35268), but `ComputeJointInfluences` returns one block per ORIGINAL vertex. The bridge now walks `vertex_index_map[split_idx → orig_idx]` and copies each vertex's influence block to all its post-split duplicates. Without this, every UV-seam vertex collapsed to `Vec3::ZERO`.
  - **Rigidly-deformed mesh broadcast** — meshes without per-vertex `jointIndices` (hair, buttons, teeth, eyelashes — bound to a single joint with `IsRigidlyDeformed()`) get a single influence block from `ComputeJointInfluences`. The bridge now broadcasts that block to every post-split vertex so the Rust hot path doesn't bounds-check out and drop the mesh to origin.
  - **SkelRoot world transform override** — for skinned meshes, the loader now uses the SkelRoot's world transform as the static instance matrix instead of the mesh prim's own world transform. Without this, mesh prims sitting under sub-Xforms (e.g. buttons translated to the chest) would double-apply the offset since the offset is also in `geomBindTransform`. New `skel_root_world_xform[16]` field on `UsdBridgeSkinBindingData` propagates the SkelRoot's xform from the C++ bridge.
  - **Skip per-frame xform animation for skinned meshes** — meshes with skin bindings now bypass the `AnimatedTransform` keyframe path. All per-frame motion comes from the joint deformation pass; applying the prim's animated xform on top would re-introduce the double-application that the SkelRoot override fixes.
  - Scoped out: blend shapes (→ v0.13.6), per-instance matrix re-baking for multiple non-identity instances of the same skinned prototype.

## [0.13.0] - 2026-04-09

### Removed

- **Welcome overlay** — removed centered "Open USD File..." dialog from empty viewport (still accessible via File menu)

### Added

- **Wireframe selection overlay** — `POLYGON_MODE_LINE` pipeline renders selected prim wireframe over the solid pass. `VariantChanged` and `FrameSelected` `AppEvent` variants for UI→renderer dispatch.
- **Qt UI spec §17-25** — context menus, multi-select, undo/redo feedback, long-op progress tiers, reduced-motion accessibility, scene tree filter/search, error state badges, workspace layout storage (global + per-project TOML), `cxx-qt` binding decision. Click targets 24px→32px rows; node labels 10px→12px.
- **CPU vertex displacement** — Post-load pass samples heightmap per vertex and offsets positions along normals (USD convention: 0.5 neutral, scale factor). `displacement.rs` with bilinear sampling, sync `image` crate loader (PNG/JPG/EXR/TIF), `Mesh::recompute_bounds()` for correct framing. Works in both viewport (wgpu) and Ivar (Embree) — same displaced positions. C++ bridge MaterialX fallback reads UsdPreviewSurface displacement when MaterialX extraction skips it. 14 unit tests.
- **Displacement texture pipeline** — UsdPreviewSurface `displacement` input + scale extracted in C++ bridge, flows through FFI to Material struct. Foundation for CPU vertex displacement.
- **Display color + shading mode** — `primvars:displayColor` flows from C++ bridge through Mesh to vertex color. Viewport shader uses it as fallback when no texture. `ShadingMode` enum (Textured/DisplayColor) with GPU uniform and UI dropdown in Display settings.
- **USD prim attribute inspector** — "Attributes" tab in property panel shows all prim attributes and primvars with types, values, and interpolation modes. C++ bridge `usd_bridge_get_prim_attributes()` enumerates by path. Arrays show count, scalars show value. Primvars color-coded with interpolation indicator.
- **Subdivision surface rendering** — Full Catmull-Clark subdivision via Embree 4. `SubdivInfo` preserves original polygon topology through MeshData pipeline. `vertices_orig` FFI passes pre-UV-split positions from C++ bridge. `rtcInterpolate` computes smooth limit-surface normals (dPdu×dPdv). Tessellation rate 8 for BVH accuracy. Fixed `RTCBufferType` enum values (Face=16, EdgeCreaseIndex=18, EdgeCreaseWeight=19). Test asset: `pig_subDivCrease_test.usd`.
- **Two-sided viewport lighting** — Viewport shader auto-flips normals facing away from camera, fixing dark surfaces on meshes with inconsistent winding.
- **HDRI show_background for Ivar** — `hdri_show_background` field on IvarState/RenderConfig. Camera rays respect toggle (solid bg when off), bounced rays always sample HDRI for correct lighting.
- **Obsidian knowledge base** — `wiki/` vault with 42 articles (architecture, USD, rendering, concepts, ADRs, UI/UX), 4 templates, LLM-optimized indexes. 92 devlog entries get `## Wiki Links` backlink sections. `bif-commit` skill updated to maintain wiki on each commit.
- **Markdown linting** — `.markdownlint.json` config + all 235 `.md` files linted/fixed. `pre-commit` framework with `markdownlint-fix` and `cargo fmt` runs on every commit.

### Added

- **Native MaterialX displacement** — 3-tier extraction: surface shader `displacement` input, `GetDisplacementOutput()` → `ND_displacement_float/vector3` node traversal (scale + texture), UsdPreviewSurface companion fallback. No longer requires manual UsdPreviewSurface wiring.
- **SceneQuery viewport migration** — `build_scene_graph_cache()` now takes `&dyn SceneQuery` instead of `&Scene`. Enables future LayerAwareScene drop-in for v0.14.0.
- **SceneQuery trait** — read-only query API in bif_core abstracting Scene field access. 15 methods covering prototypes, instances, materials, cameras, lights, timeline, metadata. `find_instance_by_prim_path` encapsulates the 3-strategy prim path lookup (exact, prefix, synthetic /BIF/ fallback). 9 tests. Enables future LayerAwareScene for M32 opinion trace without viewport changes.
- **Dispatch split** — extracted `render_dispatch.rs`, `selection_dispatch.rs`, `project_dispatch.rs` from monolithic `dispatch_events()` in render.rs. 20 AppEvent match arms → individual handler methods following `node_dispatch.rs` pattern. `dispatch_events()` is now a thin router.

### Fixed

- **Code review hardening (v0.13.0 ship prep)** — Validated faceVarying UV indices (count match, non-negative, in-bounds) with fallback to per-vertex path. Removed misleading top-level `displacement` input check in `extract_materialx_properties()` that could clobber scale on standard_surface materials. Added `checked_mul` overflow guard in FFI conversion. Added `ND_displacement_vector3` diagnostic warning (downstream only handles scalar). Flipped `get_light_attr` lookup order to try `inputs:` prefix first (new schema default).
- **Subdiv faceVarying UVs** — Full pipeline: C++ bridge preserves raw faceVarying UV data before vertex split, flows through FFI to Embree which sets up dual topology (vertex + faceVarying) via `rtcSetGeometryTopologyCount`/`rtcSetGeometryVertexAttributeTopology`. Fixes broken textures on subdivision surfaces.
- **Houdini DomeLight_1 not detected** — C++ bridge now checks `UsdLuxDomeLight_1` (new USD Lux schema). Attribute lookup tries `inputs:texture:file` first (new schema), falls back to `texture:file` (old schema).
- **UsdStage thread-safety soundness hole** — removed `unsafe impl Sync for UsdStage`, wrapped in `Arc<Mutex<UsdStage>>` across 10 files. Prevents potential data races from concurrent C++ stage access (set_variant_selection mutates through `&self`). Batch render and animation paths lock before each FFI call.
- **Deadlock in handle_prim_selected** — stage_guard held lock, then re-locked same non-reentrant Mutex. Now reuses existing guard.
- **Synthetic /BIF/ path double-slash bug** — `find_instance_by_prim_path` and `resolve_instance_index` now strip leading `/` before prepending `/BIF/`.
- **Mutex lock diagnostics** — all 19 `.lock().unwrap()` sites replaced with `.lock().expect("UsdStage mutex poisoned")` for actionable crash messages.
- **Animation lock granularity** — lock-per-iteration in vertex animation loop → lock-once-before-loop.

### Changed

- **Embree feature-gated** — `bif_renderer` Embree dependency behind `embree` feature (default on). Enables Linux CI without Embree linking. `cargo check -p bif_renderer --no-default-features` now passes.
- **Linux CI expanded** — `check-linux` job now runs clippy + tests for `bif_renderer --no-default-features` (97 non-Embree tests).
- **setup_usd_env.sh** — added `bin/usd` plugin directory scan for parity with PS1 script (MaterialX plugin may land in either `bin/usd` or `lib/usd`).
- **Identity pivot** — BIF reframed as "USD Orchestration Tool" (layer-aware editing + procedural assembly + rendering). Docs updated: README, BIF_USD_WORKFLOW, SESSION_HANDOFF, CLAUDE.md.
- **Drive migration D: → G:** — vcpkg/OIDN paths updated in `build.rs`, `setup_usd_env.ps1`, `CLAUDE.md`. Repo relocated to `G:\__projects\_programming\rust\bif`.
- **Qt UI design spec** — Consolidated UI_DESIGN.md as authoritative pre-implementation spec (16 sections, ~730 lines). 14 Stitch mockups across 2 batches. Vertical code split layout variant, Bjorn asset manager, active layer safety system, opinion encoding table, command palette details, canonical component specs, workspace configs. UX Architect + UX Researcher reviews conducted and incorporated.
- **Stitch UI mockups** — 2 batches of Google Stitch-generated mockups covering Assembly (3 variants), Lighting (2 + command palette), Materials (2 + node graph safety), Render (2 + catalog), and First Launch onboarding screen. Obsidian Graphite "Quiet Confidence" design system.

- **MaterialX file format support** — usdMtlx plugin detection with startup diagnostic, `resolve_mtlx_input()` follows Material interface connections for scalar values, deep descendant shader search by `info:id`, refactored extraction into shared helper. `setup_usd_env.ps1` scans both `bin/usd` and `lib/usd` for plugin resources. Enables loading external `.mtlx` references (OpenPBR Shader Playground pattern). Requires `vcpkg install usd[materialx]:x64-windows`.
- **Power-weighted light sampling** — `LightList` now selects lights proportional to emitted power via CDF instead of uniform 1/N. `Light` trait gains `power()` method. Foundation for future hierarchical light tree (v0.20.0). 9 new tests.
- **Max Depth UI slider** — interactive Ivar path tracer now exposes max bounce depth (1–32) in the render settings panel, with restart on change
- **Material roughness trait** — `Material::roughness()` method (default 1.0) implemented for Metal and OpenPbrSurface, used by SHARC cache skip logic
- **SHARC cache roughness skip** — radiance cache reads/writes skip surfaces with roughness < 0.1 to prevent blurred reflections on glossy/mirror materials
- **GitHub Pages site** — mdBook-based dev diary + manual at byvfx.github.io/bif. Auto-deploys on push via `scripts/generate-site.sh` (copies devlog/docs, generates SUMMARY.md) + GitHub Actions workflow. Manual sections: getting started, architecture, USD reference, changelog.
- **FFI bridge split** — extracted `ffi_raw.rs` (898 lines, raw C types + extern block) and `ffi_convert.rs` (2,054 lines, 17 conversion functions + 44 tests) from monolithic `cpp_bridge.rs`. Conversion logic now testable without C++ DLLs. Phase 1 of architecture deepening plan.
- **Architecture refactors plan** — `ARCHITECTURE_REFACTORS.md` documenting 5-phase plan: FFI split, Linux support, node graph eval engine, scene pipeline, renderer decomposition. 45-65 new tests targeted.
- **Cross-platform build foundation** — platform-detect CMake generator, vcpkg triplet, lib paths in `build.rs`. Linux CI job (bif_math + bif_renderer). `setup_usd_env.sh` for Linux/macOS. Windows `process::exit(0)` guarded with `#[cfg(windows)]`.
- **Node graph eval engine** — `node_graph/eval.rs` with `collect_auto_compute_events()` pure function + 19 tests. Decouples auto-compute decisions from egui rendering (fixes nodes-scrolled-out-of-view bug). Phase 3 of architecture deepening.
- **Scene pipeline extraction** — `scene_pipeline.rs` with 5 pure functions (prim path resolution, material lookup, instance expansion, world bounds, axis correction) + 16 tests. Testable without wgpu. Phase 4 of architecture deepening.
- **Renderer dispatch decomposition** — extracted `handle_node_graph_event()` (726 lines) + 2 helpers into `node_dispatch.rs`. render.rs `dispatch_events()` becomes thin router. Phase 5 of architecture deepening.
- **ffi_convert wiring** — 17 UsdStage `get_*` methods now delegate to `ffi_convert::convert_*()`. `cpp_bridge.rs` reduced from 4,542 to 2,824 lines (38% reduction). Code review fixes: dedup `resolve_prim_path`, `panic!` → `unreachable!`, `log::warn` on fallbacks.
- **USD performance metrics harness** (`bif_perf`) — new `benchmarks/` crate with modular Metric trait, 7 metrics (StageOpen, PayloadLoad, MeshExtract, MaterialLoad, PrimTraversal, StageClose, FullLoad), scene registry with tier filtering, terminal/YAML/CSV reporters, CLI (`run`, `list`, `audit`). Based on USD `ref_performance_metrics.html` methodology (N iterations, warmup, min/max/mean/median/p95).
- **USD best-practices audit** — AuditCheck trait with 6 checks from maxperf.html: binary format, payload usage, prim count, instance usage, Alembic detection, layer count (skip). CLI `audit` subcommand with pass/warn/fail/skip output.
- **External measurement targets** — usdview (Python/pxr subprocess) and Houdini (hython subprocess) targets for cross-tool USD load time comparison
- **Benchmark comparison** — `compare` subcommand loads two YAML result files, shows per-metric delta_ms/delta_%/FASTER/SLOWER/~same
- **Historical results storage** — `--save` flag auto-saves YAML to `benchmarks/results/` with timestamp + `latest_{target}.yaml`
- **Asset download helper** — `download` subcommand shows missing official assets with download URLs (Kitchen Set, ALab, Moore Lane)
- **Per-tile UDIM loading** — UdimTileSet/UdimGridLayout types in bif_core, per-tile sampling (CPU+GPU), contiguous texture array blocks with shader tile offset. Eliminates atlas stitching (~3s/set). Unified CPU/GPU path ready for material editor.
- **Box SubDiv crease test assets** — `box_subDivCrease_test.usd/usda` for subdivision surface crease weight validation.
- **Kilo config** — `kilo.jsonc` with MCP context-mode plugin and bash/skill permission presets.
- **Site SUMMARY.md** — updated mdBook nav index covering full devlog history (Jan 2025 – Apr 2026).

### Fixed

- **Selection outline rendering** — replaced broken `PolygonMode::Line` + `shading_mode` `queue.write_buffer` hack (DX12 depth bias + write ordering made lines invisible) with a normal-expanded back-face silhouette pipeline (`outline.wgsl`) and a dedicated `wireframe_cam_bind_group`. Clean silhouette outline (no internal edges) on selected prims.
- **Bidirectional tree ↔ viewport selection sync** — viewport click → tree row highlights (via new `select_at_screen()` + `denormalize_synthetic_path()`), tree click → outline appears on corresponding mesh (prefix + synthetic `/BIF/{path}/{idx}` fallbacks in `PrimSelected` handler). Tree auto-expands ancestors so selected rows become visible. Click on empty viewport space deselects; clicks on UI panels preserve selection.
- **Camera persistence bug** — reset viewport/batch camera source on new scene load; stale USD camera from previous scene no longer persists
- **HDRI background toggle** — removed `is_loaded` guard on UpdateHdriParams so params propagate for auto-loaded DomeLight HDRIs; background now correctly hides when unchecked
- **PointInstancer time-sampled data** — C++ bridge now falls back to stage startTimeCode or first time sample when Default yields empty arrays (fixes Pixar PointInstancedMedCity.usd and similar files with no default values)
- **PointInstancer Xform prototype resolution** — prototype_map now includes parent Xform paths so instancer targets like `/Prototypes/proto_0` resolve to child mesh `/Prototypes/proto_0/mesh_0`
- **bif_perf code review fixes** — stable Rust compat (`count % 2` over nightly `is_multiple_of`), safe `u64::try_from` for duration stats, sample stddev (N-1), metadata surfaced in reports, iterations>=1 guard, CSV field escaping, `CARGO_MANIFEST_DIR` workspace root, `serde_yml` replacing deprecated `serde_yaml`, removed unused `csv` dep
- **Audit review fixes** — AlembicUsageCheck→Skip (prim paths don't contain file refs), InstanceUsageCheck now includes native instances, PayloadUsageCheck uses root prim count, run_audit runs path-only checks before payload load, `result()` helper on AuditCheck trait, 5 unit tests

### Changed

- **Tree browser selection visuals** — removed node-source row tinting (green/blue); only the selected row paints a background. Selection color now uses `from_rgba_unmultiplied(74, 144, 217, 75)` (was premultiplied which produced near-additive blending against dark panel). `selectable_label` passed `false` to avoid double-painting on top of manual row bg.
- **Documentation test count sync** — Updated test counts to 516 total (was stale 160+/400+). Per-crate: bif_math (74), bif_core (163), bif_renderer (111), bif_viewport (149), bif_viewer (19). Phase 1 FFI split marked complete in ARCHITECTURE_REFACTORS.md.
- **Async texture loading for all paths** — working scene rebuild and legacy loader now use async placeholders + streaming instead of blocking sync load. Viewport interactive immediately on scene load.

### Fixed

- **OIDN denoiser using geometric normals** — switched to shading normals for better edge preservation on normal-mapped surfaces
- **Zombie process on close** — `process::exit(0)` after event loop prevents native DLL teardown deadlock on Windows
- **Unsaved changes dialog not showing** — `mark_dirty()` added to gizmo drag, undo, redo, keyframe operations
- **Save dialog hidden behind window** — store window Arc in Renderer, hide main window while rfd MessageDialog shows
- Shader `tex_offset` comment for future UDIM texture slots (roughness, normal, etc.)
- Deduplicate `find_udim_tiles` calls — `prepare_texture_placeholders` passes expanded paths to async loader
- Removed vestigial `udim_grid_*` fields from `Texture` struct (16 bytes/texture savings)
- Clean up stale `.bif_cache/udim/` directories on scene load
- Panic guard: bounds check in `upload_streamed_texture` + views/textures sync in `prepare_texture_placeholders`
- UDIM capacity check before allocating — skip sets that won't fit in texture array
- Validate UDIM ID range (1001-1200) in `UdimGridLayout::from_tiles`
- `#[must_use]` on `grid_slots()`, negative UV test, clamping behavior documented

### Removed

- UDIM atlas stitching, disk cache (UdimCacheMeta, cache dir/key/load/save/clear), scale_pixels_box, ClearUdimCache UI, serde_json dep from bif_core
- `create_gpu_textures_for_scene` sync loader (replaced by async path)

- **M31: Per-node scene graph visualization** — source_node tagging on ProceduralPrim, prim count `[N]` badges on node headers, Scene/Node tab bar with NodeFilteredProvider for upstream-filtered browsing, row highlighting for selected node's prims in full scene browser. 4 new tests.
- **M30 Phase 6: Cache node** — SceneNode::Cache with bypass toggle, visual indicators (Cached/Stale/Bypassed), property inspector, CacheToggleBypass/CacheClear events. Data serialization deferred.
- **M30 Phase 5: eval modes** — Auto/Manual/OnMouseRelease with dirty node tracking, eval mode ComboBox toolbar, Cook All/Cook Selected buttons, dirty visual indicators, CookNode event for deferred compute dispatch
- **M30 Phases 3-4: File menu + save/load UI** — File > New/Open/Save/SaveAs with Ctrl+N/O/S/Shift+S, Recent Files submenu, dirty tracking on node/transform events, dynamic title bar with unsaved indicator, unsaved-changes prompt on close/new/open, extract/apply project for full state round-trip
- **M30 Phase 2: ProjectFile persistence** — save/load .bif (bincode) + .bifa (JSON), CameraData snapshot, relative path resolution, RecentFiles (8 max), ProjectState (dirty flag + window title), EvalMode enum, format versioning. 10 unit tests.
- **M30 Phase 1: serde foundation** — Serialize/Deserialize derives on all types needed for .bif/.bifa persistence across 4 crates (SceneNode, GraphNodeId, BatchRenderSettings, Camera types, USD enums, renderer configs). egui-snarl serde feature enabled, bincode added. Round-trip tests for all 10 node variants + Snarl graph.
- **M29.5 UI overhaul** — centralized theme system (theme.rs), scene browser promoted to primary left panel, viewport stats overlay, File/View/Render menu bar with Ctrl+O, node params moved from show_body() to property inspector (all 10 types), welcome screen on empty state, Unicode prim icons replacing emoji, tooltips on all controls, node selection via header click with accent highlight
- **VNDF GGX sampling** — Heitz 2018 visible normal distribution sampling replaces NDF sampling for 2-4x convergence on rough metals at grazing angles
- **GraphNodeId newtype** — framework-agnostic node ID decouples node graph evaluation from egui_snarl, preparing for M30 persistence and Qt migration
- **GpuMaterialState / GpuTextureState** — extracted 12 GPU fields from Renderer into focused sub-structs
- **types.rs** — moved PurposeMode, DisplaySettings, UsdLoadStatus, AsyncChannels, SceneInstances out of lib.rs
- **State mutation convention** — documented direct-mutation vs EventBus patterns in render.rs
- **set_instance_purpose(index)** — add_instance returns index; replaces fragile set_last_instance_purpose API
- **Stage::Load() eager population** — 8.3x USD loading speedup (8.8s→1s on Glasses.usd) by forcing eager USD composition
- **GPU buffer size guards** — cap triangle material, vertex, and index buffers to device limits with placeholder fallback (prevents crash on 342M vert scenes)
- **Chunked texture loading** — load 16 textures at a time (was all-at-once), paced GPU uploads (32/frame)
- **Texture count warning** — log when >511 textures exceed viewport GPU slot limit
- **Texture backpressure** — `sync_channel(32)` prevents unbounded RAM growth on 500+ texture scenes
- **Adaptive texture downscale** — auto 512px for 200+ textures, 1024px for 50+ (background thread downscale before channel send)
- **MAX_VIEWPORT_TEXTURES** — raised 512→2048 for production scenes
- **Free C++ mesh cache** — `usd_bridge_free_mesh_geometry()` frees normals/UVs/subdivision after Rust copy (~8GB on 335M vert scenes)
- **Texture streaming progress** — periodic log of loaded/total count
- **LoadNone deferred payloads** — `UsdStage::Open(LoadNone)` opens hierarchy only; `load_payloads()` loads geometry on demand
- **UDIM tile downscale before stitch** — tiles downscaled to adaptive size before atlas assembly (was full-res → OOM)
- **Pick scene size guard** — skip Embree pick BVH for >50M tris (prevents 25GB OOM)
- **C++ debug log flags** — `g_log_textures`, `g_log_timing`, `g_log_variants` toggle output sections
- **Parallel UV seam split** — 3-pass `cache_stage_data()` refactor using USD `WorkParallelForN`; per-mesh geometry extraction in parallel with per-thread `UsdGeomXformCache`
- **Viewport .tx texture cache** — viewport prefers pre-converted .tx files over source JPG/PNG via `resolve_tx_path`; auto-triggers background .tx conversion on scene load
- **Parallel .tx conversion** — `convert_textures_to_tx` uses rayon for concurrent subprocess spawning (~4x speedup)
- **Parallel Ivar texture pre-warm** — `pre_warm_parallel` loads all textures concurrently before material build (9s→2.7s on 13 network textures)
- **Clear .tx cache** — UI button to delete cached .tx files for current scene
- **UDIM .tx fallback** — tile discovery checks for .tx variant when source file missing
- **OpenPBR MaterialX import** — C++ bridge recognizes `ND_open_pbr_surface` with fallback input names (`base_metalness`, `geometry_normal`, `geometry_opacity`)
- **Shading normal AOV** — `Ns` layer in EXR + "Shading Normal" in viewport AOV dropdown; shows normal-mapped normals vs geometric `N`

### Removed

- Dead `show_ui` toggle (field + early return, never wired to keybinding)
- Emoji prim type icons (replaced with colored Unicode geometric shapes)
- 35+ inline Color32 literals (replaced with theme constants)

### Fixed

- **M30 review fixes (13 items)** — SavePromptResult enum fixes "Yes" = silent cancel data loss; eval_mode now persists round-trip; reset_project clears scene/instances/stage/selection; cached recent files (was per-frame disk I/O); pub FORMAT_VERSION const; Cache node dirty propagation; path canonicalization for recent files; compute nodes marked dirty on project load; OnMouseRelease labeled TODO; SelectNode no longer sets dirty flag; bincode fragility documented; title bar cached; save_recent_files logs warnings
- **OpenPBR energy conservation** — diffuse attenuated by (1-F_specular) to prevent energy creation at grazing angles
- **Shadow ray shading normal** — offset uses shading normal instead of geometric normal, fixing dark bands with normal maps
- **Distant light angle units** — convert degrees→radians in constructor, fix cos_max formula for correct soft shadows
- **Normal matrix zero-scale guard** — fallback to identity for degenerate transforms (prevents NaN on hidden USD instances)
- **SHARC cache NaN guard** — filter non-finite values from lock-free cache torn reads
- **Point light falloff** — use max() instead of additive epsilon for correct near-light energy
- **OpenPBR is_delta()** — any transmission with roughness<0.001 treated as delta (saves wasted shadow rays)
- **Crease data validation** — validate index/sharpness counts before Embree FFI
- **NaN guards** — HDR direction_to_uv zero-length, texture sample non-finite UV inputs
- **UsdBridgeError Success** — safe fallback instead of unreachable!() panic
- **Box filter boundary** — half-open interval avoids double-counting at bucket edges
- **Embree Drop safety** — documented field-order invariant preventing use-after-free
- **Node graph unwrap** — let-else pattern match prevents potential panic on disconnect
- **Mesh dedup hash** — 10→50 vertex/index samples + normal hashing reduces collision risk
- **u32 overflow in triangle count display** — use u64 for large scene stats (607M tris × 13K instances)
- **Normals lost on meshes without UVs** — deferred normals copy was inside UV block; meshes with normals but no UVs got flat shading
- **UV seam split hash collisions** — PairHash uses bit mixing instead of MSVC identity hash

### Changed

- **Copy on Transform** — derive Copy on Transform struct, removing redundant .clone() calls across codebase
- **HdrImage::downscale_to_max_dim** — returns Option<Self> to avoid cloning when no downscale needed
- **Prototype::bounds removed** — redundant field, use mesh.bounds directly
- **IBL Vec3 ops** — replaced local [f32;3] math helpers with glam Vec3 operations
- **Orthonormal basis dedup** — light.rs uses bif_math::build_orthonormal_basis instead of local copy
- **HDRI pole clamp** — resolution-dependent half-texel clamp replaces fixed epsilon
- **SHARC TOCTOU race** — documented known lock-free EMA blend race condition
- **Deferred normals copy** — skip 289ms wasted copy when UV seam split rebuilds normals
- **Bulk vertex/normal copy** — `assign()` replaces push_back loops in C++ bridge
- **UV seam split** — `std::map` → `std::unordered_map` (O(log n) → O(1))
- **Deferred Rust clones** — mesh dedup hash from references, clone only unique meshes
- **Disable Ivar prewarm** — materials built on-demand at render time (saves 8+ GB RAM on large scenes)
- **UDIM atlas cap** — 64MP → 32MP (1GB → 512MB max per atlas)
- **Remove texture upload clone** — skip `tex.data.clone()` when no downscale needed

## [0.12.0] - 2026-03-21

### Added

- **Purpose filtering** — USD purpose attr (render/proxy/guide) toggle in viewport; C++ bridge `compute_inherited_purpose()` with hierarchy walk for instance proxies
- **Purpose enum** — `Purpose` type on `Instance` with per-instance filtering in combined mesh build
- **Native instance purpose** — own purpose from scene hierarchy (not inherited from prototype mesh)
- **Material diagnostics** — debug-level Ivar material/texture logging (`RUST_LOG=bif_renderer=debug`)
- **USD export** — stage metadata, materials (UsdPreviewSurface + OpenPBR MaterialX), lights (4 types), cameras, visibility, GeomSubsets, invisible_ids roundtrip
- **Curves/Points import** — UsdGeomBasisCurves and UsdGeomPoints via C++ bridge with viewport preview
- **Bound Material inspector** — property inspector shows OpenPBR params for selected prim
- **Implicit geometry** — C++ bridge tessellates UsdGeomSphere/UsdGeomCube with dedup + native instances
- **DomeLight** — auto-HDRI, rotation, color temperature via Tanner Helland
- **Glass/transmission** — extract from MaterialX/UsdPreviewSurface, Snell's law refraction + TIR
- **USD spec compliance** — 15 FFI fields (visibility, doubleSided, subdivisionScheme, velocities, cameras, lights)
- **Embree subdivision** — Catmull-Clark from USD polygon topology + crease data
- **UDIM texture atlas** — probe/stitch pipeline for Ivar + viewport with memory caps
- **Blue noise sampling** — Cranley-Patterson rotation, pixel reconstruction filters
- **EventBus** — typed `AppEvent` enum replacing 23 string-keyed temp-data slots
- **Subsystem extraction** — SceneManager, SelectionManager, CameraState, IvarContext, NodeGraphContext
- **50+ new tests** — bif_math, bif_renderer, bif_viewer

### Fixed

- **UsdPreviewSurface specular** — `specularColor` was averaged to `specular_weight`, breaking dielectrics; now always 1.0 (IOR/Fresnel-controlled)
- **Ivar double-filtering** — purpose filter applied twice causing material index misalignment
- **OIIO mip buffer overflow** — `read_image()` always passed `miplevel=0` into smaller mip buffers
- **UDIM double V-flip** — `transform_uv()` returned pixel-space causing second flip in `sample()`
- **Memory leak** — `UsdEditLayer::save()` nulled pointer preventing Drop from freeing C++ handle
- **SHARC race** — two threads CAS-increment but only one's radiance survived
- **Point light specular ring** — MIS power heuristic crushed delta light specular peak
- **Shadow ray self-intersection** — offset along surface normal prevents acne
- **Ivar texture paths** — resolve relative paths via `material.source_dir`
- **Material dedup** — instance proxy materials cached once per prototype
- **Instance proxy binding** — resolve `bound_material_path` during traversal
- **Backface culling** — `FrontFace::Cw` → `FrontFace::Ccw` for USD rightHanded convention

### Changed

- **OpenPBR migration** — Disney Principled BSDF → OpenPBR Surface v1.1 across all 6 crates
- Renderer decomposed from ~75 fields into sub-structs
- `render()` (2,695 lines) split into 6 phase methods
- `node_graph.rs` (2,272 lines) split into module directory
- Winding convention CW → CCW throughout
- `ControlFlow::Poll` → `Wait` (was burning 100% CPU idle)
- Texture loading ~25-50x faster (raw u8 path, GPU mipmaps, async streaming)

### Removed

- Rust USDA parser (all USD loading via C++ bridge)
- Dead `instanced_geometry_bvh.rs` (274 lines, broken UB)
- Stub LayerStack/Composition property inspector tabs (will return in M30+)

## [0.11.0] - 2026-03-13

### Added

- Ivar material cache + pre-warm (background texture/material loading)
- Embree indexed geometry path (shared vertices, parallel hit data)
- `MeshData::extract_positions/normals/uvs()` SOA helpers
- Shared `build_materials()` helper

### Performance

- Ivar subsequent builds: 6.7s → ~47ms (cached materials)
- Embree indexed geometry: 37s → 47ms BVH build

## [0.1.0] - 2026-03-12

Initial versioned release. Viewport rendering, instancing, USD C++ bridge, Embree ray tracing, materials, MaterialX, animation, batch render, node graph, scatter, SHARC cache, OIDN denoising.
