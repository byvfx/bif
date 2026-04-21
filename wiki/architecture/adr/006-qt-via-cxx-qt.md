---
title: ADR-006 — Qt 6 via cxx-qt (v0.15.0 migration)
type: adr
status: accepted
tags: [architecture, qt, ui, ffi, v0.15, wgpu]
created: 2026-04-13
updated: 2026-04-13
---

# ADR-006 — Qt 6 via cxx-qt (v0.15.0 migration)

## Status

Accepted — Phase 0 spike gate PASSED 2026-04-13 on Qt 6.8.3 LTS + MSVC 2022 + wgpu 22.1. Phase A (bif_qt crate) unblocked.

## Context

egui shipped BIF from v0.1 through v0.14 but has hit its ceiling for a DCC-class UI: no native docking, no rich text, poor list virtualization, no standard widget library for trees/tables, no Wacom support. ADR-002 (2025-03) already scoped egui as **temporary**. The 2026-04 v0.14 retrospective (in `docs/ux/UI_DESIGN.md`) made the migration concrete: T-layout with 4 workspaces, command palette, breadcrumb, 100K-prim virtualized scene browser, layer color coding with opinion attribution. None of these are reasonable to build on egui.

v0.15.0 migrates the panel layer to Qt 6. `bif_core` data types (`SceneLayerState`, `LayerStack`, `OpinionSource`, `PrimStackEntry`, `PayloadPolicy`) are already UI-agnostic (that was the ADR-005 payoff), so the churn is confined to `bif_viewport` + `bif_viewer`.

## Decisions

### 1. Binding: Qt 6 via `cxx-qt 0.7` + `qt-build-utils 0.7`

Evaluated alternatives:

