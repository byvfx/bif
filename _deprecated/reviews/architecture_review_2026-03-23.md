# BIF Architecture Review - 2026-03-23

## Executive Summary

BIF is a ~48k LOC Rust VFX scene assembler/renderer organized as a 6-crate workspace. The architecture is **surprisingly well-structured for a side project at this stage**. The crate dependency graph is clean and acyclic, error handling uses thiserror consistently, and the recent introduction of `EventBus`, `SceneManager`, `SelectionManager`, and sub-context structs (`GpuContext`, `CameraState`, `IvarContext`, `NodeGraphContext`) shows intentional decomposition of the former monolith. The "God object" Renderer in `bif_viewport` is still large but has already been partially tamed.

Key risks for upcoming milestones center on: (1) the `render.rs` `run_egui_frame` method directly mutating `self` inside egui closures, which will make the Qt migration harder than expected, and (2) the node graph's coupling to `egui_snarl` data structures (`NodeId`, `Snarl<SceneNode>`) that leak into non-UI code paths.

**Overall rating:** Solid foundation for M29-M32. One critical item and several important items should be addressed before M33+.

---

## 1. Crate Boundary Analysis

### Dependency Graph

```text
bif_math          (leaf - no internal deps)
   |
bif_core          (depends on: bif_math)
   |
bif_renderer      (depends on: bif_math, bif_core)
   |
bif_viewport      (depends on: bif_math, bif_core, bif_renderer)
   |
bif_viewer        (depends on: bif_math, bif_core, bif_viewport)

bif_maketx        (depends on: bif_core[oiio])
```

**Verdict: CLEAN.** The dependency graph is strictly acyclic and directional. No circular dependencies. Each crate has a clear "layer" in the stack:

| Layer | Crate | Role |
|-------|-------|------|
| 0 | bif_math | Pure math types, zero deps |
| 1 | bif_core | Scene graph, USD, textures |
| 2 | bif_renderer | CPU path tracer |
| 3 | bif_viewport | GPU viewport, UI, app logic |
| 4 | bif_viewer | Entry point |

**One concern:** `bif_viewer` directly depends on `bif_math` and `bif_core` (for `bif_core::load_usda` and `bif_math::Vec3` in input handling). The `bif_core` dependency is justified for the CLI fallback path. The `bif_math` dependency is only used for `GizmoAxis` direction vectors in `main.rs` line ~398 -- these could be pushed into `bif_viewport` helper methods to reduce viewer's surface area, but this is low priority.

### Responsibility Separation

| Crate | Responsibilities | Verdict |
|-------|-----------------|---------|
| bif_math | Vectors, matrices, camera, frustum, AABB, ray, intervals | Clean, well-scoped |
| bif_core | Scene graph, USD bridge, textures, primitives, scatter, undo, HDR, point clouds | Slightly broad but cohesive -- all "scene data" |
| bif_renderer | Path tracing, BVH, Embree, materials, lights, denoising, EXR output, picking | Cohesive -- all "CPU rendering" |
| bif_viewport | GPU render, egui UI, node graph, scene browser, batch render, timeline, gizmo | Too broad -- this is where complexity accumulates |
| bif_viewer | Window, event loop, input handling | Appropriate |

---

## 2. God Object Analysis: `Renderer` Struct

**File:** `crates/bif_viewport/src/lib.rs`, line 318

The Renderer struct has been **partially decomposed** into sub-structs. Current field count is approximately 40 direct fields, with the heaviest state delegated to:

### Already Extracted (Good)

