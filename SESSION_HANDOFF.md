# Session Handoff - March 13, 2026

**Last Updated:** VFX code review fixes for M26.1 (shared helpers, FnMut, per-vertex storage, tests)
**Next Milestone:** Resume M29 USD export or next milestone
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26 (OIDN), M26.1 (Ivar material cache) |
| Current | Ivar build pipeline fully optimized: materials cached + geometry indexed |
| Tests | 84 renderer, 41 math, 24 viewport, 27+ bif_core |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~47ms (was 6.7s) |

---

## Recent Work

### Ivar Material Cache + Pre-warm (Mar 12, 2026)

Eliminated 6.7s texture loading on every Ivar scene build. Three-part unified approach:

1. **`ivar_materials` cache** — `Vec<Arc<DisneyBSDF>>` persists on Renderer, cheap Arc clone across builds
2. **Channel return** — build thread sends materials back alongside BVH for caching
3. **Pre-warm on scene load** — `prewarm_ivar_materials()` spawns background thread immediately after scene load

**Invalidation:** Only on scene reload or material edit (not camera/transform/geometry).
**Batch render:** `SceneBuilderData` carries `ivar_materials` for per-frame reuse.

**Key files:** `ivar_build.rs` (cache logic, prewarm, invalidate), `lib.rs` (fields), `batch_render.rs` (SceneBuilderData), `scene_loader.rs` (prewarm + invalidate calls), `ivar_state.rs` (channel type)

### Embree Indexed Geometry (Mar 12, 2026)

New `try_from_indexed()` / `from_indexed()` paths that use shared vertex buffers instead of per-triangle vertex arrays. Parallel hit data construction with rayon.

**Key files:** `embree.rs` (new indexed path), `pick_scene.rs` (indexed pick scene), `mesh_data.rs` (SOA extractors)

### VFX Code Review Fixes (Mar 13, 2026)

Addressed all important + suggestion issues from VFX code review:
- Shared `build_materials()` helper (was 4x duplicated)
- `SceneBuilderFn`: `Fn`+`Mutex` → `FnMut` (correct semantics)
- Per-vertex UV/normal storage with index lookup (~6x memory savings)
- Prewarm/build race guard (50ms recv_timeout)
- Index buffer OOB validation, 4 new `from_indexed()` tests

### Texture Loading Optimization (Mar 11-12, 2026)

3-tier optimization: GPU mips, async streaming, u8 direct path. 125s → ~2s.

---

## Blockers / Known Issues

- `test_should_restart_no_render` — known flaky timing test
- bif_viewport tests need USD DLLs (`setup_usd_env.ps1`)
- bif_viewer.exe locked during build if app is running

---

## Next Steps

- Verify material cache in practice: load scene → wait → switch to Ivar → check timing log
- Resume M29 USD export remaining items
- Consider M30 project save/load
