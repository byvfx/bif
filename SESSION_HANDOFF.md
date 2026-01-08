# Session Handoff - January 8, 2026

**Last Updated:** Milestone 13b Complete
**Next Milestone:** 14 (Materials/MaterialX)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| ✅ Complete | Milestones 0-13b (Scene Browser + Node Graph) |
| 🎯 Next | Milestone 14 (Materials/MaterialX) |
| 📦 Tests | 60+ passing |
| 🚀 Performance | 60 FPS viewport, 28ms Embree BVH |

---

## Recent Work

### Milestone 13b: Node Graph + Dynamic USD Loading ✅ (Jan 6, 2026)

**Goal:** Nuke-style node graph for scene assembly

**Implementation:**

- egui-snarl 0.5 node graph with USD Read + Ivar Render nodes
- rfd 0.14 native file dialogs (Browse button)
- `load_usd_scene()` for dynamic USD loading from node graph
- Houdini-style table layout (Path, Type, Children, Kind, Visibility)

**Files:**

- [node_graph.rs](crates/bif_viewport/src/node_graph.rs) (NEW) - ~350 LOC
- [scene_browser.rs](crates/bif_viewport/src/scene_browser.rs) - Table redesign
- [lib.rs](crates/bif_viewport/src/lib.rs) - +load_usd_scene, +events

---

### Milestone 13a: USD Scene Browser + Property Inspector ✅ (Jan 5, 2026)

**Goal:** Gaffer-style hierarchy browser + property inspector

**Implementation:**

- 7 new prim traversal APIs in C++ bridge
- `PrimDataProvider` trait abstraction for USD data
- Scene browser with expandable tree and type icons
- Property inspector with transforms and bounding boxes

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~5s ✅ |
| Build (release) | ~2m ✅ |
| Tests | 60+ passing |
| Vulkan FPS | 60+ (VSync) |
| Embree BVH | 28ms |
| Instances | 100 (28M triangles) |

### Known Issues

- Lucy still loads when no CLI args (should start empty)

---

## Next Session: Milestone 14

**Materials (UsdPreviewSurface + MaterialX)**

- Parse UsdPreviewSurface shader nodes
- Map to Ivar materials (Lambertian, Metal, Dielectric)
- Add Disney Principled BSDF
- Basic PBR in Vulkan viewport

See [MILESTONES.md](MILESTONES.md#milestone-14-materials-usdpreviewsurface--materialx-) for details.

---

## Quick Commands

```bash
# Build
cargo build                    # Dev (~5s)
cargo build --release          # Release (~2m)

# Test
cargo test                     # All tests

# Run
cargo run -p bif_viewer                                 # Empty viewport
cargo run -p bif_viewer -- --usda assets/lucy_100.usda  # Load USD

# USD environment (required for USDC/references)
. .\setup_usd_env.ps1
```

---

## Session Start Prompt Template

```text
I'm continuing work on BIF (VFX renderer in Rust).

#file:SESSION_HANDOFF.md
#file:MILESTONES.md
#file:CLAUDE.md
#codebase

Status: Milestone 13b Complete! 🎉

✅ Milestones 0-13b done (Scene Browser + Node Graph)
✅ 60+ tests passing
✅ Dual renderers: Vulkan (60 FPS) + Ivar (Embree two-level BVH)
✅ USD C++ integration: USDC, references, scene browser, node graph

Current state:
- Node graph with USD Read + Ivar Render nodes
- Houdini-style scene browser (table layout)
- Dynamic USD loading via node graph
- Property inspector panel

Next: Milestone 14 (Materials/MaterialX)

Let's implement UsdPreviewSurface material parsing.
```

---

**Last Commit:** c7c83d4 (docs: Update SESSION_HANDOFF.md for 13a/13b completion)
**Branch:** main
**Ready for:** Milestone 14 (Materials)! 🚀
