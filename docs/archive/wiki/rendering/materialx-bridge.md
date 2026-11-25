---
title: MaterialX Bridge
type: article
tags: [rendering, materialx]
created: "2026-04-05"
updated: "2026-04-05"
sources: [../../FEATURES.md, ../../CHANGELOG.md]
---

# MaterialX Bridge

BIF's MaterialX integration enables material interchange and provides a CPU vertex displacement fallback when GPU compute isn't available.

## What is MaterialX

MaterialX is an open standard for material and look description, originally developed by ILM. It provides:

- Vendor-neutral material definitions
- Node-based shading graphs
- Standard material models (including OpenPBR)

See [[materialx|MaterialX]] concept note for details.

## BIF Integration

### Material Export

- OpenPBR materials in BIF map directly to MaterialX OpenPBR nodes
- Clean export path since BIF chose OpenPBR as its native model
- MaterialX files are interchangeable with other DCCs (Houdini, Maya, etc.)

### CPU Vertex Displacement

Added in v0.12.0 as a fallback:

- When GPU compute displacement isn't available, BIF evaluates MaterialX displacement on CPU
- Modifies vertex positions before upload to GPU
- Slower but ensures displacement always works
- Bridges the gap until full GPU MaterialX evaluation

## Architecture

- Lives in `bif_core` (MaterialX parsing/evaluation)
- Feeds into `bif_renderer` (displacement results)
- Export via `bif_core/src/usd/export.rs`

## Related

- [[materialx|MaterialX]] — The standard itself
- [[openpbr-surface|OpenPBR Surface]] — BIF's material model (native MaterialX)
- [[openpbr|OpenPBR]] — Concept note
- [[wgpu-pipeline|wgpu Pipeline]] — Where displacement results render