| Sub-struct | Location | Fields | UI-Agnostic? |
|-----------|----------|--------|-------------|
| `GpuContext` | lib.rs:268 | 4 (surface, device, queue, config) | Yes |
| `CameraState` | lib.rs:276 | 7 (camera, uniform, buffer, bind group, source, lock, usd_cam) | Yes |
| `IvarContext` | lib.rs:287 | 9 (state, texture, view, sampler, bind group, layout, pipeline, materials, tex cache) | Yes |
| `NodeGraphContext` | lib.rs:302 | 9 (graph state, proto map, cloud map, scatter map, instancer results, scene graph, dirty flags) | **No** -- uses `egui_snarl::NodeId` |
| `SceneManager` | scene_manager.rs:18 | 17 (scene, instances, animations, materials, mesh, USD stage, undo) | Yes |
| `SelectionManager` | selection.rs:10 | 5 (prim path, properties, instance index, browser state, gizmo) | Yes |
| `SceneInstances` | lib.rs:251 | 6 (transforms, current, material_ids, prototype_ids, prim_paths, purposes) | Yes |
| `AsyncChannels` | lib.rs:206 | 5 (USD load, texture, batch, cancel flag, materials) | Yes |

### Still Directly on Renderer

These ~40 fields remain on the Renderer struct:

- **GPU pipeline state** (8 fields): `pipeline`, `vertex_buffer`, `index_buffer`, `num_indices`, `instance_buffer`, `num_instances`, `depth_texture`, `depth_view`
- **Material GPU state** (7 fields): `material_uniform`, `material_buffer`, `material_bind_group_layout`, `material_bind_group`, `material_table_buffer`, `material_table_len`, `triangle_material_buffer`, `has_triangle_materials`
- **Texture GPU state** (4 fields): `gpu_textures`, `texture_sampler`, `texture_bind_group_layout`, `texture_bind_group`
- **egui state** (3 fields): `egui_ctx`, `egui_state`, `egui_renderer`
- **UI state** (5 fields): `show_ui`, `fps`, `frame_count`, `fps_update_timer`, `num_triangles`
- **Sub-systems** (10+ fields): `gnomon`, `grid`, `multi_draw`, `culling`, `environment`, `lights`, `pick_scene`, `point_preview`, `curve_preview`, `event_bus`, `mipmap_generator`
- **Display toggles** (3 fields): `show_grid`, `apply_axis_correction`, `apply_unit_scaling`

### Decomposition Strategy (IMPORTANT)

The most impactful next extraction would be a `GpuMaterialState` struct:

```rust
pub(crate) struct GpuMaterialState {
    pub uniform: MaterialUniform,
    pub buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub table_buffer: wgpu::Buffer,
    pub table_len: u32,
    pub triangle_material_buffer: wgpu::Buffer,
    pub has_triangle_materials: bool,
}
```

And similarly a `GpuTextureState`:

```rust
pub(crate) struct GpuTextureState {
    pub textures: GpuTextureSet,
    pub sampler: wgpu::Sampler,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}
```

This would reduce Renderer's direct field count from ~40 to ~25, matching the pattern already established by `GpuContext` and `IvarContext`.

**Priority: NICE-TO-HAVE now, IMPORTANT before M33+ (when more GPU state accumulates).**

---

## 3. Qt Migration Readiness

### What Is UI-Agnostic (Ready for Qt)

| Component | File | Notes |
|----------|------|-------|
| `EventBus` / `AppEvent` | app_event.rs | Explicitly designed for Qt migration. Clean typed events. |
| `SceneManager` | scene_manager.rs | No egui imports. Pure data. |
| `SelectionManager` | selection.rs | No egui imports. |
| `CameraState` | lib.rs | No egui imports. |
| `IvarState` | ivar_state.rs | No egui imports. |
| `TimelineState` | timeline.rs | No egui imports (presumed). |
| `DisplaySettings` | lib.rs | No egui imports. |
| `CullingManager` | culling_manager.rs | No egui imports. |
| `EnvironmentManager` | environment_manager.rs | No egui imports. |
| `LightsManager` | lights.rs | No egui imports. |
| All of bif_core | -- | No egui dependency at all. |
| All of bif_renderer | -- | No egui dependency at all. |

### What Is Coupled to egui (IMPORTANT)

