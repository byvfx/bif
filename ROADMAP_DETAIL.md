# BIF Roadmap — Detailed Breakdown

Per-version task lists, acceptance criteria, and technical notes.
For the high-level roadmap, see [MILESTONES.md](MILESTONES.md). For completed milestone history, see [MILESTONES_HISTORY.md](MILESTONES_HISTORY.md).

---

## v0.13.0 — Pipeline Foundation

**Status:** Released (2026-04-09)
**Milestones:** M29.5 (UI overhaul), M30 (persistence + eval modes), M31 (per-node viz)
**Last egui feature release.** See [CHANGELOG.md](CHANGELOG.md) for full release notes.

### Acceptance Criteria

- .bif/.bifa save/load round-trips all 10 node types
- Auto/Manual/OnMouseRelease eval modes functional
- Cache node with bypass toggle
- Per-node scene graph filtering in browser
- Centralized theme system applied throughout UI

---

## v0.13.6 — UsdSkel Blend Shapes

**Status:** Implementation complete, pending validation + release
**Estimate:** 8-12h
**Dependencies:** v0.13.5 (UsdSkel skinning)

### Tasks

- [x] C++ FFI: `UsdBridgeBlendShapeTarget/BindingData` structs, 3 extern functions, dense expand at load, shape-order remap, `UsdSkelAnimQuery` caching
- [x] Rust FFI: raw structs, safe wrappers (`blend_shape_binding_count`, `get_blend_shape_binding`, `compute_blend_shape_weights`), `convert_blend_shape_binding`
- [x] Mesh struct: `BlendShapeTarget`, `BlendShapeBinding`, `Mesh::blend_shapes`, `Mesh::bind_normals`
- [x] `skinning::apply_blend_shapes()` — linear delta add, unclamped weights
- [x] Loader: walk bindings, attach by mesh path, snapshot bind_normals
- [x] Playback: inline + multi-draw paths compose shapes → skin
- [x] GPU stub: `GpuBlendShapeLayout` in bif_renderer
- [x] Test asset: `two_bone_arm.usda` with 2 BlendShape prims + animated weights
- [x] 6 unit tests (passthrough, single@1, two@0.5, normals, unclamped, composition)
- [ ] Manual validation on HumanFemale.walk.usd (blinks/face)
- [ ] Wiki concept note: `wiki/concepts/usdskel-blend-shapes.md`
- [ ] Version bump + release commit

### Technical Notes

- Dense expansion in C++ (zero-pad via pointIndices) for auto-vectorizable Rust loop
- Per-frame weight eval via `UsdSkelAnimQuery::ComputeBlendShapeWeights` (mirrors `ComputeJointSkelTransforms` pattern)
- Shape-order remap: mesh `skel:blendShapes` token order ≠ anim `blendShapes` order, remapped once at load
- Weights unclamped per USD spec (>1.0 exaggeration, <0 anti-shapes legal)
- Scratch buffers stashed on `SkinnedMeshEntry` to avoid per-frame allocation

### Acceptance Criteria

- Blend shapes load and deform at correct times
- Composition with skinning produces correct vertex positions (shapes first, then LBS)
- HumanFemale blinks/face shapes visible when scrubbing timeline
- No regression on skinning-only meshes (shapes-absent path unchanged)
- 6 unit tests pass

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
- **Namespace Editor:** Rename/reparent prims (writes to edit layer) — deferred to v0.20.0

### Technical Notes

- Requires new C++ bridge work for PcpPrimIndex/SdfLayerStack (different domain from current 79 FFI functions)
- Variant switching forces full re-cache via `cache_stage_data()` architecture
- Budget extra time for C++ composition API work
- M33 estimated 10-15h, M32 estimated 15-20h

### Tasks — Layer Color Coding (Proof-of-Concept)

- Auto-assign color per layer from 8-color palette (teal, purple, orange, gold, pink, blue, green, red)
- Scene tree: tiny colored dot next to each prim showing strongest opinion source
- Property inspector: colored left-border on each property row showing owning layer
- Node graph: node header tint matches target layer
- Colors stored in layer metadata, user-overridable in settings

