# BIF Roadmap — Detailed Breakdown

Per-version task lists, acceptance criteria, and technical notes.
For the high-level roadmap, see [MILESTONES.md](MILESTONES.md). For completed milestone history, see [MILESTONES_HISTORY.md](MILESTONES_HISTORY.md).

---

## v0.13.0 — Pipeline Foundation

**Status:** In progress (most work complete)
**Milestones:** M29.5 (UI overhaul), M30 (persistence + eval modes), M31 (per-node viz)
**Last egui feature release.**

### Remaining Tasks

- Final validation of M29 USD export on production files
- Release packaging and version bump

### Acceptance Criteria

- .bif/.bifa save/load round-trips all 10 node types
- Auto/Manual/OnMouseRelease eval modes functional
- Cache node with bypass toggle
- Per-node scene graph filtering in browser
- Centralized theme system applied throughout UI

---

## v0.14.0 — USD Debugging

**Milestones:** M32 (composition inspector + opinion trace), M33 (debugging tools)
**Estimate:** 35-50h
**Dependencies:** v0.13.0
**Last release on egui UI.** Logic is UI-agnostic for later Qt port.

### Tasks — Composition Inspector (M32)

- **C++ Bridge Extensions:**
  - Expose `PcpPrimIndex` — composition arcs per prim
  - Expose `SdfLayerStack` — layer ordering
  - Expose opinion queries — which layer sets which attribute
- **Composition Inspector Panel:**
  - Tree view of composition arcs (references, payloads, sublayers, inherits, variants)
  - Click arc to jump to source layer/prim
- **Opinion Trace Panel:**
  - Select attribute to see full opinion stack (all layers)
  - Winning opinion highlighted, blocking reason explained
  - LIVRPS ordering visualized
