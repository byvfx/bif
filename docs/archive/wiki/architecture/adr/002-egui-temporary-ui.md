---
title: "ADR-002: egui as Temporary UI"
type: adr
tags: [architecture, ui]
created: "2026-04-05"
updated: "2026-04-05"
sources: [ARCHITECTURE.md, MILESTONES.md, memory/project_qt_migration.md]
---

# ADR-002: egui as Temporary UI

## Context

BIF needed a UI framework for the proof-of-concept phase. The project is a Rust application with an embedded wgpu 3D viewport, a node graph, a scene browser, and property editors. Two options were considered: start with egui (pure Rust, immediate-mode) and potentially migrate later, or go directly to Qt 6 via cxx-qt (industry standard, but significant FFI complexity).

The decision was made early in development and has guided all subsystem design since.

## Decision

**egui is the temporary UI framework. All subsystems must have UI-agnostic APIs so that a Qt migration can happen without rewriting business logic.**

Concretely:

- New subsystems must NOT import egui types in their public API
- UI code is isolated in thin adapter layers within bif_viewport
- Interfaces must be callable from a Qt adapter without modification
- egui-snarl is used for the node graph but the evaluation engine is being extracted to a UI-independent module

The Qt migration is planned for **v0.15.0** and is considered "the pivot" — everything after is Qt-native.

## Alternatives Considered

| Option | Pros | Cons | Verdict |
|--------|------|------|---------|
| **egui now, Qt later** | Fast iteration, single language, pure Rust, validates workflow before committing to FFI complexity | Migration cost, egui limitations (no native menus, docking is basic, no multi-monitor) | **Chosen** |
| **Qt from day one** | No migration needed, professional UI from start | Slow iteration, cxx-qt learning curve, FFI complexity delays core features | Rejected |
| **egui permanently** | No migration cost | Insufficient for production DCC (docking, accessibility, multi-monitor, performance at scale) | Rejected |
| **Tauri/web** | Cross-platform, modern UI | GPU viewport integration is painful, serialization overhead | Not considered |

## Consequences

**Positive:**

- egui enabled rapid prototyping through M0-M23 (24 milestones) with minimal UI friction
- egui-snarl provided a working node graph without building a custom graph editor
- All core logic (scene graph, export, materials, rendering) is already UI-independent
- The architecture is validated before committing to Qt complexity

**Negative:**

- v0.13.0 is the "last egui feature release" — new features are blocked on Qt for full potential
- Some borrow-checker workarounds exist (e.g., `StatsPanelParams` pattern in render_ui.rs) that are egui-specific
- The Renderer God object in bif_viewport accumulated UI state alongside rendering state, requiring planned decomposition

**Migration Plan:**

- v0.14.0 (Layer-Aware Stage) — logic is UI-agnostic, UI adapters exist for both egui and future Qt
- v0.15.0 (Qt Migration) — port scene browser, property inspector, node graph, viewport
- Target layout: viewport-dominant T-layout with Qt dock widgets

## Related

- [[crate-structure|Crate Structure]] — bif_viewport contains all egui-dependent code
- [[node-graph-system|Node Graph System]] — Node graph eval engine extraction removes egui dependency
- [[design-philosophy|Design Philosophy]] — Qt UI design spec ("quiet confidence" aesthetic)