| Component | File | Coupling Type |
|----------|------|--------------|
| `render.rs::run_egui_frame` | render.rs:135-700 | **Heavy.** 500+ lines of egui panel construction directly in `impl Renderer`. Mutates `self` fields inside egui closures. |
| `render_ui.rs` | render_ui.rs | Moderate. Uses `egui::Ui` parameter but communicates via `EventBus`. Portable pattern. |
| `scene_browser.rs::render_scene_browser` | scene_browser.rs:227 | Moderate. Takes `egui::Ui` but uses `PrimDataProvider` trait. Portable pattern. |
| `property_inspector.rs` | property_inspector.rs | Moderate. Same pattern as scene_browser -- egui for rendering, events for output. |
| `node_graph/viewer.rs` | node_graph/viewer.rs | **Heavy.** Implements `egui_snarl::SnarlViewer` trait. |
| `node_graph/mod.rs` | node_graph/mod.rs | **Heavy.** `NodeGraphState` contains `Snarl<SceneNode>`, `egui_snarl::NodeId`. |
| `NodeGraphContext` | lib.rs:302 | **Critical.** Uses `egui_snarl::NodeId` as HashMap keys throughout non-UI code. |

### Migration Risk Assessment

**CRITICAL: `NodeGraphContext` leaks egui_snarl types into business logic.**

The `node_proto_map`, `node_cloud_map`, `node_scatter_surface_map`, and `instancer_results` all use `egui_snarl::NodeId` as keys. These maps are used in scene evaluation code (`render.rs::handle_node_graph_event`, `scene_loader.rs`), not just UI rendering.

When Qt replaces egui, the node graph will need a framework-agnostic node ID type. The fix:

```rust
// In bif_viewport or a future bif_graph crate:
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct GraphNodeId(u64);

// NodeGraphContext uses GraphNodeId instead of egui_snarl::NodeId
// A mapping layer converts between GraphNodeId and framework-specific IDs
```

**Priority: IMPORTANT. Should be done before M30 (persistence), since persisted node IDs should not depend on egui_snarl's internal representation.**

**IMPORTANT: `run_egui_frame` method is 500+ lines of direct `self` mutation inside egui closures.**

The current pattern in `render.rs` lines 135-700 is:

1. Extract copies of state into local variables (to avoid borrow conflicts)
2. Run egui closures that read/mutate `self` and local variables
3. Write local variable changes back to `self`

This works but creates a tight coupling between the UI layout and the Renderer. For Qt migration, this code would need to be completely rewritten. The `StatsPanelParams` struct in `render_ui.rs` shows the right direction -- it packages state for the UI layer without exposing `self`.

**Recommendation:** Continue the `StatsPanelParams` pattern. Extract each panel's state into a params struct. The render loop becomes:

```rust
fn run_ui_frame(&mut self) {
    let stats = self.collect_stats_params();
    let property = self.collect_property_params();
    let node_graph = self.collect_node_graph_params();
    // Pass to UI layer (egui today, Qt tomorrow)
    ui_framework.render(stats, property, node_graph, &mut self.event_bus);
}
```

**Priority: IMPORTANT for Qt migration, but can be incremental. Each panel can be extracted independently.**

---

## 4. Module Cohesion Analysis

### Well-Cohesive Modules

| Module | LOC (est.) | Cohesion | Notes |
|--------|-----------|----------|-------|
| bif_math (all) | 1.8k | High | Each file is one type. Clean re-exports. |
| bif_core/scene.rs | ~800 | High | Scene, Prototype, Instance, Material -- all related. |
| bif_core/usd/cpp_bridge.rs | ~1k | High | FFI boundary, well-isolated. |
| bif_core/undo.rs | ~300 | High | Command pattern, clean trait. |
| bif_renderer/embree.rs | ~500 | High | Embree scene management. |
| bif_renderer/openpbr.rs | ~500 | High | OpenPBR surface material. |
| bif_viewport/app_event.rs | 113 | High | Pure event definitions. |
| bif_viewport/scene_manager.rs | ~90 | High | Clean data ownership. |
| bif_viewport/selection.rs | ~80 | High | Focused state management. |

