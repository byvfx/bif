# BIF Architecture Refactors

Architectural deepening plan for testability, cross-platform support, and alignment with the USD editor workflow (`BIF_USD_WORKFLOW.md`).

**Created:** 2026-03-28
**Status:** Approved, Phase 1 complete
**Estimated:** 13-18 weeks at 10-20 hrs/week (~45-65 new tests)

---

## Current Architecture Assessment

### Codebase Stats

| Crate | LOC | Modules | Tests | Role |
|-------|-----|---------|-------|------|
| bif_math | ~2,000 | 8 | 74 | Pure geometry/transforms (leaf) |
| bif_core | ~13,900 | 17 | 163 | USD bridge, scene, textures, scatter |
| bif_renderer | ~10,000 | 21 | 111 | CPU path tracer, materials, Embree |
| bif_viewport | ~14,000+ | 37 | 149 | GPU viewport, UI, node graph |
| bif_viewer | ~800 | 1 | 19 | Entry point (thin) |
| bif_maketx | ~200 | 1 | 0 | Texture conversion tool |

### Critical Findings

| Finding | File(s) | LOC | Tests | Severity |
|---------|---------|-----|-------|----------|
| Monolithic FFI bridge | cpp_bridge.rs | 4,542 | 5 | Critical |
| Untested render loop | render.rs | 2,582 | 0 | Critical |
| Untested scene loading | scene_loader.rs | 2,350 | 0 | Critical |
| Untested node graph eval | node_graph/mod.rs | 1,370 | 0 | Critical |
| Untested material pipeline | ivar_build.rs | 1,214 | 0 | High |
| God object Renderer | lib.rs | 1,775 | — | High |
| Windows-only build | build.rs, ci.yml | — | — | High |

### Dependency Graph (Clean)

```text
bif_math (leaf — zero internal deps)
    ↑
bif_core (depends on bif_math)
    ↑
bif_renderer (depends on bif_core + bif_math)
    ↑
bif_viewport (depends on bif_core + bif_renderer + bif_math)
    ↑
bif_viewer (depends on bif_viewport + bif_core + bif_math)
```

---

## Phase 1: C++ FFI Bridge Split

- **Status:** Complete (ffi_raw.rs, ffi_convert.rs, cpp_bridge.rs — 44 tests in ffi_convert)
- **Scope:** Medium (2-3 weeks)
- **Target Tests:** 15-20

### Problem

`crates/bif_core/src/usd/cpp_bridge.rs` — 4,542 LOC in one file:

- 21 pub structs + 15 pub enums
- `#[repr(C)]` raw FFI types mixed with safe Rust domain types
- `UsdStage` wrapper with unsafe pointer→Vec conversion inline in every method
- Conversion logic untestable without C++ DLLs and `setup_usd_env.ps1`

Per `BIF_USD_WORKFLOW.md`, this bridge will soon need layer stack queries, payload loading/unloading, and opinion authoring — making the monolith unsustainable.

### Design

Split into 3 files in `crates/bif_core/src/usd/`:

| File | Contents | Visibility |
|------|----------|------------|
| `ffi_raw.rs` | `#[repr(C)]` structs, `extern "C"` blocks | `pub(crate)` |
| `ffi_convert.rs` | Pure Rust: raw slices → domain types | `pub(crate)`, tested |
| `cpp_bridge.rs` (slimmed) | `UsdStage` wrapper + public domain types | `pub` |

**Refactor pattern:**

```text
Before: UsdStage::get_mesh() — unsafe deref + 50-line conversion inline
After:  UsdStage::get_mesh() → slice_raw_mesh() [ffi_raw] → convert_mesh_data() [ffi_convert]
```

### Files

- **Create:** `crates/bif_core/src/usd/ffi_raw.rs`
- **Create:** `crates/bif_core/src/usd/ffi_convert.rs`
- **Modify:** `crates/bif_core/src/usd/cpp_bridge.rs`
- **Modify:** `crates/bif_core/src/usd/mod.rs`

### Test Targets

All tests run without C++ DLLs:

- `convert_mesh_data` — vertices, normals, UVs, face materials
- `convert_instancer_data` — transforms, prototype indices
- `convert_material_data` — texture paths, PBR params
- `convert_light_data` — light type enum mapping
- `convert_mesh_empty` — graceful empty input handling

### Risks

Low. Pure mechanical refactoring. No API changes. Public types unchanged. Rollback = revert file split.

---

## Phase 2: Cross-Platform / Linux Foundation

