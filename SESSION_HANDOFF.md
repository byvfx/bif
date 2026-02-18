# Session Handoff - February 17, 2026

**Last Updated:** Bugfix — auto-create, proto hiding, grid/sphere scale
**Next Milestone:** M29 USD Export + Non-Destructive Layers
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-21.2 + bugfixes |
| Current | Planning next milestone |
| Tests | 68 viewport, 75+ bif_core passing |
| Performance | 60 FPS viewport, 100K instances with LOD |

---

## Recent Work

### Bugfix: Auto-Create, Proto Hiding, Scale (Feb 17, 2026)

Three related fixes in primitive/instancing pipeline:

| Fix | Details |
|-----|---------|
| Auto-create | Primitives create on node add, live size updates (no button) |
| Proto cleanup | Old prototype removed on re-creation, prevents orphaned instances at origin |
| Grid/Sphere scale | generate_grid/sphere_points now accept ScatterConfig, produce per-point scales/orientations/IDs |
| DRY | Extracted remove_and_reindex_prototype helper, used by CreatePrimitive + DeleteNode |
| Debug log | instanced_proto_ids logged for filtering verification |

### M21.2: Auto-Compute + Code Review Fixes (Feb 15, 2026)

Houdini-style auto-cooking + 14 code review fixes.

### M21.1: Point Instancer Node (Feb 15, 2026)

First node that resolves snarl connections for actual data flow.

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 200+ passing |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Auto-Compute (M21.2 — latest):**
- Nodes auto-cook when inputs connect/change (Houdini-style)
- Scatter recompute → instancer auto-recomputes
- Prototype source geometry hidden when consumed by instancer
- Delete node → downstream instancers invalidated
- Ivar renders instancer instances

**Point Instancer (M21.1):**
- 2 inputs (points + proto) → expands into renderable instances
- Snarl connection resolution for data flow

**Scatter Points:**
- Points-only output — feeds into Point Instancer
- Surface / Grid / Sphere point sources
- Lloyd relaxation with surface projection

### Known Issues

- Multi-prototype instancing not yet supported (single proto per instancer)
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
