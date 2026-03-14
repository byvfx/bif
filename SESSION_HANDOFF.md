# Session Handoff - March 14, 2026

**Last Updated:** M19.6 bif_viewport structural cleanup complete
**Next Milestone:** Finish M29 USD Export, then M29.5 egui upgrade
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26 (OIDN), M26.1 (Ivar material cache), M19.6 (viewport cleanup) |
| Current | M29 USD Export (most phases done, needs validation) |
| Tests | 84 renderer, 41 math, 24 viewport, 27+ bif_core |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~47ms (was 6.7s) |

---

## Recent Work

### M19.6 bif_viewport Structural Cleanup (Mar 14, 2026)

Purely structural refactoring, no behavior changes:

- **render.rs:** 2,695-line `render()` split into 6 phase methods + 3 extracted helpers (2,764 → 2,100 lines)
- **render_ui.rs:** New file (763 lines) — extracted stats panel with `StatsPanelParams<'a>` struct
- **lib.rs:** 3 sub-structs (`AsyncChannels`, `UiLayout`, `SceneInstances`) replace 15 flat Renderer fields
- **node_graph.rs:** 2,272 lines split into `node_graph/` directory (mod.rs, viewer.rs, ops.rs)
- VFX code review: no correctness bugs, no critical issues

### Roadmap Overhaul (Mar 13, 2026)

Evaluated 7 feature areas against current architecture (~42K LOC). Key decisions:

- **Nodes stay** — BIF's node graph is operations (verbs), not scene hierarchy. Correct model.
- **New M29.5-M36+** — egui upgrade, persistence, per-node viz, opinion trace, USD debug, Python hooks, API cleanup, framework extraction
- **Old M31-M34** (lights, materials, contexts, MaterialX) → renumbered M37-M40

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
