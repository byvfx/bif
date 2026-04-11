# BIF Architecture Review

**Date:** 2026-03-15
**Closed:** 2026-04-11
**Status:** Archived — 7 of 8 prioritized items shipped, #4 (node graph extension checklist) moved to `wiki/architecture/node-graph-system.md`. See §10 Status column and section callouts for commit refs.
**Scope:** Full codebase, all 6 crates, ~35K lines of Rust + C++ bridge
**Goal:** Identify structural issues, rank by impact/effort, suggest pragmatic improvements

---

## 1. Codebase Map

```text
bif_math (760 LOC, 0 deps)         -- Pure math: Camera, AABB, Ray, Transform, Frustum
    |
bif_core (5800 LOC, depends: bif_math)  -- Scene graph, USD bridge, textures, undo
    |
bif_renderer (8250 LOC, depends: bif_math, bif_core)  -- "Ivar" CPU path tracer
    |
bif_viewport (17330 LOC, depends: bif_math, bif_core, bif_renderer) -- wgpu viewport, node graph, UI
    |
bif_viewer (730 LOC, depends: bif_math, bif_core, bif_viewport) -- winit app shell
    |
bif_maketx (standalone CLI tool)
```

**Dependency flow is clean and unidirectional.** No circular deps. Each layer only
depends on layers below it. This is the most important thing to preserve.

---

## 2. The Renderer God Object

> **Update 2026-04-11 — RESOLVED.** Sub-structs extracted in commit `7a95329`: `GpuContext`, `IvarContext`, `SceneManager`, `SelectionManager`, `NodeGraphContext`, `EnvironmentManager`, `LightsManager`, `CullingManager`, `MultiDrawState`, `EventBus`, `TimelineState`, `DisplaySettings`, `ProjectState`. Dispatch logic split across `node_dispatch.rs` / `selection_dispatch.rs` / `project_dispatch.rs` / `render_dispatch.rs` (commits `6b0440b`, `d3d8f79`). Direct pub fields collapsed from 99 → ~12 top-level. Self-borrow conflicts resolved as a side effect; `StatsPanelParams` stayed as a one-off (see §12 Q4). Section below preserved for historical context.

### Diagnosis

`bif_viewport::Renderer` is the central problem. It lives in `lib.rs` (L242-L443)
with **99 fields** and methods spread across 4 files totaling **~7500 lines**:

| File | Lines | Responsibility |
|------|-------|---------------|
| `lib.rs` | 1655 | Struct definition (99 fields), `new()`, camera, transforms, undo, export |
| `render.rs` | 2104 | Frame rendering, egui UI orchestration, node graph event handling |
| `scene_loader.rs` | 2035 | USD loading, scene data upload, mesh management |
| `render_ui.rs` | 790 | Stats panel, settings UI (uses `StatsPanelParams` -- good extraction) |

The 99 fields mix at least 7 distinct concerns:

1. **GPU plumbing** (surface, device, queue, config, pipelines, buffers, bind groups) ~25 fields
2. **Scene data** (working_scene, mesh_data, instances, materials, textures) ~20 fields
3. **Camera state** (camera, camera_uniform, camera_buffer, viewport_camera_source) ~8 fields
4. **Node graph** (node_graph_state, node_proto_map, node_cloud_map, instancer_results) ~10 fields
5. **Ivar integration** (ivar_state, ivar_pipeline, ivar_materials) ~5 fields
6. **UI state** (scene_browser_state, selected_prim_path, gizmo_state, display_settings) ~12 fields
7. **Edit system** (undo_stack, edit_state, working_scene) ~5 fields
8. **Environment/lighting** (environment, lights) ~5 fields
9. **Misc** (show_grid, apply_axis_correction, async_channels, mipmap_generator) ~9 fields

### Why it matters now

Every new feature (M30 persistence, M31 per-node viz, M32 opinion trace) will
add more fields and methods to this struct. The borrow checker already fights
back -- `StatsPanelParams` in render_ui.rs exists specifically to work around
self-borrow conflicts. This will get worse.

### Practical Decomposition (Phase 1 -- low risk, high value)

Extract sub-structs without changing the overall architecture. The Renderer
struct keeps ownership but delegates to typed containers:

