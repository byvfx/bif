# Session Handoff - March 13, 2026

**Last Updated:** Roadmap overhaul — new M29.5-M36+ milestones from architecture evaluation
**Next Milestone:** Finish M29 USD Export, then M29.5 egui upgrade
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26 (OIDN), M26.1 (Ivar material cache) |
| Current | M29 USD Export (most phases done, needs validation) |
| Tests | 84 renderer, 41 math, 24 viewport, 27+ bif_core |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~47ms (was 6.7s) |

---

## Recent Work

### Roadmap Overhaul (Mar 13, 2026)

Evaluated 7 feature areas against current architecture (~42K LOC). Key decisions:

- **Nodes stay** — BIF's node graph is operations (verbs), not scene hierarchy. Correct model.
- **New M29.5-M36+** — egui upgrade, persistence, per-node viz, opinion trace, USD debug, Python hooks, API cleanup, framework extraction
- **Old M31-M34** (lights, materials, contexts, MaterialX) → renumbered M37-M40
- **No MDL** — MaterialX is ASWF standard, MDL is Nvidia-only
- **Python hooks via PyO3** — embedded interpreter, not subprocess
- **Framework vision** — "Arch Linux for VFX": library crates → widget crates → plugins → DCC connectors
- **File formats** — `.bif` (bincode binary) + `.bifa` (JSON pretty-print ascii)
- **Vertical node layout** — egui-snarl 0.6+ `NodeLayout::Sandwich`, needs egui 0.30

### VFX Code Review Fixes (Mar 13, 2026)

Fixed all issues from vfx-code-reviewer: shared `build_materials()`, `FnMut` scene builder, per-vertex indexed storage, prewarm race guard, 4 new tests.

---

## Blockers / Known Issues

- `test_should_restart_no_render` — known flaky timing test
- bif_viewport tests need USD DLLs (`setup_usd_env.ps1`)
- bif_viewer.exe locked during build if app is running

---

## Next Steps

1. Finish M29 remaining items (validate exported USD in Houdini/usdview, docs)
2. M29.5: egui 0.29→0.30 upgrade + egui-snarl 0.5→0.6 + vertical node layout
3. M30: Node graph save/load (`.bif`/`.bifa`) + evaluation modes + cache node
4. M31: Per-node scene graph visualization (click node → see tree at that point)
