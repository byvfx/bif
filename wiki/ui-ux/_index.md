---
title: UI/UX Index
type: index
updated: "2026-04-05"
---

# UI/UX Design

BIF's interface design — "quiet confidence" aesthetic inspired by Clarisse, Houdini, and Nuke.

## Articles

- [[design-philosophy|Design Philosophy]] — T-layout, progressive disclosure, quiet confidence
- [[material-editor|Material Editor]] — Material editor design spec

## Source Docs

- [DCC_UI_RESEARCH.md](../../docs/ux/DCC_UI_RESEARCH.md) — DCC UI research
- [UI_DESIGN.md](../../docs/ux/UI_DESIGN.md) — Overall UI design
- [MATERIAL_EDITOR_DESIGN.md](../../docs/ux/MATERIAL_EDITOR_DESIGN.md) — Material editor spec
- [UX_ARCHITECT_REVIEW.md](../../docs/ux/UX_ARCHITECT_REVIEW.md) — UX architecture review
- [UX_RESEARCHER_REVIEW.md](../../docs/ux/UX_RESEARCHER_REVIEW.md) — UX research findings

## Key Decisions

- egui is temporary — all subsystems have UI-agnostic APIs for future Qt migration
- See [[architecture/adr/002-egui-temporary-ui|ADR 002: egui Temporary UI]]

## See Also

- [[architecture/_index|Architecture]] — How UI connects to subsystems
