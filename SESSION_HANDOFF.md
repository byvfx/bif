# Session Handoff - February 15, 2026

**Last Updated:** M21.1 Point Instancer node complete
**Next Milestone:** M29 USD Export + Non-Destructive Layers
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-21, M21.1 Point Instancer node |
| Current | Planning next milestone |
| Tests | 68 viewport, 74 bif_core passing |
| Performance | 60 FPS viewport, 100K instances with LOD |

---

## Recent Work

### M21.1: Point Instancer Node (Feb 15, 2026)

First node that resolves snarl connections for actual data flow:

| Component | Details |
|-----------|---------|
| PointInstancer variant | 2 inputs (points + proto), 1 output (scene) |
| Connection resolver | `resolve_input_connection()` reads snarl wiring |
| Instance/Re-instance | Buttons appear when both inputs connected |
| Instancer storage | `HashMap<NodeId, Vec<Instance>>` on Renderer, separate from scene |
| GPU integration | `reload_working_scene()` appends instancer instances |
| Cleanup | DeleteNode removes instancer results + reloads |
| Buffer fix | Culling manager clamps writes to `max_instances` capacity |

**Workflow:** Cube → Scatter Points (Grid 5x5) → Point Instancer → Instance → 25 cubes

### Code Review Fixes (Feb 13, 2026)

Fixed all 8 issues from VFX code review (overflow, cloud ID, params struct, dead field, rename).

### Scatter Points Redesign (Feb 13, 2026)

Points-only output, grid/sphere sources, Lloyd relaxation.

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 200+ passing |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Point Instancer (M21.1 — latest):**
- Separate node takes points + prototype mesh → expands into renderable instances
- First use of snarl connections for data flow (not just decorative)
- Re-instance after scatter regenerate works
- Delete instancer → instances disappear

**Scatter Points:**
- Points-only output — feeds into Point Instancer
- Surface / Grid / Sphere point sources
- Lloyd relaxation with surface projection

### Known Issues

- Instancer does not auto-reinstance on upstream scatter change (manual Re-instance needed)
- Multi-prototype instancing not yet supported (single proto per instancer)
- Ivar rendering does not include instancer instances (viewport multi-draw only)
- OIIO `load_texture_with_mips` crashes on .tx files on Windows
- bif_core tests need USD DLLs (run via `setup_usd_env.ps1`)
- Renderer struct ~60 fields (God object)

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
