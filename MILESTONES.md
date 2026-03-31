# BIF Milestones

Roadmap organized by semantic version. Each release is testable, demoable, and gets a devlog + CHANGELOG entry.

[Milestone History](MILESTONES_HISTORY.md) | [Detailed Breakdown](ROADMAP_DETAIL.md) | [Changelog](CHANGELOG.md)

---

## Released

| Version | Theme | Date | Highlights |
|---------|-------|------|------------|
| v0.1.0 | Initial Release | 2026-03-12 | Viewport, instancing, USD C++, Embree, materials, MaterialX, animation, batch render, node graph, scatter, SHARC cache, OIDN denoising |
| v0.11.0 | Ivar Cache | 2026-03-13 | Ivar material cache + pre-warm, Embree indexed geometry |
| v0.12.0 | USD Export | 2026-03-21 | USD export pipeline, OpenPBR migration, subsystem extraction, curves/points import, UDIM atlas |

---

## Upcoming

| Version | Theme | Est. Hours | Key Milestones |
|---------|-------|-----------|----------------|
| **v0.13.0** | **Pipeline Foundation** | — | M29.5, M30, M31, subdiv, displacement *(in progress)* |
| v0.13.5 | UsdSkel Import | 20-30h | Skeleton eval, skinning, bind pose + anim playback |
| v0.14.0 | Layer-Aware Stage | 35-50h | M32, M33 + workflow Phase 1 |
| v0.15.0 | Qt Migration | 50-60h | M28 (three-panel layout target) |
| v0.16.0 | Edit Operations + Save | 30-40h | Workflow Phase 2 + templates |
| v0.17.0 | Viewport Performance | 25-35h | M22 + payload policies |
| v0.18.0 | AI Integration | 38-59h | Material creator, scene builder, ComfyUI |
| v0.19.0 | Context System | 30-40h | M39 |
| v0.20.0 | Scene Authoring + Layer Diff | 35-45h | M37, M38 + workflow Phase 7 |
| v0.21.0 | MaterialX Authoring | 25-30h | M40 |
| v0.22.0 | GPU Path Tracing | 30-40h | M27 |
| v0.23.0 | Volumes & OpenVDB | 20-30h | M25 |
| v0.24.0 | API & Integration | 40-55h | M35, M34 |
| v0.25.0+ | Framework Extraction | 40+h | M36+ |

**Total estimated:** ~418-564h remaining to 1.0

---

### v0.13.0 — Pipeline Foundation *(in progress)*

M29.5 (UI overhaul), M30 (persistence + eval modes), M31 (per-node viz), unreleased perf fixes. **Last egui feature release.** Ship current work.
- Subdivision surfaces: crease indices/lengths/sharpnesses, corner sharpnesses, interpolateBoundary, faceVaryingLinearInterpolation
- OpenSubdiv evaluation (CPU-side, limit surface tessellation for catmullClark/loop/bilinear meshes)
- Displacement mapping: UsdPreviewSurface `displacement` input → vertex displacement along normals
- **Validation**: Load subdivided mesh from Houdini, verify smooth surface matches usdview; displacement visible on dense mesh

### v0.13.5 — UsdSkel Import

Skeletal animation import + CPU skinning for rendering characters in assembled scenes.
- C++ bridge: UsdSkelCache, UsdSkelSkeletonQuery, UsdSkelSkinningQuery
- Read skeleton topology (joints, bind transforms, rest transforms)
- Read skin weights + joint indices per-vertex
- CPU linear blend skinning (LBS) at bind pose → bake to Mesh
- Animated playback: evaluate skeleton at current timeline frame, re-skin per frame
- Blend shapes (UsdSkelBlendShape): read targets + weights, apply to base mesh
- **Validation**: Load skinned character (e.g., from Houdini/Maya), see bind pose; scrub timeline, see animation

### v0.14.0 — Layer-Aware Stage

