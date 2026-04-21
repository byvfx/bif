// Window builder — Phase B shell assembly.
//
// Constructs the QMainWindow skeleton: menu bar, status bar,
// RenderWidget as the central widget (wgpu-into-Qt, ported from
// the Phase 0 spike), and four QDockWidget slots (left/right/
// bottom). Phase C fills the docks with real panels.
//
// ViewportCallbacks is a Rust-opaque struct declared in the cxx-qt
// bridge at src/main_window.rs. It owns the wgpu Viewport on the
// Rust side; C++ forwards RenderWidget signals to it via the
// viewport_on_* trampolines cxx generates.

#pragma once

#include "rust/cxx.h"

// Forward-declared by the cxx-qt-generated bridge header.
struct ViewportCallbacks;

extern "C" {

// Creates QApplication + QMainWindow (with RenderWidget central),
// wires viewport callbacks, applies the stylesheet, shows the
// window, runs exec(), returns the exit code. Blocks until the
// event loop terminates.
int bif_qt_run_shell(ViewportCallbacks* viewport_cb, ::rust::Str stylesheet);

}  // extern "C"
