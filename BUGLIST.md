# BUGLIST

Last updated: 2026-04-19

## Active Bugs

- **bif_qt scene browser: residual child-count gap under some sections vs egui.** Tier 0 routed Qt through `CompositeProvider` (parity with egui's data path), but a few sections still show fewer children than egui in side-by-side. Likely a `UsdStage::child_prim_paths` quirk (composed-stage iteration vs root-layer iteration) or empty `inst.prim_path` synthesis not reaching the cache in Qt builds. Noted 2026-04-16.
- **bif_qt: scale factor hardcoded to 1.0.** `Viewport::new` / `resize` ignore `QScreen::devicePixelRatio()`; HiDPI monitors render at wrong scale. Wire through the cxx-qt bridge. Noted 2026-04-15.
- OCIO ACES is not working in the viewport (Hill/Narkowicz approx active) full OCIO still needs to be implemented.

- Pre-existing C++ bridge test crashes: `test_load_pointinstancer_external_prototype` (lucy_100_fixed.usda), `test_load_relative_reference_usda` (lucy_100.usda), `test_define_scope_prim` — all crash at `UsdStage::Open` with STATUS_BREAKPOINT. Not caused by recent changes.
- `inst.prim_path` left empty for some USD load paths (observed on lucy.usd) — workaround via synthetic `/BIF/` path fallbacks in selection handler.
- implement an on/off button for the grid
- need to see aovs, add aovs, plan this out, i want to implement aovs with ease and flexibility.
- need viewport switch between vulkan and render

## Fixed (since last update)

- bif_qt: no way to view through USD cameras or standard orthographic views — camera-picker QComboBox added with UsdGeomCamera enumeration + 6 ortho presets + free-fly toggle (commit c835eaf, 2026-04-19).
- bif_qt property inspector opinion dot used prim-level winning layer for all attribute rows — replaced with per-attribute `get_attribute_opinions` call (Tier 1.5, 2026-04-17).
- bif_qt animation playback — `Renderer::set_time` + `on_frame_changed` invokable wired to QTimer (commit 962a3b7, 2026-04-17).
- bif_qt close stage — `reset_scene_state` helper + File → Close Stage (commit 962a3b7, 2026-04-17).
- bif_qt scene browser showed hardcoded demo tree — replaced with `CompositeProvider` traversal (commit 962a3b7, 2026-04-17).
- bif_qt property inspector showed fake attrs — real `get_prim_attributes` + `get_prim_stack` (commit 962a3b7, 2026-04-17).
- bif_qt timeline keyframes hardcoded — real `AnimatedTransform` keyframes (commit 962a3b7, 2026-04-17).
- bif_qt gizmo raycast not wired — `on_prim_pick` invokable + `pick_instance_at` (commit 962a3b7, 2026-04-17).
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
