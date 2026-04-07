# Session Handoff - April 6, 2026

**Last Updated:** Obsidian knowledge base wiki/ + devlog backlinks
**Current Version:** v0.13.0-dev (Open Any USD Scene)
**Project:** BIF - USD Orchestration Tool for VFX

---

## Quick Status

| Status | Details |
|--------|---------|
| Released | v0.1.0, v0.11.0, v0.12.0 |
| Current | v0.13.0-dev — Phase 1-2 done, Phase 3 in progress |
| Next | Dome light bug, native MaterialX displacement, Embree dicing, curves in Ivar, OpenVDB |
| Tests | 530 total across all crates (14 new displacement, 111 renderer pass, 6 pre-existing HDRI failures) |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms |

---

## Recent Work

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

## Next Steps

1. **v0.13.0 release** — final validation, version bump, ship current work
2. **v0.14.0 planning** — USD composition inspector + opinion trace (M32, M33)
3. **v0.15.0 research** — Qt 6 Rust bindings evaluation (cxx, ritual, qt-build-utils)
