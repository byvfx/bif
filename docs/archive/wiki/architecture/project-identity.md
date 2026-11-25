---
title: "Project Identity: USD Orchestration Tool"
type: article
tags: [architecture, identity, positioning]
created: "2026-04-06"
updated: "2026-04-07"
sources: [README.md, BIF_USD_WORKFLOW.md]
---

# Project Identity: USD Orchestration Tool

## Overview

BIF is a **USD Orchestration Tool** for VFX. It combines layer-aware USD editing with procedural scene assembly and integrated rendering. The identity crystallized on 2026-04-06 after recognizing that building layer awareness, edit targets, and opinion inspection was turning BIF into a USD-native editor — and that's a strong, underserved niche.

## What "Orchestration" Means

Orchestration = you're the conductor, not the instrument player. Artists bring assets authored in Houdini, Maya, or Blender. BIF arranges them (composition arcs), transforms them (procedural nodes), overrides materials, and outputs clean USD.

### Scope Fence

| In Scope (Orchestration) | Out of Scope |
|--------------------------|--------------|
| Compose USD layers | Geometry modeling |
| Override materials | Rigging / skinning |
| Scatter / instance | Physics simulation |
| Transform / layout | Texture painting |
| Lighting setup | Character animation |
| Render (integrated CPU) | Deep animation curves |
| USDA live preview | |
| Opinion inspection | |

The fence keeps BIF focused. Features outside the fence are deferred indefinitely or rejected.

## Competitive Positioning

| Tool | USD Native? | Procedural? | Layer-Aware? | Integrated Render? | Price |
|------|------------|-------------|--------------|-------------------|-------|
| Katana | Yes | No | Yes (strong) | Deferred to farm | $$$$ |
| Houdini LOPs | Bolted-on | Yes (strong) | Weak | Karma | $$$ |
| Clarisse | Import/export | Limited | No | Yes | $$ |
| usdview | Read-only | No | No | Hydra Storm | Free |
| **BIF** | **Native** | **Yes** | **Growing** | **Yes (CPU)** | **Free/low** |

### Key Differentiators

1. **USD-native** — always in USD, not import/export
2. **Procedural + layer-aware** — nobody else does both
3. **Integrated CPU renderer** — see what you're building
4. **USDA live preview** — see the USD your edits generate
5. **Affordable** — targeting indie/small studio pricing

## Target Audience

**Primary:** Solo VFX artists and small studios who need USD tooling but can't afford Katana.

**Secondary:** Pipeline TDs building USD pipelines who need a visual debugger/editor. Layout and lighting departments at mid-size studios.

## Business Model Direction

Inspired by tyFlow (Tyson Ibele, 3DS Max):

1. **Free during beta** — build community, no support burden
2. **Users become evangelists** — reels and demos are free marketing
3. **Low-cost paid tier at stable release** — free version stays available
4. Natural transition: free while egui/beta, paid when Qt lands and feels production-ready

## Future: Hydra Delegate

Worth pursuing post-v1.0. If BIF's renderer becomes a Hydra delegate:

- Other renderers (Arnold, RenderMan) could plug into BIF
- BIF's renderer could plug into usdview/Katana
- Should influence renderer architecture now — keep render interface abstractable

## Architecture Implications

The orchestration identity validates the existing roadmap ([[003-hybrid-usd-workflow]]). No pivot needed, but emphasis shifts:

- **P0:** FFI expansion (SetEditTarget, GetPrimStack) for v0.14
- **P1:** Stage invalidation strategy, EditOperation enum, EditHistory for v0.16
- **P2:** Renderer god object decoupling (incremental)
- **Parked:** Python scripting until Hydra work pulls it forward

## Key Takeaways

- "Orchestration" is the unifying concept — it captures both procedural nodes and layer editing
- The scope fence prevents BIF from becoming a general DCC
- The existing roadmap (v0.14 layer awareness -> v0.16 edit ops) already builds the orchestration tool
- Competitive gap is real and underserved — no tool does procedural + layer-aware + USD-native

## Related

- [[003-hybrid-usd-workflow|ADR 003: Hybrid USD Workflow]]
- [[crate-structure|Crate Structure]]
- [[design-philosophy|Design Philosophy]]
