---
title: "Node Graph System"
type: article
tags: [architecture]
created: "2026-04-05"
updated: "2026-04-05"
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

Node evaluation is event-driven, not a general dataflow pass. UI interactions generate `NodeGraphEvent` enum variants, which are handled in `render.rs::handle_node_graph_event()` (~968-line match block). Each event triggers specific recomputations that update the `working_scene`.

The `working_scene` (a `bif_core::Scene`) is the single source of truth consumed by both the GPU viewport and the CPU path tracer.

### Pin Types

Connections are typed via a `PinType` enum with colored wires:

- **Scene** — geometry + materials + transforms
- **Points** — point cloud data
- **Environment** — HDRI environment map
- **Image** — rendered output

## Architecture Details

### Key Files

- `node_graph/mod.rs` (~1,370 LOC) — `SceneNode` enum, viewer implementation, evaluation logic
- `node_graph/ops.rs` — BFS dirty propagation, node operations
- `node_graph/viewer.rs` — `SceneNodeViewer` implementing `SnarlViewer`
- `render.rs` — `handle_node_graph_event()` event handler

### Extension Pattern

Adding a new node type requires changes in 5+ places:

1. Add variant to `SceneNode` enum
2. Add constructor in `SceneNode` impl
3. Add match arms in `name()`, `input_count()`, `output_count()`, `input_pin()`, `output_pin()`
4. Add variant to `NodeGraphEvent` if custom behavior needed
5. Add `add_xxx()` to `NodeGraphState`
6. Handle event in `render.rs::handle_node_graph_event()`
7. Possibly update `mark_node_dirty()` in `ops.rs`

This is the classic enum-based dispatch pattern. It works well for 10 nodes. At 15-20+ nodes, consider migrating to a `NodeBehavior` trait for open extensibility — at the cost of losing exhaustive match checking.

### Future: Blue/Orange Classification

Per the [[003-hybrid-usd-workflow|ADR 003: Hybrid USD Workflow]], nodes will be classified as:

- **Blue (Composition)** — structural nodes that define what's in the scene (UsdRead, UsdPrim, GraftBranches)
- **Orange (Operations)** — edit nodes that author `EditOperation`s on the working layer (Xform, material overrides)

This classification matters for layer-aware evaluation ordering: composition nodes must evaluate before operation nodes.

## Planned Refactors

### Node Graph Eval Engine (Phase 3 of ARCHITECTURE_REFACTORS)

Extract evaluation logic from the UI into a testable `eval.rs` module:

- `evaluate_dirty_nodes()` — pure function returning `Vec<EvalCommand>` (no egui dependency)
- `topological_order()` — correct evaluation ordering with branches
- `EvalCommand` enum — typed commands for LoadUsd, AuthorEdit, ComputeScatter, StartRender, etc.
- Target: 10-15 new tests, all runnable without egui context

## Related

- [[crate-structure|Crate Structure]] — Where the node graph lives in the crate hierarchy
- [[scene-browser|Scene Browser]] — How node output feeds the scene browser
- [[002-egui-temporary-ui|ADR 002: egui Temporary UI]] — Node graph will be re-implemented in Qt
- [[003-hybrid-usd-workflow|ADR 003: Hybrid USD Workflow]] — Blue/orange node classification
