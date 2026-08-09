# Session Handoff

> Active sessions: last 5. Older entries → [SESSION_HANDOFF_ARCHIVE.md](docs/archive/SESSION_HANDOFF_ARCHIVE.md)

## Current State (2026-08-06)

- **Repo location: `G:\__projects\_programming\rust\bif` (server, SMB `\\freya\D_rename`) — settled.** A brief local clone at `C:\...` on 2026-08-06 was a wrong turn (the server requirement wasn't known then); it's been removed. **The repo lives on the server. Don't relocate it to local disk.**
- **✅ SMB CORRUPTION — ROOT-CAUSED AND FIXED (2026-08-07).** Cause was **SMB client metadata caching**. Windows defaults were `DirectoryCacheLifetime 10 / FileInfoCacheLifetime 10 / FileNotFoundCacheLifetime 5` (seconds); git would stat a path, get a cached answer that no longer matched the server, and act on it. Fixed by setting all three to `0` (elevated PowerShell):
  ```powershell
  Set-SmbClientConfiguration -DirectoryCacheLifetime 0 -FileInfoCacheLifetime 0 -FileNotFoundCacheLifetime 0 -Force
  ```
  **Proof it was the cause:** the identical `git reset --hard origin/main` failed twice before the change (`unable to create file CLAUDE.md: File exists` — a stale *FileNotFound* hit) and succeeded immediately after, writing 558 files with zero errors. Machine-wide setting; cost is metadata latency on all shares, not throughput. Revert with `10 / 10 / 5`.
  - **If corruption ever returns, it is NOT these settings** — next suspects are SMB oplocks/leasing or something server-side on `freya`. Stop and diagnose; each failed attempt leaves the tree half-applied.
  - Defender exclusion was investigated and is **not** relevant — Defender is fully disabled on this box (`RealTimeProtectionEnabled: False`).
  - Symptoms it caused, for recognition: `git switch` silently *deleting* tracked files; `git rebase` reverting files mid-flight; an undeletable broken ref making `git fetch` fatal; `Test-Path`/`Get-Item` reporting deleted files as still present while `git status` correctly showed `D`. **The object database was never damaged** — `git fsck` stayed clean throughout, which is why the checkout was repairable in place instead of needing a re-clone.
- **Post-fix verification on `G:`:** `git reset --hard` clean (558 files) · full workspace build 1m15s incl. USD bridge · bif_core 257, bif_viewport 203, bif_renderer 122, bif_math 76, bif_qt 15 all green · fmt + CI clippy clean · `git status` and `git fsck` clean after all that I/O.
- **Gotcha the move exposed:** CMakeCache bakes the source path and lives in the *shared* cargo target dir, so **any** path change (relocation *or* a `.claude/worktrees/agent-*` worktree) fails `bif_core`'s build script until the stale `usd_bridge_build` dirs are deleted. Documented in CLAUDE.md gotchas with the one-liner fix.
- **Keep the cargo target dir local** (`C:/Users/brandon/.cargo-target`, set globally in `~/.cargo/config.toml`) — SMB is slow for incremental build artifacts. This is correct as-is; don't move it to the share.
- **Also on `G:`:** `setup_usd_env.ps1` hardcodes `G:\__projects\_programming\vcpkg` (USD) + the OIDN root — consistent with the repo living there.
- **Branch:** `main` at `5769bc8`. **#29 + #32 both MERGED**; `claude-code-review.yml` deleted (no `ANTHROPIC_API_KEY` secret — it failed 5/5 PRs since 08-01; Copilot reviews PRs now).
- **CI note:** a force-push to an open PR does **not** reliably fire `pull_request: synchronize` — #29 sat with zero Actions runs until a close/reopen. Not a billing/minutes problem (that was a wrong theory); Actions works fine.
- **Focus:** v0.17.0 — the parked HDRI intensity/rotation bug (#22 parity family).
- **RESOLVED — it was ONE bug, not two. Smoke test passed; nothing to file.**
  1. **Live update never existed in the Qt port.** Spinboxes (`node_graph_widget.cpp:106-114`) had **no `valueChanged` connects** and there was **no bridge fn** for a param-only update. egui emitted `NodeGraphEvent::UpdateHdriParams` (`property_inspector.rs:1818`); the Qt port never got a counterpart. Backend handler (`node_dispatch.rs:118-133`) was correct all along. **This was the whole defect. FIXED.**
  2. **Apply was never broken.** Its chain is correct end to end — panel → bridge → `snarl_load_hdri` → `LoadHdri` → `start_hdri_load` (bakes `to_radians()`+intensity) → `apply_ibl_result` sets GPU params **and** `ivar_state.hdri_rotation/intensity` (`render.rs:373-375`); shaders consume both (`skybox.wgsl:74-75`, `basic.wgsl:368-380`); CPU uses `config.hdri_rotation.unwrap_or(env.rotation())` (`renderer.rs:162-163`). The 07-31 "Apply also fails" note was a misread — with no live update *and* a silent `unwrap_or(false)` bridge, a working Apply and a dead one looked identical. **Smoke test confirms both paths work now.**
- **Bridge feedback kept as hardening, not a bugfix.** `report_node_bridge_failure` is what would have made this a one-session diagnosis; it changes nothing on the success path.
- **Shipped on the branch:** `report_node_bridge_failure` extending PR #29's three-way pattern to `load_hdri`/`set_xform_params`/`set_usd_read_path`; full live-update path (`snarl_set_hdri_params` → `Renderer::node_graph_set_hdri_params` → `on_node_graph_update_hdri_params` → `valueChanged` connects, `QSignalBlocker` on repopulate); +5 headless `TestHarness` tests. Gate green: fmt · CI's clippy invocation · bif_qt clippy · 203/15/76/122 tests · `bif_viewer` links.
- **Units:** rotation is **degrees** in node/panel/event; the single `.to_radians()` is in the `UpdateHdriParams` handler. Never call `update_environment_params` (radians) from the Qt side — silent 57× error.
- **⚠ BLOCKER — GitHub Actions isn't scheduling.** PR **#29** rebased onto main (`23f3685`, clean, diff unchanged) and force-pushed, but **zero** Actions runs fired for the new SHA (API `total_count=0`) while external GitGuardian ran fine. Repo is **private** → Actions minutes billed, `windows-latest` = 2× multiplier. Looks like an exhausted minutes/spending cap. Unconfirmed: `gh` token lacks the `user` scope for the billing API — run `gh auth refresh -h github.com -s user` to check. **#29 can't merge green until resolved.**
- **⚠ SMB hazard.** `git switch` on the `G:` share failed a write mid-checkout and **deleted** `bif_core/src/hdr.rs` + `bif_viewport/src/gizmo.rs`; `Test-Path`/`Get-Item` still reported them present (stale metadata cache) while `git status` correctly said `D`. Recovered via `git checkout --`; tree verified against `origin/main`. **Trust `git status` over `Test-Path` here.** The #29 rebase was done in a `git worktree` on local `C:` (still present at `…/scratchpad/pr29` — remove after #29 lands).
- **Server migration (`D:` → `G:` = `\\freya\D_rename`) audit: CLEAN.** No script, config, or source references a `D:` path. Only hits are platform-gated test constants in `persistence.rs:538-546`, historical devlog/CHANGELOG text, and dead `_deprecated/` logs — all correct as-is, don't rewrite. Unrelated: `assets/lucy/geo/lucy_900.usd` fails `usdchecker` on an unresolvable `C:/Users/brandon/thumbnail.png` + missing `defaultPrim` (Houdini-authored, cosmetic, not filed).
- **Next:** (1) **Unblock Actions** — blocks *both* this branch and #29 (no CI, no auto-review). (2) Land #29, then PR + land this branch. (3) Untouched queue: #28 (PointInstancer crash) → #26 (framing) → walk the 5 remaining #22 nodes → close #22.

## Current State (2026-07-31)

- **Branch:** `main` (`bd77a42`). **#27 MERGED** (PR #30, closed #27) — HDR loader normalizes any valid `#?<program>` Radiance signature to `#?RADIANCE` before decode (`bif_core/src/hdr.rs`); +3 tests; smoke-confirmed on a real `#?RGBE` 8k HDRI. **Also merged #31** — a *pre-existing* Windows-CI clippy break (new stable `rustc` promotes the `f32: From<f64>` type-fallback future-incompat lint to a hard error under `-D warnings`; 11 sites in `bif_viewport`). It was reddening **every** PR (main latently red); fixed with explicit `f32` typing. Landed clippy PR first, then rebased #30 onto it.
- **Version:** **v0.16.9** tagged; `CHANGELOG [Unreleased]` now carries two `### Fixed` entries (#27 HDR `#?RGBE`, #31 clippy).
- **Toolchain note:** local `rustc 1.92.0` predates the `f32`-fallback lint, so local clippy can't reproduce CI's newer-stable failures. Consider pinning via `rust-toolchain.toml` (deferred).
- **Focus:** v0.17.0 — Node Graph Parity. Ran the #22 node-graph audit on the Windows box (human-in-the-loop).
- **Audit result — dogfood loop RESTORED ✅:** `Cube → ScatterPoints (1000 pts) → PointInstancer (1000 instances) → HdriEnvironment (8k RADIANCE HDRI + importance sampling) → IvarRender (16 SPP / 1.34s)`, end-to-end. Backend intact; parity gaps are Qt param coverage (as the 2026-07-20 exploration predicted).
- **Findings filed (all v0.17.0):** #26 framing broken (`on_frame_selected` stub, no Frame All) · #27 HDRI decoder rejects valid `#?RGBE` signature (`bif_core/src/hdr.rs:65`; fix documented) · #28 crash (`STATUS_ACCESS_VIOLATION`) on empty/mis-wired PointInstancer.
- **#22 still OPEN** — 5 node types untested: UsdRead, Xform, UsdExport, UsdPrim, GraftBranches. Results table posted as a #22 comment.
- **NEW BUG (separate, UNFILED) — HDRI intensity/rotation don't apply.** Found dogfooding after #27. Full flow traced (Qt panel → cxx-qt → bif_viewport → renderers). Two problems: (1) Qt spinboxes (`node_graph_widget.cpp:106-114`) have **no `valueChanged` wiring** → no live update (egui ref `property_inspector.rs:1817-1823` emitted `NodeGraphEvent::UpdateHdriParams`; the Qt port lacks it *and* a lightweight bridge fn). (2) User confirms **Apply also fails** — yet `environment_manager::start_hdri_load` (no path-dedup) reloads and bakes `rotation.to_radians()`/intensity correctly, so a **second break sits upstream or in a specific consumer**. Two independent param paths: GPU viewport env (`environment_manager`→`gpu_env.params`) vs CPU IvarRender (`ivar_state.hdri_rotation/intensity`→renderer config). **OPEN Q before filing:** is the unchanged image the viewport, the IvarRender result, or both? (localizes which path is broken).
- **Next:** (1) get the viewport-vs-IvarRender answer → localize → file the intensity/rotation issue (#22 parity family) → fix branch. (2) fix PRs **#28** (PointInstancer crash) → **#26** (framing). (3) walk remaining 5 nodes → close #22; add headless `bif_viewport` node-graph population tests (`working_scene` asserts).
- **Gotcha:** handoff's HDRI path `E:\...` is the *other* machine's drive — this box uses `Z:\_HDRIs\HDRI_Haven\`. Gate needs BOTH `setup_usd_env.ps1` + `setup_qt_env.ps1` (bif_qt `QtMissing` otherwise).

---

# Session Handoff — 2026-06-29 (PRs #23/#24 merged)

- **Branch:** `main` — PRs #23 and #24 merged. Local working tree is 1 commit behind `origin/main` due to Windows file lock on `crates/bif_core/src/usd/ffi/` during branch switch. Run `git reset --hard origin/main` after closing Rust Analyzer.
- **Version:** **v0.16.9** tagged (2026-06-04). `[Unreleased]` = issue #6 phases 1–3b + Qt node graph usability + cpp_bridge split (#13).
- **Status:** Two PRs landed this sprint — #23 (Qt node graph basic usability, 198 bif_viewport tests) and #24 (cpp_bridge split into usd/ffi/ submodules, 254 bif_core tests, closes #13). Code review on #24 surfaced 6 findings; all fixed before merge.
- **Next:** v0.17.0 issues #14 (viewport perf — defer GPU upload for invisible prototypes, LRU cache, `PrototypeState` enum) and #15 (payload policy — `PayloadPolicy::Manual`).
- **Gotcha:** `cargo clippy` on full workspace needs both `. .\setup_usd_env.ps1` AND `. .\setup_qt_env.ps1` sourced (bif_qt fails with `QtMissing` otherwise).

---

# Session Handoff — 2026-06-09 (issue #6 Phase 3b — `register_prims` + RFC close)

**Last Updated:** 2026-06-09 on `refactor/issue6-phase3b-register-prims` (commit `1e3752b`, off `main`).

**Context:** Final slice of the issue #6 RFC. Decomposed Phase 3 into 3a/3b/3c; did 3a + 3b; assessed 3c as not-fitting and skipped it. Plan: [`docs/agent-plans/2026-06-09-issue6-phase3b-register-prims.md`](docs/agent-plans/2026-06-09-issue6-phase3b-register-prims.md).

**Done:** `scene_browser::build_scene_graph_cache`'s lone per-variant arm (`UsdPrim` authored-prim registration) → `SceneNode::register_prims(&self, id, &mut ProcPrimSink)` in `behavior.rs`. `ProcPrimSink` (in `scene_browser`) wraps the prim-cache map. +1 pure test. Implemented inline.

**Key finding:** `build_scene_graph_cache` is scene-driven (proto/cloud prims from `working_scene`, tagged via `node_outputs` reverse maps) — only `UsdPrim` was per-variant. So 3b was deliberately tiny.

**3c skipped (RFC done):** all 21 `node_dispatch` arms traced — after Phase 2's `SceneCmd`, the residual is irreducible `Renderer` orchestration (loaders/file-IO/`reload`/GPU/flags). `apply() -> Vec<SceneCmd>` captures ~nothing new. Issue #6 substantively complete with 1/2/3a/3b.

**Validation:** build 0, clippy 0 (`-D warnings`), fmt 0, **188 bif_viewport tests** (187+1). Grep: no `match node` left in `scene_browser`. A verify-step caught that `usd_prim()` defaults `prim_path` non-empty (fixed the empty-case test).

**Next:** PR → `/vfx-code-reviewer` → squash-merge → close issue #6. Then v0.17.0.

---

# Session Handoff — 2026-06-09 (issue #6 Phase 3a — `SceneNode::evaluate`)

**Last Updated:** 2026-06-09 on `refactor/issue6-phase3a-evaluate` (commit `0220991`, stacked on PR #9).

**Context:** Phase 3 of the RFC, decomposed into 3a/3b/3c. Did 3a (`evaluate`). Plan: [`docs/agent-plans/2026-06-09-issue6-phase3a-evaluate.md`](docs/agent-plans/2026-06-09-issue6-phase3a-evaluate.md).

**Done:** `eval.rs::evaluate_node`'s per-variant `match` → `SceneNode::evaluate(&mut self, id, &EvalCtx, EvalMode) -> EvalOutcome` in new `node_graph/behavior.rs`; `evaluate_node` now a generic read/mutate-phase delegator. `extract_scatter_params` inlined + deleted. +6 pure `evaluate` tests.

**Design:** `EvalCtx` is precomputed snarl-free connection data (not a `&snarl` borrow) — required so `evaluate(&mut self)` can borrow the node out of the snarl while connection facts arrive via `ctx`. Payoff: `evaluate` is graph-pure + Snarl/GPU/USD-free testable. `&mut self` (node flips own flags) keeps ONE per-variant match.

**Validation:** build 0, clippy 0 (`-D warnings`), **187 bif_viewport tests** (181+6). Diff reviewed vs original = 1:1 behavior-preserving. Grep: no `extract_scatter_params`; one `fn evaluate_node` (the delegator).

**Gotcha for next time:** implementer subagents can't source the PowerShell USD env (`. .\setup_usd_env.ps1`) from their Bash tool, so they can't run `cargo test -p bif_viewport`. Tell them to stop at `cargo build`+`cargo clippy` (no env needed) and let the controller run the USD test suite.

**Next:** merge PR #9 → open 3a PR → `/vfx-code-reviewer`. Then 3b (`register_prims`).

---

# Session Handoff — 2026-06-09 (issue #6 Phase 2 — `SceneCmd`)

**Last Updated:** 2026-06-09 on `refactor/issue6-phase2-scenecmd` (commit `6d7deca`).

**Context:** Phase 2 of the issue #6 node-deepening RFC. Brainstormed → plan ([`docs/agent-plans/2026-06-08-issue6-phase2-scenecmd.md`](docs/agent-plans/2026-06-08-issue6-phase2-scenecmd.md)) → subagent-driven execution.

**Done:**
1. **`scene_cmd.rs`** (`6c5d740`) — `SceneCmd` 6-verb enum (`RemoveNodeProtos`/`RecordProtos`/`RemoveNodeCloud`/`AddCloud`/`UploadPointPreview`/`MarkSceneGraphDirty`) + `Renderer::execute(&mut self, SceneCmd)`. `AddCloud` boxes its `PointCloud` (clippy `large_enum_variant`; good for phase-3 `Vec<SceneCmd>`).
2. **Routed 7 arms** (`6d7deca`) in `node_dispatch.rs` through `execute()`: LoadUsdFile, CreatePrimitive, ScatterPointsCompute, PointInstancerCompute, UsdPrimCreate, GraftBranchesCompute, DeleteNode. Net −74 lines.

**Design calls:** coupling-scoped (only the `node_outputs ↔ working_scene` proto/cloud bookkeeping; loaders/`reload`/`compact`/flags stay direct); no `Custom` (the "complex" arms are computation/loaders, not mutations; `ExportUsd` mutates no scene state); `execute() -> ()` infallible, per-cmd inline.

**Validation:** build 0, clippy 0 (`-D warnings`), fmt 0, **181 bif_viewport tests** pass. Grep guard: no `node_outputs.entry`/`add_point_cloud`/`remove_point_cloud` left in `node_dispatch.rs`. Diff reviewed vs original — behavior preserved (only two `log::info!` lines lost per-id detail). 14 non-bookkeeping arms untouched.

**Next:** open PR → `/vfx-code-reviewer` → squash-merge. Then Phase 3 (`behavior.rs`).

---

# Session Handoff — 2026-06-03 (release CI bring-up)

**Last Updated:** 2026-06-03 on `main` (commit `73337c1`).

**Context:** Started as a quick check on `oiio`/`oidn` feature defaults (left off — intentional) + a vcpkg 404 from Copilot. Turned into a full release-pipeline bring-up: the tagged-release job had **never** built end-to-end since the v0.15.0 Qt migration; it always died at vcpkg first, masking five downstream walls.

**Done (each verified via throwaway `v0.0.0-ci-test` tag, then cleaned up):**
1. vcpkg baseline `a42af01` → `d015e31` (2026.05.25) + `builtin-baseline` in `vcpkg.json` — fixes zlib 404.
2. Qt 6.8 via `jurplel/install-qt-action` — cxx-qt was `QtMissing`.
3. Added `usd` + `embree` to `vcpkg.json`/install.
4. Classic-mode vcpkg install into `$VCPKG_ROOT/installed` + `-DVCPKG_MANIFEST_MODE=OFF` on both bridge CMake configures (manifest mode landed pkgs in a GUID dir + hijacked sub-builds via repo `vcpkg.json`).
5. `build.rs` probes zlib import-lib name (`z.lib` new vcpkg vs `zlib.lib` old).
6. `windeployqt` + bulk-copy all vcpkg DLLs (hand-picked list dropped `embree4.dll`).
7. vcpkg binary caching (`x-gha`): one-time USD/OIIO source build reused.

**Validation:** release job green; artifact = 96-file / 207 MB zip, all runtime DLLs present (confirmed by download + inspect). fmt clean (pre-commit). Files: `.github/workflows/ci.yml`, `vcpkg.json`, `crates/bif_core/build.rs`.

**Gotcha for next time:** CI Release now depends on Qt 6.8 (`win64_msvc2022_64`) + vcpkg `usd`/`embree` + the `x-gha` cache. If a future release fails on a missing lib, first `Get-ChildItem $VCPKG_ROOT\installed\x64-windows\lib\*.lib` (the diagnostic listing in the install step) — vcpkg renames import libs across baselines.

---

# Session Handoff — 2026-06-02 (branch consolidation + v0.16.8 release)

**Last Updated:** 2026-06-02 on `main` (commit `20eb17a`, tag `v0.16.8`).

**Context:** `main` had drifted to the v0.16.6 tag while v0.16.7/.8 + RFC #5 sat on stacked unmerged branches. Executed [`docs/agent-handoffs/2026-05-29-branch-consolidation.md`](docs/agent-handoffs/2026-05-29-branch-consolidation.md) — pure integration + hygiene, no behavior changes.

**Done:**
1. **Squashed the dogfood tail** (`v0.16.8-dogfood-polish`, 4 commits) into one release commit `f519bcf`.
2. **Doc reconcile** `0f6f66f` — CHANGELOG `[Unreleased]` → `[0.16.8] - 2026-05-29`; full MILESTONES reconcile (Released rows v0.16.3/.5/.6/.7/.8, fixed Latest line, replaced stale v0.16.5 In-Progress block).
3. **Landed RFC #5** via `git rebase --onto main 8b140ed` (plain rebase conflicted on the already-squashed tail) → ff-merged. PathTracer changelog entry moved to `[Unreleased]` (`cde3ccc`) since v0.16.8 is tagged pre-#5.
4. **Skills** — cherry-picked the live `.claude/commands/` files from `skills-split-save-handoff` (`20eb17a`); dropped the deprecated `agents/` mirrors.
5. **Tagged + pushed** `v0.16.8`; pruned 15 local + 6 origin branches (left `convoy/*`/`gt/*` bots). Regenerated the mdBook site.

**Validation:** build 0, clippy 0, fmt 0, bif_renderer 119 + bif_viewport 177. `main..origin/main` = 0.

**Gotcha for next time:** the gate needs **both** `setup_usd_env.ps1` **and** `setup_qt_env.ps1` sourced — USD alone fails cxx-qt with `QtMissing`.

**Next:** Re-scoped #6 (`NodeOutputs` + routing) or v0.17.0 Context System.

---

# Session Handoff — 2026-05-28 (RFC #5 path-trace core deepening)

**Last Updated:** 2026-05-28 on `refactor/deepen-modules` (commit `546e6d4`).

**Context:** Whole-codebase architecture review (Ousterhout deep-module lens) → 6 deepening candidates → filed 2 RFCs ([#5](https://github.com/byvfx/bif/issues/5) path-trace core, [#6](https://github.com/byvfx/bif/issues/6) node-type). Both chose the pragmatic "common-caller" Design C.

**Done — RFC #5 (TDD, 2 commits):**
1. `3196226` — pure helpers `should_skip_cache` / `roulette_survival` extracted from `ray_color_with_aovs`; RR start named `DEFAULT_RR_START_BOUNCE`.
2. `546e6d4` — `PathTracer<'a>` deep module: owns scene+config, resolves HDRI/cache once in `new()`, single `trace()` entry, `sample_pixel`/`sample_pixel_color` hold SPP+filter loop. `ray_color_with_aovs`/`render_pixel`/`render_pixel_with_aovs` → thin wrappers (API unchanged). `bucket.rs` builds one tracer per bucket (per-frame construction). `rr_start_bounce` exposed via `with_rr_start_bounce`.

3. `cbdeda7` — NEE/MIS boundary tests via `PathTracer`: NEE illuminates a diffuse surface, shadow rays respect occlusion, area light exercises the MIS-weighted branch. (Caught two real semantics while writing: `DistantLight` angle>0 → cone pdf; `RectLight` one-sided emission.)

**Validation:** `bif_renderer` 119 + `bif_viewport` 177 tests pass; clippy clean; workspace builds; fmt clean.

**RFC #6 — paused.** Reading `node_dispatch.rs` showed the ~26 dispatch arms are imperative orchestrations (e.g. `CreatePrimitive` calls `load_primitive` which mutates + returns the id the "command" would need), so a global `SceneCmd` surface over-fits — it'd need `Custom(Box<dyn FnOnce>)`, defeating testability. Re-scope recorded on [issue #6](https://github.com/byvfx/bif/issues/6): do `NodeOutputs` (merge `node_proto_map` + `node_cloud_map`) + `CookNode`/node-routing consolidation; drop global `SceneCmd`. No #6 code written.

**Next:** Re-scoped #6 (NodeOutputs + routing). Push `refactor/deepen-modules` + open PR.

---

# Session Handoff — 2026-05-23/28 (v0.16.8 production crash chain)

**Last Updated:** 2026-05-28 on `v0.16.8-dogfood-polish` (commit `0d9f4c7`).

**Problem:** `STATUS_STACK_BUFFER_OVERRUN` crash loading `rt_010_base.usda` (production shot — heavy PointInstancer + large texture set). Five crash modes patched in cascade:

1. **OOM in instancer animation** — 24 GiB `Vec` allocation attempt. Added `AllocTooLarge` error variant + checked allocation guard with 4 GiB cap.
2. **OOM in combined mesh** — 16 GiB `Vec` allocation attempt. Pre-allocation guard in `mesh_data.rs`, cap at 1 GiB, fallback to multi-draw path.
3. **Device-lost panics on `Queue::submit`** — error scopes don't catch fatal panics across FFI boundary. Wrapped with `catch_unwind`.
4. **GPU health tracking** — process-global `GPU_UNHEALTHY` atomic checked at all render entry points to skip work after device-lost.
5. **Texture VRAM budget** — 1.5 GiB default cap with per-upload charge prediction; uploads beyond budget are skipped with a log warning.

**Result:** App no longer crashes. Loads `rt_010_base.usda` partially — VRAM budget exhausted at ~192/1650 textures (expected behavior; remaining textures logged as skipped).

**Validation:** Crash chain confirmed resolved on production shot. `cargo build` + clippy + fmt clean.

**Next:** Dynamic VRAM budget detection (query actual GPU VRAM vs. hardcoded 1.5 GiB default) deferred to v0.17. See `MILESTONES.md`.

---

# Session Handoff — 2026-05-16 (v0.16.7 keybinding editor)

**Last Updated:** 2026-05-16 on `v0.16.7-keybinding-editor` (commit `07b15a1`).

**Current work:** File ▸ Preferences dialog with keybinding UI.

- New `PreferencesDialog` modal: `QTreeWidget` with Action / Shortcut columns, populated from `ShortcutRegistry::registered_defaults()`. Double-click → `QKeySequenceEdit` inline editor. Reset All button.
- Changes persist to `QSettings("shortcuts/<id>")` — same path `ShortcutRegistry::lookup()` already reads on startup.
- Wired to File ▸ Preferences (no default shortcut to avoid conflicts).

**Limitations (deferred):**
- Widget-owned shortcuts (e.g. `QAbstractItemView` built-ins) bypass `ShortcutRegistry` and won't appear in the dialog.
- Hot reload deferred: changes take effect on next app launch.

**Validation:** `cargo build -p bif_qt` clean. `cargo test -p bif_qt --lib` pass. Clippy + fmt clean. Manual smoke: dialog opens, shortcut edit commits, persists across restart.

**Next:** Dialog validation (reject conflicting shortcuts). Convert File/Workspace menu items through shortcut registry. Hot reload via `QShortcut::setKey()`.

---

# Session Handoff — 2026-05-15/16 (v0.16.6 NaN AABB fix + close)

**Last Updated:** 2026-05-16. Branch `v0.16.6-ui-polish` merged to `main`, tagged `v0.16.6`.

**Fix (commit `982e0b3`):** NaN AABB panic on `comprehensive.usda` load. Two bugs:
1. `Renderer::compute_scene_bounds()` returned `Aabb::INFINITE` sentinel for empty/invalid scenes without NaN checking — downstream camera framing called `.center()` on NaN bounds → panic.
2. Degenerate USD xform ops in `scene_loader.rs` propagated NaN through matrix transforms; added `is_finite()` guard before applying.

**Fix:** `Aabb::is_valid()` check before framing; sentinel `Aabb::ZERO` for empty scenes. Matrix guard skips degenerate xforms.

**Validation:** `comprehensive.usda` loads without panic. `cargo test -p bif_viewport` 149/149. Code review via `vfx-code-reviewer` passed. Tagged `v0.16.6 - 2026-05-16`.

---

> Older sessions archived in [SESSION_HANDOFF_ARCHIVE.md](docs/archive/SESSION_HANDOFF_ARCHIVE.md)
