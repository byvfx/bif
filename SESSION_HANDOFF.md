# Session Handoff - March 24, 2026

**Last Updated:** M30 Phase 1 (serde foundation) complete
**Next Milestone:** M30 Phase 2 (ProjectFile + save/load logic)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26, M26.1, M19.6, M29.5, M30 Phase 1 (serde) |
| Current | M30 persistence — Phase 1 done, Phase 2 next (ProjectFile, save/load) |
| Next | M30 Phases 2-6 (ProjectFile, dirty tracking, File menu, eval modes, cache node) |
| Tests | 390+ total across all crates |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms |

---

## Recent Work

### M30 Phase 1: Serde Foundation (Mar 24, 2026)

Added Serialize/Deserialize derives to all types needed for .bif/.bifa project files:
- **bif_math:** ProjectionMode, OrthoPreset (Camera handled via CameraData conversion)
- **bif_core:** PrimitiveKind, PointSource, ScatterMode, UsdSpecifier, UsdKind, UsdPrimType
- **bif_renderer:** PixelFilter, PixelFilterConfig, SamplerMode, ExrCompression, RadianceCacheConfig
- **bif_viewport:** SceneNode (all 10 variants, runtime fields `#[serde(skip)]`), GraphNodeId, CameraSource, AovSettings, BatchRenderSettings, PurposeMode, DisplaySettings
- Enabled egui-snarl `serde` feature (Snarl<SceneNode> serializes nodes+connections+positions)
- Added bincode dep for .bif binary format
- Round-trip tests: JSON + bincode for all SceneNode variants, Snarl graph, GraphNodeId

**Plan:** Full M30 plan at `~/.claude/plans/woolly-meandering-avalanche.md` — 6 phases total.

### M29.5 egui UI Overhaul (Mar 23, 2026)

8-phase restructure of the egui UI:
1. **Theme** — new theme.rs with 28 color constants + apply_theme()
2. **Colors** — replaced 35+ inline Color32 literals with theme:: constants
3. **Panels** — scene browser promoted to top of left panel, stats moved to viewport overlay, render settings below browser, removed dead show_ui toggle
4. **Menu bar** — File (Open USD, Export, Quit), View (Grid, Points), Render (Vulkan, Ivar, Rebuild)
5. **Node inspector** — params moved from show_body() to property inspector (all 10 node types, ~600 lines)
6. **Icons** — emoji replaced with colored Unicode geometric shapes
7. **Tooltips** — 37+ tooltips on all interactive controls
8. **Empty state** — welcome screen with Open USD button on first launch
9. **Node selection** — show_header() click detection + accent highlight

Code review fixes: extracted open_usd_file_dialog helper (was 3x duplicated), added BG_OVERLAY_BACKDROP theme constant.

### Known Limitations (from review)
- show_body() auto-compute coupled to UI rendering — off-screen nodes don't cook (pre-existing, document before M31)
- egui-snarl 0.5 hardcodes left-click for background panning — middle-mouse needs snarl upgrade
- `select_stoke` is an upstream typo in egui-snarl (compiles, works, just misspelled)

---

## Next Steps

1. **M30 persistence** — save/load node graphs (serde on SceneNode + NodeGraphState)
2. **egui 0.30 upgrade** — enables snarl 0.6+ with vertical node layout (Sandwich)
3. **Consider:** split render_node_properties() into per-node-type functions (~720 lines)
4. **Consider:** add explicit Recompute button in scatter inspector (safety net for off-screen nodes)
