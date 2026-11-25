---
title: "Tier 0 + Phase F — egui bridge deleted"
type: journal
tags: [phase-f, qt-migration, egui-deletion, scene-browser]
created: 2026-04-16
updated: 2026-04-16
---

# Tier 0 + Phase F — egui bridge deleted

Same-day continuation of the [[2026-04-16-phase-e2-finish|Phase E.2 finish]] session. Two adjacent work units shipped together:

## Tier 0 — Qt Scene Browser parity (parity gate)

Pre-Phase-F gate: needed Qt scene browser to match egui's data model so the side-by-side comparison stayed meaningful right up to the moment egui got deleted.

- **CompositeProvider routing.** New `with_scene_browser_provider` helper in `bif_qt::main_window` builds a [[primdataprovider-trait|PrimDataProvider]]-trait `CompositeProvider` (USD stage + procedural prim cache + synthetic `/BIF/`) per call. The 4 tree invokables (`root_prim_count`, `root_prim_path_at`, `child_prim_count`, `child_prim_path_at`) plus `prim_type_name_at` route through it. Same data path egui uses.
- **Helper required a split-borrow trick** — clone `Arc<Mutex<UsdStage>>` first so the `MutexGuard` borrows from a local; that frees `renderer` for the `cached_scene_graph()` method call. Method calls can't be split-borrowed across struct fields like direct field access can.
- **3 new invokables** for Type/Children/Kind columns: `prim_kind_at`, `prim_is_visible_at`, `prim_is_active_at`. Specifier dropped — `UsdPrimInfo` from the C++ bridge doesn't surface it from the composed stage; egui doesn't show it either.
- **`SceneBrowserModel` 1→4 columns.** New `Columns` enum, 4 new `PrimRoles`, `PrimNode` extended, header now visible with `Stretch` / `ResizeToContents` section modes.
- **Row chrome.** `PrimRowDelegate` paints an eye glyph (filled visible / strike-through hidden) before the existing layer color dot in column 0; inactive prims dimmed via `QPalette::Text` alpha override across all columns. Read-only — toggle interactivity is Tier 1+.

## Phase F — egui bridge deletion

`bif_viewer` is now a 14-line shim over `bif_qt::run()`. Default `cargo build` produces a Qt-only viewer.

- **Renderer egui surface gone.** `egui_ctx`/`egui_state`/`egui_renderer` fields, `attach_egui`/`egui_state_mut`/`egui_ctx`/`reset_property_inspector_cache` methods. `Renderer::render` now `render(&mut self, clear_color: wgpu::Color) -> Result<()>`. Internal `run_egui_frame` (~870 LOC) and `submit_gpu_frame`'s egui paint pass deleted.
- **`render_ui.rs` deleted** (~729 LOC).
- **Cargo.toml:** dropped `egui-wgpu` + `egui-winit` from `bif_viewport`. Dropped `wgpu`/`winit`/`egui`/`egui-wgpu`/`egui-winit`/`pollster` from `bif_viewer`; added `bif_qt`. Confirmed via `cargo tree` that neither pulls `egui-wgpu` or `egui-winit` anymore.
- **`bif_viewer/src/main.rs`** rewritten 905 → 14 lines.
- **`bif_qt` fallout — one line.** `viewport.rs::Viewport::render` dropped the `None` raw_input arg.

## Plan adaptations worth remembering

- Original plan's T0.1 ("switch QTreeWidget → QTreeView") was **stale** — already QTreeView since Phase E.2. Real T0.1 work was bumping `columnCount()` 1→4 and wiring invokables.
- Original Phase F's `egui_panels_legacy` feature flag **skipped**. Cleanly gating panel modules required also gating `NodeGraphContext` (egui-snarl typed) — large refactor for a "future cannibalization" benefit only. The panel modules (`property_inspector`, `layer_stack_panel`, `node_graph`, `theme`) stay in-tree as dead code; revisit when Qt replacements land. `egui` + `egui-snarl` deps stay in `bif_viewport` for the same reason.
- Recon-first paid off: 1 batch of greps caught 4 stale plan premises before any code changed.

## Residual

- **CompositeProvider parity gap** under a couple sections in lucy.usd — Qt still trails egui by a few children even with same data path. Likely a composed-stage-vs-root-layer iteration question in `UsdStage::child_prim_paths`. Filed in BUGLIST. Acceptable to ship.
- **CLI autoload regression** — `bif_viewer --usd <path>` doesn't pass through `bif_qt::run()`. Use File → Open Stage. Filed in BUGLIST.

## Next

[[../_index|Tier 1]]: edit-target pill + 2px viewport edge tint + status bar chip, auto-pick writable sublayer, schema name labels, save flow feedback, breadcrumb layer segment, viewport toolbar.