- **Status:** Not started
- **Scope:** Medium-Large (3-4 weeks)
- **Depends on:** Phase 1 (build.rs is in bif_core)

### Problem

Build system is Windows-only:

| Issue | Location | Detail |
|-------|----------|--------|
| CMake generator | build.rs:222 | Hardcodes `"Visual Studio 17 2022"` |
| vcpkg paths | build.rs:22,88-111 | Hardcodes `D:\\..\\vcpkg`, `C:\\vcpkg` |
| vcpkg triplet | build.rs:89-90 | Hardcodes `x64-windows` |
| CI | ci.yml | `runs-on: windows-latest` only |
| Setup script | setup_usd_env.ps1 | PowerShell-only |
| Exit hack | main.rs:797 | `process::exit(0)` unconditional |
| Test paths | persistence.rs | Examples use `D:\\` |

### Design

- **build.rs:** Platform-detect CMake generator, vcpkg triplet, search paths
- **setup_usd_env.sh:** Bash equivalent of PowerShell setup
- **ci.yml:** Add Linux job (bif_math + bif_renderer — no USD crates initially)
- **main.rs:** Guard `process::exit(0)` with `cfg!(windows)`

### Files

- **Modify:** `crates/bif_core/build.rs`
- **Create:** `setup_usd_env.sh`
- **Modify:** `.github/workflows/ci.yml`
- **Modify:** `crates/bif_viewer/src/main.rs`

### Risks

Medium. Build system changes are finicky. OIIO bridge section (build.rs:252-397) has same hardcoded patterns. USD library names may differ on Linux. Windows CI must keep passing identically.

---

## Phase 3: Node Graph Eval Engine

- **Status:** Not started
- **Scope:** Medium (2-3 weeks)
- **Target Tests:** 10-15

### Problem

`node_graph/mod.rs` (1,370 LOC) — eval logic, UI, and scene mutation in one module:

- `evaluate()` takes `&mut` borrows of scene, materials, transforms simultaneously
- Auto-compute logic embedded in egui `show_body()` callbacks in viewer.rs
- 0 tests because evaluation requires an egui context

Per `BIF_USD_WORKFLOW.md`, the node graph splits into:

- **Composition nodes (blue):** structural — what's in the scene
- **Operation nodes (orange):** edits — author `EditOperation`s on the working layer

### Design

Create `crates/bif_viewport/src/node_graph/eval.rs`:

```rust
pub enum EvalCommand {
    // Composition (blue nodes)
    LoadUsd { node_id: GraphNodeId, path: String },
    LoadPayload { prim_path: String },
    UnloadPayload { prim_path: String },
    // Operation (orange nodes → EditOperation)
    AuthorEdit { node_id: GraphNodeId, operation: EditOperation },
    // Procedural
    ComputeScatter { node_id: GraphNodeId, params: ScatterPointsParams },
    ComputeInstancer { node_id: GraphNodeId, points_source: GraphNodeId, proto_source: GraphNodeId },
    StartRender { spp: u32 },
}

pub fn evaluate_dirty_nodes(
    snarl: &Snarl<SceneNode>,
    dirty_nodes: &HashSet<GraphNodeId>,
    display_node: Option<GraphNodeId>,
) -> Vec<EvalCommand>

pub fn topological_order(
    snarl: &Snarl<SceneNode>,
    dirty: &HashSet<GraphNodeId>,
) -> Vec<GraphNodeId>
```

### Files

- **Create:** `crates/bif_viewport/src/node_graph/eval.rs`
- **Modify:** `crates/bif_viewport/src/node_graph/mod.rs`
- **Modify:** `crates/bif_viewport/src/node_graph/viewer.rs`

### Test Targets

Tests create `Snarl<SceneNode>` directly (no egui):

- `test_dirty_propagation_linear_chain` — A→B→C dirty propagation
- `test_topological_order` — correct ordering with branches
- `test_composition_vs_operation_ordering` — blue before orange
- `test_display_node_filters_inactive` — only upstream nodes evaluated
- `test_no_commands_when_clean` — all computed, no commands
- `test_scatter_then_instancer_order` — dependency ordering

### Risks

Medium. Auto-compute logic in viewer.rs is interleaved with UI rendering. Must carefully identify which conditions trigger computation vs. display.

---

## Phase 4: Scene Loading Pipeline

- **Status:** Not started
- **Scope:** Large (3-4 weeks)
- **Target Tests:** 15-20

### Problem

`scene_loader.rs` (2,350 LOC) + `ivar_build.rs` (1,214 LOC):