```text
GpuContext { surface, device, queue, config, pipelines, bind_groups, buffers }
SceneState { working_scene, mesh_data, instances, materials, textures, scene_cameras }
CameraState { camera, camera_uniform, camera_buffer, camera_source, camera_locked }
NodeGraphContext { node_graph_state, node_proto_map, node_cloud_map, instancer_results }
EditContext { undo_stack, edit_state }
```

**Effort:** ~4-6 hours. Mechanical refactoring, no behavior changes.
**Benefit:** Self-borrow conflicts go away. Each sub-struct gets its own `impl` block.
**Risk:** Low. Internal-only change, tests keep passing.

### Practical Decomposition (Phase 2 -- medium risk, for later)

Move sub-structs into their own modules with their own `impl` blocks. The
Renderer becomes a coordinator that holds these objects and passes them to
each other as needed. This is the modular monolith pattern.

**Wait until:** M30 (persistence) forces the issue because serialization
needs clean boundaries anyway.

---

## 3. Node Graph System

### Current Design

The node graph uses `egui_snarl` with a `SceneNode` enum (10 variants, one per
node type). Each variant carries its own state as inline fields.

**What works well:**

- `SceneNodeViewer` implements `SnarlViewer<SceneNode>` cleanly
- `NodeGraphEvent` enum decouples UI from execution
- `ops.rs` has clean BFS dirty propagation (84 lines, well-factored)
- Pin types with `PinType` enum and colored connections

**Extensibility concern:** Adding a new node type requires changes in **5+ places**:

1. Add variant to `SceneNode` enum
2. Add constructor in `SceneNode` impl
3. Add match arms in `name()`, `input_count()`, `output_count()`, `input_pin()`, `output_pin()`
4. Add variant to `NodeGraphEvent` if it has custom behavior
5. Add `add_xxx()` to `NodeGraphState`
6. Handle event in `render.rs::handle_node_graph_event()` (968-line match)
7. Possibly add to `mark_node_dirty()` in `ops.rs`

This is the classic enum-based dispatch problem. It is **fine for 10 nodes** and
will become painful at ~20+.

### Recommendation

**Do nothing yet.** The enum approach is simpler to debug and understand than a
trait-based plugin system. The 10-node count is well within manageable range.

**When to reconsider:** If you hit 15+ node types, or if third-party/user-defined
nodes become a goal. At that point, consider a `NodeBehavior` trait:

```rust
trait NodeBehavior {
    fn name(&self) -> &str;
    fn inputs(&self) -> &[PinDef];
    fn outputs(&self) -> &[PinDef];
    fn show_body(&mut self, ui: &mut egui::Ui) -> Vec<NodeGraphEvent>;
    fn execute(&mut self, inputs: &[NodeData]) -> NodeData;
}
```