### Modules That Are Too Large or Mixed

| Module | LOC (est.) | Issue | Recommendation |
|--------|-----------|-------|----------------|
| bif_viewport/lib.rs | ~1300 | Contains Renderer struct definition + constructor + 20+ utility methods + type definitions (UsdLoadStatus, SceneInstances, etc.) | Extract `UsdLoadStatus`, `SceneInstances`, `DisplaySettings`, `PurposeMode` into their own files. Keep Renderer struct + constructor in lib.rs. |
| bif_viewport/render.rs | ~900+ | `run_egui_frame` is 500+ lines, mixing UI construction with state management. | Split into `render_pipeline.rs` (GPU submission) and move UI construction to dedicated per-panel modules. |
| bif_viewport/scene_loader.rs | ~1000+ | Handles both USD loading and scene rebuilding. | Fine for now -- these are tightly coupled operations. |
| bif_viewport/node_graph/viewer.rs | ~1000+ | Single file implementing all node type UIs. | Fine for 10 node types. If it grows past 15 types, consider a `node_ui/` directory with one file per node type. |
| bif_core/usd/cpp_bridge.rs | ~1000+ | Large but necessarily so -- FFI boundary. | No change needed. Single boundary point is correct. |

---

## 5. Error Handling

### Current State

Each crate defines domain-specific error types using `thiserror`:

| Crate | Error Types | Pattern |
|-------|------------|---------|
| bif_core | `HdrError`, `OiioError`, `TextureError`, `LoadError`, `UsdBridgeError` | `thiserror::Error` derive, domain-specific variants |
| bif_renderer | `DenoiseError`, `EmbreeError`, `ExrError`, `PickError` | Same pattern |
| bif_viewport | Uses `anyhow::Result` | Top-level error handling |

**Verdict: GOOD.** The pattern is consistent:

- Library crates (bif_core, bif_renderer) use `thiserror` for typed errors
- Application crate (bif_viewport) uses `anyhow` for aggregation
- Each error type covers one subsystem

**One gap:** bif_viewport does not define its own error enum -- it uses `anyhow::Result` everywhere. This is fine for now (application-level code), but if `bif_viewport` is ever used as a library (e.g., embedded in another app), typed errors would be needed.

**Priority: NICE-TO-HAVE.**

---

## 6. State Management

### Application State Flow

```text
User Input (winit events)
    |
    v
App (bif_viewer/main.rs)
    |  -- delegates to -->
    v
Renderer (bif_viewport/lib.rs)
    |
    |-- render() called per frame
    |     |-- poll_async_work()     [Phase 1: async USD, textures, scene ops]
    |     |-- poll_environment()    [Phase 2: IBL, batch, culling]
    |     |-- run_egui_frame()      [Phase 3: UI panels -> EventBus]
    |     |-- dispatch_events()     [Phase 4: drain EventBus, execute]
    |     |-- submit_gpu_frame()    [Phase 5: GPU command submission]
    |
    |-- EventBus mediates UI -> Logic
    |-- SceneManager owns all scene data
    |-- SelectionManager owns selection
    |-- IvarContext owns path tracer state
```

**Verdict: WELL-STRUCTURED.** The 5-phase render loop is clean and predictable. The EventBus pattern decouples UI from logic effectively. State ownership is clear through the sub-struct pattern.

**One concern:** The `run_egui_frame` method (Phase 3) directly mutates `self` fields alongside emitting events. This creates two state-mutation paths: direct mutation inside egui closures AND deferred mutation via `dispatch_events`. This dual path makes it harder to reason about state transitions.

**Example from render.rs:189:**