### Tasks — Opinion Stack (Basic)

- Per-property expandable view in property inspector
- Collapsed: winning value + owning layer dot
- Expanded: full stack — all contributing layers with values, strongest highlighted
- Requires `GetPrimStack` FFI (part of M32 C++ bridge work)

### Acceptance Criteria

- Can inspect opinion source for any USD property
- Can switch variants and see scene update live
- Layer stack is browsable with opinion highlighting
- LIVRPS composition ordering is visible per attribute
- Prim metadata (kind, purpose, apiSchemas, custom data) inspectable
- Layer colors visible in scene tree and property inspector
- Opinion stack expandable per property
- Read-only — no namespace editing (deferred to v0.20.0)

---

## v0.15.0 — Qt Migration

**Status:** Released (2026-04-22)
**Milestones:** M28 (Qt 6 UI framework)
**Dependencies:** v0.14.0
**The pivot release — everything after is Qt-native.**

### Shipped Scope

- Qt 6 application shell via cxx-qt in `crates/bif_qt`
- Embedded wgpu viewport in a Qt widget; `bif_viewer` now boots `bif_qt::run()`
- Viewport-dominant dockable layout with Layer Stack, Scene Browser, Property Inspector, Timeline, Node Graph, and Render Settings
- Real USD stage load/close, layer stack wiring, scene browser parity, property inspector composition arcs/attributes, timeline playback/keyframe detection, camera orbit/pan/zoom, prim picking, and breadcrumb sync
- Three-panel selection sync and layer-color/opinion cues across the Qt shell
- Lazy `fetchMore` scene-browser population and release-validation hardening
- egui bridge deleted from the shipped viewer path; legacy egui panel code remains in-tree only for future cannibalization

### Deferred From Original Draft

- Asset browser (M28.1)
- Asset library (M28.2)
- Drag-and-drop workflows
- Wacom pressure/tilt

### Acceptance Criteria Met

- Qt replaced the egui application path on `main`
- Current browsing, inspection, and rendering flows run through the Qt shell
- Layer-aware panels work against real USD stages
- Scene tree populates lazily instead of eager full-subtree walks
- `bif_viewer` boots the shipped Qt shell
- Release validation passed in a Qt/USD-ready shell

---

## v0.16.0 — Edit Operations + Save

**Status:** In progress
**Estimate:** 30-40h
**Dependencies:** v0.15.0
**BIF becomes a real editor.**

### Tasks — Edit Operations

- `EditOperation` enum with `to_usda()` for core types (Transform, MaterialAssign, Visibility, MaterialParamOverride)
- `EditHistory` with current-state map + undo/redo (builds on existing `EditState` + `UndoStack`)
- Existing nodes (scatter, instancer) gain `to_usda()` — write to active layer continuously
- Material overrides per-instance (per-instance material binding table)

### Tasks — Save Pipeline

- Save to layer file on disk (Ctrl+S writes active layer only)
- Auto-save to `.bif_autosave_<layer>.usd`
- USDA code preview becomes **editable** (parse + validate on save)
- Shot templates: JSON-configurable presets (`~/.bif/templates/`), `BIF_TEMPLATE_DIR` env var override

### Tasks — Opinion Stack (Full Hover)

- Hover any property → tooltip shows full layer contribution stack
- All contributing layers with values, winning opinion highlighted
- Click through to jump to source layer

### Tasks — Workspace Presets

- 4 built-in presets that reconfigure panels + payload policy:
  - **Assembly**: Node graph prominent, all layers visible, LoadAll
  - **Lighting**: Viewport dominant, light properties, CameraFrustum loading
  - **Materials**: Material editor + lookdev viewport, material layer active
  - **Review**: Viewport maximized, render settings, minimal UI
- Switch via `Ctrl+1/2/3/4` or workspace tabs in top bar
- Workspace-driven payload loading: switching workspace auto-adjusts what's in memory

### Tasks — Material Parameter Sheet