| Binding | Pros | Cons |
|---|---|---|
| **cxx-qt** | First-class Rust QObject macros, signals/slots as regular fns, cxx FFI under the hood (BIF already uses cxx for USD), active upstream | Qt Widgets coverage weaker than QtQuick, API churn between minor versions |
| `qmetaobject-rs` | Maintained, works | Less active than cxx-qt, weaker Widgets story |
| `Slint` | Pure Rust, no Qt runtime | Not Qt — abandons the Qt ecosystem rationale (plugins, Designer, QSettings, QDockWidget, etc.) |
| Vanilla `cxx` + C++ Qt app | Minimum bootstrap, battle-tested pattern (matches BIF's usd_bridge/) | Loses ergonomics of QObjects in Rust — panels would be C++-authored |

**Choice:** `cxx-qt` for Rust-side QObjects (Phase A+). The Phase 0 spike used plain `cxx` + `qt-build-utils` to isolate the wgpu-QWindow question from cxx-qt macro maturity; both paths proved viable. Phase A commits to cxx-qt macros for `bif_qt` panels.

Locked versions at decision time: Qt 6.8.3 LTS, cxx 1.0.194, cxx-qt 0.7, qt-build-utils 0.7.3.

### 2. Qt version: 6.8 LTS

- Qt 6.8 LTS supported through Oct 2027 (OSS) — multi-year binary stability.
- cxx-qt 0.7 explicitly tests against Qt 6.5 and 6.8.
- Qt 6.9+ / 6.10+ / 6.11+ are 6-month windows — unnecessary churn for a side project.

### 3. Licensing: LGPL v3 + dynamic linking

Open-source Qt installer. BIF ships Qt DLLs alongside `bif_viewer.exe`. Commercial-license revisit is deferred — not imminent. Phase H release notes will document the LGPL source-offer + relinking obligations.

### 4. wgpu viewport embedding: QWidget subclass with suppressed Qt painting

Phase 0 spike validated the pattern:

```cpp
class RenderWidget : public QWidget {
    Q_OBJECT
public:
    RenderWidget(QWidget* parent = nullptr);
    std::uint64_t nativeWinId() const;        // winId() → HWND on Windows
    std::uint64_t nativeHInstance() const;    // GetModuleHandleW(NULL)
protected:
    QPaintEngine* paintEngine() const override { return nullptr; }  // Qt gives up painting
    void paintEvent(QPaintEvent*) override;   // forward to Rust renderer
    void resizeEvent(QResizeEvent*) override; // forward to Rust surface reconfigure
    void showEvent(QShowEvent*) override;     // first-show triggers wgpu init
};
```

Critical QWidget attributes set in the constructor:

```cpp
setAttribute(Qt::WA_NativeWindow);             // force own HWND
setAttribute(Qt::WA_DontCreateNativeAncestors);
setAttribute(Qt::WA_PaintOnScreen);            // bypass backing store
setAttribute(Qt::WA_OpaquePaintEvent);
setAttribute(Qt::WA_NoSystemBackground);
```

The `paintEngine() → nullptr` override is the key trick — Qt's raster engine never touches the widget, so wgpu owns every pixel. This matches the pattern used by Qt+Vulkan / Qt+Metal samples upstream.

Rust side: `winId()` cast to `HWND` → `NonZeroIsize` → `Win32WindowHandle` → `RawWindowHandle::Win32` → `wgpu::Instance::create_surface_unsafe` → standard wgpu surface flow. Full working code is in `crates/bif_qt_spike/`.

### 5. Phase 0 spike scope: minimum viable triangle

`crates/bif_qt_spike/` proved:

- qt-build-utils finds Qt 6.8 LTS from `Qt6_DIR` / `CMAKE_PREFIX_PATH` set by `setup_qt_env.ps1`
- cxx 1.0 + cxx_build compile a Rust↔C++ bridge with Qt on the C++ side
- moc runs correctly as part of the cargo build (via `qt-build-utils::QtBuild::moc()`)
- `cl.exe` compiles Qt headers with MSVC 2022 after adding `/Zc:__cplusplus`, `/permissive-`, `/utf-8`, `/EHsc`
- Qt DLLs load from `C:\Qt\6.8.3\msvc2022_64\bin` via PATH
- QApplication + QMainWindow + embedded RenderWidget + wgpu::Surface + triangle draw works end-to-end
- Resize flow (Qt `resizeEvent` → Rust `Renderer::resize`) works
- Clean shutdown (no hang, no crash, 167MB RSS)

The spike is a gate crate — deleted at Phase H release.

## Consequences

### Positive

- Every `bif_core` type remains UI-agnostic (zero changes in v0.15 Phase F).
- Qt's native widget library (QTreeView, QTableView, QDockWidget, QGraphicsScene, QSettings, QFileDialog) eliminates ~900 LOC of egui panel-assembly in `bif_viewport/src/render.rs`.
- `cxx-qt` Rust-side QObjects let panels live in Rust with properties/signals, matching how a native Qt C++ app would feel.
- Qt Designer becomes available for future layout iteration (deferred to v0.16+).

### Negative

- `setup_qt_env.ps1` becomes a build prerequisite alongside `setup_usd_env.ps1` — CI and fresh-clone onboarding gain a step.
- Qt DLL distribution adds ~40MB to release archives.
- `cxx-qt 0.7 → 0.8+` upgrades may require touch-ups; pin the minor version in Cargo.toml and bump deliberately.
- MSVC is now a hard requirement — no MinGW fallback. Matches existing USD / wgpu toolchain, so this is a non-cost in practice.

### Neutral

- `cxx-qt-lib` covers Qt Core types (QString, QVariant, QVector) well but QtWidgets coverage is thinner. Phase A will determine whether to fill gaps with raw `cxx` bridges or upstream PRs.

## Migration strategy

Shell-first on `v0.15-qt` branch (per plan `C:\Users\brandon\.claude\plans\iridescent-soaring-hamster.md`):

1. **Phase 0** — wgpu-QWindow spike (this ADR's gate). **DONE.**
2. **Phase A** — `crates/bif_qt/` scaffolding + theme port + empty dock shell.
3. **Phase B** — shell: viewport widget, menu, docks, command palette, breadcrumb, workspaces, first-launch, zen mode.
4. **Phase C** — core panels: Layer Stack, Scene Browser (virtualized), Property Inspector.
5. **Phase D** — secondary panels: Timeline, Node Graph (QGraphicsScene, biggest single port), Render Settings.
6. **Phase E** — input + event wiring.
7. **Phase F** — delete egui from `bif_viewport` + `bif_viewer`.
8. **Phase G** — tests + validation.
9. **Phase H** — release plumbing, merge `v0.15-qt` → `main`, tag `v0.15.0`.

`main` stays on v0.14.0 egui and shippable throughout. Merge is the commitment.

## Gotchas captured from the spike

- **MSVC requires `/Zc:__cplusplus`** — without it, `__cplusplus` reports `199711L` even on `-std:c++17`, and Qt's `qcompilerdetection.h` fatal-errors out.
- **`QT_NO_KEYWORDS` breaks `signals:` / `emit`** — cxx-qt projects typically define it to keep `slots` from colliding with Rust keywords. Phase A must convert to `Q_SIGNALS:` / `Q_EMIT` if we enable it.
- **moc must run on Q_OBJECT headers** — qt-build-utils `QtBuild::moc(header, MocArguments)` returns the generated `.cpp` which we feed into `cxx_build::bridge(...).file(moc.cpp)`.
- **argc/argv lifetime** — `QApplication(int&, char**)` holds its argc as a reference. Rust-side leaks via `Box::leak` or (spike's choice) C++ side uses `static char arg0[] = "bif_qt_spike"; static int argc = 1;` and owns them.
- **HWND access requires `WA_NativeWindow` before `winId()` is called** — set all relevant attributes in the QWidget constructor.

## Related

- Plan: `C:\Users\brandon\.claude\plans\iridescent-soaring-hamster.md`
- Spike: `crates/bif_qt_spike/`
- Setup script: `setup_qt_env.ps1`
- Predecessor: [[002-egui-temporary-ui]] — scoped egui as temporary
- Design: `docs/ux/UI_DESIGN.md` — 25-section Qt UI specification
- Research: `docs/ux/DCC_UI_RESEARCH.md` — Houdini / Katana / usdview comparison
