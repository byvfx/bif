# BIF Development Instructions

## Overview

You Create a new DCC that is inspired by Clarisse / Houdini, focused on VFX scene assembly and rendering using Rust, wgpu, USD, and MaterialX. I have a background in Go and Python/PyQt, and I'm learning Rust and graphics programming. I want you to help me learn effectively while building this project.

- In all intercation and commit messages, be extremely consise and sacrifice grammar for brevity.

## Plans

- at the end of each plan, give me a list of unresoved questions to answer, if any and be extremely consise and sacrifice grammar for brevity.

## Project Context

**BIF** - VFX scene assembler/renderer (like Clarisse/Houdini).

- **Status:** v0.12.0 released (USD export, OpenPBR, subsystem extraction). v0.13.0 in progress.
- **Current:** v0.13.0 — Pipeline Foundation (M29.5 UI overhaul, M30 persistence, M31 per-node viz)
- **Next:** v0.14.0 (layer-aware stage) → v0.15.0 (Qt migration) → v0.16.0 (edit ops + save) — see [MILESTONES.md](MILESTONES.md)
- **Goal:** Layer-aware USD editor + scene assembler — open stage, pick layer, edit, save clean USD
- **Design:** [BIF_USD_WORKFLOW.md](BIF_USD_WORKFLOW.md) — hybrid approach (procedural nodes + layer awareness)
- **Timeline:** Side project, 10-20 hrs/week

### Key Architecture

- **6 crates:** bif_math, bif_core, bif_renderer, bif_viewport, bif_viewer, bif_maketx
- **Node graph:** egui-snarl, 10 node types (UsdRead, Primitive, Scatter, PointInstancer, Xform, UsdExport, UsdPrim, GraftBranches, HdriEnvironment, IvarRender)
- **Scene browser:** CompositeProvider merges USD stage + procedural prims via CachedSceneGraph
- **Export:** `export_scene()` in `bif_core/src/usd/export.rs`
- **Materials:** OpenPBR Surface v1.1 (`OpenPbrSurface` in bif_renderer, IOR-based Fresnel)
- **Renderer:** `Renderer` struct (~75 fields, God object — cleanup deferred)
- **516 tests** across crates (353+ without USD env, full suite needs `setup_usd_env.ps1`)

## Quick Commands

```bash
# Build
cargo build                    # Dev (~10s)
cargo build --features oiio    # With OpenImageIO
cargo build --features oidn    # With Intel OIDN denoising

# Test
cargo test -p bif_math         # 74 tests (no deps)
cargo test -p bif_renderer     # 111 tests (includes denoise, materials)
cargo test -p bif_viewport     # 149 tests
. .\setup_usd_env.ps1          # Required before bif_core tests
cargo test -p bif_core -- --test-threads=1  # 163 tests (needs USD DLLs)

# Run
cargo run -p bif_viewer
cargo run -p bif_viewer --features oidn  # With denoising

# Checks
cargo clippy -- -D warnings
cargo fmt --check
```

## Gotchas

- **USD env required:** `setup_usd_env.ps1` must be sourced before running bif_core tests or loading USD scenes
- **bif_core tests are single-threaded:** USD C++ bridge is not thread-safe, use `--test-threads=1`
- **C++ bridge builds via CMake:** `bif_core/build.rs` triggers CMake for `cpp/usd_bridge/` — needs Visual Studio 2022 C++ workload
- **OIDN DLLs must be in PATH:** Set `OIDN_DIR` and add its `bin/` to PATH for `--features oidn`
- **Feature flags are optional:** `oiio` and `oidn` are off by default, UI gracefully degrades without them
- **`test_should_restart_no_render`:** Known timing-sensitive flaky test
- **USD debug tools at `D:\__projects\_programming\usd_25_11\`:** usdcat, usdchecker, usddumpcrate, usdtree, etc. Run `scripts\set_usd_env.bat` first, or use Python with `from pxr import Usd, UsdGeom`. Custom `dump_usd.py` script for shader inspection also lives there.

## Related Docs

- [README.md](README.md) - setup, commands
- [MILESTONES.md](MILESTONES.md) - architecture, roadmap
- [SESSION_HANDOFF.md](SESSION_HANDOFF.md) - current state

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
- Update CHANGELOG.md `## [Unreleased]` section
- Make new devlog entry
- Update SESSION_HANDOFF.md if needed
- Update MILESTONES.md if needed
- update README.md if needed
- update CLAUDE.md if needed
- Public items have doc comments
- No commented-out code or debug statements

## After Committing

- Clear context and run `/vfx-code-reviewer` skill on the commit (reviews for VFX production patterns)

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

---

Prioritize clarity and maintainability over cleverness.
