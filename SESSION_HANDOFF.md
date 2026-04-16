# Session Handoff - April 15, 2026 (sessions 2–9, end of Phase E.2-prep)

**Last Updated:** v0.15.0 Phase E.2-prep landed on `v0.15-qt` (uncommitted). `bif_viewport::Renderer` is winit-free. Next session: Phase E.2 moves 1+2+4 — now unblocked.
**Current Version:** v0.14.0 shipped on `main`; v0.15.0 in progress on `v0.15-qt`.
**Project:** BIF - USD Orchestration Tool for VFX

## ⚡ Phase E.2-prep (2026-04-15) — Renderer winit decoupling

**Why:** Phase E.2's move 1 (pull `bif_viewport::Renderer` into `bif_qt`) was gated by hidden winit coupling the original handoff didn't flag. `Renderer::new(Arc<winit::Window>)` + `render(…, &winit::Window)` + egui's per-frame `take_egui_input`/`handle_platform_output`/`on_window_event` handshake all needed a live winit Window reference. bif_qt has only a raw HWND.

**What landed:**
- **`bif_viewport::Renderer::new`** now takes `(surface, device, queue, config, size: (u32,u32), scale_factor: f32)` — pure wgpu primitives, caller owns surface creation. Synchronous.
- **`Renderer::attach_egui(egui_state)`** — optional egui overlay attachment. `bif_viewer` uses it; `bif_qt` won't.
- **`Renderer::egui_state_mut()`** — caller drives the winit ↔ egui handshake (`on_window_event`, `take_egui_input`, `handle_platform_output`) outside `bif_viewport`.
- **`Renderer::render(clear_color, raw_input: Option<RawInput>) -> Result<Option<PlatformOutput>>`** — headless when `None`, egui-overlay when `Some`.
- **`Renderer::set_dialog_focus_hook(Fn(bool) + 'static)`** — installable visibility toggle for the Windows z-order workaround. `bif_viewer` installs `move |v| window.set_visible(v)`; `bif_qt` leaves it `None`. All 5 existing internal `with_dialog_focus` callers unchanged.
- **`Renderer::resize((u32,u32), f32)`** — takes scale factor.
- **Constants exposed:** `bif_viewport::REQUIRED_FEATURES` + `required_limits()` so callers request the right wgpu device.
- **Zero direct `winit::*` imports** in `bif_viewport/src/lib.rs` + `render.rs` code (doc comments + `egui_winit::State` type only — the transitive winit dep stays until egui is retired in Phase F).

**`bif_viewer` compat shim:**
- New `fn create_renderer(window: Arc<Window>) -> Result<Renderer>` in `main.rs` builds wgpu primitives + `egui_winit::State` + installs dialog hook. Three existing call sites converted.
- `handle_egui_event` call replaced with inline `egui_state_mut().on_window_event(window, &event).consumed`.
- `render(clear_color, window)` replaced with `take_egui_input` + `render(clear_color, Some(raw_input))` + `handle_platform_output`.
- Both `resize` call sites pass `window.scale_factor() as f32`.

**Build gates:** `cargo build -p bif_viewport -p bif_viewer` clean, `cargo clippy -- -D warnings` clean, `cargo fmt --check` clean, `cargo test -p bif_math` 74/74 pass.

**Still to verify:** `cargo run -p bif_viewer --release` end-to-end — golden path load/orbit/pan/zoom + egui panels + native dialog. Low risk but non-zero; the egui handshake is a mechanical 3-call move.

**Next session — Phase E.2 moves 1+2+4 (original plan, now unblocked):**
1. `bif_qt::viewport::Viewport` swaps its triangle for `bif_viewport::Renderer` via `Renderer::new(surface, device, queue, config, size, scale)` — HWND surface creation pattern already lives in `viewport.rs:38–101`.
2. Add `BifShellState::load_stage_at_path(QString)` invokable → `SceneManager::load_usd_scene` → bump `layer_state_revision`.
3. Add USD timeline FFI (`usd_stage_get_time_range` in `bif_core/cpp/usd_bridge/`) + `detect_timeline_from_stage()` invokable.

AppEvent bridge decision: deferred. Moves 1+2+4 don't need it; will revisit when dispatch-wide ops land.

---

## 🚀 How to pick up next session

1. `. .\setup_qt_env.ps1` — required every session (Qt 6.8.3 LTS env).
2. `git checkout v0.15-qt` — the migration branch. `main` is v0.14.0-frozen.
3. `cargo run -p bif_qt --bin bif_qt_shell` — dogfood binary. Viewport renders a triangle (Phase E.2 swaps in the real `bif_renderer::Renderer`).
4. Read **"Phase E.2 Starting Notes"** below — has the ordered first moves + AppEvent bridge options.
5. All Phase A–E.1 gotchas are in the **cxx-qt + Qt gotchas** list below. Worth skimming before touching the bridge.

---

## Quick Status

