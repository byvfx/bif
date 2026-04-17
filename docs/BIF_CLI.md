# bif_cli — Headless CLI for BIF

**Status:** Draft spec. Not implemented. Targeting v0.17.0 after v0.16 save + templates + hooks.
**Goal:** Farm-renderable BIF without requiring the GUI. Ship the same path tracer already in `bif_viewport::Renderer` behind a subcommand parser.

---

## Philosophy

- **Same binary philosophy as GUI, separate crate.** `bif_cli` is a new binary crate that reuses `bif_core` + `bif_viewport::Renderer` (headless mode). Keeps `bif_qt_shell` clean GUI, keeps CLI clean headless.
- **Farm-friendly first.** Deterministic output, stable exit codes, newline-delimited progress, no interactive prompts.
- **Pairs with `docs/HOOKS.md`** — `after_render` hook fires per run (or per frame if `--hook-per-frame`).
- **Not a pipeline.** `bif_cli` renders, inspects, and exports. Wrapping it into a publish/versioning workflow is the user's hook script's job.

---

## Subcommands (v0.17 launch set)

```
bif_cli render   <stage> [options]    # headless render
bif_cli info     <stage>              # stage summary (layers, frames, cameras, root prims)
bif_cli export   <stage> [options]    # flatten / sublayer export (same as UsdExport node)
bif_cli version                       # print version + USD + wgpu backend
```

Deferred to v0.18+: `bif_cli validate`, `bif_cli templates list|create`, `bif_cli diff <layerA> <layerB>`.

---

## `bif_cli render`

### Required
- `<stage>` — positional, absolute or relative path to USD file

### Output
- `--output <dir>` (**required**) — directory for rendered frames
- `--name-pattern <pat>` — default `{stage_name}.{frame:04d}.{ext}`. Vars: `{stage_name}`, `{frame}`, `{camera}`, `{ext}`
- `--format <exr|png>` — default `exr` (float32, linear)