```rust
// Direct mutation inside egui closure:
ui.checkbox(&mut self.show_grid, "Grid");

// Event-based mutation:
event_bus.emit(AppEvent::StageCorrectionsChanged);
```

The `show_grid` toggle mutates directly; the stage correction emits an event. The inconsistency is not a bug, but it means a Qt port would need to audit every closure for direct mutations.

**Recommendation:** For simple toggles (show_grid, show_ui, point_preview.visible), direct mutation is pragmatic. For anything that triggers side effects (scene reload, camera sync), events are correct. Document this convention.

**Priority: NICE-TO-HAVE (document the convention).**

---

## 7. Trait Design

### Current Traits

| Trait | Crate | Purpose | Quality |
|-------|-------|---------|---------|
| `Mat4Ext` | bif_math | Extension methods on `glam::Mat4` | Good -- extends external type cleanly |
| `UndoCommand` | bif_core | Command pattern for undo/redo | Good -- clean `execute`/`undo`/`description` |
| `PrimDataProvider` | bif_viewport | Abstraction over USD stage vs procedural prims | Good -- enables `CompositeProvider` pattern |
| `Hittable` | bif_renderer | Ray intersection interface | Good -- classic ray tracer pattern |
| `Material` | bif_renderer | Material scattering interface | Good -- supports multiple material types |
| `Light` | bif_renderer | Light sampling interface | Good -- clean `sample`/`pdf`/`emission` |

**Verdict: WELL-BALANCED.** The codebase uses traits where polymorphism is genuinely needed (materials, lights, hittables, prim data providers) and concrete types everywhere else. There is no over-abstraction.

**One area of potential under-abstraction:** There is no `Renderer` trait or `ViewportBackend` trait that would allow swapping the GPU backend. This is fine for now -- you have one backend (wgpu). But if Vulkan-direct or Metal-direct becomes relevant, a trait boundary here would help.

**Priority: Not needed until you actually have two backends. YAGNI applies.**

---

## 8. Testing Architecture

### Test Distribution

| Crate | Test Count | Strategy | Notes |
|-------|-----------|----------|-------|
| bif_math | 41 | Unit tests in each module | Clean, no external deps |
| bif_core | 27+ | Unit tests + integration (needs USD env) | `--test-threads=1` for USD bridge |
| bif_renderer | 68+ | Unit tests for materials, BVH, filtering | Good coverage |
| bif_viewport | 24 | Unit tests for state managers, events | Lower coverage, expected for GPU code |
| bif_viewer | 15 | CLI arg parsing tests | Thorough for what it tests |

**Verdict: REASONABLE for the project stage.** The test strategy is sound:

- Pure logic modules (math, materials, BVH) have high coverage
- State managers have basic tests (SceneManager, SelectionManager, EventBus)
- GPU-dependent code is harder to test and appropriately has fewer tests

### Testability Concerns

**IMPORTANT:** The Renderer struct requires a GPU context to instantiate (`Renderer::new` is async and needs a window + wgpu adapter). This makes it impossible to unit test any method on Renderer without a running GPU.

Methods like `dispatch_events`, `poll_async_work`, and the entire node graph evaluation pipeline cannot be tested in isolation because they are `impl Renderer` methods that access `self.gpu`, `self.scene`, etc.

**Recommendation for M30+:** When persistence and node evaluation become more complex, extract the node evaluation logic into a pure function:

```rust
// In a new file: node_eval.rs
pub fn evaluate_graph(
    graph: &Snarl<SceneNode>,
    scene: &mut Scene,
    // ... other pure inputs
) -> Vec<NodeGraphEvent> {
    // Pure logic, no GPU, no Renderer, fully testable
}
```

**Priority: IMPORTANT for M30 (persistence) and M31 (per-node visualization).**

---

## 9. Extensibility Assessment

### Adding a New Node Type

Current process (estimated from code):

