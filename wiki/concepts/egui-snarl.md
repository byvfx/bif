---
title: "egui-snarl"
type: concept
tags: [ui, node-graph, egui, rust]
created: "2026-04-05"
updated: "2026-04-05"
---

## Summary

egui-snarl is a node graph editor library built on top of egui, Rust's popular immediate-mode GUI framework. It provides the visual node graph canvas -- draggable nodes, input/output pins, connection wires, and graph layout -- that BIF uses for its procedural scene assembly workflow. It is a temporary UI layer; the long-term plan is migration to Qt (see [[wgpu]] for viewport context).

## Details

### Architecture

- **Snarl<T>**: The graph data structure, generic over node type `T`. Stores nodes and connections.
- **SnarlViewer**: Trait you implement to define how nodes render, what pins exist, and how connections validate.
- **SnarlStyle**: Visual configuration (colors, wire thickness, pin shapes).
- **Immediate-mode**: No persistent widget state -- the graph is re-rendered every frame from the `Snarl<T>` data.

### Key Features

- Drag-and-drop node creation
- Pin-to-pin connection with type validation
- Zoom and pan on the canvas
- Custom node rendering via the viewer trait
- Serialization support (serde)

### Limitations

- **No grouping/subgraphs** -- all nodes live in a flat canvas.
- **No minimap** -- navigation relies on zoom/pan.
- **Performance** -- egui's immediate mode means the full graph is processed every frame. Fine for hundreds of nodes, problematic for thousands.
- **Styling** -- limited compared to Qt/native node graph editors.

## In BIF

egui-snarl is the foundation of BIF's node graph system:

- **Node graph module**: `bif_viewport::node_graph` implements `SnarlViewer` for BIF's 10 node types (UsdRead, Primitive, Scatter, PointInstancer, Xform, UsdExport, UsdPrim, GraftBranches, HdriEnvironment, IvarRender).
- **Evaluation**: `bif_viewport::node_graph::eval` walks the snarl graph to build the scene pipeline.
- **Operations**: `bif_viewport::node_graph::ops` handles node creation, deletion, connection.
- **Persistence**: `bif_viewport::persistence` serializes/deserializes the snarl graph.
- **Viewer integration**: `bif_viewport::node_graph::viewer` implements the `SnarlViewer` trait.
- **Node IDs**: `bif_viewport::node_graph::node_id` manages stable identity for nodes across serialization.

### Migration Plan

egui-snarl is explicitly temporary. All node graph logic is kept in UI-agnostic APIs so that the Qt migration (v0.15.0) can replace the visual layer without rewriting evaluation logic.

## Related

- [[wgpu]] -- renders alongside the egui-snarl canvas
- [[point-instancer]] -- one of the node types in the graph
- [[edit-target]] -- future layer-aware editing will surface in the node graph
