---
title: "Architecture"
type: article
tags: [architecture]
created: "2026-04-05"
updated: "2026-04-16"
sources: [ARCHITECTURE.md, ARCHITECTURE_REVIEW.md, ARCHITECTURE_REFACTORS.md, MILESTONES.md]
---

# Architecture

BIF is a USD Orchestration Tool for VFX built in Rust — layer-aware USD editing + procedural scene assembly + integrated rendering. The architecture prioritizes clean crate layering, prototype/instance scalability, and dual rendering (GPU viewport + CPU path tracer).

## Articles

- [[project-identity|Project Identity: USD Orchestration Tool]] — Identity, scope fence, competitive positioning, business model direction
- [[crate-structure|Crate Structure]] — The 6-crate workspace, responsibilities, and dependency flow
- [[node-graph-system|Node Graph System]] — egui-snarl based node graph with 10 node types and dirty-propagation evaluation
- [[scene-browser|Scene Browser]] — CompositeProvider merging USD stage hierarchy with procedural prims

- [[architecture-review|Architecture Review]] — Audit status of ARCHITECTURE_REVIEW.md and ARCHITECTURE_REFACTORS.md
- [[scene-query-trait|SceneQuery Trait]] — Read-only query API abstracting Scene for future LayerAwareScene
- [[event-dispatch-pattern|Event Dispatch Pattern]] — Thin router + category dispatch files for AppEvent handling
- [[phase-e2-qt-migration|Phase E.2 — Qt migration roadmap]] — 9 moves connecting `bif_qt_shell` to the live stage (all shipped, dogfood pending)
- [[cxx-qt-bridge-patterns|cxx-qt bridge patterns]] — invokable signatures, `rust_mut` mutability, qproperty revisions, trait-shadow gotcha

## Architecture Decision Records

- [[001-openpbr-over-disney|ADR 001: OpenPBR Over Disney]] — Why BIF uses OpenPBR instead of Disney BSDF
- [[002-egui-temporary-ui|ADR 002: egui Temporary UI]] — egui is a stepping stone; all subsystems must be UI-agnostic for Qt migration
- [[003-hybrid-usd-workflow|ADR 003: Hybrid USD Workflow]] — Procedural nodes + layer-aware editing, not a full pivot to either
- [[004-cpp-bridge-for-usd|ADR 004: C++ Bridge for USD]] — CMake-built C++ bridge for USD FFI instead of pure Rust
- [[005-layer-aware-read-model|ADR 005: Layer-aware read model (v0.14.0)]] — Read-only layer inspection in v0.14; UI-agnostic `SceneLayerState` / `LayerStack` / `OpinionSource`
- [[006-qt-via-cxx-qt|ADR 006: Qt 6 via cxx-qt (v0.15.0)]] — Qt binding choice, wgpu-QWidget embedding recipe, LGPL dynamic linking, shell-first migration on `v0.15-qt` branch
- [[007-shell-state-to-viewport-bridge|ADR 007: BifShellState ↔ ViewportCallbacks bridge (v0.15.0)]] — `thread_local!` raw-pointer bridge from cxx-qt invokables to the renderer; single-threaded Qt UI lifetime invariant

## Key Principles

1. **Prototype/Instance Everything** — 10MB mesh x 100K instances = ~16MB, not 1TB
2. **USD-Compatible Scene Graph** — Rust types map to USD equivalents (Scene=Stage, Prototype=Mesh, Instance=PointInstancer)
3. **Dual Rendering** — GPU viewport for interactive assembly (60 FPS), CPU path tracer "Ivar" for production quality
4. **Unidirectional Dependencies** — bif_math -> bif_core -> bif_renderer -> bif_viewport -> bif_viewer (no cycles)
5. **egui for PoC, Qt for Production** — Validate workflow first, migrate UI framework only when needed

## Related

- [[design-philosophy|Design Philosophy]] — UI/UX design direction
- [[material-editor|Material Editor]] — Material editing design spec