Merges workflow Phase 1 + old M32/M33. BIF starts understanding USD layers. **Last release on egui UI** — logic is UI-agnostic for Qt port.
- FFI expansion: minimal subset — `SdfLayer` read, `GetEditTarget`, `GetPrimStack`, payload load/unload
- Open USD stage → parse sublayer stack → display layer list in UI
- Select working layer → layer isolation mode (edit layer writable, others locked)
- `PayloadPolicy::LoadAll` and `PayloadPolicy::BoundingBoxOnly`
- Opinion inspector (which layer contributes which value)
- Composition arc visualization
- File watching: detect external sublayer changes, offer reload
- Node graph: add "Layer Stack" node, color-code nodes (blue=composition, orange=operations)
- Layer muting (hide layer contributions without removing)
- Layer offsets (time offset/scale on sublayers and references)
- **Layer color coding (proof-of-concept):** Auto-assign colors per layer, show as dots in scene tree + borders in property inspector
- **Opinion stack (basic):** Expandable per-property view showing which layers contribute values
- **Validation**: Open multi-layer USD from Houdini, see layer stack, toggle layers, see opinion sources

### v0.15.0 — Qt Migration

M28 (Qt 6 UI framework). **The pivot — everything after is Qt-native.** Target: viewport-dominant T-layout (see [UI Design](docs/ux/UI_DESIGN.md)).
- Port scene browser, property inspector, node graph, viewport
- **T-layout:** Viewport-dominant center, left dock (scene tree + layers), right dock (properties), tabbed bottom dock (node graph | USDA preview | render log)
- **Layer color coding (full):** Colors flow through all panels — tree dots, property borders, node badges, USDA syntax highlighting
- USDA code preview panel (read-only, syntax highlighted, shows active layer content)
- Three-panel sync: select in one → highlights in others
- Command palette (`Ctrl+P`) — fuzzy-search prims, commands, layers, node types
- Breadcrumb bar: `stage > layer (edit) > /selected/prim`
- Multi-monitor: pop-out panels via QDockWidget
- Virtualized scene tree (design for 100K+ prims)
- **"Quiet confidence" theme:** `#1c1c1c` bg, shadow gaps, single blue accent, 6px radius
- Micro-interactions: 150ms panel collapse, 100ms selection fade

### v0.16.0 — Edit Operations + Save

Workflow Phase 2. BIF becomes a real editor.
- `EditOperation` enum with `to_usda()` for core types (Transform, MaterialAssign, Visibility, MaterialParamOverride)
- `EditHistory` with undo/redo (builds on existing `EditState` + `UndoStack`)
- Save to layer file on disk (Ctrl+S writes active layer only)
- Auto-save to `.bif_autosave_<layer>.usd`
- **USDA code preview becomes editable** (parse + validate on save)
- Live USDA code preview updates as artist works
- Existing nodes (scatter, instancer) gain `to_usda()` — write to active layer continuously
- Shot templates: JSON-configurable presets (`~/.bif/templates/`), `BIF_TEMPLATE_DIR` env var override
- Material overrides per-instance (per-instance material binding table)
- **Opinion stack (full hover):** Hover any property → see full layer contribution stack
- **Workspace presets:** Assembly, Lighting, Materials, Review — reconfigure panels + payload policy
- **Validation**: Make edits in BIF, save, open in usdview, verify edits compose correctly

### v0.17.0 — Viewport Performance

M22 (Vulkan 1.3, lazy loading, GPU-driven rendering) + deferred loading from workflow doc.
- `PayloadPolicy::CameraFrustum` and `PayloadPolicy::Manual`
- `RenderContext` with on-demand prototype loading
- `PrototypeState` enum (BoundingBox / Loaded / Deferred)
- LRU cache for prototype eviction + Embree BVH integration
- Camera depth of field and lens distortion

### v0.18.0 — AI Integration

