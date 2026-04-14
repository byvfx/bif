# bif_qt_spike

**Phase 0 gate crate for v0.15.0 Qt migration.** Delete after ADR-006 lands.

## What it proves

wgpu can render into a native Qt 6 QWidget's HWND. If the spike runs and shows a three-colored triangle that resizes cleanly with the window, the Qt embedding path is viable and Phase A (`crates/bif_qt`) can proceed.

## Architecture

```
Rust main ──► qt_spike_run(*SpikeCallbacks)
                 │
              C++ side
              ├── QApplication (argc/argv static)
              ├── QMainWindow
              │     └── RenderWidget (QWidget subclass)
              │           ├── WA_NativeWindow, WA_PaintOnScreen
              │           ├── paintEngine() == nullptr
              │           └── signals: surfaceReady, resized, frameRequested
              │
              └── signals ─connect─► cxx trampolines ─► Rust renderer
                                                          ├── wgpu::Instance
                                                          ├── wgpu::Surface (from HWND)
                                                          └── draw triangle
```

## Run

```powershell
. .\setup_usd_env.ps1    # (optional — spike doesn't need USD)
. .\setup_qt_env.ps1     # required — sets Qt6_DIR + CMAKE_PREFIX_PATH + PATH
cargo run -p bif_qt_spike
```

## Gate criteria

- [ ] Compiles cleanly (`cargo build -p bif_qt_spike`)
- [ ] Launches — window appears
- [ ] Dark-grey background + three-colored triangle visible
- [ ] Window resizes, triangle reflows
- [ ] DPI scaling works on high-DPI monitors
- [ ] Closes cleanly (no hang, no crash)

**All ✅ → Phase A proceeds. Any ✗ → reopen binding decision (Slint / qmetaobject / keep egui).**
