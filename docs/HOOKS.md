# BIF Hooks — User Extension Points

**Status:** Draft spec. Not implemented. Targeting v0.16.0 alongside save + shot templates.
**Goal:** Let studios and power users wire BIF into their pipelines without BIF shipping a pipeline.

---

## Philosophy

- Hooks = **subprocess + config**. No dynamic loading, no plugin ABI, no Qt extension surface.
- BIF stays narrow. Pipeline stays in the user's tools/scripts, which BIF just shells out to.
- Wins 80% of integration needs at 20% of the effort. Does not lock BIF into long-term plugin contracts.
- Hooks are **strictly opt-in** — never auto-run on a fresh install.

**Hooks are NOT:**

- A Rust plugin system
- A Python/Lua scripting environment (see v0.20+ pyo3 story)
- A way to mutate scene state mid-eval
- A UI extension mechanism

---

## Config

Two config files, merged user-first:

```
~/.bif/hooks.toml                   # user-global
<stage_dir>/.bif/hooks.toml         # per-project (auto-detected beside opened stage)
```

Project-local hooks require **first-run confirmation** per project path. BIF stores approval in `~/.bif/trusted_projects.toml`. Prevents drive-by hook execution from downloaded USD bundles.

Schema (TOML):

```toml
# Event-driven hook
[[hooks.after_save]]
name = "Publish to pipeline"
command = ["python", "~/tools/bif_publish.py", "--layer", "{layer_path}"]
timeout_secs = 30            # optional, default 10
continue_on_error = true     # optional, default true for after_*, false for before_*
env = { "PIPELINE_ROOT" = "/studio/proj01" }  # optional

# User command (fires on palette / shortcut, not on event)
[[commands]]
name = "Submit to Deadline"
shortcut = "Ctrl+Shift+R"    # optional
icon = "farm"                # optional, maps to BIF icon set
command = ["deadlinecommand", "-SubmitJob", "{stage_path}"]
timeout_secs = 60
```

---

## Events

| Event | Fires | Can abort? | Vars guaranteed |
|---|---|---|---|
| `before_open` | Before `SceneManager::load_scene` | ✅ non-zero exit aborts | `stage_path`, `stage_name`, `user_home` |
| `after_open` | After stage load succeeds | ❌ informational | `stage_path`, `stage_name`, `layer_path`, `layer_name` |
| `before_save` | Before writing edit-target layer | ✅ non-zero aborts | `stage_path`, `layer_path`, `opinion_count` |
| `after_save` | After layer flush to disk | ❌ informational | `stage_path`, `layer_path`, `opinion_count`, `bytes_written` |
| `before_export` | Before `export_scene` | ✅ non-zero aborts | `stage_path`, `export_path`, `flatten_mode` |
| `after_export` | After export write | ❌ informational | `stage_path`, `export_path`, `flatten_mode` |
| `after_render` | After CLI render (`bif_cli render`, v0.17+) | ❌ | `stage_path`, `output_dir`, `frame_range`, `duration_ms` |
| `on_selection` | Prim selected in scene browser/viewport | ❌ (best-effort, debounced 250ms) | `stage_path`, `selected_prim`, `selected_prim_type` |

Hook order within an event = config file order. User-global runs before project-local.

---

## Variable interpolation

Substituted in `command` and `env` values via `{name}`.

| Variable | Meaning |
|---|---|
| `{stage_path}` | Absolute path to master stage USD |
| `{stage_dir}` | Parent directory of stage |
| `{stage_name}` | Stage filename without extension |
| `{layer_path}` | Current edit-target layer absolute path |
| `{layer_name}` | Edit-target filename without extension |
| `{selected_prim}` | Current selection USD path, e.g. `/world/hero` (empty if none) |
| `{selected_prim_type}` | USD type name e.g. `Xform`, `Mesh` |
| `{frame}` | Current timeline frame (int) |
| `{opinion_count}` | Number of opinions authored on edit-target layer (save events) |
| `{export_path}` | Target path (export events) |
| `{flatten_mode}` | `full` / `layer` / `flat` (export events) |
| `{output_dir}` | Render output dir (render event) |
| `{frame_range}` | `1-100` style string (render event) |
| `{user_home}` | `$HOME` / `%USERPROFILE%` |
| `{bif_root}` | BIF install directory |
| `{timestamp}` | ISO-8601 UTC |

Unknown `{var}` → substituted empty + warning in log panel. Not a hard error.

Tildes (`~`) in `command[0]` expanded to home dir.

---

## Execution contract