### Frames + camera
- `--frames <range>` — `1-100`, `1,3,5`, `1-100x2` (step), `current` (stage's current frame). Default `current`
- `--camera <prim_path>` — USD camera prim. Default: first `UsdGeomCamera` found; error if none
- `--resolution <WxH>` — e.g. `1920x1080`. Default: read from `RenderSettings` prim if present, else `1920x1080`

### Quality
- `--spp <n>` — samples per pixel. Default `64`
- `--max-depth <n>` — path depth. Default `8`
- `--seed <n>` — RNG seed (deterministic output). Default `0`
- `--denoise` / `--no-denoise` — OIDN denoising. Default `--denoise` if built with `oidn` feature, else off
- `--display <shaded|wireframe|wire-on-shaded|points|bbox>` — default `shaded`
- `--purpose <render|proxy|guide>` — repeatable. Default: `render` + `proxy`

### Execution
- `--threads <n>` — CPU threads. Default `num_cpus`
- `--gpu-backend <vulkan|dx12|auto>` — default `auto`
- `--timeout-secs <n>` — per-frame wall clock cap, kills frame on overrun

### Config + hooks
- `--config <path>` — TOML config file; any flag can be set there. CLI flags override file
- `--hooks` / `--no-hooks` — fire hooks from `~/.bif/hooks.toml` + `<stage_dir>/.bif/hooks.toml`. Default `--hooks`
- `--hook-per-frame` — fire `after_render` per frame instead of per run. Off by default
- `--trust-project-hooks` — bypass first-run trust modal (required for farm since no human to click OK)

### Logging
- `--quiet` / `-q` — errors only
- `--verbose` / `-v` — per-frame progress, ETA
- `--json-log` — newline-delimited JSON per event, for farm parsers

### Examples

```bash
# Single-frame local test
bif_cli render shot_010.usd --output ./renders/ --frames 42 --spp 32

# 100-frame sequence with farm-ready logging
bif_cli render shot_010.usd \
  --output /render/proj/sh010/v003/ \
  --frames 1-100 \
  --camera /world/cameras/main \
  --resolution 1920x1080 \
  --spp 256 \
  --denoise \
  --json-log \
  --trust-project-hooks

# Config-driven
bif_cli render shot_010.usd --config shot_010.render.toml
```

### Config file (`--config` TOML)

```toml
[render]
output = "/render/proj/sh010/v003/"
frames = "1-100"
camera = "/world/cameras/main"
resolution = "1920x1080"
spp = 256
max_depth = 12
denoise = true
format = "exr"
name_pattern = "{stage_name}.{frame:04d}.{ext}"
seed = 42

[render.hooks]
enabled = true
hook_per_frame = false
```

---

## `bif_cli info`

Prints stage summary. Defaults to human-readable; `--json` for machine parsing.

```
$ bif_cli info shot_010.usd

Stage:         shot_010.usd
Root layer:    shot_010.usd
Sublayers:     layout.usd, anim.usd, fx.usd, lighting.usd (edit target on save)
Frames:        1 - 100 (24 fps)
Cameras:       /world/cameras/main
               /world/cameras/shot
RenderSettings: <none>
Root prims:    /world (Xform, 4 children)
               /RenderSettings (RenderSettings)
Defaults:      resolution=1920x1080 (inferred)
               purpose=render,proxy
```

`--json` yields the same data as a single JSON object — good for `bif_cli info … | jq` in farm scripts.

---

## `bif_cli export`

Same semantics as the UsdExport node, driven from shell.

```
bif_cli export <stage>
  --output <path>
  --flatten <full|layer|flat>    # default: layer (current layer only)
  --include-payloads             # resolve payloads into export
  --session-layer-only           # export session overrides only
```

---

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Argument / config error (never attempted work) |
| 2 | Stage load failure (file missing, parse error, payload resolution failure) |
| 3 | Render failure — at least one frame failed (individual frame errors logged) |
| 4 | Hook failure — before_* hook aborted, or after_* hook failed with `--strict-hooks` |
| 5 | Export failure |
| 130 | SIGINT — graceful shutdown, partial output preserved |

Farms should treat `0` as full success, `3` as partial success (check per-frame logs), everything else as retry/escalate.

---

## Progress protocol

Human (default, verbose):

```
[1/100] frame 1 … done (spp=256, 2834ms)
[2/100] frame 2 … done (spp=256, 2811ms)
...
```

JSON (`--json-log`), one event per line:

```json
{"ts":"2026-04-16T19:12:03Z","event":"start","frames_total":100}
{"ts":"...","event":"frame_start","frame":1}
{"ts":"...","event":"frame_done","frame":1,"path":"/render/.../shot_010.0001.exr","duration_ms":2834,"spp_actual":256}
{"ts":"...","event":"frame_error","frame":42,"error":"gpu_device_lost"}
{"ts":"...","event":"hook_done","event_name":"after_render","hook_name":"Publish","duration_ms":812}
{"ts":"...","event":"done","frames_succeeded":99,"frames_failed":1,"duration_ms":281332}
```

Events: `start`, `frame_start`, `frame_done`, `frame_error`, `hook_start`, `hook_done`, `hook_error`, `warn`, `done`.

---

## Hook integration (from `docs/HOOKS.md`)

- `--hooks` (default on) loads `~/.bif/hooks.toml` + `<stage_dir>/.bif/hooks.toml`
- `--trust-project-hooks` required for project-local hooks since no modal is shown
- Events fired during render:
  - `before_render` — can abort (non-zero exit code → exit code 4)
  - `frame_done` — after each frame if `--hook-per-frame`
  - `after_render` — after all frames
- Variables exposed to hooks (in addition to globals): `{output_dir}`, `{frame_range}`, `{frames_succeeded}`, `{frames_failed}`, `{duration_ms}`, `{camera}`, `{resolution}`

Example farm submit flow:
1. Farm runs `bif_cli render … --json-log`
2. `after_render` hook fires → `publish.py` reads `BIF_RENDER_OUTPUT` env + posts to tracker
3. Farm parses exit code + JSON log for per-frame status

---

## Determinism contract

Same stage + same args + same BIF version → byte-identical output. Requires:

- Fixed `--seed` (default 0, exposed as flag)
- Single-threaded BVH traversal OR order-stable accumulation
- Deterministic denoise (OIDN is deterministic given same input + version)
- No `SystemTime::now()` in render path
- No wall-clock-seeded RNG anywhere downstream

Exception: GPU reduction ops (e.g. SHARC) may introduce rounding-order variance. Documented.

---

## Implementation sketch

- **New crate:** `bif_cli` (binary). Depends on `bif_core`, `bif_viewport`, `bif_renderer`, `clap`, `serde`, `toml`.
- **Headless viewport:** add `Renderer::new_headless(width, height, device_prefs) -> Renderer`. No surface, renders to an offscreen texture, reads back to CPU via `wgpu::CommandEncoder::copy_texture_to_buffer`.
- **EXR writer:** use `exr` crate (pure Rust, already OSS). PNG via `image` crate.
- **Arg parser:** `clap` with derive. Config file merged first, CLI overrides.
- **Hooks:** reuse `bif_hooks` crate from v0.16.
- **Progress:** async channel from render loop → logger (human or JSON).
- **Binary size target:** < 80 MiB including USD / wgpu / OIDN DLLs.
- **Est:** 1200–1800 LOC for v0.17 launch set (render + info + export).

---

## Tests

- Unit: arg parsing, frame-range parsing (`1-100x2`), config merge precedence, name-pattern interpolation.
- Integration: render a known USDA stage at low spp, assert EXR exists and has expected dimensions.
- Golden image (optional, gated): render `test_assets/cornell_box.usda` at fixed seed/spp, compare to reference EXR within PSNR threshold. Gated behind `BIF_GOLDEN_TESTS=1` env (GPU-specific).
- Determinism: render same stage twice with same seed, assert byte-identical output.
- Hooks: mock subprocess that writes a marker file, verify `after_render` fires.
- Exit code: inject a missing stage → exit 2; inject a hook abort → exit 4.

---

## Non-goals

- **Tile rendering** (split one frame across machines). Farm does per-frame parallelism; one frame = one node = one BIF process.
- **Interactive REPL** or live preview over network.
- **Full AOV suite** (only beauty + denoised-beauty at launch; per-light / per-material AOVs v0.18+).
- **Multi-GPU on one host** (single-GPU for v0.17; multi-GPU post-v1.0 if anyone asks).
- **Husk / Karma / Arnold / RenderMan backend.** v1.0+ optional, behind feature flag. Farm installers are licensing minefield.
- **`bif_cli gui`** — no, just run `bif_qt_shell`. Don't blur the boundary.
- **Watch mode** (`--watch` re-render on file change). Interactive workflow belongs in the GUI, not the CLI.

---

## Farm integration notes (reference)

- **Deadline:** custom plugin wraps `bif_cli render --frames {Frame} --json-log`. Parse JSON events for progress callbacks.
- **OpenCue:** same pattern. `outline.cuerun` template.
- **Tractor:** `Cmd -cmd { bif_cli render ... }` nodes, leverage tractor's retry.
- **Generic:** `--trust-project-hooks` + `--json-log` is the minimum viable farm contract.

---

## Unresolved questions

1. **`bif_cli` vs `bif render` subcommand of `bif_qt_shell`?** Separate binary is cleaner (headless deps, smaller surface, no Qt needed on farm). Lean: **separate**.
2. **USD env inheritance vs embedded** — does `bif_cli` require `setup_usd_env.ps1` before running, or do we ship USD DLLs in its install dir and use runtime path manipulation? (Affects farm packaging.) Lean: **embed USD DLLs** for farm-friendliness; keep `setup_usd_env.ps1` as dev-only.
3. **Config precedence when both config file and CLI flags set** — CLI always wins (confirmed in spec above), but should `--config` warnings list overrides? Lean: **yes**, print override list under `--verbose`.
4. **Single `hooks.toml` shared with GUI, or separate `cli_hooks.toml`?** Spec above assumes shared. Confirm? Lean: **shared** — unified mental model.
5. **Progress output when stdout is a pipe vs TTY** — auto-switch to JSON log when non-TTY? Or always explicit via flag? Lean: **always explicit** — surprise auto-behavior is farm-unfriendly.
6. **License for `--json-log` schema** — stabilize now or mark experimental? (Farm integrations will depend on it). Lean: **mark experimental v0.17**, stabilize v1.0.
7. **Memory budget flag** — `--max-memory <GB>` for farm nodes with strict limits? Or trust the OS? Lean: **defer** — YAGNI.
8. **Camera override via command line Xform** — `--camera-translate 0,0,10` etc. for lookdev sweeps? Lean: **defer** — wrap in a hook script instead.
