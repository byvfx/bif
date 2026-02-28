# Session Handoff - February 26, 2026

**Last Updated:** VFX review fixes — cached scene graph, enum, export dedup
**Next Milestone:** M29 validation (Houdini/usdview), Ivar Xform, M26 Denoising
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23 |
| Current | M29 export: 3 export bugs fixed, live scene graph added |
| Tests | 95 passing (68 viewport, 27+ bif_core) |
| Performance | 60 FPS viewport, 100K instances with LOD |

---

## Recent Work

### VFX Review Fixes (Feb 26, 2026 — Session 2)

| Change | Details |
|--------|---------|
| CachedSceneGraph | Pre-computed children_index, rebuild only on dirty flag (not every frame) |
| ProceduralPrimKind enum | Mesh/PointInstancer/Scope replaces optional fields |
| proto_prim_path helper | Deduplicated path resolution in export.rs |
| Pre-compute cloud paths | resolve_proto_paths called once per cloud, not twice |
| All protos in browser | Removed starts_with('/') skip — all prototypes visible |
| Unit tests | is_direct_child, kind type names, children_index |

### Export Fix + CompositeProvider (Feb 26, 2026 — Session 1)

| Change | Details |
|--------|---------|
| cloud.prototype_ids fix | PointInstancer compute now sets `cloud.prototype_ids = vec![pid]` so export resolves correct prototype |
| Remove starts_with('/') guard | Export was skipping BIF-created protos with prim_paths (e.g. `/World/Cube1`) — removed guard |
| Standalone mesh export | New loop writes all prototype meshes not already written by instancer loop |
| CompositeProvider | Merges USD stage + working_scene procedural prims into unified scene graph |
| Scene browser enrichment | Selection shows vertex/triangle/point counts + prototype refs for procedural prims |
| Empty message update | "No USD scene loaded" → "No scene content" |

### write_mesh + Prim Paths + Material Fix (Feb 24, 2026 - Session 2)

| Change | Details |
|--------|---------|
| C++ write_mesh | UsdGeomMesh authoring: points, indices, normals, UVs, extent, subdivisionScheme=none |
| Rust FFI wrapper | `UsdEditLayer::write_mesh` flattens mesh data for C++ bridge |
| Export prototype meshes | Writes actual mesh prims alongside PointInstancers (fixes dangling proto refs) |
| Prim paths on nodes | Primitive + PointInstancer get `prim_path` field, auto-increment `/World/Cube1` etc |

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 95 passing |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Scene Assembly:**
- UsdPrim node: define Scope/Xform prims with Kind + Specifier
- GraftBranches node: merge branches under destination path
- Primitive node: prim_path control, scene pass-through input
- PointInstancer node: prim_path control for export naming

**USD Export (M29):**
- C++ bridge: sublayer, reference, default prim, PointInstancer, define_prim, set_prim_kind, write_mesh
- `export_scene()` writes authored prims, xform overrides, keyframes, point clouds, prototype meshes, standalone meshes
- Standalone cube export confirmed working in usdview
- Graft prefix support, auto-increment prim paths

**Scene Browser:**
- CompositeProvider merges USD stage + procedural prims (Mesh, PointInstancer, Scope)
- Auto-generates intermediate Scope prims for path hierarchy
- Selection populates property inspector with vertex/triangle/point counts

### Known Issues

- **Ivar doesn't reflect Xform transforms** — baked mesh_data path doesn't include Xform modifications
- Xform prim_filter is V1 placeholder (always all upstream)
- OIIO `load_texture_with_mips` crashes on .tx files on Windows
- bif_core tests need USD DLLs (run via `setup_usd_env.ps1`)
- `test_should_restart_no_render` — pre-existing timing-sensitive test failure
- Renderer struct ~60 fields (God object)
- Instancer prim_path nesting issue (e.g. `/World/Cam/World/instancer1`) — default prim_path may need review

---

## Next Session

**Goal:** Continue M29 validation, fix instancer path nesting

1. Investigate instancer prim_path nesting bug (`/World/Cam/World/instancer1`)
2. Run VFX code reviewer on latest commit
3. Ivar: apply Xform transforms to baked mesh_data
4. Xform prim_filter — implement glob matching
5. Consider M26 Denoising (Intel OIDN)

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
**Ready for:** M29 instancer path fix, VFX code review, Ivar Xform fix, M26 Denoising
