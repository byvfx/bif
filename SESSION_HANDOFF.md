# Session Handoff - February 22, 2026

**Last Updated:** Xform node, multi-USD material fix, display flag gating
**Next Milestone:** M29 validation + Ivar Xform support + M26 Denoising
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23 |
| Current | M29 USD Export — Xform node + multi-USD + display flag done |
| Tests | 68 viewport, 91 bif_core (6 export), 253+ total passing |
| Performance | 60 FPS viewport, 100K instances with LOD |

---

## Recent Work

### Xform Node + Multi-USD Material Fix (Feb 22, 2026)

| Feature | Details |
|---------|---------|
| Xform node | `SceneNode::Xform` with T/R/S, node body UI + property inspector panel (colored X/Y/Z DragValues) |
| Transform apply | BFS upstream walk finds affected prototypes, post-multiplies T/R/S matrix in `reload_working_scene` |
| Multi-USD materials | `Material.source_dir` tracks originating USD dir; `resolve_texture_path` per-material; gpu_textures rebuilt from accumulated working_scene |
| node_proto_ids | `HashMap<NodeId, Vec<usize>>` for multi-proto UsdRead nodes; cleanup on delete + re-index |
| Display flag gating | `collect_upstream_nodes` BFS determines active subgraph; hidden protos excluded from viewport + Ivar |
| Toggle display | Context menu shows Set/Clear Display; Xform is display-flag-eligible |
| C++ bridge | TF_WARN diagnostics, PointInstancer array validation, checked i32 conversion |
| Deletion | Always reload after DeleteNode (handles Xform/display changes properly) |

### M29 USD Export Pipeline (Feb 21, 2026)

| Phase | Details |
|-------|---------|
| Prim paths | `Instance.prim_path` tracks real USD paths; `/BIF/` convention for BIF-created |
| C++ bridge | 4 new APIs: sublayer, reference, default prim, PointInstancer write |
| Export core | `export_scene()` in `bif_core::usd::export` — GUI-agnostic, writes xforms + keyframes + instancers |
| UsdExport node | Sink node with path/sublayer/root config, Browse button, status display |

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 253+ passing |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Xform Node:**
- T/R/S in node body + property inspector (click node to select)
- Upstream BFS walks to find affected prototypes
- Chaining: UsdRead → Xform1 → Xform2 applies transforms in order
- Delete Xform → scene rebuilds without its transforms
- Display flag properly gates Xform contributions

**Multi-USD Materials:**
- Loading 2+ USD files preserves all materials
- Per-material `source_dir` resolves relative texture paths correctly
- `gpu_textures` rebuilt from accumulated `working_scene` on each reload

**USD Export (M29):**
- C++ bridge: sublayer composition, reference, default prim, PointInstancer write
- `export_scene()` writes xform overrides, keyframed transforms, point clouds
- Display flag (blue dot) for gating which node feeds viewport/export

### Known Issues

- **Ivar doesn't reflect Xform transforms** — baked mesh_data path doesn't include Xform modifications
- Xform prim_filter is V1 placeholder (always all upstream)
- OIIO `load_texture_with_mips` crashes on .tx files on Windows
- bif_core tests need USD DLLs (run via `setup_usd_env.ps1`)
- Renderer struct ~60 fields (God object)
- Export not yet validated in Houdini/usdview/Maya

---

## Next Session

**Goal:** Fix Ivar Xform support, validate M29 export, consider M26 Denoising

1. Ivar: apply Xform transforms to baked mesh_data (multi-proto path)
2. Export a scene from BIF → open in usdview or Houdini → verify composition
3. Xform prim_filter — implement glob matching against instance prim paths
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
**Ready for:** Ivar Xform fix, M29 export validation, M26 Denoising