1. Add variant to `SceneNode` enum in `node_graph/mod.rs`
2. Add UI rendering in `node_graph/viewer.rs` (`show_input` match arm)
3. Add evaluation in `scene_loader.rs` or `render.rs::handle_node_graph_event`
4. Update `input_count()`, `output_count()`, `name()` on `SceneNode`

**Verdict:** 4 files to touch is reasonable for 10 node types. The `SceneNode` enum approach is correct for this scale. If it grows past 20 types, a trait-based plugin system would be better, but that is premature now.

### Adding a New Material

1. Implement `Material` trait in bif_renderer
2. Add to material resolution in `openpbr.rs` or new file
3. GPU side: add to `MaterialGpu` in `gpu_types.rs`

**Verdict:** Clean path. The `Material` trait makes this extensible.

### Adding a New Render Pass / AOV

1. Add to `AovChannel` enum in `ivar_state.rs`
2. Add buffer to `IvarState`
3. Add extraction in `ivar_build.rs::upload_ivar_pixels`
4. Add to EXR writer in `bif_renderer/exr_writer.rs`

**Verdict:** Linear path, no architectural obstacles.

### Adding a New Exporter

1. Add to `bif_core/usd/export.rs` or new module
2. Wire up via `AppEvent` variant
3. Add UI trigger in node graph or menu

**Verdict:** Clean. The `ExportConfig` struct makes this parameterizable.

---

## 10. Findings Summary

### CRITICAL (Blocking Future Work)

| # | Finding | Files | Impact | Fix Effort |
|---|---------|-------|--------|-----------|
| C1 | `NodeGraphContext` uses `egui_snarl::NodeId` as HashMap keys in non-UI code | `lib.rs:302-315`, `render.rs`, `scene_loader.rs` | Blocks clean Qt migration. Blocks M30 (persistence) if node IDs are serialized as egui_snarl internal types. | Medium (1-2 sessions). Introduce `GraphNodeId` newtype, add bidirectional mapping. |

### IMPORTANT (Increasing Tech Debt)

| # | Finding | Files | Impact | Fix Effort |
|---|---------|-------|--------|-----------|
| I1 | `run_egui_frame` is 500+ lines mixing UI construction with state management | `render.rs:135-700` | Makes Qt migration a full rewrite of this method. Makes it hard to test UI-triggered logic. | High (3-4 sessions). Extract per-panel params structs following `StatsPanelParams` pattern. |
| I2 | No separation between node graph evaluation and node graph rendering | `node_graph/viewer.rs`, `scene_loader.rs` | Evaluation logic cannot be tested without egui. Blocks M31 (per-node visualization) where you need to evaluate subgraphs. | Medium (2-3 sessions). Extract `evaluate_graph()` pure function. |
| I3 | Renderer methods that do pure logic (undo/redo, scene ops) are not testable | `render.rs:30-108` | Growing number of untestable code paths. | Medium. Extract pure logic into functions that take `&mut SceneManager` instead of `&mut self`. |
| I4 | Dual state mutation paths (direct in egui closures + deferred via EventBus) | `render.rs:189`, `render.rs:224` | Confusing for maintenance. Qt port needs to audit all closure mutations. | Low. Document the convention; incrementally move side-effect-producing mutations to events. |

### NICE-TO-HAVE (Cleanliness)

| # | Finding | Files | Impact | Fix Effort |
|---|---------|-------|--------|-----------|
| N1 | GPU material/texture state not extracted into sub-structs | `lib.rs:328-341` | 12 fields could be 2 sub-structs. | Low (1 session). |
| N2 | `UsdLoadStatus`, `SceneInstances`, `PurposeMode`, `DisplaySettings` defined in lib.rs | `lib.rs:87-265` | lib.rs is 1300 lines with type defs + struct + constructor. | Low. Move types to dedicated files. |
| N3 | bif_viewport has no typed error enum | Entire crate | Uses `anyhow` throughout. Fine for application code. | Low. Only needed if bif_viewport becomes a library. |
| N4 | `bif_viewer` directly uses `bif_math::Vec3` for gizmo axis directions | `bif_viewer/main.rs:398` | Minor cross-crate leak. | Trivial. Move to bif_viewport helper. |