- Reference: [havocado/usd-opinion-trace](https://github.com/havocado/usd-opinion-trace) (reimplement natively, don't clone)

### Tasks — USD Debugging Tools (M33)

- **Variant Set Selector:** Interactive variant switching in property inspector
- **Layer Stack Viewer:** Which layers contribute to selected prim
- **Prim Metadata Inspector:** kind, purpose, apiSchemas, custom data
- **Namespace Editor:** Rename/reparent prims (writes to edit layer) — deferred to v0.18.0

### Technical Notes

- Requires new C++ bridge work for PcpPrimIndex/SdfLayerStack (different domain from current 79 FFI functions)
- Variant switching forces full re-cache via `cache_stage_data()` architecture
- Budget extra time for C++ composition API work
- M33 estimated 10-15h, M32 estimated 15-20h

### Acceptance Criteria

- Can inspect opinion source for any USD property
- Can switch variants and see scene update live
- Layer stack is browsable with opinion highlighting
- LIVRPS composition ordering is visible per attribute
- Prim metadata (kind, purpose, apiSchemas, custom data) inspectable
- Read-only — no namespace editing (deferred to v0.18.0)

---

## v0.15.0 — Qt Migration

**Milestones:** M28 (Qt 6 UI framework), M28.1 (asset browser), M28.2 (asset library)
**Estimate:** 85-105h total (M28: 50+h, M28.1: 20-25h, M28.2: 15-20h)
**Dependencies:** v0.14.0
**The pivot release — everything after is Qt-native.**

### Tasks — Qt 6 Shell (M28)

- Qt 6 application shell via cxx-qt (C++ <-> Rust bridge)
- Embed wgpu viewport in Qt widget
- QDockWidget — true floating/docking panels
- QTreeView with model/view separation (scene browser)
- Professional node editor (QGraphicsScene-based)
- QMenuBar, QToolBar, QShortcut — standard DCC conventions
- QUndoStack integration (replace simple undo stack from M20)
- Port theme/styling to Qt stylesheets

### Tasks — Asset Browser (M28.1)

- **Format Registry:**
  - `AssetFormat` trait: `extensions()`, `icon()`, `can_thumbnail()`, `generate_thumbnail()`, `node_type()`
  - Ships with: USD (.usd/.usda/.usdc), textures (.exr/.tx/.png/.jpg), HDRI (.hdr/.exr), OBJ (.obj)
  - New formats added without touching browser code
- **Directory Scanner:**
  - Async recursive scan with background thread
  - File watcher for live updates (notify crate)
  - Respect `.bifignore` for excluding paths
- **Thumbnail Pipeline:**
  - Background thread pool for thumbnail generation
  - Thumbnail cache on disk (`.bif_thumbs/`)
  - Texture/HDRI decode via existing image crate / OIIO
  - Type icons for USD and unknown formats
- **Drag-and-Drop:**
  - Node graph: drop creates appropriate node (UsdRead, HdriEnvironment, etc.) at cursor position
  - Viewport: drop USD at raycast hit point (reuse Embree pick from M20), creates UsdRead + Xform
  - Panels: drop texture onto material slot, drop HDRI onto environment panel
- **Panel UI:** Path breadcrumb bar, grid/list toggle, sort by name/date/size/type, filter by format
- Reference: Clarisse iFX browser, Houdini file chooser, Blender asset browser

### Tasks — Asset Library (M28.2)

- **SQLite Schema (rusqlite):**
  - Assets table: path, format, tags, metadata, thumbnail_hash, created, modified
  - Tags with many-to-many junction table
  - Collections and smart collections (saved filter queries)
- **Asset Registration:** directory scan, file watcher auto-registration, bulk import with progress
- **Search:** FTS5 full-text search on name + tags, filter by format/collection/date, tag autocomplete
- **Collections:** user-created (manual grouping), smart (saved queries, auto-populate), favorites (starred)
- **UI:** toggle between file browser / library view, inline tag chips, search bar with autocomplete

### Technical Notes

- All subsystems already have UI-agnostic APIs (per project design principle)
- Largest single release — consider phasing: v0.15.0 (core shell + viewport) then v0.15.x (browser + library)
- Node graph is the highest-risk port (egui-snarl has no Qt equivalent — may need custom QGraphicsScene widget)
- Qt C++ interop through cxx-qt
- `bif_core::asset_browser` and `bif_core::asset_library` are UI-agnostic core modules

### Acceptance Criteria

- All current UI functionality works in Qt
- Dockable/undockable panels
- Keyboard shortcuts match egui version
- Viewport rendering unchanged (wgpu backend)
- .bif/.bifa project files still load
- Asset browser with thumbnails, drag-and-drop into graph/viewport/panels
- Asset library with SQLite-backed search, tags, collections

---

## v0.16.0 — Viewport Performance

**Milestones:** M22 (viewport performance)
**Estimate:** 20-30h
**Dependencies:** v0.15.0
**Already Done:** Frustum culling, LOD system, polygon budget.

### Tasks

- Upgrade to Vulkan 1.3 features:
  - Dynamic rendering (simplify render passes)
  - Buffer device address (bindless buffers)
  - Descriptor indexing (bindless textures)
  - Synchronization2 (cleaner barriers)
- Lazy geometry loading (load on demand)
- Async texture streaming
- GPU-driven rendering (indirect draw calls)

### Technical Notes

- Not hitting viewport limits yet — this is optimization, not features
- Reference: [howtovulkan.com](https://howtovulkan.com) — Modern Vulkan patterns

### Acceptance Criteria

- Measurable FPS improvement on large scenes (>1M instances)
- GPU memory usage reduced for scenes not fully visible
- Smooth interaction at production scale

---

## v0.17.0 — Context System

**Milestones:** M39 (Assembly/Materials/Animation contexts)
**Estimate:** 30-40h
**Dependencies:** v0.15.0 (Qt)
**Highest architectural risk.**

### Tasks

- Refactor `SceneNode` into context-specific enums or trait-based system
- **Assembly context** (current graph): UsdRead, Scatter, Instancer, Xform, Export
- **Materials context**: shader graph nodes from M38, expanded
- **Animation context**: timeline-focused, keyframe/expression nodes
- Context switcher UI (tabs or dropdown)
- Each context has its own `Snarl` graph (or Qt equivalent)
- Shared data flows between contexts
- Context-specific node palettes (e.g., Material nodes only in Materials context)
- NodeBehavior trait pattern for extensible node types

### Technical Notes

- RISK: Touches scene_loader.rs (~2,356 lines), render.rs (~2,490 lines), property_inspector.rs (~1,737 lines)
- Mitigation: Implement NodeBehavior trait before splitting SceneNode enum
- Built in Qt (not egui) — avoids building twice

### Acceptance Criteria

- Can switch between Assembly/Materials/Animation contexts
- Each context has its own node graph
- Existing Assembly workflow unbroken
- Materials and Animation contexts scaffolded (empty but functional)

---

## v0.18.0 — Scene Authoring

**Milestones:** M37 (lights authoring), M38 (materials authoring)
**Estimate:** 30-40h (M37: 10-15h, M38: 20-25h)
**Dependencies:** v0.17.0 (context system)

### Tasks — Lights (M37)

- Light graph nodes: DistantLight, PointLight, RectLight (output Scene pin)
- Property inspector: edit color, intensity, position, radius, angle
- Viewport light visualization (wireframe icons)
- Lights in scene browser hierarchy
- Export lights to USD
- HDRI environment node controls (rotation, intensity, preview)
- **Existing code:** `bif_renderer/src/light.rs` (DistantLight, SphereLight, RectLight + NEE/MIS)

### Tasks — Materials (M38)

- Material creation: presets (diffuse, metal, glass, emissive)
- Property inspector: PBR params (diffuse color, roughness, metallic, specular, textures)
- Texture slot assignment (file picker)
- Material assignment to prims
- Material library panel
- Basic shader graph in Materials context: simple nodes (Mix, Multiply, Texture, Output)
- Export authored materials to USD (UsdPreviewSurface)
- Namespace editor (rename/reparent prims — moved from M33)
- **Existing code:** `bif_core/src/scene.rs` Material struct, `bif_renderer/src/material.rs`

### Technical Notes

- All authoring operations must include undo commands (QUndoStack from M28)
- Shader graph built on context system architecture (M39)
- Material presets stored as .mtlx templates

### Acceptance Criteria

- Can author lights and materials entirely in BIF
- Authored lights/materials export to valid USD
- Viewport shows light gizmos
- Undo/redo works for all authoring operations
- Shader graph functional in Materials context

---

## v0.19.0 — MaterialX Authoring

**Milestones:** M40 (MaterialX authoring)
**Estimate:** 25-30h
**Dependencies:** v0.18.0 (materials), v0.17.0 (context system)

### Tasks

- Full `standard_surface` node graph (beyond M38 presets)
- MaterialX node types in graph editor: Math, Color, Texture, Noise, Normal map, etc.
- MaterialX XML export/import (round-trip)
- Connect to existing MaterialX parsing from M16 (`bif_core/src/materialx.rs`)
- Node preview thumbnails (sphere render per node using Ivar)
- Layer/blend material stacking
- Material assignment to prims via drag-and-drop

### Technical Notes

- Builds on M38 basic shader graph, expanding to full MaterialX node set
- Existing code: `bif_core/src/materialx.rs` (M16 parser), `bif_renderer/src/material.rs`

### Acceptance Criteria

- MaterialX graphs export valid .mtlx XML
- Can import existing .mtlx files and edit them
- Node previews render in graph (using Ivar)
- Round-trip: import -> edit -> export -> re-import preserves data

---

## v0.20.0 — GPU Path Tracing

**Milestones:** M27 (GPU path tracing)
**Estimate:** 30-40h
**Dependencies:** v0.16.0 (viewport perf)

### Tasks

- wgpu compute shader path tracer
- GPU BVH construction and traversal
- ReSTIR (basic reservoir sampling first)
- Spatiotemporal resampling (full ReSTIR)
- Shared memory optimizations
- Wavefront path tracing architecture
- Progressive refinement in viewport
- Fallback to CPU Ivar for unsupported features

### Technical Notes

- Positioned after authoring — there's content to preview
- Fast material preview enables better authoring workflows
- May require wgpu ray tracing extensions (experimental)
- 10-100x speedup over CPU expected

### Acceptance Criteria

- Interactive path tracing in viewport (>10 FPS on moderate scenes)
- Material preview renders in <1s
- ReSTIR improves convergence on complex lighting
- CPU Ivar remains available as fallback

---

## v0.21.0 — Volumes & OpenVDB

**Milestones:** M25 (volumes + OpenVDB)
**Estimate:** 20-30h
**Dependencies:** v0.13.0

### Tasks

- OpenVDB integration via C++ bridge
- UsdVolume support (load from USD)
- Null-scattering path integral formulation
- Equi-angular sampling for point lights in media
- Delta tracking for heterogeneous volumes
- Viewport volume preview (ray marching)
- VDB grid display in viewport (bounding box + density preview)
- Volume node type in Assembly context

### Technical Notes

- Reference: Arnold papers on participating media, null-scattering path integral (2019)

### Acceptance Criteria

- Can load and render .vdb files
- Fog/smoke volumes render in Ivar
- Viewport shows volume bounds and density preview
- USD VolumeAPI support

---

## v0.22.0 — API & Integration

**Milestones:** M35 (API cleanup), M34 (PyO3 pipeline integration)
**Estimate:** 40-55h (M35: 10-15h, M34: 15-20h)
**Dependencies:** v0.18.0+ (features stabilized)

### Tasks — API Cleanup (M35, do first)

- bif_core + bif_renderer documented as Rust libraries
- Doc comments on all public items
- Example programs (load scene, render to EXR, export USD)
- Internal-only but crates.io-ready code quality
- bif_math already clean — add examples
- Stable public API surface with semver guarantees
- Remove/hide internal-only types from public API

### Tasks — Pipeline Integration (M34)

- Embedded Python via PyO3 (static linking, ~30-50MB dist size)
- Bundle Python interpreter (like Houdini hython, Maya mayapy)
- Expose bif_core types to Python (Scene, Prim, Material, etc.)
- Hook points: pre-load, post-load, pre-export, post-export, pre-render, post-render
- Configurable template-based output paths (`$SHOW/$SHOT/$ASSET/...`)
- Metadata propagation through pipeline (custom USD attributes carried through)
- CLI mode for headless batch operations

### Technical Notes

- PyO3 + USD's embedded TfPython = dual Python interpreter conflict
- May need vcpkg USD rebuild without Python, or careful isolation
- API cleanup first — clean what you expose before binding it

### Acceptance Criteria

- `cargo doc` generates clean, complete API docs
- Example programs compile and run
- Python scripts can drive BIF headlessly
- Hook system fires at all documented points
- No TfPython conflicts at runtime

---

## v0.23.0+ — Framework Extraction

**Milestones:** M36+ (framework phase 2)
**Estimate:** 40+h
**Dependencies:** v0.22.0 (clean API)

### Tasks

- Extract `bif_node_graph` — configurable node graph widget crate
- Extract `bif_scene_browser` — scene browser widget crate
- Extract `bif_viewport_3d` — 3D viewport widget crate
- Dynamic node registry: trait objects replacing `SceneNode` enum, `.dll`/`.so` plugin loading
- Houdini Live-Link: shared USD stage over network (UsdUtilsStageCache), first DCC connector
- PyO3 bindings for bif_core/bif_renderer
- Plugin system architecture

### Prerequisites

- Renderer decomposition: God object (~30 fields remaining) -> focused sub-structs (~10-15h)

### Acceptance Criteria

- Widget crates usable outside BIF (independent Cargo.toml, published)
- At least one DCC connector prototype (Houdini TOP or HDA)
- Plugin system can load external node types

---

## Pre-1.0 Hardening

**Version TBD** (may be distributed across patch releases)

### Tasks

- Error recovery / auto-save (~5-8h) — crash recovery dialog, periodic auto-save
- User-facing error notifications (~5-10h) — toast/notification system replacing log spam
- Undo hardening — ensure every authoring operation has undo commands
- OCIO color management — ACES/ACEScg support for production use
- User documentation — getting started guide, node reference, workflow tutorials

### Acceptance Criteria (1.0 gates)

1. Reliable USD round-trip (Houdini -> BIF -> export -> re-import, no data loss)
2. Scene authoring without external tools (lights, materials, scatter, export)
3. Save/load projects with auto-save and crash recovery
4. Undo/redo for all authoring operations
5. Production batch rendering with denoising
6. GPU path tracing for interactive preview
7. Volume rendering (VDB)
8. Qt-based professional UI
9. User-facing error messages (not log spam)
10. User documentation
