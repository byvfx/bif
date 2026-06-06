# Session Handoff

> Active sessions: last 5. Older entries → [SESSION_HANDOFF_ARCHIVE.md](docs/archive/SESSION_HANDOFF_ARCHIVE.md)

## Current State

- **Branch:** `main` (only local branch)
- **Version:** **v0.16.9** tagged (2026-06-04). `[Unreleased]` = issue #6 Phase 1 (`NodeOutputs` merge).
- **Status:** Release CI works end-to-end (v0.16.9 shipped a launchable Windows zip). **Issue #6 Phase 1 landed** (PR #8, `e765d37`) — `node_proto_map` + `node_cloud_map` merged into one `NodeOutputs` map, behavior-preserving, 179 bif_viewport tests green. PR-based review workflow now live (auto `claude-review` + manual `/vfx-code-reviewer` → squash-merge); Kilo bot removed.
- **Next:** **Issue #6 Phase 2** — add `SceneCmd` enum + `Renderer::execute()`, route the 21 `handle_node_graph_event` dispatch arms through it one at a time (multi-step LoadUsd/Export via `SceneCmd::Custom`). Then Phase 3 (`behavior.rs` — move per-node `evaluate`/`register_prims`/`apply` + add the trait-boundary tests). Plan for Phase 1: [`docs/agent-plans/2026-06-05-issue6-phase1-node-outputs.md`](docs/agent-plans/2026-06-05-issue6-phase1-node-outputs.md). **Or** v0.17.0 Context System (30–40h, highest arch risk).
- **Gotcha:** `cargo test -p bif_viewport` needs `. .\setup_usd_env.ps1` sourced first (transitively links USD DLLs via bif_core → `STATUS_DLL_NOT_FOUND` otherwise).

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