---

## 11. Recommended Action Plan

### Phase 1: Before M30 (Persistence) -- ~3-4 sessions

1. **C1: Introduce `GraphNodeId`** -- Create a framework-agnostic node ID type. This is critical for M30 because persisted node graphs need stable IDs that don't depend on egui_snarl internals. The `NodeGraphContext` maps, `instancer_results`, and `node_proto_map` should all use `GraphNodeId`.

2. **I2: Extract graph evaluation** -- Pull the node evaluation logic out of `scene_loader.rs` and viewer callbacks into a pure `evaluate_graph()` function. This makes persistence testing possible (load graph from disk, evaluate, check results) and prepares for M31.

### Phase 2: Before M33 (Qt Migration Planning) -- ~4-5 sessions

1. **I1: Per-panel state extraction** -- Continue the `StatsPanelParams` pattern for the property inspector, timeline, and node graph panels. Each panel gets a params struct built by Renderer and consumed by the UI layer.

2. **N1 + N2: Renderer cleanup** -- Extract `GpuMaterialState`, `GpuTextureState`, and move type definitions out of lib.rs.

### Phase 3: Ongoing

1. **I3: Extract testable logic** -- When touching Renderer methods, check if the core logic can be a free function taking `&mut SceneManager` or similar. Gradually increase testable surface area.

2. **I4: Document state mutation convention** -- Add a comment in render.rs explaining when direct mutation vs EventBus is appropriate.

---

## 12. Architecture Strengths (Keep Doing These)

1. **EventBus pattern** -- `app_event.rs` is exactly right. Typed events, frame-scoped drain, Qt-ready. This is the best piece of architecture in the codebase.

2. **Sub-struct extraction** -- `GpuContext`, `CameraState`, `IvarContext`, `SceneManager`, `SelectionManager` show good instincts for decomposition. Keep going.

3. **`PrimDataProvider` trait** -- The `CompositeProvider` that merges USD stage data with procedural prims via a trait is elegant and extensible. This pattern should be replicated for other cross-cutting concerns.

4. **Consistent error handling** -- `thiserror` in library crates, `anyhow` in application code. Textbook Rust error handling.

5. **Feature flags** -- `oiio` and `oidn` as optional features with graceful degradation is production-quality design.

6. **Test discipline** -- 160+ tests with clear `#[cfg(test)]` modules, AAA pattern, meaningful assertions.

7. **Clean crate boundaries** -- The 5-layer dependency graph with no cycles is better than most production Rust projects I've reviewed.

---

## Appendix A: File-Level Map

### bif_math (8 files)

```text
src/lib.rs          - Re-exports, 2 trivial tests
src/ray.rs          - Ray struct
src/interval.rs     - Interval struct
src/aabb.rs         - Axis-aligned bounding box
src/camera.rs       - Camera (orbit, pan, dolly, ortho presets)
src/transform.rs    - Mat4Ext trait
src/frustum.rs      - View frustum
src/basis.rs        - Orthonormal basis construction
```

### bif_core (17 files)

```text
src/lib.rs          - Module declarations, re-exports
src/scene.rs        - Scene, Prototype, Instance, Material, Transform, Animation
src/mesh.rs         - Mesh struct
src/point_cloud.rs  - PointCloud, DistributionMethod
src/primitives.rs   - PrimitiveKind enum + generators
src/scatter.rs      - Point scattering on surfaces
src/texture.rs      - TextureCache, Texture loading
src/hdr.rs          - HDR image loading
src/ibl.rs          - IBL preprocessing
src/undo.rs         - UndoCommand trait, UndoStack, EditState
src/oiio.rs         - OpenImageIO FFI (feature-gated)
src/usd/mod.rs      - USD module re-exports
src/usd/cpp_bridge.rs - C++ FFI (4.5k LOC)
src/usd/loader.rs   - USD to Scene conversion
src/usd/export.rs   - Scene to USD export
src/usd/types.rs    - USD-specific types (UsdMesh, XformOp, etc.)
src/usd/validate.rs - USD validation
```

