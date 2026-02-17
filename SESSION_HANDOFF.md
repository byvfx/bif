# Session Handoff - February 15, 2026

**Last Updated:** M21.2 Auto-Compute + Code Review Fixes complete
**Next Milestone:** M29 USD Export + Non-Destructive Layers
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-21.2 (auto-compute, prototype hiding) |
| Current | Planning next milestone |
| Tests | 68 viewport, 75 bif_core passing |
| Performance | 60 FPS viewport, 100K instances with LOD |

---

## Recent Work

### M21.2: Auto-Compute + Code Review Fixes (Feb 15, 2026)

Houdini-style auto-cooking + 14 code review fixes:

| Component | Details |
|-----------|---------|
| Auto-compute | Nodes cook when inputs connect (no buttons) |
| Dirty propagation | connect/disconnect/delete marks downstream dirty |
| Scatter→Instancer | Scatter recompute triggers instancer recompute |
| Proto hiding | Source geometry hidden when consumed by instancer |
| BTreeMap | Deterministic instancer iteration |
| expand_with_prototype() | Avoids clone+modify pattern |
| Parallel arrays | prim_paths + animations fixed for instancer instances |
| Ivar | Instancer instances baked into combined mesh_data |
| Culling | Truncation warning when visible > buffer capacity |

**Workflow:** Cube → Scatter Points (Grid) → Point Instancer → auto-computes → cube hidden, instances shown

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
