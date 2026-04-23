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
| v0.13.0 | Pipeline Foundation | 2026-04-09 | M29.5 UI overhaul, M30 persistence + eval modes, M31 per-node viz, subdiv + CPU displacement, faceVarying UVs, native MaterialX displacement, DomeLight_1, SceneQuery trait, Embree feature gate |
| v0.13.5 | UsdSkel Import | 2026-04-10 | UsdSkelCache + SkeletonQuery, CPU LBS skinning module, Mesh::skin/bind_positions, per-frame anim eval FFI, multi-draw skinning path, joint-order remap, UV-seam vertex expansion, rigid-binding broadcast, SkelRoot world xform override, validated on Pixar HumanFemale |
| v0.13.6 | UsdSkel Blend Shapes + Rigid Fix | 2026-04-12 | CPU morph target deformation via `UsdSkelBlendShape` (dense-expand at load, shape-order remap, per-frame `UsdSkelAnimQuery` eval, shapes→skin composition), multi-joint rigid binding fix (`SkinKind::Rigid` gated on `element_size==1`; hair/fingernails on HumanFemale now render correctly) |
| v0.14.0 | Layer-Aware Stage | 2026-04-13 | `SdfLayer` + `GetPrimStack` + `GetPropertyStack` FFI, `SceneLayerState` on `SceneManager` (sublayer tree + mute set + `layer_for_prim` map), `LayerStackPanel` egui panel (mute checkbox + working-layer radio + isolation header + layer-color dots), composition-arc collapsing header + per-attribute winning-layer dot in property inspector, scene-browser layer color dots, `PayloadPolicy::{LoadAll, LoadNone}` stage open, 4 integration tests on a 3-layer fixture |
| v0.15.0 | Qt Migration | 2026-04-22 | Qt 6 shell via `bif_qt`, docked panel port (layer stack, scene browser, property inspector, timeline, node graph, render settings), real USD stage load + selection sync, lazy scene-browser loading, egui bridge deletion |

---

## In Progress

### v0.16.0 — Edit Operations + Save

Current active milestone. BIF moves from a Qt-native USD viewer/orchestrator into authored edit ops + save.

- `EditOperation` enum + undo/redo for authored layer changes
- Ctrl+S save path for the active layer + auto-save recovery
- Editable USDA panel with validation on save
- Material param sheet + lookdev orb
- usdview round-trip validation for authored edits
- Lower-priority Qt follow-up spillover from v0.15.0: asset browser, asset library, drag-and-drop, Wacom pressure/tilt

---

## Next Releases

| Version | Theme | Est. Hours | Key Milestones |
|---------|-------|-----------|----------------|
| v0.16.5 | Qt Polish (Graphite) | 8-12h | Obsidian Graphite styling pass, workspace chrome polish, design-token cleanup |
| v0.17.0 | Viewport Performance | 25-35h | M22 + payload policies + texture nodes in material editor |
| v0.18.0 | AI Integration | 38-59h | Material creator, scene builder, ComfyUI |
| v0.19.0 | Context System | 30-40h | M39 |
| v0.20.0 | Scene Authoring + Layer Diff | 35-45h | M37, M38 + workflow Phase 7 |
| v0.21.0 | MaterialX Authoring | 25-30h | M40 |
| v0.22.0 | GPU Path Tracing | 30-40h | M27 |
| v0.23.0 | Volumes & OpenVDB | 20-30h | M25 |
| v0.24.0 | API & Integration | 40-55h | M35, M34 |
| v0.25.0+ | Framework Extraction | 40+h | M36+ |

---

Latest release: v0.15.0 shipped 2026-04-22. Full release notes in [CHANGELOG.md](CHANGELOG.md), archived details in [MILESTONES_HISTORY.md](MILESTONES_HISTORY.md).

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

M28 shipped on 2026-04-22. **The pivot release — everything after is Qt-native.**

- Qt 6 shell via `bif_qt` + cxx-qt
- Viewport-dominant docked layout with Layer Stack, Scene Browser, Property Inspector, Timeline, Node Graph, and Render Settings
- Real USD stage load/close, layer-aware property inspector, scene-browser lazy `fetchMore`, three-panel selection sync, timeline playback/keyframe wiring
- Camera orbit/pan/zoom + prim picking routed through the live renderer
- `bif_viewer` reduced to a thin `bif_qt::run()` shim; egui bridge deleted from the shipped viewer path
- Release validation green in a Qt/USD-ready shell

Deferred from the original Qt roadmap: asset browser, asset library, drag-and-drop, Wacom pressure/tilt.

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
- **Material param sheet:** Right-panel property editor for OpenPBR/UsdPreviewSurface (sliders, swatches, texture slots, collapsible sections)
- **Lookdev orb:** Floating 192px preview sphere in viewport corner (1 SPP drag, progressive to 64 SPP)
- **Shading model dropdown:** OpenPBR / UsdPreviewSurface switch with auto-conversion + lossy-param warnings
- **Opinion stack (full hover):** Hover any property → see full layer contribution stack
- **Workspace presets:** Assembly, Lighting, Materials, Review — reconfigure panels + payload policy
- **Validation**: Make edits in BIF, save, open in usdview, verify edits compose correctly

### v0.16.5 — Qt Polish (Graphite)

Dedicated styling/polish pass after the v0.16.0 functional editor work lands. Keep behavior changes out; this milestone is for presentation, consistency, and finish.

- Apply the Obsidian Graphite / "Quiet Confidence" design system from `assets/stitch_bif_ui/obsidian_graphite/DESIGN.md`
- Polish dock chrome, toolbar spacing, status surfaces, and workspace differentiation without changing core workflows
- Consolidate Qt styling tokens and remove one-off widget styling drift introduced during the functional tranche
- Validation: visual pass against `docs/ux/UI_DESIGN.md` plus the Graphite design doc, with no regressions to v0.16.0 editing flows

### v0.17.0 — Viewport Performance

M22 (Vulkan 1.3, lazy loading, GPU-driven rendering) + deferred loading from workflow doc.

- `PayloadPolicy::CameraFrustum` and `PayloadPolicy::Manual`
- `RenderContext` with on-demand prototype loading
- `PrototypeState` enum (BoundingBox / Loaded / Deferred)
- LRU cache for prototype eviction + Embree BVH integration
- Camera depth of field and lens distortion
- **Material editor texture nodes:** UsdUVTexture, PrimvarReader, Transform2d nodes in material graph

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

M40 (standard_surface graph, XML round-trip, node previews). Built on context system in Materials context. Full node-based material editor. See [Material Editor Design](docs/ux/MATERIAL_EDITOR_DESIGN.md).

- **Full material node graph** in bottom dock tab (separate from scene graph, same framework)
- MtlX Standard Surface node + MaterialX pattern nodes (Image, Noise, Mix, Ramp, NormalMap, Math ops)
- MaterialX XML round-trip (import/export .mtlx files)
- MaterialOut node with embedded 128px preview thumbnail
- 8 material pin types (Surface, Color3f, Float, Normal3f, Float2, Token, Asset, Displacement) with industry-standard colors
- Node color coding: blue=OpenPBR, green=UsdPreview, gold=MaterialX, light blue=textures, gray=utility
- Two-mode sync: param sheet edits update graph nodes and vice versa

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
