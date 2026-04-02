# BUGLIST

Last updated: 2026-03-31

## Active Bugs

- Camera persistence bug: after loading a second USD, the previous camera remains in the list; duplication only works from camera view.
- OCIO ACES is not working in the viewport.
- HDRI properties bug: background display does not update correctly.

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