- Right-panel material editor when material is selected (OpenPBR + UsdPreviewSurface)
- Collapsible sections: Base, Specular, Coat, Emission, Transmission, Subsurface (auto-expand when non-default)
- Color swatches (click → HSV picker), sliders (drag + double-click numeric), texture slots (drag from asset browser)
- Shading model dropdown: OpenPBR / UsdPreviewSurface with auto-conversion + lossy-param warning dialog
- Inline USDA preview (6 lines max, expandable) showing authored material opinions
- "Open in Graph" button → switches to Material Graph tab in bottom dock
- See [Material Editor Design](docs/ux/MATERIAL_EDITOR_DESIGN.md)

### Tasks — Lookdev Preview Orb

- Floating 192x192 sphere in viewport bottom-right corner (appears on material selection)
- Ivar path tracer: 1 SPP during parameter drag (~16ms), progressive to 64 SPP on release
- Click to cycle preview shapes: sphere (default), cube, plane, custom mesh
- Draggable edge to resize (128-384px), right-click: Pin/Pop Out/Hide
- Prebuilt unit sphere BVH, environment from scene HDRI or baked-in default studio HDRI

### Acceptance Criteria

- Make edits in BIF, save, open in usdview, verify edits compose correctly
- Undo/redo works across all edit operation types
- Auto-save recovers work after crash
- USDA panel is editable with validation feedback
- Opinion stack hover shows full layer contributions
- Workspace presets switch layout + payload policy in one click
- Material param sheet edits OpenPBR/UsdPreviewSurface with sliders, swatches, textures
- Lookdev orb renders preview sphere with <20ms feedback during drag
- Shading model conversion works with lossy-param warnings

---

## v0.17.0 — Viewport Performance

**Milestones:** M22 (viewport performance)
**Estimate:** 20-30h
**Dependencies:** v0.15.0
**Already Done:** Frustum culling, LOD system, polygon budget.

### Tasks — Vulkan/wgpu Modernization

- Upgrade to Vulkan 1.3 features:
  - Dynamic rendering (simplify render passes)
  - Buffer device address (bindless buffers)
  - Descriptor indexing (bindless textures)
  - Synchronization2 (cleaner barriers)
- Async texture streaming
- GPU-driven rendering (indirect draw calls)

### Tasks — Embree Two-Level BVH Streaming

Core architecture for rendering scenes that don't fit in memory. Exploits BIF's prototype/instance split — instance transforms are tiny (~64 bytes each), prototype geometry is loaded on demand.

- **Top-level BVH** (always in memory): instance bounding boxes + transforms only
  - 1M instances × 64 bytes = 64MB — cheap, always resident
  - Built once when entering render mode, shape never changes
- **Bottom-level BVHs** (per prototype, loaded on demand): full triangle meshes
  - Created via `rtcNewInstance` + `rtcSetGeometryInstancedScene`
  - Instance starts with just a bounding box — no geometry needed
  - On first ray hit to unloaded prototype: load mesh from USD, build BVH, attach to all instances
- **`StreamingEmbreeScene`** struct:
  - `top_scene: RTCScene` — instances only
  - `prototype_scenes: HashMap<PrototypeId, PrototypeEntry>` — one Embree scene per prototype
  - `cache: LruCache<PrototypeId, ()>` — eviction tracking
  - `memory_budget` / `memory_used` — hard limit on prototype memory
- **`PrototypeState` enum**:
  - `BoundingBox(AABB)` — viewport mode, wireframe display only
  - `Loaded { mesh, embree_geom_id }` — full geometry, ready to intersect
  - `Deferred { usd_prim_path, bounds, estimated_memory_bytes }` — knows where data lives, loads on first ray hit
- **`RenderContext`** with on-demand loading:
  - `prototype_cache: LruCache<PrototypeId, Arc<Mesh>>` — LRU eviction when budget exceeded
  - `ensure_loaded()` transitions `Deferred → Loaded` by reading from USD via C++ bridge
  - `trace_ray()` — if ray hits unloaded prototype bbox, load geometry, rebuild, re-trace
- **Eviction**: `unload_prototype()` detaches Embree child scene, reverts to bounding-box-only hits
- **Memory budget example**: 500 prototypes × 5MB avg = 2.5GB total; 4GB budget → all fit. At 10MB avg (5GB total) → ~380 in memory, 120 evict/reload. Typical frames hit ~200 prototypes (camera frustum).