| Status | Details |
|--------|---------|
| Released | v0.1.0, v0.11.0, v0.12.0, v0.13.0, v0.13.5, v0.13.6, **v0.14.0 (2026-04-13)** — pushed to origin |
| **Active branch** | **`v0.15-qt`** — Qt migration. `main` stays v0.14.0 shippable until Phase H merge. |
| v0.15.0 Phase 0 ✅ | wgpu-into-QWidget spike gate PASSED 2026-04-13. Qt 6.8.3 LTS + MSVC 2022 + cxx 1.0 + qt-build-utils 0.7 toolchain proven in `crates/bif_qt_spike/`. ADR-006 authored. |
| v0.15.0 Phase A ✅ | `crates/bif_qt/` scaffolding landed 2026-04-13. First real `#[cxx_qt::bridge]` — `BifShellState` QObject with 2 qproperties + 1 qinvokable. Theme port (34 colors + stylesheet generator). C++ QMainWindow assembly with 4 dock placeholders + menu bar. |
| v0.15.0 Phase B ✅ | All 8 slices. Viewport + stylesheet + menu + Zen mode + workspaces + first-launch + breadcrumb + command palette. |
| v0.15.0 Phase C ✅ | 3 panels. Layer Stack · Scene Browser · Property Inspector. |
| v0.15.0 Phase D ✅ | 3 secondary panels. Timeline · Node Graph · Render Settings. Tabification between bottom + right. |
| v0.15.0 Phase E.1 ✅ | Input stubs. Viewport mouse/camera signals, keyboard shortcuts (F/Space/Left/Right/±Shift), ShortcutRegistry with QSettings override path, real QFileDialog, QTimer-driven timeline playback, fps + realtime + loop toggles, Nuke-style 3-zone toolbar, inline Start/End range + detect-from-stage button. 9 new qproperties, 6 new invokables. |
| v0.15.0 Phase E.2-prep ✅ | `bif_viewport::Renderer` decoupled from winit (2026-04-15). New API: `new(surface, device, queue, config, size, scale) + attach_egui + egui_state_mut + set_dialog_focus_hook + render(clear_color, Option<RawInput>) -> Option<PlatformOutput>`. `bif_viewer` keeps working via a new `create_renderer` helper. Zero direct `winit::*` imports in `bif_viewport` code. Build/clippy/fmt clean. |
| v0.15.0 Phase E.2 move 1 ✅ | `bif_qt::Viewport` hosts real `bif_viewport::Renderer` (2026-04-15). Triangle demo deleted. `bif_qt` depends on `bif_viewport`. HWND-built wgpu primitives feed `Renderer::new(...)` — zero winit, no egui attached. `render(clear_color, None)` headless path. `scale_factor` hardcoded to 1.0 until `QScreen::devicePixelRatio()` wired. `ViewportCallbacks::viewport_mut()` exposes the renderer for moves 2+4's invokables. **Needs manual dogfood — `. .\setup_qt_env.ps1; cargo run -p bif_qt --bin bif_qt_shell`.** |
| v0.15.0 Phase E.2 move 4 ✅ | Real timeline detection (2026-04-15). Path stored by `on_stage_path_opened` → `detect_timeline_from_stage` opens throwaway `UsdStage`, calls existing `get_timeline()`, writes start/end/fps qproperties. Standalone — no shared stage handle yet. |
| v0.15.0 ADR-007 ✅ | BifShellState ↔ ViewportCallbacks bridge locked in: **β (thread-local raw-pointer)**. See `wiki/architecture/adr/007-shell-state-to-viewport-bridge.md`. Rationale: Qt UI single-threaded → `thread_local!<Cell<*mut _>>` is sound + lock-free; ViewportCallbacks lives on app.rs's stack for the full event loop. Encapsulated in `with_viewport_mut(|vp| ...)` with one small `unsafe` block. |
| v0.15.0 Phase E.2 move 2 ✅ | Real stage load (2026-04-15). `on_stage_path_opened` fires `QFileDialog` → `with_viewport_mut(|vp| vp.renderer_mut().load_usd_scene(&path))` → clones `renderer.scene.layer_state` onto `BifShellState` → bumps `layer_state_revision`. Layer Stack panel now shows real layers. |
| Next | **Phase E.2 polish + remaining moves.** (a) Revisit move 4 to use the live in-renderer stage instead of reopening one per click (replace `UsdStage::open(path)` with reading through `with_viewport_mut`). (b) Scene Browser real data via `CompositeProvider` traversal (move 7). (c) Property Inspector real attributes via `UsdPrim::GetAttributes` + composition arcs from `UsdStage::get_prim_stack` (move 8). (d) Gizmo raycast via `selection.rs` on LMB pick (move 5). (e) Breadcrumb wire to `selected_prim_pathChanged` (move 6). (f) Wire `QScreen::devicePixelRatio()` into `Viewport::new` / `resize` — currently hardcoded to 1.0. (g) Manual dogfood on a real USD stage end-to-end. |
| Tests | ~627 total (90 new in v0.14.0) + spike has no unit tests (deletion-scheduled) |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms |

---

## ➡️ v0.15.0 Qt Migration — Phase E Starting Notes

**Strategy (ADR-006):** Shell-first on `v0.15-qt`. Panels one-at-a-time. Merge at Phase H.

**Binding locked:** `cxx-qt 0.7` + `qt-build-utils 0.7` + Qt 6.8.3 LTS + LGPL dynamic linking. Phase A validated cxx-qt macros. C++ owns window assembly (QMainWindow / QDockWidget / QMenuBar) — cxx-qt-lib's QtWidgets coverage is thin and Rust-side boilerplate would be pure cost without ergonomic win. Rust owns QObjects (BifShellState + future panel models).

**Phase A/B/C/D cxx-qt + Qt gotchas (for Phase E authors):**
- `#[qinvokable]` declared inside `extern "RustQt"`, IMPLEMENTED in a regular impl block OUTSIDE the bridge (non-empty impls inside the bridge = compile error).
- `CxxQtType` trait must be in scope for `.rust()` / `.rust_mut()` accessors.
- cxx-qt-generated header path: `bif_qt/src/main_window.cxxqt.h` (crate + src prefix).
- Cannot mix `#[cxx::bridge]` + `#[cxx_qt::bridge]` in same crate. `extern "Rust"` AND `extern "RustQt"` CAN coexist inside one cxx-qt bridge.
- `type Foo = path::Foo;` is rejected in `extern "Rust"` — use `type Foo;` and resolve via parent-module `use crate::module::Foo;`.
- `#![allow(clippy::missing_safety_doc)]` at crate scope required — clippy can't see bridge-macro-expanded fn docs.
- C++ Q_OBJECT headers need `CxxQtBuilder::qobject_header("path/to.h")` for moc; the `.cpp` goes in `cc_builder.file()`.
- `QKeySequence::Quit` is empty on Windows — hardcode `Ctrl+Q`.
- `extern "Rust"` opaque types pass to C++ as `Foo*` raw pointers (no UniquePtr unless explicitly boxed).
- `rust::Str` ↔ `QString`: `QString::fromUtf8(s.data(), static_cast<int>(s.size()))`.
- **Qt's `QList`/`QVector` can't hold move-only types** like `std::unique_ptr` — use `std::vector`.
- **Custom tree delegates that shift paint rects MUST also override `editorEvent`** with the same shift, otherwise checkbox hit-testing misses the visual checkbox.
- **`self: &Self` in invokable impl blocks trips `clippy::needless_arbitrary_self_type`** — use plain `&self` in the impl body even though the bridge declaration requires `self: &BifShellState`.
- **Nested private C++ struct types aren't accessible from anonymous namespaces in the .cpp** — promote to public when helper builders need them.
- **Auto-generated property-changed signals are `<snake>Changed`** — for `#[qproperty(i32, layer_state_revision)]` the signal is `layer_state_revisionChanged`. Use that, not a custom `#[qsignal]`, for triggering model refreshes.
- **cxx-qt bridge `#[qinvokable]` read-only functions require `self: &BifShellState`, NOT `&self`.** Impl bodies use `&self` normally (clippy wants that). Mutators use `self: Pin<&mut BifShellState>`.
- **When reading `.rust()` inside a `Pin<&mut Self>` invokable**, bind the `self.as_ref()` temporary to a variable before calling `.rust()`, or the borrow dangles (E0716).
- **`QGraphicsView` consumes wheel events for its built-in scroll.** Override `wheelEvent` on a `QGraphicsView` subclass, not on the wrapping `QWidget` — wheel events don't propagate.
- **`QGraphicsPathItem` is NOT a QObject** — it can't be the connect context. Use the sender (also the QObject) as the context for 3-arg connect.
- **`BifNodeGraphicsItem::moved` signal pattern:** emit from `itemChange(ItemPositionHasChanged, ...)` on a `QGraphicsObject` subclass. Wires subscribe for re-routing.