**Trade-off:** Trait approach gains open extensibility, loses exhaustive match
checking (compiler won't catch missing arms). Given your milestone pace, the
enum is the right call for the next 6-12 months.

---

## 4. USD/C++ Bridge

> **Update 2026-04-11 — RESOLVED.** `unsafe impl Sync for UsdStage` removed in commit `3338f26`; `UsdStage` now wraps in `Arc<Mutex<UsdStage>>` at every shared callsite (~20 callsites across 10 files). Only `unsafe impl Send for UsdStage` / `unsafe impl Send for UsdEditLayer` remain, which is sound because ownership transfer is explicit. `cpp_bridge.rs` itself split into `ffi_raw.rs` / `ffi_convert.rs` / `cpp_bridge.rs` via Phase 1 (`7ae09e5`, `5a4b890`) — see `ARCHITECTURE_REFACTORS.md`.

### Safety Analysis

`bif_core/src/usd/cpp_bridge.rs` -- 2601 lines, well-structured FFI layer.

**Good patterns observed:**

- Typed error enum `UsdBridgeError` with `thiserror` derive
- Custom result type `UsdBridgeResult<T>` used consistently
- Every FFI call wrapped in a safe Rust function that checks the error code
- `Drop` impl on `UsdStage` and `UsdEditLayer` for cleanup
- Pointer lifetime management through `raw` field pattern

**Safety concerns:**

1. **`unsafe impl Send + Sync for UsdStage`** (L1015-1016): The CLAUDE.md says
   "USD C++ bridge is not thread-safe, use `--test-threads=1`". But `Send+Sync`
   tells Rust it IS safe to share across threads. This is a **soundness hole**.
   The `UsdStage` is wrapped in `Arc<UsdStage>` in the viewport, meaning any
   thread could call methods on it concurrently.

   **Mitigation:** In practice, the current code only accesses the stage from
   the main thread (node graph events, scene loading). But this is a time bomb.

   **Fix options:**
   - (Best) Remove `Sync`, keep `Send`. Wrap in `Mutex<UsdStage>` where shared.
   - (Quick) Add a comment documenting the invariant and why it's safe today.

2. **`unsafe impl Send for UsdEditLayer`** (L2163): Same concern but less
   dangerous since edit layers are short-lived.

3. **Slice construction from raw pointers** (L1125, L1139, etc.): Each
   `std::slice::from_raw_parts` trusts the C++ side for length. This is
   standard FFI practice but worth noting -- a C++ bug here causes UB in Rust.

4. **No `cpp/` directory found**: The `build.rs` triggers CMake for
   `cpp/usd_bridge/` but `find` returned no results. The C++ source likely
   exists outside the scanned path or is in a submodule. Could not verify
   C++ side safety.

### Recommendation

Priority fix: Change `UsdStage` from `unsafe impl Sync` to a `Mutex`-wrapped
access pattern. **Effort:** 2-3 hours. This prevents a class of bugs that are
extremely hard to diagnose when they do occur.

---

## 5. Error Handling

### Current Patterns

The codebase uses **two error strategies** consistently within their domains:

| Crate | Strategy | Types |
|-------|----------|-------|
| bif_core | `thiserror` per-module | `HdrError`, `TextureError`, `UsdBridgeError`, `LoadError`, `ParseError` |
| bif_renderer | `thiserror` + manual | `DenoiseError` (manual Display), `EmbreeError`, `ExrError`, `PickError` |
| bif_viewport | `anyhow::Result` | No custom error types |
| bif_viewer | `anyhow::Result` | No custom error types |

**This is actually the right pattern.** Library crates (core, renderer) use typed
errors for programmatic handling. Application crates (viewport, viewer) use
`anyhow` for ergonomic error reporting. No changes needed.

**Minor issue:** `DenoiseError` in bif_renderer manually implements `Display`
and `Error` instead of using `thiserror`. Worth a quick cleanup but not urgent.

**Missing:** There is no unified `BifError` type that wraps all sub-errors.
This is fine for now -- `anyhow` handles the conversion at the application
boundary. Consider a unified type only if you build a public API or plugin system.

---

## 6. Trait Design

Only **6 traits** across the entire codebase:

| Trait | Crate | Purpose |
|-------|-------|---------|
| `Mat4Ext` | bif_math | Extension trait on glam::Mat4 |
| `UndoCommand` | bif_core | Command pattern for undo/redo |
| `Hittable` | bif_renderer | Ray intersection (classic pattern) |
| `Light` | bif_renderer | Light sampling interface |
| `Material` | bif_renderer | BRDF scattering interface |
| `PrimDataProvider` | bif_viewport | Scene browser data source |

**Assessment:** This is appropriate for the project's scale. The renderer traits
(`Hittable`, `Light`, `Material`) are textbook abstractions. `PrimDataProvider`
enables the `CompositeProvider` pattern cleanly.

**Missing abstractions worth considering later:**

- A `SceneDataSource` trait that unifies USD stage + procedural scene access
  for the renderer (currently tight coupling between scene_loader and
  bif_core::Scene struct)
- A `RenderOutput` trait if you add more output formats beyond EXR

---

## 7. Test Architecture

### Coverage

| Crate | Test Files | #[test] Count | Notable |
|-------|-----------|---------------|---------|
| bif_math | 8 modules | ~41 | Excellent coverage, pure functions |
| bif_core | 14 modules | ~95 | Good, but USD tests need env setup |
| bif_renderer | 17 modules | ~68 | Good, radiance_cache has 18 tests |
| bif_viewport | 13 modules | ~24 | Low -- most UI/GPU code untested |
| bif_viewer | 0 | 0 | Expected for app shell |

**Key gap:** `bif_viewport` has 17K LOC but only 24 tests. The tested modules
are the data-oriented ones (mesh_data, gpu_types, frustum_culling, gizmo,
node_graph). The untested modules are:

- `render.rs` (2104 LOC, 0 tests) -- the main frame loop
- `scene_loader.rs` (2035 LOC, 0 tests) -- all scene loading
- `texture_loader.rs` (1222 LOC, 0 tests) -- texture pipeline
- `ivar_build.rs` (1126 LOC, 0 tests) -- Ivar scene construction
- `compute_ibl.rs` (564 LOC, 0 tests) -- IBL computation

These are hard to test because they require GPU context. Two approaches:

1. **Extract pure logic** from GPU-dependent functions. The math/data transforms
   in scene_loader.rs and ivar_build.rs could be tested without GPU.
2. **Integration test with headless wgpu** -- possible but higher effort.

**Recommendation:** When touching scene_loader.rs or ivar_build.rs, extract the
pure data transformation functions and add tests. Do not attempt to backfill
all at once.

---

## 8. Performance Architecture

### Current Design

- **Frustum culling** -- dedicated `CullingManager` with LOD support. Good.
- **Multi-draw** -- `MultiDrawState` batches GPU draw calls. Good.
- **Embree** -- used for both Ivar rendering and viewport picking. Good.
- **Rayon** -- parallel bucket rendering in Ivar. Good.
- **Texture caching** -- `TextureCache` in bif_core with mip chain support. Good.

### Potential Bottlenecks

1. **Scene loading is synchronous-then-async hybrid.** `load_usd_scene()` blocks
   on the C++ bridge call, then textures load async. For large USD stages, the
   initial block could freeze the UI. The async path via `load_usd_scene_async()`
   exists but only the USD open happens on a thread -- mesh processing still
   blocks main thread in `finalize_usd_scene()`.

2. **Single working_scene copy.** The `working_scene: bif_core::Scene` in
   Renderer is the single source of truth, mutated in place. This prevents
   concurrent read access during rendering. Not a problem today since Ivar
   copies what it needs, but worth noting for future multi-threaded viewport.

3. **Node graph evaluation is event-driven, not dataflow.** Events trigger
   specific recomputations, but there is no general "evaluate graph" pass.
   This works for 10 nodes but could miss dependency edges as complexity grows.
   The `propagate_dirty` BFS in ops.rs is the right foundation -- it just
   needs a corresponding "evaluate dirty nodes" pass.

---

## 9. Inter-Crate Boundary Issues

> **Update 2026-04-11 — RESOLVED.** `SceneQuery` trait added in commit `d46f6ae` (`crates/bif_core/src/scene_query.rs` — `trait SceneQuery` + `impl SceneQuery for Scene`, 7 consumer callsites), already in use for the MaterialX displacement viewport migration (commit `d1a8599`). `bif_math` re-exports removed from `bif_renderer` in commit `7a95329` — downstream crates now import `Vec3` / `Aabb` / `Interval` directly from `bif_math`.

### bif_viewport depends on too many bif_core internals

The viewport directly accesses `bif_core::Scene` fields, `UsdStage` methods,
`Material` fields, `Instance` fields, etc. This is currently fine because
bif_core IS the domain model. But if bif_core changes its Scene representation
(e.g., for opinion trace in M32), it will ripple through viewport extensively.

**Mitigation for M32:** Define a `SceneQuery` API in bif_core that hides the
internal representation. The viewport calls query methods instead of reaching
into struct fields.

### bif_renderer re-exports bif_math types

`bif_renderer/lib.rs` re-exports `pub use bif_math::{Aabb, Interval, Vec3}`.
This creates a situation where downstream crates could import `Vec3` from either
`bif_math` or `bif_renderer`. Not harmful but slightly confusing. Consider
removing the re-exports since all downstream crates already depend on bif_math
directly.

---

## 10. Prioritized Recommendations

| # | Item | Effort | Impact | When | Status |
|---|------|--------|--------|------|--------|
| 1 | Extract Renderer sub-structs (GpuContext, SceneState, etc.) | 4-6 hrs | High -- unblocks future features, fixes borrow fights | Before M30 | ✅ Done (`7a95329`) |
| 2 | Fix `unsafe impl Sync for UsdStage` -- use Mutex | 2-3 hrs | Medium -- prevents subtle threading bugs | Next session | ✅ Done (`3338f26`) |
| 3 | Extract pure logic from scene_loader.rs for testing | 3-4 hrs | Medium -- catches data bugs in loading pipeline | Opportunistic | ✅ Done (`c5b62a2` — `scene_pipeline.rs`, 16 tests; loader shrinkage deferred as Phase 4.5) |
| 4 | Document node graph extension checklist | 30 min | Low -- reduces friction for new nodes | Before next node type | ✅ Done (`wiki/architecture/node-graph-system.md`) |
| 5 | Remove bif_math re-exports from bif_renderer | 30 min | Low -- reduces import confusion | Anytime | ✅ Done (`7a95329`) |
| 6 | Add `SceneQuery` API for M32 opinion trace | 4-6 hrs | High for M32 | During M32 planning | ✅ Done (`d46f6ae`, consumed in `d1a8599`) |
| 7 | Convert DenoiseError to thiserror | 15 min | Low -- consistency | Anytime | ✅ Done (`7a95329`) |
| 8 | Add graph evaluation pass alongside dirty propagation | 3-4 hrs | Medium -- needed for >15 nodes | Before M31 per-node viz | ✅ Done (`21235b9` — `eval.rs`, 19 tests) |

---

## 11. What NOT to Change

These decisions are correct and should be preserved:

- **Unidirectional crate dependency graph** -- clean layering, keep it
- **Enum-based node types** -- right choice at current scale
- **thiserror in libraries, anyhow in apps** -- textbook Rust error handling
- **egui_snarl for node graph** -- good enough, avoid framework churn
- **Renderer in bif_viewport, not bif_viewer** -- viewer is just the app shell
- **Scene as flat arrays** (prototypes + instances) -- matches USD mental model
- **Existing trait design** -- minimal but well-placed abstractions

---

## 12. Unresolved Questions

1. `cpp/usd_bridge/` directory not found in scan -- is C++ source in a submodule
   or different location? Need to verify Drop/cleanup correctness on C++ side.
   - answer; it's in `cpp/` but the glob in build.rs only looks at `cpp/usd_bridge/`. The C++ bridge code is in `cpp/usd_bridge/` but the USD C++ source is expected to be provided by the user via environment variable (PXR_USD_PATH) and is not included in the repository. The build script compiles only the bridge code, not the USD source.
2. `UsdStage` is `Send+Sync` but docs say "not thread-safe" -- is the current
   single-thread access pattern enforced by anything other than convention?
   - answer: not sure lets investigate further. The `unsafe impl Sync for UsdStage` is a soundness hole because it allows `UsdStage` to be shared across threads, which is not safe given that the underlying C++ USD library is not thread-safe. The current code only accesses `UsdStage` from the main thread, but this invariant is not enforced by the type system. The recommended fix is to remove `Sync` and wrap access to `UsdStage` in a `Mutex` to ensure that it cannot be accessed concurrently from multiple threads.
   - **RESOLVED 2026-04-07 (`3338f26`):** `Sync` removed, `UsdStage` now wraps in `Arc<Mutex<UsdStage>>` at every shared callsite. ~20 callsites refactored across 10 files. Borrow-checker conflicts fixed via guard extraction / pre-extraction patterns. Only `unsafe impl Send for UsdStage` and `unsafe impl Send for UsdEditLayer` remain — sound because ownership transfer is explicit and the C++ side tolerates single-threaded access across Send moves.
3. Node graph has no serialization yet (M30) -- will `SceneNode` enum variants
   be serde-friendly or need a separate schema?
   answer: well the node graph will only need to be used for current dev work, ill be moving to Qt at somepoint so serialization is not a priority right now. For M30, we can add serde support to the `SceneNode` enum by deriving `Serialize` and `Deserialize` from the `serde` crate. This will allow us to serialize the node graph state to a file or other storage format. If we need more control over the serialization format, we can implement custom `Serialize` and `Deserialize` traits for `SceneNode`. But for now, deriving should be sufficient for basic persistence needs.
4. `StatsPanelParams` pattern in render_ui.rs -- is this the borrow-splitting
   pattern you want to standardize, or a one-off workaround?
   -answer not sure lets do what is the best over all.
   - **RESOLVED 2026-04-11:** kept as a one-off. Sub-struct decomposition (`7a95329`) made the pattern unnecessary for most new code — typed containers let callers borrow orthogonal sub-fields independently. `StatsPanelParams` stays where it is for a case we already have working; don't generalize. New code should reach for sub-structs first, reach for param structs only when the borrow-checker genuinely forces it.
