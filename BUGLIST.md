# BUGLIST

Last updated: 2026-04-08

## Active Bugs

- OCIO ACES is not working in the viewport (Hill/Narkowicz approx active, full OCIO deferred).

- Pre-existing C++ bridge test crashes: `test_load_pointinstancer_external_prototype` (lucy_100_fixed.usda), `test_load_relative_reference_usda` (lucy_100.usda), `test_define_scope_prim` — all crash at `UsdStage::Open` with STATUS_BREAKPOINT. Not caused by recent changes.
- `inst.prim_path` left empty for some USD load paths (observed on lucy.usd) — workaround via synthetic `/BIF/` path fallbacks in selection handler.

## Fixed (since last update)

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