### Tasks — Payload Policies

- `PayloadPolicy::CameraFrustum` — load geometry visible to camera + padding
- `PayloadPolicy::Manual` — artist manually picks what to load/unload
- Task-driven inference: suggest payloads based on active working layer
- UI for payload management in stage tree (right-click load/unload)

### Technical Notes

- Not hitting viewport limits yet — this is optimization, not features
- Reference: [howtovulkan.com](https://howtovulkan.com) — Modern Vulkan patterns
- Embree streaming is what made Clarisse revolutionary for environment work — BIF gets it from prototype/instance design + Embree's native two-level traversal
- Loading one prototype enables rendering ALL its instances (could be thousands)
- Top-level BVH never changes shape — only child scenes get populated/evicted
- No custom intersection code needed — Embree handles two-level traversal natively

### Acceptance Criteria

- Measurable FPS improvement on large scenes (>1M instances)
- GPU memory usage reduced for scenes not fully visible
- Smooth interaction at production scale
- Render scenes exceeding memory budget via LRU prototype eviction
- Prototype load-on-demand: first ray hit triggers geometry load from USD
- Payload policies functional: CameraFrustum, Manual, BoundingBoxOnly

---

## v0.18.0 — AI Integration

**Estimate:** 38-59h (5 phases)
**Dependencies:** v0.15.0 (Qt — for panel UI), bif_core stable API
**Stretch goal — ships independently as feature-gated `bif_ai` crate.**

### Architecture

- New `bif_ai` crate: depends on bif_core only, feature-gated (`--features ai`)
- Async bridge: owns tokio runtime, channel-based polling from UI loop
- Provider-agnostic: LlmProvider trait (Ollama default, OpenAI, Anthropic)
- AI produces inert data (MaterialParams, ScenePlan) — viewport executes

### Phase 1: Material Creator (10-15h)

- LlmProvider trait + Ollama/OpenAI/Anthropic implementations
- AiService async bridge (tokio runtime, mpsc channels)
- Text → MaterialParams (OpenPBR subset, 13 validated fields)
- Validation: clamp ranges, physical plausibility checks (metalness binary, IOR >= 1.0)
- egui/Qt panel: prompt input, generate, apply to scene
- New AppEvent variants: AiMaterialReady, AiError, AiProgress

### Phase 2: Provider Breadth (4-6h)

- OpenAI + Anthropic providers
- Config UI: provider selection, API key from env vars (BIF_OPENAI_API_KEY, etc.)
- Model selection per provider

### Phase 3: Scene Builder (12-18h)

- SceneAction enum: CreateNode, Connect, SetDisplayNode (uses logical temp_ids)
- ScenePlan generation from text prompt via structured JSON output
- Preview/confirm UI (mandatory — AI never auto-applies)
- Viewport translates SceneAction → NodeGraphEvent (temp_id → real NodeId)
- Single undo group for entire AI build
- Connection validation before execution

### Phase 4: ComfyUI Integration (8-12h)

- REST client: POST /upload/image, POST /prompt, GET /history/{id}
- Workflow template system (JSON files, BIF_INPUT node convention)
- Ship 2-3 templates: upscale_2x, denoise, style_transfer
- User custom workflows in ~/.bif/comfyui_workflows/
- Progress polling → AppEvent::AiComfyUiReady

### Phase 5: Polish (4-8h)

- Error UX, prompt refinement, response caching
- Preset materials as non-AI fallback
- Multi-turn scene editing (conversation history + graph state as context)

### Technical Notes

- Zero async contagion: bif_ai owns tokio runtime, exposes sync poll API
- API keys via env vars only, `#[serde(skip)]` — never serialized to disk
- Default Ollama (free, local) — best onboarding, no API key needed
- MockProvider for deterministic testing, golden-file tests for CI
- SceneAction vocabulary limited to existing 10 node types
- JSON schema in system prompt (not function calling) for provider-agnostic structured output

### Acceptance Criteria

- "brushed steel" → valid OpenPBR Material with clamped params
- "red cube next to blue sphere" → ScenePlan → preview → apply → nodes in graph
- Render → ComfyUI upscale → result displayed in viewport
- All AI features disabled cleanly without `--features ai`
- Ctrl+Z undoes entire AI scene build as one operation
- Works with Ollama (local), OpenAI, and Anthropic providers

---

## v0.19.0 — Context System

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

## v0.20.0 — Scene Authoring

**Milestones:** M37 (lights authoring), M38 (materials authoring)
**Estimate:** 30-40h (M37: 10-15h, M38: 20-25h)
**Dependencies:** v0.19.0 (context system)

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

### Tasks — Ground-Clamp Placement

- Raycast down from scatter/placed position → find surface hit (reuse Embree pick from M20)
- Snap object origin to hit point
- Orient to surface normal (align Y-up to hit normal)
- Random rotation jitter (configurable range per axis)
- Integration with `ScatterInstances` operation — ground-clamp as optional post-step
- Works with both individual prims and PointInstancer instances
- Scatter density painting (brush-based density control)
- Exclusion zones (mask regions where scatter is suppressed)

### Technical Notes

- All authoring operations must include undo commands (QUndoStack from M28)
- Shader graph built on context system architecture (M39)
- Material presets stored as .mtlx templates
- Ground-clamp reuses existing Embree scene for raycasting — no new dependencies

### Acceptance Criteria

- Can author lights and materials entirely in BIF
- Authored lights/materials export to valid USD
- Viewport shows light gizmos
- Undo/redo works for all authoring operations
- Shader graph functional in Materials context
- Scattered objects sit on terrain surfaces without floating/intersecting
- Ground-clamp works with both individual prims and PointInstancer output

---

## v0.21.0 — MaterialX Authoring

**Milestones:** M40 (MaterialX authoring)
**Estimate:** 25-30h
**Dependencies:** v0.20.0 (materials), v0.19.0 (context system)

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

## v0.22.0 — GPU Path Tracing

**Milestones:** M27 (GPU path tracing)
**Estimate:** 30-40h
**Dependencies:** v0.17.0 (viewport perf)

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

## v0.23.0 — Volumes & OpenVDB

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

## v0.24.0 — API & Integration

**Milestones:** M35 (API cleanup), M34 (PyO3 pipeline integration)
**Estimate:** 40-55h (M35: 10-15h, M34: 15-20h)
**Dependencies:** v0.20.0+ (features stabilized)

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

## v0.25.0+ — Framework Extraction

**Milestones:** M36+ (framework phase 2)
**Estimate:** 40+h
**Dependencies:** v0.24.0 (clean API)

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

## Post-1.0 Backlog — Physics Painter

**Estimate:** ~3-4 weeks
**Dependencies:** v0.20.0 (ground-clamp placement, scatter, PointInstancer export)
**Scope hard-capped:** Gravity + convex hull + freeze. No joints, constraints, friction tuning, soft body.

### Tasks

- rapier3d integration: `PhysicsPipeline`, rigid bodies, convex hull colliders via parry3d
- Convex hull from USD mesh: read vertices (or sim proxy geometry), `ConvexHull::from_points()`
- Paint interaction: viewport raycasting, spawn bodies on click/drag
- Live sim feedback: update transforms each physics step, render in viewport
- Freeze: stop sim, bake final resting transforms
- Bake to USD: PointInstancer arrays (positions/orientations/protoIndices) or individual prim xformOps
- UI controls: drop height, sim duration, reset

### Technical Notes

- rapier3d is pure Rust, bundles parry3d for collision geometry — no external deps
- Watch coordinate system (Y-up vs Z-up per stage) and scale (rapier tuned for meters, USD often cm)
- Don't step physics on render thread — fixed timestep with interpolation for display
- Trimesh colliders for ground/terrain are one-sided — verify face normals
- Sleeping thresholds: tune or force-sleep after N stable frames to avoid vibration

### Acceptance Criteria

- Paint objects that settle via gravity onto surfaces
- Supports convex hull collision from USD mesh or sim proxy
- Bake to both PointInstancer and individual xforms
- No scope creep beyond rigid body + gravity + freeze

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