### bif_renderer (21 files)

```text
src/lib.rs           - Module declarations, re-exports
src/renderer.rs      - render(), ray_color(), RenderConfig
src/camera.rs        - Render camera (separate from viewport camera)
src/bvh.rs           - BVH acceleration structure
src/embree.rs        - Embree scene (production BVH)
src/embree_ffi.rs    - Embree C FFI bindings
src/material.rs      - Material trait + basic materials
src/openpbr.rs       - OpenPBR Surface material
src/light.rs         - Light trait + DistantLight, SphereLight, RectLight
src/hittable.rs      - Hittable trait, HitRecord, HittableList
src/triangle.rs      - Triangle intersection
src/sphere.rs        - Sphere intersection
src/instanced_geometry.rs - InstancedGeometry
src/bucket.rs        - Bucket rendering (tiled)
src/blue_noise.rs    - Blue noise sampling
src/filter.rs        - Pixel reconstruction filters
src/radiance_cache.rs - SHARC radiance cache
src/hdri.rs          - HDRI environment loading
src/denoise.rs       - OIDN denoising (feature-gated)
src/exr_writer.rs    - EXR output
src/pick_scene.rs    - Embree-based viewport picking
```

### bif_viewport (34 files)

```text
src/lib.rs              - Renderer struct, constructor, utility methods
src/render.rs           - render() 5-phase loop, UI frame, event dispatch
src/render_ui.rs        - Stats panel UI (egui)
src/scene_loader.rs     - USD loading, scene rebuild
src/scene_manager.rs    - SceneManager (data ownership)
src/scene_browser.rs    - PrimDataProvider trait, scene browser UI
src/selection.rs        - SelectionManager
src/app_event.rs        - EventBus, AppEvent enum
src/node_graph/mod.rs   - SceneNode enum, NodeGraphState, events
src/node_graph/viewer.rs - egui_snarl SnarlViewer impl
src/node_graph/ops.rs   - Node graph operations (dirty propagation)
src/property_inspector.rs - Property inspector UI
src/timeline.rs         - TimelineState
src/animation.rs        - Animation evaluation
src/ivar_state.rs       - Ivar render state machine
src/ivar_build.rs       - Ivar scene building + pixel upload
src/ivar_renderer.rs    - Ivar GPU resources (texture, pipeline)
src/batch_render.rs     - Batch (offline) rendering
src/gpu_types.rs        - GPU struct definitions (Vertex, InstanceData, uniforms)
src/mesh_data.rs        - MeshData (CPU-side vertex/index data)
src/multi_draw.rs       - Multi-draw indirect state
src/culling_manager.rs  - Frustum culling manager
src/frustum_culling.rs  - Frustum culling algorithm
src/texture_loader.rs   - GPU texture upload, mipmap generation
src/environment.rs      - GPU environment (IBL bind groups)
src/environment_manager.rs - Environment state management
src/compute_ibl.rs      - IBL compute shaders
src/skybox.rs           - Skybox rendering
src/lights.rs           - LightsManager (GPU light uniform)
src/gizmo.rs            - Translate gizmo
src/gnomon.rs           - Axis gnomon renderer
src/grid.rs             - Ground grid renderer
src/point_preview.rs    - Point cloud preview renderer
src/curve_preview.rs    - Curve/points preview renderer
```

### bif_viewer (2 files)

```text
src/main.rs  - App struct, winit event loop, CLI parsing
```

---

*Review conducted by analyzing all Cargo.toml files, lib.rs files, key module structures, the Renderer struct definition, event system, error types, trait definitions, and cross-crate dependencies.*
