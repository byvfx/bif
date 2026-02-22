# Session Handoff - February 21, 2026

**Last Updated:** M29 USD Export pipeline implemented (7 phases complete)
**Next Milestone:** M29 validation + M26 Denoising
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23 |
| Current | M29 USD Export — core pipeline done, needs real-world validation |
| Tests | 68 viewport, 91 bif_core (6 new export), 247+ total passing |
| Performance | 60 FPS viewport, 100K instances with LOD |

---

## Recent Work

### M29 USD Export Pipeline (Feb 21, 2026)

| Phase | Details |
|-------|---------|
| Prim paths | `Instance.prim_path` tracks real USD paths; `/BIF/` convention for BIF-created |
| C++ bridge | 4 new APIs: sublayer, reference, default prim, PointInstancer write |
| Export core | `export_scene()` in `bif_core::usd::export` — GUI-agnostic, writes xforms + keyframes + instancers |
| Display flag | `display_node` on NodeGraphState, blue dot indicator (Houdini-style) |
| UsdExport node | Sink node with path/sublayer/root config, Browse button, status display |
| write_xform fix | OverridePrim → DefinePrim(Xform) fallback for empty stages |
| Tests | 6 round-trip tests: valid file, xform, keyframes, instancer, sublayer, create+save |

### Lock-free Radiance Cache (Feb 21, 2026)

| Finding | Details |
|---------|---------|
| Change | Replaced deferred writes + sharded RwLock with lock-free `AtomicU32::from_ptr` per field |
| Design | CAS on `sample_count` guards writes; atomic loads for reads; zero locks in hot path |
| Tests | 18 total (3 new: lock-free lookup, concurrent stress, CAS contention) |

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 253+ passing |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**USD Export (M29):**
- C++ bridge: sublayer composition, reference, default prim, PointInstancer write
- `export_scene()` writes xform overrides, keyframed transforms, point clouds as USD layers
- Instance prim paths track real USD paths from loaded files
- UsdExport sink node in node graph with Browse, sublayer toggle, export root config
- Display flag (blue dot) for gating which node feeds viewport/export
- 6 round-trip validation tests passing

**SHARC Radiance Cache (M23):**
- Lock-free atomics for read/write, CAS-based updates
- Cache heatmap AOV, egui controls, live stats

### Known Issues

- Multi-prototype instancing not yet supported (single proto per instancer)
- OIIO `load_texture_with_mips` crashes on .tx files on Windows
- bif_core tests need USD DLLs (run via `setup_usd_env.ps1`)
- Renderer struct ~60 fields (God object)
- Export not yet validated in Houdini/usdview/Maya
- Display flag doesn't yet gate rendering (visual indicator only)

---

## Next Session

**Goal:** Validate M29 export in external tools, then M26 Denoising

1. Export a scene from BIF → open in usdview or Houdini → verify composition
2. Test sublayer workflow: load USD → edit transforms → export → verify in Houdini
3. Consider M26 Denoising (Intel OIDN) or display flag rendering gating

---

## Quick Commands

```bash
# Build
cargo build                    # Dev (~10s)
cargo build --features oiio    # With OIIO support

# Test
cargo test -p bif_math         # 41 tests
cargo test -p bif_renderer     # Renderer + radiance cache tests

# USD tests (require setup)
. .\setup_usd_env.ps1
cargo test -p bif_core -- --test-threads=1

# Run
cargo run -p bif_viewer                          # Without OIIO
cargo run -p bif_viewer --features oiio          # With OIIO
```

---

**Branch:** main
**Ready for:** M29 export validation in external tools, then M26 Denoising
