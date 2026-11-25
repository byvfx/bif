---
title: "egui-snarl"
type: concept
tags: [ui, node-graph, egui, rust]
created: "2026-04-05"
updated: "2026-11-24"
---

## Summary

egui-snarl is a node graph editor library built on top of egui, Rust's popular immediate-mode GUI framework. It provides the visual node graph canvas -- draggable nodes, input/output pins, connection wires, and graph layout. In BIF it is now **backend-only**: the `Snarl<SceneNode>` graph data structure survives, but since v0.15.0 the visual layer is Qt (`node_graph_widget`), and egui renders nothing in production (see [[node-graph-system|Node Graph System]] and [[wgpu]] for viewport context).

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

egui-snarl's `Snarl<T>` is BIF's node-graph data structure:

- **Node graph module**: `bif_viewport::node_graph` implements `SnarlViewer` for BIF's 10 node types (UsdRead, Primitive, Scatter, PointInstancer, Xform, UsdExport, UsdPrim, GraftBranches, HdriEnvironment, IvarRender).
- **Evaluation**: `bif_viewport::node_graph::eval` walks the snarl graph to build the scene pipeline.
- **Operations**: `bif_viewport::node_graph::ops` handles node creation, deletion, connection.
- **Persistence**: `bif_viewport::persistence` serializes/deserializes the snarl graph.
- **Viewer integration**: `bif_viewport::node_graph::viewer` implements the `SnarlViewer` trait.
- **Node IDs**: `bif_viewport::node_graph::node_id` manages stable identity for nodes across serialization.

### Migration Status

The Qt migration is **done** (v0.15.0). The visual node-graph layer is now `crates/bif_qt/cpp/node_graph_widget.h/.cpp`; only the `Snarl<T>` data structure and `SnarlViewer` trait remain from egui-snarl. See [[node-graph-system|Node Graph System]].

## Related

- [[wgpu]] -- renders alongside the egui-snarl canvas
- [[point-instancer]] -- one of the node types in the graph
- [[edit-target]] -- future layer-aware editing will surface in the node graph
