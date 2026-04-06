---
title: Material Editor
type: article
tags: [ui-ux, materials]
created: "2026-04-05"
updated: "2026-04-05"
sources: [../../docs/ux/MATERIAL_EDITOR_DESIGN.md]
---

# Material Editor

Design spec for BIF's material editor — the interface for creating and editing OpenPBR materials.

## Overview

The material editor provides:

- Visual parameter editing for OpenPBR Surface properties
- Real-time preview of material changes
- Texture slot assignment
- Material library browsing
- Node-based material graph (future)

## Layout

Integrated into the properties panel (right side of T-layout):

- **Header:** Material name, type indicator, preview thumbnail
- **Parameter groups:** Collapsible sections matching OpenPBR categories
  - Base (color, weight, roughness, metalness)
  - Specular (weight, color, roughness, IOR)
  - Transmission, Subsurface, Coat, Emission
- **Texture slots:** Drag-and-drop texture assignment per parameter
- **Preview:** Real-time material ball or selected object preview

## Interaction Patterns

- Slider + numeric input for float parameters
- Color picker with HDR support for color parameters
- Drag-and-drop for textures
- Right-click for reset/copy/paste parameter values
- Undo/redo integration

## See Also

- [[openpbr-surface|OpenPBR Surface]] — The material model being edited
- [[design-philosophy|Design Philosophy]] — Overall UI approach
- [Full design spec](../../docs/ux/MATERIAL_EDITOR_DESIGN.md) — Detailed source document
