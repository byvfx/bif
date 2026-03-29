# Session Handoff - March 27, 2026

**Last Updated:** Documentation overhaul — semver milestones, README rewrite
**Current Version:** v0.13.0-dev (Pipeline Foundation)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Released | v0.1.0, v0.11.0, v0.12.0 |
| Current | v0.13.0-dev — M29.5, M30, M31 complete, shipping current work |
| Next | v0.14.0 (USD debugging), then v0.15.0 (Qt migration) |
| Tests | 400+ total across all crates |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms |

---

## Recent Work

### Documentation Overhaul (Mar 27, 2026)

Reworked project documentation to correlate milestones with semantic versioning:
- **MILESTONES.md** — rewritten as lean semver roadmap (v0.13.0 through v0.23.0+)
- **MILESTONES_HISTORY.md** — new file, all completed milestones (M0-M31) moved here
- **ROADMAP_DETAIL.md** — new file, per-version task lists + acceptance criteria
- **README.md** — full rewrite, new positioning ("lightweight scene assembly"), updated stats
- **CHANGELOG.md** — targeting v0.13.0 note
- **Cargo.toml** — version bumped to 0.13.0-dev
- **bif-commit skill** — updated for new file structure
- **vfx-code-reviewer agent** — added version scope awareness

Key decisions informed by software architect + engineer reviews:
- Qt migration (v0.15.0) promoted before context system — avoids building UI twice
- M22/M25/M27 no longer deferred — all scheduled in roadmap
- 1.0 criteria defined (10 gates)

### M30 Complete (Mar 24-26, 2026)

All 6 phases landed: serde foundation, ProjectFile persistence, file menu + save/load UI, eval modes (Auto/Manual/OnMouseRelease), cache node with bypass toggle.

### M31 Complete (Mar 26, 2026)

Per-node scene graph visualization — source node tagging, prim count badges, filtered provider.

### M29.5 Complete (Mar 23, 2026)

egui UI overhaul — centralized theme, panel restructure, property inspector, menu bar, Unicode icons.

---

## Next Steps

1. **v0.13.0 release** — final validation, version bump, ship current work
2. **v0.14.0 planning** — USD composition inspector + opinion trace (M32, M33)
3. **v0.15.0 research** — Qt 6 Rust bindings evaluation (cxx, ritual, qt-build-utils)
