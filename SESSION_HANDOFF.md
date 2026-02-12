# Session Handoff - February 11, 2026

**Last Updated:** 3 bug fixes + progressive Ivar viewport
**Next Milestone:** M21 Point Instancing + Scattering
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-20 + post-M20 polish + bug fixes |
| Current | M21 Point Instancing + Scattering |
| Tests | 200+ passing (all crates) |
| Performance | 60 FPS viewport, 10K instances with LOD |

---

## Recent Work

### Bug Fixes + Progressive Ivar (Feb 11, 2026)

3 bugs + 1 feature:

| Item | Fix | Key Files |
|------|-----|-----------|
| Cube normals | CW winding fix (was CCW, pipeline expects CW) | `primitives.rs` |
| HDR resolution | Auto-downscale >8192px with bilinear resampling | `hdr.rs`, `environment_manager.rs` |
| Transform→Ivar | Use `current_transforms` not `instance_transforms`, invalidate on edit | `ivar_build.rs`, `lib.rs`, `render.rs` |
| Progressive Ivar | 1 SPP/pass accumulation, camera/transform reset, UI progress | `ivar_state.rs`, `ivar_build.rs`, `render.rs` |

**Progressive Ivar architecture:**
- `start_progressive_pass()` renders 1 SPP, sends `PassComplete` on finish
- `poll_ivar_messages()` accumulates into running sum, divides by pass count for display
- Main loop auto-starts next pass when idle, until `target_spp` reached
- Camera/transform changes call `reset_accumulation()` → instant restart
- UI shows `accumulated_samples/target_spp` progress bar + target slider

### Bug Fix Session (Feb 10, 2026)

5-phase bug fix from BUGLIST.md:

| Phase | Fix | Key Files |
|-------|-----|-----------|
| 1 | Surface error on minimize: handle Outdated + skip 0x0 frames | `main.rs` |
| 2 | Grid visibility toggle checkbox in top toolbar | `lib.rs`, `render.rs` |
| 3 | Node deletion: right-click menu + click-to-select + keyboard delete | `node_graph.rs`, `render.rs` |
| 4 | Multi-object persistence: working_scene, reload_working_scene, undo | 7 files |
| 5 | Camera dropdown fix (solved by Phase 4 persistence) | verified in `render.rs` |

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 200+ passing |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Progressive Ivar (this session):**
- Switch to Ivar → 1 SPP fast preview → progressively refines
- Camera move after render completes → resets and re-renders
- Transform edits in property inspector → Ivar re-renders with new position
- SPP progress bar + target slider (1-256)

**Bug Fixes (this session):**
- Cube normals face outward correctly
- HDR >8192px auto-downscaled with warning
- Transform edits update Ivar render

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