New `bif_ai` crate (feature-gated `--features ai`). Three AI-assisted workflows: material creation from text, scene building from natural language, ComfyUI render post-processing. Provider-agnostic (Ollama default, OpenAI, Anthropic). Async bridge via channels — zero async contagion. AI produces inert data, viewport executes. Ships independently across 5 phases.
- Phase 1: Material creator (text → OpenPBR params, validated)
- Phase 2: Provider breadth (OpenAI + Anthropic + config UI)
- Phase 3: Scene builder (text → SceneAction plan → preview/confirm → node graph)
- Phase 4: ComfyUI integration (render → workflow template → post-processed result)
- Phase 5: Polish (error UX, caching, multi-turn refinement)
- **Validation**: "brushed steel" → valid Material; "red cube next to blue sphere" → node graph; render → ComfyUI upscale

### v0.19.0 — Context System

M39 (Assembly/Materials/Animation contexts, multi-graph). Built in Qt. Highest architectural risk — touches scene_loader, render, property_inspector.

### v0.20.0 — Scene Authoring + Layer Diff

M37 (lights) + M38 (materials) + workflow Phase 7. "Create content + see what you changed."
- Layer diff panel: semantic diff of edit layer vs composed base
- Point edit mode with soft-select (vertex nudging, `points` override)
- `AnimKey` operation for simple keyframe overrides (`timeSamples` output)
- `ScatterInstances` operation integrated into edit layer authoring
- Ground-clamp placement: raycast down → snap to surface, orient to surface normal, jitter/randomize rotation
- Scatter density painting + exclusion zones
- New operation nodes: Material Override, Anim Key, Point Edit
- Light linking (UsdLuxLightListAPI — control which geometry a light affects)
- Color temperature (Kelvin → RGB conversion for lights)
- Shadow control per-light (UsdLuxShadowAPI — enable, color, distance, falloff)
- Portal lights (DomeLight portals for interior scenes)

### v0.21.0 — MaterialX Authoring

M40 (standard_surface graph, XML round-trip, node previews). Built on context system in Materials context.
- Custom shader networks (arbitrary UsdShade graphs beyond UsdPreviewSurface/OpenPBR)

### v0.22.0 — GPU Path Tracing

M27 (wgpu compute, BVH on GPU, ReSTIR). Fast material preview for authoring workflows.

### v0.23.0 — Volumes & OpenVDB

M25 (fog, smoke, clouds, VDB support). Fills the biggest production content gap.

### v0.24.0 — API & Integration

M35 (API cleanup) then M34 (PyO3 pipeline integration). "Embed BIF in studio pipelines."

### v0.25.0+ — Framework Extraction

M36+ (widget crates, plugin system, DCC connectors). "Reusable VFX framework crates."

---

## Backlog (Low Priority / Unversioned)

- UsdGeomNurbsPatch, UsdGeomNurbsCurves import
- Intrinsic geometry USD read (Capsule, Cone, Cylinder — currently procedural-only)
- Texture animation (UV offset keyframes)
- Velocity-based motion blur (UsdGeomMotionAPI)
- Native instancing read (instanceable prims — currently only PointInstancer)
- Physics painter (post-1.0): rapier3d rigid-body settle for organic object piling (gravity + convex hull + freeze, no constraints/friction)

---

## Pre-1.0 Hardening

- Error recovery / auto-save
- User-facing error notifications (toast system)
- Undo hardening for all authoring operations
- OCIO color management (ACES/ACEScg)
- User documentation

## 1.0 Criteria

1. Layer-aware USD editing (open stage, pick layer, edit, save clean USD)
2. Reliable USD round-trip (Houdini → BIF → export → re-import, no data loss)
3. Scene authoring without external tools (lights, materials, scatter, export)
4. Save/load with auto-save and crash recovery
5. Undo/redo for all authoring operations
6. Production batch rendering with denoising
7. GPU path tracing for interactive preview
8. Volume rendering (VDB)
9. Qt-based professional UI with three-panel layout
10. User-facing error messages
11. User documentation

---

## Principles

1. One version per release — no partial work
2. Each release must be testable and demoable
3. Each release gets a devlog + CHANGELOG entry
4. v0.15.0 (Qt) is the pivot — everything after is Qt-native
5. Undo commands required for all authoring operations
