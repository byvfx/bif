---
title: "Node Graph System"
type: article
tags: [architecture]
created: "2026-04-05"
updated: "2026-05-01"
sources: [ARCHITECTURE.md, ARCHITECTURE_REVIEW.md, ARCHITECTURE_REFACTORS.md]
---

# Node Graph System

BIF uses a node-based workflow built on **egui-snarl**, an immediate-mode node graph library for egui. The graph covers the full import-scatter-instance-render-export pipeline.

## Overview

The node graph lives in `crates/bif_viewport/src/node_graph/`. It uses a `SceneNode` enum with 10 variants (one per node type), evaluated via dirty propagation. The `SceneNodeViewer` struct implements snarl's `SnarlViewer<SceneNode>` trait to render nodes in the egui UI.

## Node Types

| Node | Purpose | Pin Types |
|------|---------|-----------|
| **UsdRead** | Load a USD file from disk | Out: Scene |
| **Primitive** | Generate procedural geometry (cube, sphere, etc.) | Out: Scene |
| **ScatterPoints** | Scatter points on a surface | In: Scene; Out: Points |
| **PointInstancer** | Instance prototypes at point positions | In: Scene + Points; Out: Scene |
| **Xform** | Apply a transform override | In: Scene; Out: Scene |
| **UsdExport** | Export scene to a USD file | In: Scene |
| **UsdPrim** | Reference a specific prim from a loaded stage | In: Scene; Out: Scene |
| **GraftBranches** | Merge multiple scene branches | In: Scene (multiple); Out: Scene |
| **HdriEnvironment** | Load HDRI for IBL + background | Out: Environment |
| **IvarRender** | Trigger CPU path trace render | In: Scene + Environment; Out: Image |

## How Evaluation Works

### Dirty Propagation

When a node's parameters change, it is marked dirty. A BFS traversal in `ops.rs` (~84 lines) propagates the dirty flag downstream through connections. Only dirty nodes are re-evaluated.

```text
UsdRead [dirty] --> Xform [propagated dirty] --> UsdExport [propagated dirty]
```

### Event-Driven Execution

Node evaluation is event-driven, not a general dataflow pass. UI interactions generate `NodeGraphEvent` enum variants, which are handled in `node_dispatch.rs::handle_node_graph_event()` (798 LOC — extracted from `render.rs` in commit `d3d8f79`, previously a 726-line inline match block). Each event triggers specific recomputations that update the `working_scene`. The pure-compute path via `eval.rs::evaluate_dirty_nodes()` runs in parallel and emits `EvalCommand` values for downstream consumers.

The `working_scene` (a `bif_core::Scene`) is the single source of truth consumed by both the GPU viewport and the CPU path tracer.

### Pin Types

Connections are typed via a `PinType` enum with colored wires:

- **Scene** — geometry + materials + transforms
- **Points** — point cloud data
- **Environment** — HDRI environment map
- **Image** — rendered output

## Architecture Details

### Key Files

- `node_graph/mod.rs` (~1,370 LOC) — `SceneNode` enum, state, viewer integration glue
- `node_graph/eval.rs` (725 LOC, 19 tests) — pure-compute evaluation engine; emits `EvalCommand` values
- `node_graph/ops.rs` — BFS dirty propagation (84 LOC)
- `node_graph/viewer.rs` (576 LOC) — `SceneNodeViewer` implementing `SnarlViewer`
- `node_dispatch.rs` (798 LOC) — `handle_node_graph_event()` event handler (extracted from `render.rs`)

### Adding a New Node Type — Checklist

Terse reference card. Every item is mandatory unless noted. Compile the crate after each group of edits — the exhaustive match check is your friend.

**1. Define the node**

1. Add a variant to `SceneNode` enum (`node_graph/mod.rs`). Include `#[derive(Serialize, Deserialize)]` coverage (inherited from enum) — verify any new field types are serde-friendly.
2. Add a constructor method on `SceneNode` (e.g. `SceneNode::my_node()`).

**2. Wire into the viewer (`node_graph/viewer.rs`)**

3. Add match arms in `SceneNodeViewer::name()`, `title()`, `input_count()`, `output_count()`, `input_pin()`, `output_pin()`. Compiler will catch missing arms.
4. If the node has a custom body UI, add a match arm in `show_body()`.

**3. Wire into events**

5. Add a `NodeGraphEvent` variant if the node emits user events (parameter changes, file pickers, etc.).
6. Add `NodeGraphState::add_my_node()` so the scene browser / menu can instantiate one.
7. Handle the new `NodeGraphEvent` variant in `node_dispatch.rs::handle_node_graph_event()` (previously lived in `render.rs`; moved in commit `d3d8f79`).

**4. Wire into evaluation (`node_graph/eval.rs`)**

8. If the node participates in dirty propagation, add a case to `mark_node_dirty()` in `ops.rs` (only if its dirty semantics are non-standard).
9. If the node produces an `EvalCommand` (the pure-compute layer), add a variant to `EvalCommand` and a case in `evaluate_dirty_nodes()`. Write a test in `eval.rs` that constructs a `Snarl<SceneNode>` with the new node and asserts the expected command is emitted.

**5. Wire into persistence (`persistence.rs`)**

10. If the new variant has fields, round-trip-test it: add an instance to `sample_project()` and extend `save_load_bifa_round_trip` / `save_load_bif_round_trip` assertions.

**Design note.** This is the classic enum-based dispatch pattern. It works well for 10 nodes. At 15-20+ nodes, consider migrating to a `NodeBehavior` trait for open extensibility — at the cost of losing exhaustive match checking. See ARCHITECTURE_REVIEW.md §3.

### Future: Blue/Orange Classification

_Future. Not implemented in v0.16. Procedural nodes still flow to USD only at `export_scene()` time; only interactive viewport edits route through `EditOperation` / `EditHistory` (see [[adr/008-edit-operation-architecture|ADR 008]])._

Per the [[003-hybrid-usd-workflow|ADR 003: Hybrid USD Workflow]], nodes will be classified as:

- **Blue (Composition)** — structural nodes that define what's in the scene (UsdRead, UsdPrim, GraftBranches)
- **Orange (Operations)** — edit nodes that author `EditOperation`s on the working layer (Xform, material overrides)

This classification matters for layer-aware evaluation ordering: composition nodes must evaluate before operation nodes.

## Refactor History

### Node Graph Eval Engine (Phase 3 of ARCHITECTURE_REFACTORS) — ✅ Shipped

Landed in commit `21235b9` (2026-03-28). `crates/bif_viewport/src/node_graph/eval.rs` — 725 LOC, **19 tests** (above the 10-15 target). Provides:

- `evaluate_dirty_nodes(snarl, dirty, display_node) -> Vec<EvalCommand>` — pure function, no egui dependency
- `topological_order(snarl, dirty) -> Vec<GraphNodeId>` — correct ordering with branches
- `EvalCommand` enum — typed commands (`LoadUsd`, `LoadPayload`, `AuthorEdit`, `ComputeScatter`, `ComputeInstancer`, `StartRender`, etc.)

Tests construct `Snarl<SceneNode>` directly without an egui context. See `ARCHITECTURE_REFACTORS.md` Phase 3 for full status.

## Related

- [[crate-structure|Crate Structure]] — Where the node graph lives in the crate hierarchy
- [[scene-browser|Scene Browser]] — How node output feeds the scene browser
- [[002-egui-temporary-ui|ADR 002: egui Temporary UI]] — Node graph will be re-implemented in Qt
- [[003-hybrid-usd-workflow|ADR 003: Hybrid USD Workflow]] — Blue/orange node classification
