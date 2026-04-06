---
title: "ADR-003: Hybrid USD Workflow"
type: adr
tags: [architecture, usd]
created: "2026-04-05"
updated: "2026-04-05"
sources: [MILESTONES.md, ARCHITECTURE.md, memory/project_usd_workflow_pivot.md]
---

# ADR-003: Hybrid USD Workflow

## Context

BIF started as a procedural scene assembler — load USD, scatter instances, render, export. As the project matured, a gap became clear: no existing tool lets artists "open a USD stage, pick a layer, make edits, and save clean USD." BIF could fill this gap.

The question was whether to pivot fully to a layer-aware USD editor (abandoning the procedural node graph) or to keep the procedural workflow and add layer awareness underneath.

Decision made: **2026-03-28**. Design spec: `BIF_USD_WORKFLOW.md`.

## Decision

**Hybrid approach: layer-aware editing added underneath the existing procedural architecture.** BIF keeps its node graph (scatter, instancer) as a differentiator while gaining the ability to understand, display, and author USD layer opinions.

Key architectural choices:

- Edits author USD opinions continuously on the active layer (not just at export time)
- The node graph gains two node classifications: **blue (composition)** nodes that define scene structure and **orange (operation)** nodes that author `EditOperation`s on the working layer
- Milestones are threaded incrementally: v0.14 = Layer-Aware Stage, workflow phases woven into v0.14-v0.19
- No BIF-proprietary file formats — everything writes standard USD
- Shot templates (JSON-configurable, `~/.bif/templates/`) provide pipeline starting points

## Alternatives Considered

| Option | Pros | Cons | Verdict |
|--------|------|------|---------|
| **Hybrid (chosen)** | Keeps differentiator (scatter/instance), fills USD editor gap, incremental migration | Complexity of two mental models (procedural + layer editing) | **Chosen** |
| **Full pivot to USD editor** | Simpler mental model, focused product | Loses the procedural workflow that makes BIF unique, major rewrite | Rejected |
| **Stay procedural only** | No pivot cost, finish what's started | Doesn't fill the USD editing gap, export-only workflow is limiting | Rejected |

## Consequences

**Positive:**

- BIF occupies a unique niche: procedural assembly + layer-aware USD editing in one tool
- Artists can use BIF both for creative work (scatter, instance) and for pipeline work (pick layer, override, save)
- Three-panel UI (node graph + stage tree + USDA preview) gives "three views of one truth"
- Non-destructive by design — base USD stages are never modified, edits live in overlay layers

**Negative:**

- Two interaction paradigms increase UX complexity — mitigated by workspace presets (Assembly vs Lighting vs Materials)
- Requires significant FFI expansion: `SdfLayer` read, `GetEditTarget`, `GetPrimStack`, payload load/unload
- Node graph evaluation ordering becomes more complex (composition nodes before operation nodes)
- Active layer safety is critical — editing the wrong layer corrupts production work (requires persistent status bar, layer-switch toast, new-opinion guard)

**Implementation Timeline:**

- v0.14.0: Layer-Aware Stage (FFI expansion, layer stack display, layer isolation, payload policies)
- v0.16.0: Edit Operations + Save (EditOperation enum, Ctrl+S writes active layer, USDA preview becomes editable)
- v0.20.0: Scene Authoring + Layer Diff (diff panel, point editing, new operation nodes)

## Related

- [[node-graph-system|Node Graph System]] — Blue/orange node classification
- [[scene-browser|Scene Browser]] — Layer color dots and opinion indicators
- [[design-philosophy|Design Philosophy]] — Layer color coding, opinion stack visualization, active layer safety
- [[004-cpp-bridge-for-usd|ADR 004: C++ Bridge for USD]] — FFI expansion needed for layer awareness
