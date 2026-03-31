# Session Handoff - March 30, 2026

**Last Updated:** GitHub Pages site setup (mdBook)
**Current Version:** v0.13.0-dev (Pipeline Foundation)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Released | v0.1.0, v0.11.0, v0.12.0 |
| Current | v0.13.0-dev — M29.5, M30, M31 complete, architecture refactors in progress |
| Next | Phase 1 complete (FFI split), Phase 2 (Linux), then v0.14.0 (layer-aware stage) |
| Tests | 400+ total across all crates (44 new ffi_convert tests) |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms |

---

## Recent Work

### UI/UX Design Brainstorm (Mar 30, 2026)

- Designed viewport-dominant T-layout, layer color coding system, opinion stack, command palette
- Full design: `docs/ux/UI_DESIGN.md` | Research: `docs/ux/DCC_UI_RESEARCH.md`
- Updated MILESTONES.md + ROADMAP_DETAIL.md with UI features threaded into v0.14–v0.16
- Material editor designed: param sheet + node graph + floating lookdev orb ([design](docs/ux/MATERIAL_EDITOR_DESIGN.md))

### Power-Weighted Light Sampling (Mar 30, 2026)

- Replaced uniform 1/N light selection with power-weighted CDF in `LightList`
- Added `power()` to `Light` trait (DistantLight, SphereLight, RectLight)
- Foundation for hierarchical light tree (v0.20.0) and env map visibility cache (v0.22.0)
- Scoped two Octane-inspired features: many-light sampling + env visibility cache

### SHARC Cache + Max Depth UI (Mar 30, 2026)

- SHARC radiance cache now skips low-roughness surfaces (< 0.1) via new `Material::roughness()` trait — fixes blurred reflections on glossy/mirror materials
- Added Max Depth slider (1–32) to interactive Ivar render panel
- Roadmap trimmed: removed AI Integration version, renumbered

### GitHub Pages Site (Mar 30, 2026)

Set up mdBook-based site with auto-deployed dev diary (85 entries) + manual (USD reference, getting started, architecture, changelog). `scripts/generate-site.sh` auto-generates SUMMARY.md from devlog tree. GitHub Actions deploys on push. **Action needed:** enable Pages source = "GitHub Actions" in repo settings.

### PointInstancer Loading Fixes (Mar 30, 2026)

Fixed two bugs preventing time-sampled PointInstancer files (e.g., Pixar's PointInstancedMedCity.usd) from loading:
1. C++ bridge now uses stage startTimeCode / first sample instead of Default when reading instancer attrs
2. Rust loader maps parent Xform paths in prototype_map for instancer prototype resolution

Test file: `assets/PointInstancedMedCity.usd` (40K instances, 8 prototypes)

### Architecture Deepening: Phase 1 FFI Bridge Split (Mar 28-29, 2026)

Split monolithic `cpp_bridge.rs` (4,542 LOC) into 3 modules:
- `ffi_raw.rs` (898 lines) — `#[repr(C)]` types + `extern "C"` block
- `ffi_convert.rs` (2,054 lines) — 17 conversion functions + 44 tests (no C++ DLLs needed)
- `cpp_bridge.rs` slimmed to 3,651 lines (-20%)

Also created `ARCHITECTURE_REFACTORS.md` (5-phase plan) and `BIF_USD_WORKFLOW.md` (layer-aware editor spec).

**Next:** Wire UsdStage methods to delegate to ffi_convert (incremental), then Phase 2 (Linux cross-platform).

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
