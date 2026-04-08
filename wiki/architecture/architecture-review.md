---
title: "Architecture Review"
type: article
tags: [architecture, refactoring, planning]
created: "2026-04-07"
updated: "2026-04-07"
---

## Summary

Periodic audit of `ARCHITECTURE_REVIEW.md` (8 recommendations) and `ARCHITECTURE_REFACTORS.md` (5 phases) against the codebase. Tracks what's been implemented and what remains.

## Status (Apr 7, 2026)

### ARCHITECTURE_REVIEW.md — 7/8 Done

| # | Item | Status |
|---|------|--------|
| 1 | Renderer sub-structs | Done (99->53 fields) |
| 2 | UsdStage Sync fix | Done (Arc<Mutex>) |
| 3 | Pure logic from scene_loader | Done (scene_pipeline.rs) |
| 4 | Node graph extension checklist | Not done |
| 5 | bif_math re-exports cleanup | Done |
| 6 | SceneQuery API | Done (trait + 9 tests) |
| 7 | DenoiseError thiserror | Done |
| 8 | Graph eval pass | Done (eval.rs) |

### ARCHITECTURE_REFACTORS.md — 5/5 Phases Done or Substantially Complete

| Phase | Status |
|-------|--------|
| 1: FFI bridge split | Complete (44 tests) |
| 2: Linux foundation | Partial (CI job + build.rs done, setup_usd_env.sh done) |
| 3: Node eval engine | Complete |
| 4: Scene pipeline | Complete |
| 5: Renderer dispatch | Complete (4/4 dispatch files) |

## Key Metrics

- Node types: 14 (approaching 15+ trait-refactor threshold)
- Renderer fields: 53 (down from 99)
- render.rs dispatch: ~50 lines (down from ~340)

## See Also

- `ARCHITECTURE_REVIEW.md` — original recommendations
- `ARCHITECTURE_REFACTORS.md` — phase definitions
- [[Event Dispatch Pattern]] — dispatch split details
