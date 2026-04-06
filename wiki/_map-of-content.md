---
title: Map of Content
type: index
updated: "2026-04-05"
---

# Map of Content

Visual overview of how knowledge areas connect in BIF.

## Core Pipeline

```text
USD Stage ──→ Scene Browser ──→ Node Graph ──→ Renderer ──→ Viewport
   │              │                 │              │
   ▼              ▼                 ▼              ▼
[[stage-layer-prim|Stage, Layer, Prim]]    [[node-graph-system|Node Graph System]]  [[wgpu-pipeline|wgpu Pipeline]]
[[composition-arcs|Composition Arcs]]      [[scene-browser|Scene Browser]]      [[openpbr-surface|OpenPBR Surface]]
[[edit-target|Edit Target]]           [[egui-snarl]]         [[materialx-bridge|MaterialX Bridge]]
```

## Architecture Decisions

- [[001-openpbr-over-disney|ADR 001: OpenPBR Over Disney]]
- [[002-egui-temporary-ui|ADR 002: egui Temporary UI]]
- [[003-hybrid-usd-workflow|ADR 003: Hybrid USD Workflow]]
- [[004-cpp-bridge-for-usd|ADR 004: C++ Bridge for USD]]

## Domain Knowledge

### USD (Universal Scene Description)

- [[composition-arcs|Composition Arcs]] — LIVRPS rule
- [[primvars|Primvars]] — Primitive variables
- [[point-instancer|Point Instancer]] — Efficient instancing
- [[edit-target|Edit Target]] — Layer-directed editing

### Rendering & Materials

- [[openpbr|OpenPBR]] — Material model
- [[materialx|MaterialX]] — Material exchange format
- [[ior-fresnel|IOR Fresnel]] — Physics-based reflectance
- [[bvh|BVH]] — Acceleration structure

### Tools & Libraries

- [[wgpu]] — Rust WebGPU implementation
- [[egui-snarl]] — Node graph library

## Learning Path

For someone new to this project:

1. Start with [[architecture/_index|Architecture Overview]]
2. Read [[crate-structure|Crate Structure]] to understand the code layout
3. Explore [[usd/_index|USD Section]] for domain context
4. Check [[rendering/_index|Rendering Section]] for the visual pipeline
5. Browse [[concepts/_index|Concepts]] for quick reference on specific topics
