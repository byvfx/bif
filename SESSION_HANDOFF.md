# Session Handoff - March 26, 2026

**Last Updated:** Per-tile UDIM loading (eliminate atlas stitching)
**Next Milestone:** M32 (USD composition inspector & opinion trace)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26, M26.1, M19.6, M29.5, M30, M31 |
| Current | Per-tile UDIM loading complete, async texture streaming next |
| Next | M32 (opinion trace), then M33 (usdview-parity debugging) |
| Tests | 394+ total across all crates |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms |

---

## Recent Work

### Fix: Zombie Process + Unsaved Changes Dialog (Mar 26, 2026)

Fixed two close-related bugs: (1) zombie process after window close — added `process::exit(0)` after event loop to avoid native DLL teardown deadlock on Windows; (2) unsaved changes dialog never showing — added `mark_dirty()` to gizmo drag, undo, redo, keyframe; (3) dialog appearing behind window — hide main window while rfd MessageDialog shows. Stored `Arc<Window>` in Renderer.

### Per-Tile UDIM Loading (Mar 26, 2026)

Replaced UDIM atlas stitching with per-tile loading. Each UDIM tile is now an individual texture in the GPU binding array. Shader computes tile offset from UV floor. New `UdimTileSet` type in bif_core provides unified CPU/GPU sampling. Tested with alab scene (743 textures, zero stitching). Next step: switch viewport to async texture streaming path for interactive loading.

### M31: Per-Node Scene Graph Visualization (Mar 25, 2026)

4-phase implementation:
1. **Source tagging** — ProceduralPrim.source_node via reverse maps from node_proto_map/node_cloud_map
2. **Prim count badges** — `[N]` overlay on node headers via NodeGraphContext.node_prim_counts
3. **Node scene browser** — Scene/Node tab bar, NodeFilteredProvider filters to upstream subgraph
4. **Row highlighting** — selected node's prims tinted green in full scene browser

**Pre-existing test failures:** test_should_restart_no_render (known flaky), test_build_property_rows_with_material (Material::default() row count mismatch — needs investigation)

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
