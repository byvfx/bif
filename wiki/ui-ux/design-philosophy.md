---
title: Design Philosophy
type: article
tags: [ui-ux, design]
created: "2026-04-05"
updated: "2026-04-05"
sources: [../../docs/ux/UI_DESIGN.md, ../../docs/ux/DCC_UI_RESEARCH.md, ../../docs/ux/UX_ARCHITECT_REVIEW.md]
---

# Design Philosophy

BIF's UI follows a "quiet confidence" aesthetic — professional, information-dense, but never overwhelming. Inspired by the best of Clarisse, Houdini, and Nuke.

## Core Principles

### Quiet Confidence

- Dark theme with muted accents (graphite/obsidian palette)
- No flashy animations or gratuitous effects
- Let the content (3D viewport, node graph) be the star
- Professional tools for professional artists

### T-Layout

Primary workspace layout:

```text
┌─────────────────────────────────┐
│           Toolbar               │
├──────┬──────────────┬───────────┤
│      │              │           │
│Scene │   Viewport   │Properties │
│Tree  │              │           │
│      │              │           │
├──────┴──────────────┴───────────┤
│         Node Graph              │
└─────────────────────────────────┘
```

### Progressive Disclosure

- Show essential controls by default
- Advanced options behind expandable sections
- Context-sensitive panels (show relevant properties for selection)
- Don't overwhelm new users, don't limit power users

## Color System

- Based on "Obsidian Graphite" palette
- Layer coloring for USD layer identification
- Subtle status indicators (modified, locked, inherited)

## Future: Qt Migration

Current egui UI is temporary. All subsystems expose UI-agnostic APIs so the frontend can be replaced with Qt (planned v0.15.0). See [[architecture/adr/002-egui-temporary-ui|ADR 002]].

## Related

- [[material-editor|Material Editor]] — Specific editor design
- [[architecture/adr/002-egui-temporary-ui|ADR 002: egui Temporary UI]]
- [[egui-snarl]] — Current node graph library