## 📂 v0.15 Phase state map

| Surface | Ships in branch | What's real | What's stub / demo |
|---|---|---|---|
| **Build + env** | `setup_qt_env.ps1`, workspace Cargo.toml | Qt 6.8.3 LTS detection via `qt-build-utils`, MSVC flags | — |
| **Shell** | `crates/bif_qt/` + `bif_qt_shell` bin | QMainWindow, menu bar, status bar, dock system, command palette (Ctrl+P), breadcrumb bar, 4 workspace presets (QSettings persistence), first-launch screen, Zen mode (Ctrl+\\), dark stylesheet | — |
| **Viewport** | `cpp/render_widget.{h,cpp}` + `src/viewport.rs` | wgpu triangle + mouse orbit/pan/zoom signals + `primPickRequested` signal | Triangle instead of real Renderer; camera input echoes status bar, doesn't drive camera |
| **Layer Stack panel** | `cpp/layer_stack_{model,widget}.{h,cpp}` | `QListView` + model, color dot delegate, mute checkbox, double-click working layer, isolation toolbar | 3-layer hardcoded demo data in `BifShellState::seed_demo_layer_stack` |
| **Scene Browser** | `cpp/scene_browser_{model,widget}.{h,cpp}` | `QTreeView` + model, hierarchical filter, selection routes to `selected_prim_path` qproperty | 10-prim hardcoded demo tree; no lazy fetch yet |
| **Property Inspector** | `cpp/property_inspector_widget.{h,cpp}` | Tabs + composition arcs group + attributes table with opinion-dot delegate | Fake attrs per prim-type, composition arcs mirror layer stack |
| **Timeline** | `cpp/timeline_widget.{h,cpp}` | Nuke-style 3-zone toolbar, custom paint ruler + playhead + keyframe diamonds, QTimer advances frames on Play, fps/RT/Loop/Start/End controls, `↻⇅` detect button | Keyframes hardcoded; detect button is a status-only stub |
| **Node Graph** | `cpp/node_graph_widget.{h,cpp}` | `QGraphicsScene` + `NodeGraphView`, 5-node demo (UsdRead→Scatter→Xform→IvarRender + HdriEnv), wheel-zoom + middle-mouse pan, bezier wires refresh on node move | Demo graph only; can't create/delete/rewire nodes (v0.16) |
| **Render Settings** | `cpp/render_settings_widget.{h,cpp}` | QFormLayout with Path Tracer (spp/depth/SHARC) + Post-Processing (exposure/gamma/OIDN) | Values local; not wired to `bif_renderer::RenderConfig` |
| **Shortcut system** | `cpp/shortcut_registry.{h,cpp}` | QSettings-override path for every shortcut declared via ID | Preferences UI (v0.16) |
| **File → Open** | `window_builder.cpp` | Real `QFileDialog`, records `QSettings("recent_stages")`, flips central stack to viewport | `BifShellState::on_stage_path_opened` doesn't actually call `scene_loader::load_usd_scene` yet |

**One-liner to understand the shape:** `BifShellState` (cxx-qt QObject in `src/main_window.rs`) is the fat singleton that every panel binds to via `#[qproperty]` + `#[qinvokable]`. It holds title / status / workspace / layer state / selection / timeline. Phase C deliberately skipped the existing `EventBus`; Phase E.2 reopens that decision.

---

## ➡️ Phase E.2 Starting Notes — Real USD Wiring (~5–8h)

**Goal:** Replace demo data with real USD reads. This is the biggest integration step in v0.15 and the hardest architectural decision (AppEvent bridge).

### 🎯 Phase E.2 — ordered first moves

1. **Pull `bif_renderer::Renderer` into bif_qt's viewport.** Today `src/viewport.rs` owns a tiny wgpu pipeline drawing a triangle. Goal: `Viewport` holds a `bif_renderer::Renderer` instead, with the same HWND-fed `wgpu::Surface`. Two flavors of this, pick one:
   - (Lightweight) Keep `Viewport` in bif_qt, delegate rendering to `bif_renderer::Renderer::render(&camera, &scene)` — needs Renderer's public API to be reachable without `bif_viewport` types.
   - (Full) Pull `bif_viewport::Renderer` (the 75-field God object) + `SceneManager` + `EventBus` into bif_qt directly, deleting the egui UI parts. This is closer to Phase F scope — might as well do it now since Phase E.2 needs it.
   - **Recommended:** the full path. Phase E.2 + Phase F essentially merge.
2. **Wire `QFileDialog` result → real stage load.** Add `BifShellState::load_stage_at_path(QString)` invokable (replacing the `on_stage_path_opened` status echo). Inside: call `bif_core::scene_loader::load_usd_scene(path)` → `SceneManager::load_scene(scene)` → update `SceneLayerState` on self → bump `layer_state_revision` so the Layer Stack panel refreshes. Drop the hardcoded demos from `seed_demo_layer_stack` + `SceneBrowserModel::seed_demo_tree`.
3. **AppEvent bridge decision + implementation.** See "AppEvent bridge options" section below.
4. **`detect_timeline_from_stage` real impl.** Call `UsdStage::GetStartTimeCode() / GetEndTimeCode() / GetTimeCodesPerSecond()` → set `start_frame`, `end_frame`, `playback_fps` qproperties. File load should also call this automatically.
5. **Gizmo raycast on LMB.** `RenderWidget::primPickRequested(x, y)` already emits. Hook it to a new Rust handler that builds a ray via the existing `bif_viewport/src/selection.rs` code → sets `BifShellState::selected_prim_path`. Scene Browser already listens to this for sync.
6. **Breadcrumb wires to `selected_prim_pathChanged`.** The `breadcrumb_set_path` helper already exists in `window_builder.cpp` — connect it to the signal.
7. **Scene Browser real data.** Replace `SceneBrowserModel::seed_demo_tree` with a proper `bif_core::CompositeProvider` traversal. For 100K+ prims use `canFetchMore` / `fetchMore` for lazy population.
8. **Property Inspector real attributes.** Replace fake attrs with `UsdPrim::GetAttributes()` via new Rust invokables on `BifShellState`. Composition arcs via `UsdStage::get_prim_stack`.
9. **Timeline keyframes real.** Pull from the selected prim's `AnimatedTransform` keyframes.