- **Process:** `std::process::Command`, no shell interpretation. `command` is argv, element 0 = executable, rest = args.
- **CWD:** `stage_dir` if set, else `bif_root`.
- **Stdout:** first non-empty line → status bar toast. Full stdout → log panel.
- **Stderr:** full → log panel at warning level.
- **Exit code 0:** OK.
- **Non-zero on `before_*`:** operation aborted, modal shows hook name + stderr tail.
- **Non-zero on `after_*`:** warning toast, operation continues.
- **Timeout:** process killed, warning toast. Default 10s, configurable.
- **Structured output (future):** `mode = "structured"` makes BIF parse stdout as JSON for richer feedback (toast levels, custom buttons). v0.17+.

---

## Custom commands

User-defined commands appear in:

1. Ctrl+P command palette (searchable by `name`)
2. Optional keyboard shortcut
3. Optional right-click context-menu category (future)

Not tied to events. Fire when user invokes them. Same variable interpolation + exec contract.

---

## USD plugin passthrough

Orthogonal to hooks but same theme — **document, don't reimplement**.

- BIF appends `<bif_root>/plugins/` to `PXR_PLUGINPATH_NAME` at startup.
- Custom ArResolvers, file format plugins, Hydra delegates work with zero BIF-side code.
- "About → Plugins Loaded" panel lists resolved plugins for troubleshooting.
- BIF never swallows USD plugin errors — they surface in the log panel at startup.

---

## Security posture

- First run of a project-local `.bif/hooks.toml` shows a modal: "Project X wants to run N hooks. Trust this project?" Yes → add to `trusted_projects.toml`. No → disable hooks for this session.
- User-global hooks at `~/.bif/hooks.toml` are always trusted (user wrote them).
- Hooks run with current user permissions. No elevation.
- No network restrictions, no sandboxing — the user's tools do what they do.
- `--no-hooks` CLI flag disables all hooks for crash-recovery / safe-mode launches.

---

## Examples

### Perforce checkout before open

```toml
[[hooks.before_open]]
name = "p4 edit"
command = ["p4", "edit", "{stage_path}"]
timeout_secs = 5
continue_on_error = true
```

### Publish to ftrack after save

```toml
[[hooks.after_save]]
name = "Publish version"
command = ["python", "{user_home}/tools/ftrack_publish.py",
           "--file", "{layer_path}",
           "--comment", "Saved {opinion_count} opinions"]
```

### Custom "Open in Houdini" command

```toml
[[commands]]
name = "Open in Houdini"
shortcut = "Ctrl+Alt+H"
command = ["houdini", "-c", "import pxr; pxr.UsdStage.Open('{stage_path}')"]
```

### Farm submit

```toml
[[commands]]
name = "Submit render to Deadline"
shortcut = "Ctrl+Shift+R"
icon = "farm"
command = ["deadlinecommand", "-SubmitJob",
           "--Plugin", "BifCli",
           "--StageFile", "{stage_path}",
           "--Frames", "1-100"]
timeout_secs = 60
```

---

## Implementation sketch (not this doc's scope)

- New crate `bif_hooks` (small): config parse, variable interpolation, executor.
- Event firing points wired into `SceneManager`, save path, export path, render CLI.
- UI: "Hooks" tab in Preferences (reload config, per-hook enable/disable, test-fire button), "Plugins Loaded" diagnostic panel.
- ~800-1200 LOC total incl. tests.

---

## Non-goals

- **No synchronous scene-graph callbacks.** Hooks get a snapshot via variables; they don't mutate in-flight state.
- **No in-process Python.** That's a separate story (v0.20+).
- **No chained / conditional hooks.** If you need DAG logic, call a script that does it.
- **No hook UI generation.** Hooks don't add widgets, settings tabs, or panels.
- **No per-node-type hooks.** Nodes are BIF-internal. Hooks fire at app/stage lifecycle, not eval.

---

## Unresolved questions

1. **Hook reload on config change** — watch `hooks.toml` with notify, or require restart? (Lean: watch, cheaper than users filing "my hook didn't fire" bugs)
2. **`on_selection` debounce tuning** — 250ms starting guess, may fight rapid click-through workflows. Expose in Preferences?
3. **Env inheritance** — inherit full BIF process env, or start clean? (Lean: inherit — studios expect `PIPELINE_ROOT` etc. from launcher)
4. **Windows vs POSIX path handling in interpolation** — always emit native sep, or always POSIX? (Lean: native; scripts that need POSIX can call `.replace("\\", "/")`)
5. **Structured JSON output contract** — defer to v0.17 or design now? (Lean: defer; YAGNI until first real use case)
6. **`bif_cli` parity** — same hooks config file used by CLI? Necessary for farm jobs to run `after_render` hooks. (Lean: yes, same file)
7. **Max output size per hook** — cap stdout/stderr at e.g. 1 MiB to avoid runaway scripts flooding log? (Lean: yes, hard cap with truncation notice)
