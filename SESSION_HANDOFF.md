# Session Handoff - February 10, 2026

**Last Updated:** Bug fix session — 5 bugs + code review critical fixes
**Next Milestone:** M21 Point Instancing + Scattering
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-20 + post-M20 polish + bug fixes |
| Current | M21 Point Instancing + Scattering |
| Tests | 41 bif_math passing (bif_core needs USD DLLs) |
| Performance | 60 FPS viewport, 10K instances with LOD |

---

## Recent Work

### Bug Fix Session (Feb 10, 2026)

5-phase bug fix from BUGLIST.md:

| Phase | Fix | Key Files |
|-------|-----|-----------|
| 1 | Surface error on minimize: handle Outdated + skip 0x0 frames | `main.rs` |
| 2 | Grid visibility toggle checkbox in top toolbar | `lib.rs`, `render.rs` |
| 3 | Node deletion: right-click menu + click-to-select + keyboard delete | `node_graph.rs`, `render.rs` |
| 4 | Multi-object persistence: working_scene, reload_working_scene, undo | 7 files |
| 5 | Camera dropdown fix (solved by Phase 4 persistence) | verified in `render.rs` |

**Key architecture change:** `Renderer` now has a `working_scene: Scene` that accumulates all primitives and USD objects. `reload_working_scene()` rebuilds all GPU state from it. This replaces the old pattern where each `load_primitive()` call created an isolated scene.

**Code review critical fixes:**
- `delete_selected()` → returns `Option<NodeId>`, callers emit `DeleteNode` (was leaking scene data)
- `remove_instance`/`remove_prototype` camera retain/re-index logic fixed
- USD merge remaps proto IDs + camera instance indices via offsets
- Instance buffer truncated at MAX_INSTANCES with warning

### Post-M20: Ground Grid, Scene Cameras, Ivar Fix (Feb 7, 2026)

| Item | Description | Key Files |
|------|-------------|-----------|
| Ivar fix | Render uses viewport rect not full window | `ivar_build.rs` |
| Ground grid | Infinite XZ grid, anti-aliased, distance fade | `grid.wgsl`, `grid.rs` |
| Scene cameras | Camera prims usable as render cameras | `scene.rs`, `ivar_state.rs`, `render.rs` |
| Code review | 5 fixes: grid depth/order, ivar bind group, shader sync, stale cam, names | 7 files |

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 41+ passing (bif_math) |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Bug Fixes (this session):**
- Multiple primitives coexist (working_scene persistence)
- Node deletion removes from scene + undo support
- Grid toggle in toolbar
- Surface error on minimize handled
- Camera dropdown works with dynamically added cameras

**Post-M20 Features:**
- Infinite ground grid (1m minor, 10m major, axis colors, depth-correct)
- Ivar render matches viewport aspect ratio (no squeeze)
- Camera primitives appear in camera dropdown, sync viewport when selected
- Scene cameras follow animation during playback

**Scene Interactivity (M20):**
- Click viewport to select instance (Embree raycast)
- Orange highlight on selected instance
- Translate gizmo with colored X/Y/Z axes
- Editable TRS in property inspector
- Undo/Redo (Ctrl+Z / Ctrl+Shift+Z)
- Set keyframes (K key), diamond markers on timeline
- Procedural cube/sphere/camera in node graph
- Orthographic views (Top/Front/Right/etc.)
- Export transform edits as USD sublayer

### Known Issues

- OIIO `load_texture_with_mips` crashes on .tx files on Windows
- bif_core tests need USD DLLs (run via `setup_usd_env.ps1`)
- Gizmo only supports translate (rotate/scale future work)
- Renderer struct ~60 fields (God object) - extract sub-structs in future session
- Undo for create/delete primitive pushes SceneOps but doesn't yet update node_proto_map

---

## Next Session

**Goal:** M21 Point Instancing + Scattering

1. UsdGeomPointInstancer support (already loads via C++ bridge)
2. Scatter points on surface (random/Poisson disk)
3. Paint points tool (brush-based placement)
4. Per-point attributes (scale, rotation, ID)

---

## Quick Commands

```bash
# Build
cargo build                    # Dev (~10s)
cargo build --features oiio    # With OIIO support

# Test
cargo test -p bif_math         # 41 tests
cargo test -p bif_renderer     # Embree tests

# Run
cargo run -p bif_viewer                          # Without OIIO
cargo run -p bif_viewer --features oiio          # With OIIO

# USD environment (required for USDC)
. .\setup_usd_env.ps1
```

---

**Branch:** main
**Ready for:** M21 Point Instancing + Scattering