- `finalize_usd_scene()` — 180-line method creating GPU buffers, material tables, instances, textures, bounds, camera, culling, Embree scene, and mesh_data simultaneously
- All methods are `impl Renderer`, untestable without GPU device
- 0 tests on 3,564 lines

Per `BIF_USD_WORKFLOW.md`, scene loading becomes layer-aware with PayloadPolicy and layer isolation.

### Design

Create `crates/bif_viewport/src/scene_pipeline.rs`:

```rust
pub struct SceneLoadResult {
    pub prototype_meshes: Vec<PrototypeMeshResult>,
    pub instances: InstanceResult,
    pub ivar_mesh: MeshData,
    pub material_table: Vec<MaterialGpu>,
    pub world_bounds: WorldBounds,
    pub texture_paths: Vec<PathBuf>,
    pub cameras: Vec<SceneCamera>,
    pub payload_states: HashMap<String, PayloadState>,
    pub layer_opinions: HashMap<String, LayerSource>,
}

/// Pure function: no GPU, no Renderer.
pub fn build_scene_load_result(
    scene: &Scene,
    hidden_proto_ids: &HashSet<usize>,
    instancer_instances: &BTreeMap<GraphNodeId, Vec<Instance>>,
    payload_policy: &PayloadPolicy,
    working_layer: Option<&str>,
) -> SceneLoadResult
```

Scene_loader.rs becomes: compute result → upload to GPU.

### Files

- **Create:** `crates/bif_viewport/src/scene_pipeline.rs`
- **Create:** `crates/bif_viewport/src/ivar_pipeline.rs`
- **Modify:** `crates/bif_viewport/src/scene_loader.rs`
- **Modify:** `crates/bif_viewport/src/ivar_build.rs`

### Test Targets

Tests create `bif_core::Scene` directly (no GPU):

- `test_single_prototype_no_instances`
- `test_multi_prototype_material_table`
- `test_payload_policy_filtering`
- `test_layer_opinion_tracking`
- `test_hidden_prototypes_excluded`
- `test_world_bounds_computation`
- `test_empty_scene`

### Risks

Medium-High. `reload_working_scene()` reads from `self.nodes.*` maps. Exact parameter boundary for the extracted function requires careful interface design.

---

## Phase 5: Renderer Hub Decomposition

- **Status:** Not started
- **Scope:** Large (3-4 weeks)
- **Target Tests:** 5-10
- **Depends on:** Phases 3 + 4

### Problem

Renderer struct: 49 fields across 7 domains. `render.rs` `dispatch_events()`: 200+ lines handling all event categories. 0 tests.

### Design

Split dispatch into category files (all `impl Renderer` blocks):

| File | Events |
|------|--------|
| `render_dispatch.rs` | Render mode, batch render, progressive refinement |
| `node_dispatch.rs` | Node graph events → EvalCommand processing |
| `selection_dispatch.rs` | Prim selection, transforms, layer-isolated edits |
| `project_dispatch.rs` | New/open/save, shot templates |

`dispatch_events()` in render.rs becomes a thin router.

### Files

- **Create:** `crates/bif_viewport/src/render_dispatch.rs`
- **Create:** `crates/bif_viewport/src/node_dispatch.rs`
- **Create:** `crates/bif_viewport/src/selection_dispatch.rs`
- **Create:** `crates/bif_viewport/src/project_dispatch.rs`
- **Modify:** `crates/bif_viewport/src/render.rs`

### Risks

Medium. Moving `impl Renderer` blocks between files requires visibility adjustments. Some methods access multiple sub-structs, creating borrow-checker challenges.

---

## Rollback Strategy

Every phase follows the same pattern:

1. New files are additive (create `ffi_convert.rs`, `eval.rs`, `scene_pipeline.rs`)
2. Old code delegates to new code
3. Tests verify the new code independently
4. If anything goes wrong, delete the new file and revert the delegation

No destructive refactoring. Partial completion is still valuable.

---

## Cross-Platform Notes

### Good Patterns Found

- `std::path::Path` / `PathBuf` used consistently (not manual string concatenation)
- FFI bindings properly abstracted in `embree_ffi.rs`
- `cfg!(windows)` guards used correctly for platform-specific code
- OIDN feature properly gated behind `#[cfg(feature = "oidn")]`

### Work Required for Linux

1. Platform-detect CMake generator in build.rs
2. Platform-aware vcpkg paths and triplets
3. Create `setup_usd_env.sh`
4. Add Linux CI job (non-USD crates initially)
5. Guard `process::exit(0)` with `cfg!(windows)`
6. Verify USD library names on Linux (`.so` vs `.lib`)
