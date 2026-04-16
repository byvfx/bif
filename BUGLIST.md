# BUGLIST

Last updated: 2026-04-15

## Active Bugs

- **bif_qt: no "reset / close stage" action.** Once a USD stage is loaded, there's no way to unload it or return to the first-launch screen without quitting the app. Needs File → Close Stage menu item (Phase E.2 follow-up). Noted 2026-04-15.
- **bif_qt: Scene Browser shows hardcoded demo tree.** `SceneBrowserModel::seed_demo_tree` is still the 10-prim hardcoded demo (World/Hero/Geom/…). Phase E.2 move 7 replaces with `CompositeProvider` traversal over the real USD stage. Noted 2026-04-15.
- **bif_qt: Property Inspector shows fake attributes per prim-type.** Phase E.2 move 8 wires `UsdPrim::GetAttributes()` + `UsdStage::get_prim_stack` for real composition arcs. Noted 2026-04-15.
- **bif_qt: Timeline keyframes are hardcoded demo.** Phase E.2 move 9 pulls keyframes from selected prim's `AnimatedTransform`. Noted 2026-04-15.
- **bif_qt: Gizmo raycast not wired.** `RenderWidget::primPickRequested(x, y)` signal fires but no selection handler consumes it. Phase E.2 move 5 connects to `bif_viewport::selection::ray_cast`. Noted 2026-04-15.
- **bif_qt: scale factor hardcoded to 1.0.** `Viewport::new` / `resize` ignore `QScreen::devicePixelRatio()`; HiDPI monitors render at wrong scale. Wire through the cxx-qt bridge. Noted 2026-04-15.
- OCIO ACES is not working in the viewport (Hill/Narkowicz approx active, full OCIO deferred).

- Pre-existing C++ bridge test crashes: `test_load_pointinstancer_external_prototype` (lucy_100_fixed.usda), `test_load_relative_reference_usda` (lucy_100.usda), `test_define_scope_prim` — all crash at `UsdStage::Open` with STATUS_BREAKPOINT. Not caused by recent changes.
- `inst.prim_path` left empty for some USD load paths (observed on lucy.usd) — workaround via synthetic `/BIF/` path fallbacks in selection handler.

## Fixed (since last update)

- Rigid-skinned mesh offset on animated characters — `SkinKind::Rigid` compression in `bif_core/src/usd/loader.rs` assumed `element_size=1, weight=1.0`, collapsing multi-joint rigid bindings (hair at w=0.333, nails at w=0.5) to a single fractional influence. Loader now only uses the compact encoding when `element_size == 1`; multi-joint rigid meshes broadcast through `SkinKind::PerVertex`. New regression test added. Fixed 2026-04-12.
- Dome light from Houdini USD not detected — C++ bridge now checks `UsdLuxDomeLight_1` (new schema), attribute lookup tries `inputs:texture:file` first. Fixed 2026-04-08.
- MaterialX displacement now natively extracted — 3-tier: surface shader input, GetDisplacementOutput() → ND_displacement node, UsdPreviewSurface fallback. Fixed 2026-04-08.
- Camera persistence bug: fixed in v0.13.0-dev (reset viewport/batch camera source on new scene load).
- HDRI properties bug: fixed in v0.13.0-dev (removed `is_loaded` guard, added `hdri_show_background`).

## Investigate & Validate

### Performance & Profiling

- Implement metrics tracking (time, memory, throughput).
- Compare bif vs usdview on reference scenes (OpenUSD docs plus custom assets).
- Review [OpenUSD v25 performance guidance](https://openusd.org/release/ref_performance_metrics.html) and audit compliance.
- Plan upgrade path to USD v26 after stabilization.
- Benchmark scene load and playback on SSD.
- Investigate RenderMan denoising integration.

### USD & Pipeline Research

- Investigate rigid-body animation flow into USD, then into bif for viewport/rendering (Houdini-style RBD procedural workflow).
- Keep selective prim loading simple for artists, with optional deeper controls.
- Validate subdivision surface behavior with self-authored assets.
- Verify proxy material fallback behavior (use display color when no material is bound).
- Check depth usage and expose useful controls in UI.
- Compare with Claude the Houdini USD nodes and what we could use, same as Katana. Let's build something simple but elegant.

### Architecture Notes

- Use GetBracketingTimeSamples instead of GetTimeSamples for large-clip performance.
- Extract evalTime logic into a helper.
- Consider exposing resolved evalTime to Rust for timeline scrubbing.
- Double-check USD schema compliance (custom vs standard).

### Scene Assembly Open Questions

- Goal: simple top layer for artists, optional deep layer for power users.
- Open design questions: UI paradigm, prim workflow, export and save patterns.
- RBD integration planning notes are in ./claude/plans.