### 🏛️ AppEvent bridge — the open decision

`bif_viewport::EventBus` + `AppEvent` + `dispatch_events` in `bif_viewport/src/*_dispatch.rs` are the existing dispatch system. Phase C–E.1 sidestepped it by mutating `BifShellState` directly. Real USD operations need dispatched handling (stage load → geometry extraction → GPU upload → scene tree refresh → selection reset → etc.). Three candidates:

| Option | Pattern | Pros | Cons |
|---|---|---|---|
| **(a) Global `Mutex<VecDeque<AppEvent>>`** polled per-frame | Rust singleton queue; panel invokables `push(AppEvent)`; a `QTimer` tick in `window_builder.cpp` drains + dispatches | Simple; fits cxx-qt pattern (invokables don't need extra state); drain can piggyback on paintEvent tick | Global state; harder to test; lock contention on high-frequency events (mouse drags) |
| **(b) cxx-qt QObject wrapping `EventBus`** passed into each panel | New `BifEventBus : QObject` with `#[qinvokable] fn push(event)`; held as child of `BifShellState`; panels access via `state->event_bus()` | Testable via dependency injection; clean ownership; fits Qt idioms | `AppEvent` is a Rust enum with heterogeneous payloads — bridging enum through cxx-qt is awkward, likely need a tagged-struct proxy |
| **(c) Per-panel Qt-native signals → Rust trampolines** | Each panel emits typed signals (`layerMuteToggled(int)`, `primSelected(QString)`, …); C++ signal connection calls Rust free fn that builds `AppEvent` + pushes to EventBus | Clean separation; panels don't know about AppEvent; easy incremental migration | Boilerplate trampolines — one per AppEvent variant (~28 variants); lots of small wiring |

**Recommendation: (a) + incremental (c) for high-frequency events.** Start with (a) — simplest plumbing, works immediately. Mouse drags (camera input, ruler scrub) that would churn through the queue can bypass via direct state writes (already the case in E.1). When the global queue hurts, migrate the hot paths to (c) per-panel signals. (b) is theoretically cleanest but the enum-bridge cost isn't worth it.

### 🗺️ Phase E.2 code map — where to look in the existing codebase

| Looking for | File |
|---|---|
| Scene load flow | `bif_viewport/src/scene_loader.rs::load_usd_scene` |
| SceneManager / Scene | `bif_core/src/scene.rs`, `bif_viewport/src/scene_manager.rs` |
| EventBus + AppEvent | `bif_viewport/src/event_bus.rs`, `bif_viewport/src/app_event.rs` |
| Dispatch modules | `bif_viewport/src/{render,node_graph,selection,project}_dispatch.rs` |
| Camera state + sensitivity | `bif_viewer/src/main.rs` (`ORBIT_SENSITIVITY`, `PAN_SENSITIVITY`) |
| Ray-cast / prim selection | `bif_viewport/src/selection.rs` |
| Renderer God object | `bif_renderer/src/lib.rs` or the renderer struct module |
| Existing egui panels (for reference) | `bif_viewport/src/{layer_stack_panel,scene_browser,property_inspector,timeline,render}.rs` |
| Timeline state + animated transforms | `bif_core/src/timeline.rs`, `bif_core/src/animation.rs` (look around `AnimatedTransform`) |

### 🏗️ Phase E.2 deliverables checklist

- [ ] `bif_qt::Viewport` renders real scenes (not a triangle)
- [ ] `File → Open` loads actual USD stage
- [ ] `BifShellState::scene_layer_state` populated from load, not demo seed
- [ ] Scene Browser shows real prim tree via CompositeProvider (+ lazy fetch for 100K+)
- [ ] Property Inspector shows real attributes via `UsdPrim::GetAttributes()`
- [ ] Composition arcs from `UsdStage::get_prim_stack`, not layer-stack mirror
- [ ] Timeline keyframes from selected prim's `AnimatedTransform`
- [ ] `detect_timeline_from_stage` reads real USD time metadata
- [ ] Gizmo raycast via `selection.rs` on LMB click
- [ ] Breadcrumb connected to `selected_prim_pathChanged`
- [ ] AppEvent bridge chosen + implemented
- [ ] `bif_qt_shell` loads `test_assets/layers/root.usda` end-to-end — Layer Stack shows 3 real layers, mute works, viewport updates

### ⚠️ Phase E.2 risk: scope overlap with Phase F

Phase E.2 as described is most of Phase F ("delete egui") bundled in. Two paths:
1. **Merge them** — Phase E.2 pulls Renderer/SceneManager/EventBus into bif_qt, egui-specific panel files in `bif_viewport` are deleted as we replace each one. Effectively the endgame of the migration.
2. **Keep separate** — Phase E.2 exposes Renderer/SceneManager via a new `bif_runtime` crate (or similar) that both `bif_qt` and `bif_viewport`'s legacy egui code depend on. Phase F then deletes `bif_viewport`'s egui parts.

**Recommendation:** Path 1. Path 2 duplicates effort.

---

## ✅ Rigid Mesh Offset Bug — FIXED (Apr 12, 2026)

**Root cause:** `SkinKind::Rigid` compression in `crates/bif_core/src/usd/loader.rs` assumed `element_size == 1, weight == 1.0`, but USD's `IsRigidlyDeformed()` is broader — it returns true for any per-prim binding, including multi-bone uniform influence (hair with 3 head/neck bones at w=0.333, fingernails with 2 tip bones at w=0.5). Taking only `joint_indices[0]` + `joint_weights[0]` collapsed each vertex by the fractional weight, visually shrinking the mesh toward its first bone.

**Fix:** loader now gates the compact `SkinKind::Rigid` path on `element_size == 1`. Multi-joint rigid meshes broadcast the single authored block across post-split vertices and flow through `SkinKind::PerVertex`.

**Regression guard:** new `skinning::tests::rigid_matches_pervertex_single_influence` — asserts `SkinKind::Rigid{J, 1.0}` produces identical output to `SkinKind::PerVertex{[J;N], [1.0;N], 1}` over a non-trivial palette.

**Diagnostic trail:** Python dump of `HumanFemale.walk.usd` via `UsdSkelSkinningQuery::ComputeJointInfluences` + joint-path resolution revealed the multi-joint rigid pattern (hair `elem=3, w=0.333`; nails `elem=2, w=0.5`; eyes/shoes `elem=1, w=1.0`). Single-joint meshes were unaffected — explains why shoes/eyes partially worked while hair/nails were visibly offset.

**Bug was pre-existing since v0.13.5.2** (commit `b61264e` introduced the compression). Not a v0.13.6 regression.

---

## Recent Work

### v0.13.6-dev Apr 12: Rigid Mesh Offset Bug Fixed (Apr 12, 2026)

- **Root cause:** `SkinKind::Rigid` compression in `bif_core/src/usd/loader.rs` assumed `element_size == 1, weight == 1.0`, collapsing multi-joint rigid bindings (hair `elem=3, w=0.333`; nails `elem=2, w=0.5`) to a single fractional-weight influence. Kernel then did `M·p·0.333`, visually shrinking each vertex toward its first bone's origin.
- **Fix:** loader gates the compact `SkinKind::Rigid` path on `element_size == 1`. Multi-joint rigid meshes now broadcast the single authored block across post-split vertices and flow through `SkinKind::PerVertex`.
- **Regression guard:** new `skinning::tests::rigid_matches_pervertex_single_influence` — locks `SkinKind::Rigid{J, 1.0}` to match equivalent `PerVertex{[J;N],[1.0;N], 1}`.
- **Diagnostic ladder:** (1) math equivalence test passed → kernel correct, bug upstream. (2) Python `ComputeJointInfluences` dump on `HumanFemale.walk.usd` revealed hair/nails are multi-joint rigid with fractional uniform weights — not the single-joint rigid the compression assumed.
- **Validated visually** on full HumanFemale walk cycle; hair, eyes, fingernails all in correct positions.
- Previous investigation notes:
  - Bisected v0.13.6 blend shape code via `#if 0` → ruled out as cause
  - C++ debug logging in `cache_skeleton_data` → verified loading data correct
  - Worktree A/B on commit `b61264e` → confirmed pre-existing v0.13.5.2 bug
  - Debug artifacts committed (`68c42e0`)
  - Removed `ARCHITECTURE_REFACTORS.md` (campaign closed, kept `ARCHITECTURE_REVIEW.md`)

### v0.13.6-dev Apr 11: UsdSkelBlendShape Implementation (Apr 11, 2026)

- **Full CPU blend shape pipeline** — C++ FFI (dense-expand at load, shape-order remap, per-frame `ComputeBlendShapeWeights` via cached `UsdSkelAnimQuery`), Rust FFI layer, `BlendShapeTarget`/`BlendShapeBinding` on `Mesh`, `apply_blend_shapes()` in skinning module, loader integration, per-frame playback hook (both inline and multi-draw paths).
- **Pipeline order:** blend shape deltas applied to `bind_positions` → scratch buffer → fed into `skin_positions`/`skin_normals`. Handles shapes-only meshes (no skin) and shapes+skin composition.
- **Test asset:** `two_bone_arm.usda` extended with 2 BlendShape prims (`squash`/`twist`) + animated weights over frames 0-36.
- **GPU stub:** `GpuBlendShapeLayout` in `bif_renderer` reserves data layout for future GPU path.
- **6 new unit tests** — all pass. Build + clippy clean.
- **TODO:** Manual validation on `HumanFemale.walk.usd` (has blink/face blend shapes per user). Wiki concept note. Version bump + release.

### v0.13.6-dev Apr 11: Architecture Refactor Campaign Closed (Apr 11, 2026)

- **Tracking docs synced.** `ARCHITECTURE_REFACTORS.md` phases 2-5 flipped from "Not started" → Complete with commit refs. `ARCHITECTURE_REVIEW.md` §10 gained a Status column; §2/§4/§9/§12 got resolution callouts. Both docs now archival.
- **Final state:** all 5 refactor phases + 7 of 8 review items shipped across v0.13.0-v0.13.5. ~79 new tests from the campaign (44 ffi_convert + 19 eval + 16 scene_pipeline). Remaining #4 (node graph extension checklist) shipped in `wiki/architecture/node-graph-system.md` as a terse 10-step reference card.
- **Phase 4.5 logged as deferred:** `scene_loader.rs` grew 2035 → 2413 LOC after Phase 4 (pipeline layer was additive, not a replacement). Trigger to resume: v0.14.0 layer-aware rewrite touching `finalize_usd_scene()`.
- **Test string cleanup:** `persistence.rs` `path_relativization_*` tests now use `#[cfg(windows)]` / `#[cfg(not(windows))]` constants instead of hardcoded `D:\\projects\\...` literals. `sample_project()` file_path dropped the `D:\\` prefix. 12/12 persistence tests green.
- **Pre-existing clippy breakage noted:** `cargo clippy --workspace -- -D warnings` fails with 56 errors (44 bif_core + 11 bif_viewport + 1 bif_perf) from a clippy version bump (`rust-1.92.0`). Confirmed unrelated to session via stash/repro against HEAD `b61264e`. Logged as separate follow-up. `cargo build` and `cargo fmt --check` are clean.

### v0.13.5 Apr 10: UsdSkel Import Complete (Apr 10, 2026)

- **All 4 phases done:** C++ SkelCache refactor, Mesh::skin wiring, CPU LBS module (8 unit tests), per-frame anim eval + viewport hookup. Plus 6 follow-up bugs fixed during HumanFemale validation: multi-draw skinning path, per-mesh joint-order remap, UV-seam vertex expansion, rigidly-deformed mesh broadcast, SkelRoot world xform override, and skipping per-frame xform animation for skinned meshes.
- **HumanFemale.walk.usd** loads coherent, all 77 skinned prototypes deform, walk animation plays correctly via the joint deformation pass. Hair, buttons, shoes all in correct positions.
- **New files:** `crates/bif_core/src/skinning.rs`, `wiki/usd/usdskel-import.md`, `test_assets/skel/two_bone_arm.usda`.
- **Tooling:** plumbed `skel_root_world_xform[16]` through 5 layers (C++ struct → header → ffi_raw → ffi_convert → cpp_bridge wrapper → loader). Multi-draw skinning path mirrors `update_vertex_animation`'s structure.
- **Remaining for release:** version bump `0.13.5-dev → 0.13.5`, MILESTONES.md Released section, release commit.

### v0.13.0 Apr 7: UsdStage Sync Fix + Architecture Audit (Apr 7, 2026)

- **Architecture audit** — reviewed ARCHITECTURE_REVIEW.md (5/8 done) and ARCHITECTURE_REFACTORS.md (3/5 phases complete). Mapped remaining work.
- **UsdStage Sync fix** — removed `unsafe impl Sync for UsdStage`, wrapped in `Arc<Mutex<UsdStage>>`. 10 files, ~20 callsites. Borrow-checker conflicts resolved with guard extraction and pre-extraction patterns.
- **setup_usd_env.sh** — added bin/usd plugin scan for PS1 parity.
- **Remaining:** SceneQuery API (bif_core trait), dispatch split (render/selection/project), Phase 2 Linux gaps.

### v0.13.0 Apr 6: Obsidian Knowledge Base (Apr 6, 2026)

- **wiki/ vault** — 42 articles across 8 sections (architecture, USD, rendering, concepts, rust, ui-ux, journal, raw). LLM-optimized indexes for Q&A. 4 templates (concept, adr, journal, article).
- **Devlog backlinks** — 92 devlog entries get `## Wiki Links` sections with Obsidian wikilinks
- **bif-commit updated** — Step 6 maintains wiki on each commit
- **CLAUDE.md updated** — Knowledge Base section with conventions

### v0.13.0 Apr 5: CPU Displacement + Selection Outline + Sync (Apr 5, 2026)

**Session 2 — CPU Vertex Displacement:**

- **CPU vertex displacement** — `displacement.rs` module: post-load pass samples heightmap per vertex, offsets along normal (USD 0.5 neutral). Bilinear sampling, sync image loader (PNG/JPG/EXR/TIF), `Mesh::recompute_bounds()`. Works in both viewport + Ivar. 14 unit tests.
- **C++ bridge MaterialX displacement fallback** — after MaterialX extraction, checks `GetSurfaceOutput()` for UsdPreviewSurface `inputs:displacement`. Handles Houdini's auto-generated preview shaders.
- **Test asset** — `displacement_test.usda` with manually patched UsdPreviewSurface displacement wiring (Houdini only generates MaterialX side).

**Known issues:**

- Dome light from Houdini USD not detected (needs investigation)
- MaterialX `ND_displacement_float/vector3` not natively extracted (workaround: UsdPreviewSurface fallback)

**Session 1 — Selection Outline + Tree/Viewport Sync:**

- **Selection outline rendering** — Replaced buggy `PolygonMode::Line` + `shading_mode` `queue.write_buffer` hack with dedicated `shaders/outline.wgsl`: normal-expanded back-face silhouette. Pipeline uses `cull_mode: Front` + `depth_compare: LessEqual` so only protruding rim passes depth test → clean Houdini-style silhouette. Dedicated `wireframe_cam_bind_group` with `shading_mode=2` baked in, updated per-frame.
- **Bidirectional tree ↔ viewport sync** — New `Renderer::select_at_screen()` handles viewport click flow (pick + set index + emit `PrimSelected` + reset gizmo + deselect on empty). `PrimSelected` handler now updates both `selected_prim_path` AND `scene_browser_state`, calls `expand_to_path()` to auto-reveal collapsed branches.
- **Robust prim_path lookup** — 3 fallbacks in `PrimSelected` handler: exact match → descendant prefix (parent Xform clicks) → synthetic `/BIF/{path}` prefix (handles empty `inst.prim_path` cases where `resolve_prim_path` synthesizes paths from proto names). `denormalize_synthetic_path()` strips `/BIF/` prefix + numeric `/{idx}` suffix for viewport → tree direction.
- **Viewport bounds guard** — `select_at_screen` early-returns on UI panel clicks so tree row clicks don't trigger deselect.
- **Dark-theme tree polish** — Removed green node-source highlight; only selected row painted. Fixed premultiplied-vs-unmultiplied alpha bug (`from_rgba_unmultiplied(74, 144, 217, 75)`). `selectable_label(false, ...)` prevents double-painting.

**Next priorities:**

1. Dome light bug — Houdini USD dome light not detected by C++ bridge
2. Native MaterialX displacement in C++ bridge (`ND_displacement_float/vector3`)
3. Embree displacement dicing (`rtcSetGeometryDisplacementFunction` callback)
4. Curves in Ivar (ribbon tessellation for BasisCurves)
5. OpenVDB volume rendering

### v0.13.0 Sessions Apr 2-4: Subdiv, Inspector, Display Color, Variants, Selection (Apr 4, 2026)

**Completed:**

- **Subdivision rendering** — Embree 4 Catmull-Clark with smooth limit-surface normals via rtcInterpolate (dPdu×dPdv). Tessellation rate 8. Fixed RTCBufferType enum values. Pre-UV-split positions via `vertices_orig` FFI.
- **USD attribute inspector** — Attributes tab in property panel, C++ bridge `usd_bridge_get_prim_attributes()`, primvars with interpolation.
- **Display color** — `primvars:displayColor` flows through pipeline to vertex color. ShadingMode toggle (Textured/DisplayColor).
- **Variant set UI** — Dropdowns in Attributes tab, `set_variant_selection()` + scene reload on change.
- **Selection sync** — Tree click maps prim_path → instance_index for viewport highlight. F to frame selected.
- **Displacement foundation** — Texture path + scale flows through FFI/Material. No vertex displacement yet.
- **Bug fixes** — Camera persistence, HDRI show_background, UNC path stripping, code review fixes (4 critical).

**WIP / Known Issues:**

- **Variant reload** — Currently does full file reload instead of re-extracting from live stage. Works but slow on large scenes. UNC path fix applied.
- **USD loader leaves `inst.prim_path` empty** for some load paths (observed on lucy.usd) — workaround via synthetic `/BIF/` path fallbacks in selection handler.

### v0.13.0 Phase 1: Bug Fixes + Subdivision Wiring (Apr 2, 2026)

- **v0.13.0 scope expanded** — from subdiv+displacement to full USD compatibility including OpenVDB. 5-phase plan (~70-108 hrs, target late May-mid June)
- **Camera persistence bug fixed** — reset viewport/batch camera source on new scene load
- **HDRI background toggle fixed** — removed is_loaded guard (blocked auto-loaded DomeLight HDRIs), added hdri_show_background to IvarState/RenderConfig/renderer
- **OCIO ACES** — verified already active in shader (Hill/Narkowicz approx), full OCIO deferred
- **Subdivision wired to Embree** — SubdivInfo preserves polygon topology through MeshData pipeline, Ivar passes SubdivData for Catmull-Clark limit surface. Single-mesh scenes only for now.
- **Plan file:** `.claude/plans/sharded-moseying-hickey.md`

### Qt UI Spec §17-25 + Wireframe Selection (Apr 4, 2026)

- `UI_DESIGN.md` extended from 16→25 sections: context menus, multi-select, undo feedback, long-op progress, reduced-motion accessibility, tree filter, error states, workspace storage, cxx-qt decision
- Click target spec corrected (24px→32px rows, 44px toolbar); node label 10px→12px
- `cxx-qt` decided as Qt/Rust binding strategy; drag-and-drop deferred to v0.16.0
- Wireframe selection overlay committed — `POLYGON_MODE_LINE` pipeline + `VariantChanged`/`FrameSelected` `AppEvent` variants

### Qt UI Design Spec Consolidation (Apr 1, 2026)

- `docs/ux/UI_DESIGN.md` promoted to single authoritative Qt UI spec (16 sections, ~730 lines)
- UX Architect + UX Researcher reviews conducted on batch 0 Stitch mockups, findings incorporated
- 14 Stitch mockups across 2 batches covering all 4 workspaces + first launch screen
- Key additions: Bjorn asset manager, active layer safety system, vertical code split layout (preferred), opinion encoding table, command palette details, Render workspace (renamed from Review), first launch onboarding
- 4 remaining mockup gaps: context menu, error states, 15+ node graph, tooltip design
- Doc hierarchy: UI_DESIGN.md (spec) + DCC_UI_RESEARCH.md (research) + DESIGN.md (tokens) + reviews (audit trail)

### MaterialX File Format Support (Mar 31, 2026)

- Rebuilt vcpkg USD 25.11 with `materialx` feature — adds `usdMtlx` plugin for `.mtlx` file references
- C++ bridge: usdMtlx plugin detection at startup, `resolve_mtlx_input()` follows Material interface connections, deep descendant shader search by `info:id`, refactored duplicated extraction into shared helper
- `setup_usd_env.ps1` scans both `bin/usd` and `lib/usd` for plugin resources
- **WIP**: Scalar values from Material interface inputs not resolving yet — needs debugging (textures load fine)
- **WIP**: Normal maps may not work correctly with composed MaterialX structure

### UI/UX Design Brainstorm (Mar 30, 2026)

- Designed viewport-dominant T-layout, layer color coding system, opinion stack, command palette
- Full design: `docs/ux/UI_DESIGN.md` | Research: `docs/ux/DCC_UI_RESEARCH.md`
- Updated MILESTONES.md + ROADMAP_DETAIL.md with UI features threaded into v0.14–v0.16
- Material editor designed: param sheet + node graph + floating lookdev orb ([design](docs/ux/MATERIAL_EDITOR_DESIGN.md))

### Power-Weighted Light Sampling (Mar 30, 2026)

- Replaced uniform 1/N light selection with power-weighted CDF in `LightList`
- Added `power()` to `Light` trait (DistantLight, SphereLight, RectLight)
- Foundation for hierarchical light tree (v0.20.0) and env map visibility cache (v0.22.0)
- Scoped two Octane-inspired features: many-light sampling + env visibility cache

### SHARC Cache + Max Depth UI (Mar 30, 2026)

- SHARC radiance cache now skips low-roughness surfaces (< 0.1) via new `Material::roughness()` trait — fixes blurred reflections on glossy/mirror materials
- Added Max Depth slider (1–32) to interactive Ivar render panel
- Roadmap trimmed: removed AI Integration version, renumbered

### GitHub Pages Site (Mar 30, 2026)

Set up mdBook-based site with auto-deployed dev diary (85 entries) + manual (USD reference, getting started, architecture, changelog). `scripts/generate-site.sh` auto-generates SUMMARY.md from devlog tree. GitHub Actions deploys on push. **Action needed:** enable Pages source = "GitHub Actions" in repo settings.

### PointInstancer Loading Fixes (Mar 30, 2026)

Fixed two bugs preventing time-sampled PointInstancer files (e.g., Pixar's PointInstancedMedCity.usd) from loading:

1. C++ bridge now uses stage startTimeCode / first sample instead of Default when reading instancer attrs
2. Rust loader maps parent Xform paths in prototype_map for instancer prototype resolution

Test file: `assets/PointInstancedMedCity.usd` (40K instances, 8 prototypes)

### Architecture Deepening: Phase 1 FFI Bridge Split (Mar 28-29, 2026)

Split monolithic `cpp_bridge.rs` (4,542 LOC) into 3 modules:

- `ffi_raw.rs` (898 lines) — `#[repr(C)]` types + `extern "C"` block
- `ffi_convert.rs` (2,054 lines) — 17 conversion functions + 44 tests (no C++ DLLs needed)
- `cpp_bridge.rs` slimmed to 3,651 lines (-20%)

Also created `ARCHITECTURE_REFACTORS.md` (5-phase plan) and `BIF_USD_WORKFLOW.md` (layer-aware editor spec).

**Next:** Wire UsdStage methods to delegate to ffi_convert (incremental), then Phase 2 (Linux cross-platform).

### Documentation Overhaul (Mar 27, 2026)

Reworked project documentation to correlate milestones with semantic versioning:

- **MILESTONES.md** — rewritten as lean semver roadmap (v0.13.0 through v0.23.0+)
- **MILESTONES_HISTORY.md** — new file, all completed milestones (M0-M31) moved here
- **ROADMAP_DETAIL.md** — new file, per-version task lists + acceptance criteria
- **README.md** — full rewrite, new positioning ("lightweight scene assembly"), updated stats
- **CHANGELOG.md** — targeting v0.13.0 note
- **Cargo.toml** — version bumped to 0.13.0-dev
- **bif-commit skill** — updated for new file structure
- **vfx-code-reviewer agent** — added version scope awareness

Key decisions informed by software architect + engineer reviews:

- Qt migration (v0.15.0) promoted before context system — avoids building UI twice
- M22/M25/M27 no longer deferred — all scheduled in roadmap
- 1.0 criteria defined (10 gates)

### M30 Complete (Mar 24-26, 2026)

All 6 phases landed: serde foundation, ProjectFile persistence, file menu + save/load UI, eval modes (Auto/Manual/OnMouseRelease), cache node with bypass toggle.

### M31 Complete (Mar 26, 2026)

Per-node scene graph visualization — source node tagging, prim count badges, filtered provider.

### M29.5 Complete (Mar 23, 2026)

egui UI overhaul — centralized theme, panel restructure, property inspector, menu bar, Unicode icons.

---

## 🎨 User preferences captured this migration

- **Ease-of-use over modal dialogs.** Inline controls on toolbars beat "Global Animation Options" popups. Timeline range/fps/loop/RT live on the toolbar, not a menu.
- **Nuke-inspired layouts** for timeline-style UIs. 3-zone toolbar (config / transport+counter / range) landed this session.
- **"Put our spin on it"** — take DCC conventions as input, not as specification. Loop toggle is a boolean now; grow to Repeat/Bounce/Stop/Continue enum when complexity justifies it.
- **Plan for customizability early.** User wants rebindable shortcuts — the ShortcutRegistry pattern (string-ID + QSettings override) lands in advance of the v0.16 Preferences dialog.
- **Auto-detect from USD by default, manual override available.** Timeline detects `timeCodesPerSecond` from the stage; user can override via the fps spinbox. Same shape for anything stage-derived.

---

## 🛠️ v0.15 Phase A–E.1 recent work

### Phase E.1 — Input + event wiring (session 8, `759a01f`)

- `RenderWidget` mouse input: Alt+LMB orbit / MMB pan / wheel zoom / unmodified LMB = primPickRequested. Signals forwarded through 4 new `on_camera_*` / `on_frame_selected` invokables on `BifShellState` (stubs until Phase E.2).
- `ShortcutRegistry` (new `cpp/shortcut_registry.{h,cpp}`) with string-ID lookup + `QSettings("shortcuts/<id>")` override path. Wired F, Space, Left/Right, Shift+Left/Right.
- Real `QFileDialog` on File → Open; writes `QSettings("recent_stages")` (max 10), flips central stack to viewport.
- QTimer-driven timeline playback with configurable fps, real-time mode, and loop toggle. Nuke-style 3-zone toolbar (config / transport+orange frame counter / range + detect).
- New qproperties: `playback_fps`, `realtime_playback`, `loop_playback`. New invokables: `jump_to_prev_keyframe`, `jump_to_next_keyframe`, `detect_timeline_from_stage` stub, `on_stage_path_opened`.

### Phase D — Secondary panels (session 7, `109d2d3`)

- Timeline (`cpp/timeline_widget.{h,cpp}`): custom `paintEvent` for ruler + playhead + keyframes. Scrub via mouse.
- Node Graph (`cpp/node_graph_widget.{h,cpp}`): `QGraphicsScene` + `NodeGraphView` subclass for wheel-zoom + middle-mouse pan. `BifNodeGraphicsItem : QGraphicsObject` with category-colored headers, pin lollipops. 5-node demo.
- Render Settings (`cpp/render_settings_widget.{h,cpp}`): `QFormLayout` in two styled `QGroupBox`es.

### Phase C — Core panels (session 6, `b859c2c`)

- Layer Stack (`cpp/layer_stack_{model,widget}.{h,cpp}`): `QListView` + `QAbstractListModel` reading `SceneLayerState` via 11 new `#[qinvokable]` methods. Color dot delegate + editorEvent-aligned checkboxes.
- Scene Browser (`cpp/scene_browser_{model,widget}.{h,cpp}`): `QTreeView` + `QAbstractItemModel` + hierarchical filter. Selection routes to `selected_prim_path`.
- Property Inspector (`cpp/property_inspector_widget.{h,cpp}`): QTabWidget + collapsible composition arcs + attribute table with opinion-dot delegate.
- New qproperties: `layer_state_revision` (bumps on mutation → triggers model refresh via `*Changed` signal), `selected_prim_path`, `selected_prim_type`.

### Phase B — Qt shell (sessions 4–5, `7499faa` + `1776fdc`)

- Viewport embedded (B.1), stylesheet (B.2), menu invokables (B.3), Zen mode Ctrl+\\ (B.4), workspace switcher with QSettings persistence (B.5), first-launch welcome screen (B.6), breadcrumb `QToolBar` (B.7), command palette Ctrl+P (B.8).
- Central layout: `QWidget(QVBoxLayout(breadcrumb, QStackedWidget(first_launch, viewport)))`.

### Phase A — `bif_qt` scaffolding (session 3, `8ce4271`)

- New `crates/bif_qt/`. First real `#[cxx_qt::bridge]` in BIF. `BifShellState` QObject with 2 qproperties + 1 qinvokable. 34-color theme port + Qt stylesheet generator. C++ window assembly with menu bar + 4 dock placeholders.

### Phase 0 — wgpu-into-QWidget spike (session 2, `c2440db`)

- `crates/bif_qt_spike/` proved wgpu + Qt coexistence on MSVC 2022 + Qt 6.8.3 LTS. ADR-006 authored. Toolchain validated (cxx + qt-build-utils + cc + moc). Deletion scheduled for Phase H.

---

## Next Steps (v0.15.0)

1. **Phase E.2 — Real USD wiring** (~5–8h). See checklist above. This is the big integration step.
2. **Phase F — egui cleanup** (~2h, may merge with E.2). Delete `egui` + `egui-wgpu` + `egui-winit` + `egui-snarl` from `bif_viewer` + `bif_viewport` Cargo.toml. Delete `bif_viewport::run_egui_frame` and the ~900 lines of panel-assembly code. Rename `bif_viewer`'s main to target `bif_qt::run`.
3. **Phase G — tests + validation** (~2h). Unit tests on Qt models (headless — no QApplication needed for model-only tests with mocks). Manual checklist from plan: load `test_assets/layers/root.usda`, mute shot.usda + anim.usda behaviors, HumanFemale.walk.usd scrub, workspace switch, command palette, Zen mode, project save/close/reopen.
4. **Phase H — release plumbing + tag** (~1h). Bump `0.15.0-dev → 0.15.0`. Promote CHANGELOG `[Unreleased]` → `[0.15.0]`. MILESTONES v0.15.0 → Released. Wiki post-mortem at `wiki/ui-ux/qt-migration.md`. Update ADR-006 with Phase A–G learnings. Release commit + `git tag -a v0.15.0`. Merge `v0.15-qt` → `main`.

## Follow-up debts across phases (v0.15.5 / v0.16)

- Command palette: fuzzy scorer (skim or inline), widen beyond menu commands to prims/layers/nodes via provider interface.
- Node Graph: can't create/delete/rewire nodes (v0.16). Orthogonal/manhattan wire routing option (TODO marker in `BifNodeWire::refresh`).
- Timeline: loop mode enum upgrade (Repeat/Bounce/Stop/Continue); consider scrub slider under the frame counter.
- Breadcrumb segment styling (placeholder-only as of Phase B.7; Phase E.2 populates).
- Preferences dialog (v0.16) — enumerates `ShortcutRegistry::registered_defaults()` with `QKeySequenceEdit` per entry.
- `BifShellState` is a fat singleton — v0.16 may split into `BifShellState` / `BifSceneState` / `BifSelectionState` if the invokable surface grows past maintainability.
- Theme polish per `docs/ux/UI_DESIGN.md` (25 sections) — Phase G spends time here; post-v0.15 iteration continues against Stitch mockups in `assets/stitch_bif_ui_01/`.
- Bjorn opinion stack, Material Editor (v0.21), rich per-panel UX per UI_DESIGN.md — future releases.

---
