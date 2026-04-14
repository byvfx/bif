// Window builder — Phase A shell assembly.
//
// Constructs the QMainWindow skeleton: menu bar, status bar, central
// placeholder widget, and four empty QDockWidget slots (left/right/
// bottom). Phase B fills these with real panels.
//
// Keeps window construction in C++ because cxx-qt-lib's QtWidgets
// coverage is thinner than Core/Gui — QMainWindow / QDockWidget /
// QMenuBar / QStatusBar wrappers would be significant Rust-side
// boilerplate with no ergonomic win in Phase A. BifShellState (the
// cxx-qt-generated Rust-backed QObject from src/main_window.rs) is
// constructed C++-side here and parented to the main window.

#pragma once

extern "C" {

// Creates QApplication + QMainWindow, shows it, runs exec(), returns
// the exit code. Blocks until the event loop terminates. Constructs
// BifShellState internally as a child of the main window.
int bif_qt_run_shell();

}  // extern "C"
