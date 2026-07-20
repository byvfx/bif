# Node Graph Audit — Execution Handoff

**Date:** 2026-07-20
**Run on:** Windows box (Qt + USD build). This Mac session can't build BIF (no vcpkg/USD/Qt).
**Issue:** [#22](https://github.com/byvfx/bif/issues/22) · **Milestone:** v0.17.0 — Node Graph Parity

---

## Goal

Confirm the target dogfood workflow works end-to-end in the running Qt app: **lay down nodes → build scene → light with HDRI → render in viewport** (the last-good egui workflow). Walk all 10 node types, record works/broken in a results table, add headless tests where feasible, and file one gap issue per confirmed break. This is the audit half of #22; fix work is scoped in a later planning round from the results.

**Key context (from 2026-07-20 exploration):** The Qt shell drives the **same** `bif_viewport::Renderer` the egui viewer used — only the UI shell was swapped. Graph evaluation (`behavior.rs`, `eval.rs`, `node_dispatch.rs`) is UI-independent and auto-cooks every frame via `flush_node_graph()` (`crates/bif_viewport/src/render.rs:113`). So the backend is intact; the gaps are Qt param-panel coverage. **The happy path is expected to already work** — this audit confirms that and maps the parity gaps precisely.

---

## Prereqs (do first, verify before acting)

```powershell
git switch main
git reset --hard origin/main   # local tree is ~1 commit behind + has spurious asset "modifications" (mount/EOL); reset from Windows only
. .\setup_usd_env.ps1
. .\setup_qt_env.ps1           # both needed or cxx-qt fails QtMissing / USD DLLs missing
cargo build
cargo run -p bif_viewer
```

- **HDRI file needed** — no `.hdr`/`.exr` ships in the repo. Have a local path ready before the HDRI step (see Open Questions).
- **UsdRead asset:** `test_assets/comprehensive.usda` (also `test_assets/scene/`, `collections.usda`).
- Audit is human-in-the-loop: a person clicks the GUI; Claude guides each step and records the verdict.

---

## Scope

### In scope
1. Exercise all 10 node types in the running app (add / wire / evaluate / delete-cleanup / scene-browser reflection).
2. Verify the full workflow chain: Primitive/Scatter → PointInstancer → HdriEnvironment → IvarRender → viewport render.
3. Record results in the table below; fill into #22.
4. Add happy-path headless `bif_viewport` tests for node types lacking coverage (#22 DoD) where doable without a live GPU/USD stage.
5. File one follow-up issue per confirmed break, milestoned v0.17.0.

### Out of scope
- Building the param panels / bridges themselves (that's the fix round after this audit).
- GraftBranches redesign (known-disabled; just confirm the disabled state).
- v0.17.5 perf work (#14/#15).

---

## Pre-seeded known gaps (confirm these; flag anything new)

From exploration — the audit should **verify**, not rediscover, these:

1. **Scatter — no param panel/bridge** (biggest blocker). Can't set source (Surface/Grid/Sphere), count, seed, distribution, scale. Default `source = Surface` (`node_graph/mod.rs:448`) needs a wired scene input; without editing you can't switch to Grid/Sphere (which need no input).
2. **Primitive — no param panel.** Auto-creates at construction defaults; size/kind not tunable from Qt.
3. **PointInstancer — no panel.** Works only by wiring both inputs (points + prototype) + auto-compute; no params/feedback.
4. **IvarRender SPP not settable** — spinbox disabled (`node_graph_widget.cpp:156` "no set_spp bridge yet"); renders use backend default spp=16.
5. **`get_node_info` returns `None` for 7 of 11 node types** (`crates/bif_viewport/src/lib.rs:2791`) — selecting Scatter/Instancer/Primitive/UsdPrim/UsdExport/GraftBranches/Cache shows an empty inspector. Only UsdRead/HdriEnvironment/Xform/IvarRender serialize.
6. **GraftBranches disabled in Qt** — "held for redesign", returns `None`, adds a visual-only node (`node_graph_widget.cpp:651-656`, `lib.rs:487`).
7. **Set Display per-node unreachable** — backend flag + persistence exist (`node_graph/mod.rs:746`, `persistence.rs:129`) but no Qt per-node menu. Can't isolate a branch to the viewport.
8. **Eval-mode stuck `Auto`** — no Qt Auto/Manual toggle, no manual cook trigger (backend default `EvalMode::Auto`, `mod.rs:774`).
9. **HDRI env-pin wire is cosmetic** — environment application is global via `ivar_state.environment`; wiring HdriEnvironment→IvarRender does nothing functionally. The render uses the last-Applied HDRI regardless. Confusing but not a hard blocker.

---

## Audit procedure — per node

For each of the 10, record: **add** (right-click menu creates a real backend node?) · **wire** (ports connect w/o crash?) · **eval** (produces prim in scene / file on disk / render?) · **cleanup** (delete removes its output, no ghost prims?) · **browser** (scene browser reflects contribution?).

Right-click canvas → add. Wire output→input pin. Watch scene browser + viewport. Del/Backspace to delete.

### Workflow smoke (the actual goal — run this first)
1. Add **Cube** (Primitive) → geometry auto-appears in viewport.
2. Add **HdriEnvironment** → set path → **Apply** → scene lights up.
3. Add **IvarRender** → **Render** → path-trace result appears live in Qt viewport.
4. Add **ScatterPoints** + **PointInstancer**, wire Cube→Scatter→Instancer → instanced cloud appears.

If steps 1–3 work, the core dogfood loop is restored — that's the headline result.

### Results table (fill in, then paste into #22)

| Node | Add | Wire | Eval | Cleanup | Browser | Verdict / notes |
|------|-----|------|------|---------|---------|-----------------|
| UsdRead | | | | | | |
| Primitive (Cube/Sphere/Camera) | | | | | | |
| ScatterPoints | | | | | | |
| PointInstancer | | | | | | |
| Xform | | | | | | |
| UsdExport | | | | | | |
| UsdPrim | | | | | | |
| GraftBranches | | | | | | expect disabled |
| HdriEnvironment | | | | | | |
| IvarRender | | | | | | SPP stuck 16 |

---

## Key files

- Qt panel + canvas: `crates/bif_qt/cpp/node_graph_widget.{h,cpp}` (`NodeParamPanel` at `.cpp:66-249` — pattern to extend later)
- Bridge invokables: `crates/bif_qt/src/main_window.rs:508-562`
- Backend graph API: `crates/bif_viewport/src/lib.rs:458-661` (add node), `2740-2917` (`get_node_info`, `snarl_connect_pins`)
- Eval: `crates/bif_viewport/src/node_graph/behavior.rs`, `eval.rs`, `node_dispatch.rs`, `ops.rs`
- HDRI chain: `node_graph_load_hdri` (`lib.rs:572`) → `LoadHdri` (`node_dispatch.rs:103`) → `environment_manager.rs:108` → `render.rs:348-375` → `ivar_build.rs:725-730`; sampling in `bif_renderer/src/hdri.rs`
- Headless test seam: call `node_graph_add_node` → `snarl_connect_pins` → `flush_node_graph` → assert on `working_scene` (no GPU needed for graph-eval assertions)

---

## Done When

- Results table complete for all 10 node types; pasted into #22.
- Workflow smoke (Cube → HDRI → Render) confirmed working or its break filed as a blocker.
- Each confirmed break has a follow-up issue on the v0.17.0 milestone.
- New happy-path tests green: `cargo test -p bif_viewport` (USD env sourced).
- #22 closed with the results summary.
- Devlog entry for the session; SESSION_HANDOFF updated.

---

## Then — Next Work

Scope the fix PRs from the results. Expected priority order (from exploration): **Scatter panel + bridge** → Primitive + PointInstancer panels → IvarRender `set_spp` bridge → `get_node_info` coverage for the remaining 7 types → Set Display + eval-mode toggle → GraftBranches redesign (likely v0.18).

---

## Open Questions

1. HDRI file for the audit — which local `.hdr`/`.exr` path on the Windows box? (repo ships none)
2. GraftBranches — confirm-disabled only this round, or start the redesign inside v0.17.0?
3. Headless test depth — how much of eval is assertable without a live GPU surface (Ivar trace needs one)? Graph-population asserts on `working_scene` are the safe floor.
