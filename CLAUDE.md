# BIF Development Instructions

## Overview

BIF is a USD Orchestration Tool for VFX — layer-aware USD editing + procedural scene assembly + integrated rendering. Built in Rust with wgpu, USD (C++ FFI), and MaterialX. Inspired by Katana's layer awareness and Houdini's procedural power. Orchestration scope: arrange, compose, override, instance. Not: model, rig, animate, simulate. I have a background in Go and Python/PyQt, and I'm learning Rust and graphics programming. Help me learn effectively while building this.

- In all intercation and commit messages, be extremely consise and sacrifice grammar for brevity.

## Plans

- at the end of each plan, give me a list of unresoved questions to answer, if any and be extremely consise and sacrifice grammar for brevity.

## Project Context

**BIF** - USD Orchestration Tool for VFX (layer-aware editing + procedural assembly + rendering).

- **Status:** v0.16.9 shipped (2026-06-04) — first working Windows release CI (Qt + USD + OIIO + OIDN + Embree bundled); Qt is primary UI framework (egui removed v0.15.0)
- **Current:** between releases — `CHANGELOG.md [Unreleased]` empty; next work = v0.17.0 (issues #13–#15)
- **Roadmap:** GitHub Milestones + Issues → https://github.com/byvfx/bif/milestones (v0.17.0 Viewport perf → v0.18.0 Scene Authoring → v0.19.0 MaterialX → v0.20.0 Volumes → v0.21.0 GPU PT → v0.22.0 API/Framework; AI = `backlog` label until app is solid). GitHub is the roadmap source of truth; [MILESTONES.md](MILESTONES.md) keeps architecture + 1.0 criteria only.
- **Goal:** Layer-aware USD editor + scene assembler — open stage, pick layer, edit, save clean USD
- **Design:** [BIF_USD_WORKFLOW.md](BIF_USD_WORKFLOW.md) — hybrid approach (procedural nodes + layer awareness)
- **Timeline:** Side project, 10-20 hrs/week

### Key Architecture

- **8 crates:** bif_math, bif_core, bif_renderer, bif_viewport, bif_viewer, bif_qt, bif_maketx, benchmarks
  - `bif_viewer` — thin entry point only (`main.rs`); no UI logic lives here
  - `bif_qt` — all Qt UI (cxx-qt 0.7 Rust-C++ bridge; panels in `crates/bif_qt/cpp/`)
- **Qt panels** (`bif_qt/cpp/`): scene_browser, layer_stack, property_inspector, node_graph, render_settings, render_widget, usda_panel, command_palette, first_launch
- **Qt Rust** (`bif_qt/src/`): `main_window.rs`, `app.rs`, `viewport.rs`, `theme.rs`, `schema_labels.rs`
- **Procedural node graph:** 10 node types in `bif_viewport` (UsdRead, Primitive, Scatter, PointInstancer, Xform, UsdExport, UsdPrim, GraftBranches, HdriEnvironment, IvarRender); `node_graph_widget` is the editor UI for these
- **Scene browser:** CompositeProvider merges USD stage + procedural prims via CachedSceneGraph
- **Export:** `export_scene()` in `bif_core/src/usd/export.rs`
- **USD bridge:** `bif_core/src/usd/cpp_bridge.rs` (monolithic, split planned for v0.17.0)
- **Materials:** OpenPBR Surface v1.1 (`OpenPbrSurface` in bif_renderer, IOR-based Fresnel)
- **Renderer:** `Renderer` struct (~75 fields, God object — cleanup deferred)

## Quick Commands

```bash
# Build
cargo build                    # Dev (~10s)
cargo build --features oiio    # With OpenImageIO
cargo build --features oidn    # With Intel OIDN denoising

# Test
cargo test -p bif_math         # 74 tests (no deps)
cargo test -p bif_renderer     # 111 tests (includes denoise, materials)
cargo test -p bif_viewport     # 179 tests (needs USD env sourced — see Gotchas)
cargo test -p bif_qt           # Qt UI crate tests
. .\setup_usd_env.ps1          # Required before bif_core tests
cargo test -p bif_core -- --test-threads=1  # needs USD DLLs

# Run
cargo run -p bif_viewer
cargo run -p bif_viewer --features oiio,oidn  # With OpenImageIO + denoising

# Checks
cargo clippy -- -D warnings
cargo fmt --check
```

## Gotchas

- **USD env required:** `setup_usd_env.ps1` must be sourced before running bif_core **or bif_viewport** tests, or loading USD scenes. bif_viewport transitively links USD DLLs via bif_core, so its test binary fails with `STATUS_DLL_NOT_FOUND` without it (run via PowerShell: `. .\setup_usd_env.ps1; cargo test -p bif_viewport`)
- **bif_core tests are single-threaded:** USD C++ bridge is not thread-safe, use `--test-threads=1`
- **C++ bridge builds via CMake:** `bif_core/build.rs` triggers CMake for `cpp/usd_bridge/` — needs Visual Studio 2022 C++ workload
- **bif_qt requires Qt 6:** needs Qt 6 dev headers + `qmake`/`cmake` in PATH; cxx-qt 0.7 generates the Rust-C++ glue at build time
- **UI logic lives in bif_qt, not bif_viewer:** bif_viewer is just `main.rs` — look in `crates/bif_qt/` for all panel/widget code
- **OIDN DLLs must be in PATH:** Set `OIDN_DIR` and add its `bin/` to PATH for `--features oidn`
- **Feature flags are optional:** `oiio` and `oidn` are off by default, UI gracefully degrades without them
- **`test_should_restart_no_render`:** Known timing-sensitive flaky test
- **USD debug tools at `G:\__projects\_programming\usd_25_11\`:** usdcat, usdchecker, usddumpcrate, usdtree, etc. Run `scripts\set_usd_env.bat` first, or use Python with `from pxr import Usd, UsdGeom`. Custom `dump_usd.py` script for shader inspection also lives there.

## Related Docs

- [README.md](README.md) - setup, commands
- [docs/WORKFLOW.md](docs/WORKFLOW.md) - branch → PR → review → squash-merge + release quick reference
- [MILESTONES.md](MILESTONES.md) - architecture, roadmap
- [SESSION_HANDOFF.md](SESSION_HANDOFF.md) - current state

## Knowledge Base (Obsidian Wiki)

- **Location:** [`wiki/`](wiki/) — Obsidian vault, open as vault in Obsidian
- **Entry point:** `wiki/_index.md` — LLM reads this first to navigate
- **Sections:** architecture, usd, rendering, concepts, rust, ui-ux, journal, raw
- **42 articles** covering architecture decisions (ADRs), USD integration, rendering pipeline, atomic concept notes
- **Templates:** `wiki/templates/` — concept, adr, journal, article

### Wiki Maintenance

- After learning something non-obvious, create/update a wiki concept note
- After making an architecture decision, create an ADR in `wiki/architecture/adr/`
- Keep section `_index.md` files updated when adding articles
- Use Obsidian wikilinks `[[Article Name]]` for cross-references
- Frontmatter: every article needs title, type, tags, created, updated

## USD Reference

- **Local curated docs:** [`docs/usd/`](docs/usd/) — AI-optimized reference covering core concepts, composition, all major schemas, datatypes, toolset, and common gotchas
  - `concepts.md` — Stage, Layer, Prim, Property, value resolution, model hierarchy
  - `composition.md` — LIVRPS, sublayers, references, payloads, variants, inherits, specializes, edit targets
  - `schemas-geom.md` — UsdGeom (Mesh, Xformable, PointInstancer, Camera, primvars, stage metrics)
  - `schemas-shade.md` — UsdShade (Material, Shader, NodeGraph, connections, binding, render contexts)
  - `schemas-lux.md` — UsdLux (all light types, LightAPI, shadow/shaping, filters)
  - `sdf-foundations.md` — SdfLayer, SdfPath, PrimSpec, file format plugins, asset resolution
  - `datatypes.md` — All USD types with C++/Rust equivalents, roles, arrays
  - `toolset.md` — usdcat, usdview, usdedit, usdchecker, usdrecord, etc.
  - `preview-surface.md` — UsdPreviewSurface spec (all inputs, texture nodes, complete example)
  - `faq.md` — Common gotchas, pitfalls, format differences
- **Deep API lookups:** Fetch from `https://openusd.org/release/api/` when specific class/method details needed
- **Key API URLs for deep dives:**
  - `https://openusd.org/release/api/class_usd_stage.html`
  - `https://openusd.org/release/api/class_usd_prim.html`
  - `https://openusd.org/release/api/class_sdf_layer.html`
  - `https://openusd.org/release/api/class_sdf_path.html`
  - `https://openusd.org/release/api/class_usd_geom_mesh.html`
  - `https://openusd.org/release/api/class_usd_geom_point_instancer.html`
  - `https://openusd.org/release/api/class_usd_shade_material.html`
  - `https://openusd.org/release/api/class_usd_shade_shader.html`
  - `https://openusd.org/release/api/class_usd_geom_xformable.html`
  - `https://openusd.org/release/glossary.html`

## Technical Background

**Strong:** Go (2000+ line raytracer), Python/PyQt, graphics (raytracing, BVH, materials)

**Learning:** Rust (intermediate), wgpu, Qt C++, USD/MaterialX

## Interaction Style

### Challenge Me

Push back when appropriate:

- "Do you need this now or is it future work?"
- "Have you considered X instead?"
- "That's optimistic - real timeline is..."
- "Easier path: do Y instead of Z"

### Explain Trade-offs

Show decision table when relevant:

| Option | Pros | Cons | When to Use |
|--------|------|------|-------------|

Recommend one with rationale.

### Ask Before Solving

Before diving into code:

- What are you actually trying to accomplish?
- How does this fit your current milestone?
- Have you finished prerequisites?

### Tone

- **Direct** - Tell me when I'm wrong
- **Constructive** - Explain better approaches
- **Pragmatic** - Working > perfect
- **Encouraging** - Long project, keep momentum

### Success Indicators

**Good:** Asking follow-up questions, challenging suggestions, trying and reporting back

**Red flags:** Just saying "okay" (probably lost), scope-creeping (need refocus)

## Code Standards

### Version Control

- Write clear, descriptive commit messages
- Never commit commented-out code - delete it
- Never commit debug `println!` or `dbg!` macros
- Never commit credentials or sensitive data

### Rust Best Practices

**Tools:**

- Use `rustfmt` for formatting
- Use `clippy` for linting, follow its suggestions
- Ensure no warnings (`cargo build` clean)
- Use `cargo test`, `cargo doc`

**Idioms:**

- Avoid `unsafe` unless necessary; document safety invariants
- Call `.clone()` explicitly on non-Copy types
- Use exhaustive pattern matching; avoid catch-all `_` when possible
- Use `format!` for string formatting
- Prefer iterators over manual loops
- Use `enumerate()` over manual counters
- Prefer `if let` / `while let` for single-pattern matching

### Testing

- Write unit tests for new functions and types
- Mock external dependencies (APIs, files, databases)
- Use `#[test]` attribute and `cargo test`
- Follow Arrange-Act-Assert pattern
- Use `#[cfg(test)]` modules for test code
- Never commit commented-out tests
- Use the setup_usd_env.ps1 script for USD environment setup in tests

### Before Committing

- All tests pass (`cargo test`)
- No compiler warnings (`cargo build`)
- Clippy passes (`cargo clippy -- -D warnings`)
- Code formatted (`cargo fmt --check`)
- Update CHANGELOG.md `## [Unreleased]` section (per-PR when on a branch)
- Make new devlog entry (per-PR when on a branch)
- Update SESSION_HANDOFF.md if needed
- Update MILESTONES.md if needed
- update README.md if needed
- update CLAUDE.md if needed
- Public items have doc comments
- No commented-out code or debug statements

## Don'ts

- Dump code without explanation
- Assume I know Rust idioms
- Over-engineer solutions
- Skip validation steps

## Workflow

### Plans

At end of each plan, list unresolved questions (if any).

### Daily Development Log

At end of each session, create/update `devlog/YYYY-MM/DEVLOG_YYYY-MM-DD.md`:

```markdown
# Development Log - YYYY-MM-DD

## Session Duration
[e.g., 2.5 hours]

## Goals
- What I planned to accomplish

## What I Did
- Changes made, files modified
- Key decisions and why
- Problems and solutions

## Learnings
- New concepts, architecture insights, mistakes

## Next Session
- Immediate next steps
- Blockers/questions
```

Also update `SESSION_HANDOFF.md` with summary, next steps, blockers.

### Github

Use Github CLI (`gh`) for all Github operations.

Remind me to create devlog at end of each session.

### Branching & Review

Real work happens on **feature branches → PR → squash-merge to `main`**. Review is done on the PR diff, NOT per commit.

- **Branch for features/bugfixes:** `git switch -c <type>/<short-desc>` (e.g. `feat/node-outputs-merge`, `fix/payload-policy`). Trivial commits (typo, version bump, docs/CHANGELOG/devlog-only) may go straight to `main`.
- **Cadence:**
  - *Per commit* (on the branch): the `Before Committing` gate — fmt + clippy + tests green, no warnings.
  - *Per PR* (once for the branch): CHANGELOG `[Unreleased]`, devlog, SESSION_HANDOFF / MILESTONES / README updates.
- **Open the PR:** `gh pr create`. CI `check` runs and `claude-code-review.yml` auto-reviews the diff.
- **Run `/vfx-code-reviewer` on the PR diff** for VFX-production-pattern review before merging. Reserve `/code-review ultra` for large/risky PRs (e.g. `cpp_bridge` split, renderer changes).
- **Address review**, push fixes (auto-review re-runs on each push).
- **Squash-merge** when green + reviewed: `gh pr merge --squash --delete-branch`. Keeps `main` history linear.
- Release tags are cut from `main` after merge — see `Releases` below.

The PR is the review unit — don't review per commit.

### Releases

**GitHub Release notes come from `CHANGELOG.md` automatically — never hand-write them.** The `Release` job in `.github/workflows/ci.yml` (tag-only, `refs/tags/v*`) builds the Windows artifact and extracts the changelog section matching the tag version as the release body.

Ship `vX.Y.Z`:

1. **During dev:** add entries under `## [Unreleased]` in `CHANGELOG.md`.
2. **Release prep (one commit):**
   - bump `[workspace.package] version` in `Cargo.toml`, then `cargo update --workspace` (refresh `Cargo.lock`)
   - stamp CHANGELOG: `## [Unreleased]` → `## [X.Y.Z] - YYYY-MM-DD`, leave a fresh empty `## [Unreleased]` above it
   - commit `release: vX.Y.Z`, push `main`
3. **Tag:** annotated `git tag -a vX.Y.Z -m ...`, then `git push origin vX.Y.Z` → triggers the Release job.
4. CI matches `## [X.Y.Z]` (the header may carry a ` - DATE` suffix) and publishes those notes + `bif-windows-x64.zip`.

CI can't launch the exe (headless) — smoke-run the published zip locally before announcing.

---

Prioritize clarity and maintainability over cleverness.
