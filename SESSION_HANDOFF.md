# Session Handoff - February 24, 2026

**Last Updated:** UsdPrim + GraftBranches nodes, export sublayer fix
**Next Milestone:** M29 validation (Houdini/usdview), Ivar Xform, M26 Denoising
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23 |
| Current | M29 export: UsdPrim/GraftBranches nodes + sublayer fix done |
| Tests | 68 viewport, 98 bif_core, 253+ total passing |
| Performance | 60 FPS viewport, 100K instances with LOD |

---

## Recent Work

### UsdPrim + GraftBranches + Export Fix (Feb 24, 2026)

| Change | Details |
|--------|---------|
| UsdPrim node | Define prims with path/type/kind/specifier (Scope, Xform, etc.) |
| GraftBranches node | Merge branches under a parent prim path (up to 4 inputs) |
| C++ bridge | `define_prim()` + `set_prim_kind()` via UsdModelAPI + Kind tokens |
| Export sublayer fix | `collect_export_context()` now walks upstream to find UsdRead source path |
| Auto sublayer | Export auto-enables sublayer when upstream UsdRead exists |
| Path canonicalization | `loaded_usd_path` and export paths canonicalized to absolute |
| Default path | UsdPrim defaults to "/root" (was "/World") to avoid overwriting imports |
| Diagnostic logging | Export logs sublayer config, warns when source_usd_path is None |

### VFX Code Review Fixes (Feb 22, 2026)

| Fix | Details |
|-----|---------|
| Parser safety | Eliminated `.unwrap()` after peek in 3 parser functions |
| Culling dirty flag | Skip redundant GPU writes when camera & data static |
| Arc\<str\> refactor | Core types use `Arc<str>` for names/paths |
| Async USD loading | Background thread with progress spinner |
| Stage metadata | C++ bridge for metersPerUnit, upAxis |

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 253+ passing |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Scene Assembly (New):**
- UsdPrim node: define Scope/Xform prims with Kind + Specifier
- GraftBranches node: merge branches under destination path
- Export collects upstream UsdRead → adds as sublayer automatically
- Canonical absolute paths for reliable sublayer resolution

**USD Export (M29):**
- C++ bridge: sublayer, reference, default prim, PointInstancer, define_prim, set_prim_kind
- `export_scene()` writes authored prims, xform overrides, keyframes, point clouds
- Graft prefix support for path remapping

### Known Issues

- **Ivar doesn't reflect Xform transforms** — baked mesh_data path doesn't include Xform modifications
- Xform prim_filter is V1 placeholder (always all upstream)
- OIIO `load_texture_with_mips` crashes on .tx files on Windows
- bif_core tests need USD DLLs (run via `setup_usd_env.ps1`)
- `test_should_restart_no_render` — pre-existing timing-sensitive test failure
- Renderer struct ~60 fields (God object)
- Export not yet validated in Houdini/usdview/Maya

---

## Next Session

**Goal:** Validate M29 export in external tools, fix Ivar Xform

1. Export a scene from BIF → open in usdview or Houdini → verify composition
2. Ivar: apply Xform transforms to baked mesh_data
3. Xform prim_filter — implement glob matching
4. Consider M26 Denoising (Intel OIDN)

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
**Ready for:** M29 export validation, Ivar Xform fix, M26 Denoising
