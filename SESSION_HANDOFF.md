# Session Handoff - February 13, 2026

**Last Updated:** Scatter Points redesign complete
**Next Milestone:** M29 USD Export + Non-Destructive Layers
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-21, Scatter Points redesign + code review fixes |
| Current | Planning next milestone |
| Tests | 140 passing (74 bif_core, 66 viewport) |
| Performance | 60 FPS viewport, 100K instances with LOD |

---

## Recent Work

### Code Review Fixes (Feb 13, 2026 - Session 3)

Fixed all 8 issues from VFX code review:

| Fix | Details |
|-----|---------|
| Integer overflow | `saturating_mul` in grid point count |
| Cloud removal | `node_cloud_map` HashMap for correct node→cloud mapping |
| Duplicate IDs | Monotonic `next_cloud_id` counter |
| Perf warning | Log warn for expensive surface relaxation |
| Dead field | Removed unused `expanded_instance_count` |
| Rename | `relax_points` → `repulsion_relax` |
| Tests | 5 new `closest_point_on_triangle` tests |
| Params struct | Extracted `ScatterPointsParams` from 17-field event |

### Scatter Points Redesign (Feb 13, 2026 - Session 2)

Scatter node produces points only (no auto-instancing). Added grid/sphere sources and relaxation:

| Component | Details |
|-----------|---------|
| Points-only | Removed expand/add_instance/reload — output is point preview only |
| PointSource enum | Surface / Grid / Sphere — controls generation mode |
| Grid generator | Spacing-based, centered at origin, flat when Y=0 |
| Sphere generator | Fibonacci (surface) or rejection sampling (volume) |
| Repulsion relax | Spatial hash repulsion + surface projection snap-back |
| Node UI | Source dropdown, conditional params, max_point_limit cap |
| Auto-preview | Point preview auto-enabled on compute |

### M21: Point Instancing + Scattering (Feb 13, 2026 - Session 1)

Full implementation of point cloud system and scatter tools:

| Component | Details |
|-----------|---------|
| PointCloud type | First-class scene type with `expand()` → instances, per-point attributes |
| USD integration | PointInstancer loading creates PointCloud (preserves authoring data) |
| Scatter | Random (area-weighted CDF) + Poisson disk (spatial hash rejection) |
| Point preview | wgpu PointList pipeline, cyan dots, configurable size |
| Node graph | Scatter node with Compute/Regenerate, mode/count/seed/etc controls |
| Undo/redo | ScatterCommand + AddPointCloud/RemovePointCloud SceneOp variants |
| Instance cap | MAX_INSTANCES 10K → 100K |

### Ivar Black Flash Fix (Feb 12, 2026)

- Nearest-neighbor resample on dim change, buffer reuse, throttle helper, full resize reset

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 200+ passing |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Scatter Points (latest):**
- Points-only output (no auto-instancing) — future Point Instancer node will handle geometry
- Surface / Grid / Sphere point sources
- Lloyd relaxation with surface projection
- Auto-enable point preview on compute

**Point Instancing (M21):**
- PointCloud as first-class type with expand() for rendering
- USD PointInstancer → PointCloud round-trip
- Scatter on any mesh surface (random + Poisson disk)
- Deterministic scatter (same seed = same result)
- Point preview visualization (cyan dots, toggleable)
- 100K instance capacity (up from 10K)
- Scatter undo/redo

### Known Issues

- Paint tool (Phase 5) deferred as stretch goal
- OIIO `load_texture_with_mips` crashes on .tx files on Windows
- bif_core tests need USD DLLs (run via `setup_usd_env.ps1`)
- Gizmo only supports translate (rotate/scale future work)
- Renderer struct ~60 fields (God object) - extract sub-structs in future session

---

## Next Session

**Goal:** M29 USD Export + Non-Destructive Layers (or choose next milestone)

1. USD stage authoring via C++ bridge
2. Opinion layer over reference (Houdini-style non-destructive editing)
3. Export scattered points as PointInstancer
4. Round-trip validation (export → reimport → verify)

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
**Ready for:** M29 USD Export or next milestone choice
