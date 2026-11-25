# BIF Development Instructions

## Overview

BIF is a USD Orchestration Tool for VFX — layer-aware USD editing + procedural scene assembly + integrated rendering. Rust + wgpu + USD (C++ FFI) + MaterialX. Inspired by Katana's layer awareness and Houdini's procedural power. Scope: arrange, compose, override, instance. Not: model, rig, animate, simulate.

I know Go + Python/PyQt well; learning Rust + graphics. Help me learn, don't lecture.

- In all interaction and commit messages, be extremely concise — sacrifice grammar for brevity.

## Project Context

**BIF** - USD Orchestration Tool for VFX.

- **Status:** v0.16.9 shipped (2026-06-04) — first working Windows release CI; Qt is the UI (egui removed v0.15.0)
- **Current:** v0.17.0 Node Graph Parity (#22 node-graph audit, #26 framing, #33 pin labels). `CHANGELOG [Unreleased]` holds cpp_bridge split #13 + Qt node-graph parity.
- **Roadmap:** GitHub Milestones → https://github.com/byvfx/bif/milestones (v0.17.0 Node Graph Parity → v0.17.5 Viewport perf → v0.18.0 Scene Authoring → v0.19.0 MaterialX → v0.20.0 Volumes → v0.21.0 GPU PT → v0.22.0 API/Framework; AI = `backlog` until app is solid). GitHub is the source of truth.
- **Design:** [BIF_USD_WORKFLOW.md](BIF_USD_WORKFLOW.md) — procedural nodes + layer awareness

### Architecture

- **7 crates + benchmarks:** `bif_math` → `bif_core` → `bif_renderer` → `bif_viewport` → `bif_qt` → `bif_viewer`; + `bif_maketx` (CLI), `benchmarks` (bif_perf)
  - `bif_viewer` — thin `main.rs` only; no UI logic
  - `bif_qt` — all Qt UI (cxx-qt 0.7; panels in `crates/bif_qt/cpp/`)
- **USD bridge:** `bif_core/src/usd/ffi/` (9 submodules); `cpp_bridge.rs` is a `pub use ffi::*` shim
- **Node graph:** 10 node types in `bif_viewport` (UsdRead, Primitive, Scatter, PointInstancer, Xform, UsdExport, UsdPrim, GraftBranches, HdriEnvironment, IvarRender)
- **Materials:** OpenPBR Surface v1.1 (`OpenPbrSurface` in bif_renderer)
- **Renderer:** `Renderer` struct (~75 fields, God object — cleanup deferred)

## Quick Commands

```bash
cargo build                    # dev (~10s)
cargo build --features oiio    # OpenImageIO
cargo build --features oidn    # Intel OIDN denoising

cargo test -p bif_math         # 78 (no deps)
cargo test -p bif_renderer     # 122
cargo test -p bif_viewport     # 203 (needs USD env)
cargo test -p bif_qt
. .\setup_usd_env.ps1; cargo test -p bif_core -- --test-threads=1  # USD DLLs

cargo run -p bif_viewer
cargo run -p bif_viewer --features oiio,oidn

cargo clippy -- -D warnings
cargo fmt --check
```

## Gotchas

- **USD env required:** source `setup_usd_env.ps1` before bif_core **or bif_viewport** tests / loading USD scenes (bif_viewport transitively links USD DLLs → `STATUS_DLL_NOT_FOUND` otherwise)
- **bif_core tests single-threaded:** USD C++ bridge not thread-safe → `--test-threads=1`
- **C++ bridge via CMake:** `bif_core/build.rs` → `cpp/usd_bridge/`; needs VS 2022 C++ tools
- **CMakeCache bakes source path** — breaks on repo move / agent worktree. Fix:
  ```powershell
  Get-ChildItem $env:CARGO_TARGET_DIR -Recurse -Directory -Filter usd_bridge_build | Remove-Item -Recurse -Force
  ```
  (target dir defaults to `C:/Users/brandon/.cargo-target` via `~/.cargo/config.toml`)
- **bif_qt needs Qt 6:** headers + `qmake`/`cmake` in PATH
- **UI logic lives in bif_qt, not bif_viewer** (`bif_viewer` = `main.rs` only)
- **OIDN:** set `OIDN_DIR` + add its `bin/` to PATH for `--features oidn`
- **`oiio`/`oidn` optional** — off by default, UI degrades gracefully
- **`test_should_restart_no_render`:** known flaky
- **USD debug tools** at `G:\__projects\_programming\usd_25_11\` (usdcat, usdchecker, usddumpcrate, usdtree; run `scripts\set_usd_env.bat`, or Python `from pxr import Usd, UsdGeom`)

## Knowledge Base

Durable knowledge lives in **Hindsight** (auto-maintains from git history + sessions). Query it before acting on architecture, decisions, or conventions. The old Obsidian wiki is archived at `docs/archive/wiki/`.

## USD Reference

- Curated docs: [`docs/usd/`](docs/usd/) — concepts, composition, schemas, datatypes, toolset, faq; `llm-reference.md` is the AI-optimized index
- Deep API: `https://openusd.org/release/api/` — `class_<lower_snake>.html` (e.g. `class_usd_stage.html`)

## Background

**Strong:** Go, Python/PyQt, graphics (raytracing, BVH, materials). **Learning:** Rust, wgpu, Qt C++, USD/MaterialX.

## How to work with me

- **Challenge me** — push back. "Do you need this now?" / "Consider X instead?" / "What's the real timeline?"
- **Explain trade-offs** — show a decision table when there's a real choice; recommend one.
- **Ask before solving** — what are you actually building? does it fit the milestone? prerequisites done?
- **Tone** — direct, constructive, pragmatic, encouraging
- **Good:** follow-up questions, challenging me, trying + reporting back. **Red flag:** "okay", scope creep.

## Code Standards

- Clear commit messages; never commit commented-out code, `dbg!`/`println!`, or secrets
- Rust: `rustfmt` + `clippy -D warnings`, zero warnings, exhaustive match (avoid `_`), `unsafe` documented, explicit `.clone()`
- Tests: `#[test]` + `#[cfg(test)]`, Arrange-Act-Assert, mock externals; USD tests source `setup_usd_env.ps1`
- Public items need doc comments

### Before committing

fmt · clippy · tests green. Then CHANGELOG `[Unreleased]` (per PR), devlog (per PR), HANDOFF/README/CLAUDE updates as needed.

## Workflow

- **Plans:** end each plan with unresolved questions.
- **Devlog:** per session, `devlog/YYYY-MM/DEVLOG_YYYY-MM-DD.md` (Goals / Did / Learnings / Next). Remind me.
- **GitHub ops** via `gh`.
- **Branch/PR:** features on `<type>/<desc>` → PR → squash-merge. Review the diff, not per commit. Trivial (typo/docs/bump) → straight to `main`.
- **Release** (from `main`): (1) bump `[workspace.package] version` + `cargo update --workspace`; (2) stamp CHANGELOG `[Unreleased]`→`[X.Y.Z] - DATE`; (3) annotated tag `vX.Y.Z` + push → CI builds zip + extracts notes; (4) smoke-run the zip locally.

---

Prioritize clarity and maintainability over cleverness.
