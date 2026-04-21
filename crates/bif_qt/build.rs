// bif_qt build script — first real cxx-qt integration in BIF.
//
// Uses `cxx_qt_build::CxxQtBuilder` which internally:
//   1. Runs qt-build-utils to find Qt 6 (setup_qt_env.ps1 env vars).
//   2. Expands `#[cxx_qt::bridge]` modules + invokes moc on generated
//      headers.
//   3. Configures cxx_build for the Rust<->C++ FFI.
//   4. Compiles our additional C++ files (window_builder.cpp for
//      QMainWindow assembly — Qt widget APIs are thin in cxx-qt-lib
//      so we keep layout code on the C++ side for Phase A).
//
// MSVC-specific flags (captured from Phase 0 spike):
//   /Zc:__cplusplus  — Qt's compiler detection needs accurate value
//   /permissive-     — strict C++ conformance
//   /utf-8           — Qt string literal encoding
//   /EHsc            — C++ exception model (Qt requires)

use cxx_qt_build::CxxQtBuilder;

fn main() {
    CxxQtBuilder::new()
        .qt_module("Widgets")
        .qt_module("Gui")
        .file("src/main_window.rs")
        .cc_builder(|cc| {
            cc.file("cpp/window_builder.cpp");
            cc.file("cpp/render_widget.cpp");
            cc.file("cpp/first_launch_widget.cpp");
            cc.file("cpp/command_palette.cpp");
            cc.file("cpp/layer_stack_model.cpp");
            cc.file("cpp/layer_stack_widget.cpp");
            cc.file("cpp/scene_browser_model.cpp");
            cc.file("cpp/scene_browser_widget.cpp");
            cc.file("cpp/property_inspector_widget.cpp");
            cc.file("cpp/timeline_widget.cpp");
            cc.file("cpp/render_settings_widget.cpp");
            cc.file("cpp/node_graph_widget.cpp");
            cc.file("cpp/shortcut_registry.cpp");
            cc.include("cpp");
            cc.std("c++17");

            if cfg!(target_env = "msvc") {
                cc.flag_if_supported("/Zc:__cplusplus");
                cc.flag_if_supported("/permissive-");
                cc.flag_if_supported("/utf-8");
                cc.flag_if_supported("/EHsc");
            }
        })
        // Q_OBJECT headers — moc runs on each, generated .cpp is
        // auto-fed into the cc_builder.
        .qobject_header("cpp/render_widget.h")
        .qobject_header("cpp/first_launch_widget.h")
        .qobject_header("cpp/command_palette.h")
        .qobject_header("cpp/layer_stack_model.h")
        .qobject_header("cpp/layer_stack_widget.h")
        .qobject_header("cpp/scene_browser_model.h")
        .qobject_header("cpp/scene_browser_widget.h")
        .qobject_header("cpp/property_inspector_widget.h")
        .qobject_header("cpp/timeline_widget.h")
        .qobject_header("cpp/render_settings_widget.h")
        .qobject_header("cpp/node_graph_widget.h")
        .build();

    println!("cargo:rerun-if-changed=src/main_window.rs");
    println!("cargo:rerun-if-changed=src/app.rs");
    println!("cargo:rerun-if-changed=src/viewport.rs");
    println!("cargo:rerun-if-changed=cpp/window_builder.cpp");
    println!("cargo:rerun-if-changed=cpp/window_builder.h");
    println!("cargo:rerun-if-changed=cpp/render_widget.cpp");
    println!("cargo:rerun-if-changed=cpp/render_widget.h");
    println!("cargo:rerun-if-changed=cpp/first_launch_widget.cpp");
    println!("cargo:rerun-if-changed=cpp/first_launch_widget.h");
    println!("cargo:rerun-if-changed=cpp/command_palette.cpp");
    println!("cargo:rerun-if-changed=cpp/command_palette.h");
    println!("cargo:rerun-if-changed=cpp/layer_stack_model.cpp");
    println!("cargo:rerun-if-changed=cpp/layer_stack_model.h");
    println!("cargo:rerun-if-changed=cpp/layer_stack_widget.cpp");
    println!("cargo:rerun-if-changed=cpp/layer_stack_widget.h");
    println!("cargo:rerun-if-changed=cpp/scene_browser_model.cpp");
    println!("cargo:rerun-if-changed=cpp/scene_browser_model.h");
    println!("cargo:rerun-if-changed=cpp/scene_browser_widget.cpp");
    println!("cargo:rerun-if-changed=cpp/scene_browser_widget.h");
    println!("cargo:rerun-if-changed=cpp/property_inspector_widget.cpp");
    println!("cargo:rerun-if-changed=cpp/property_inspector_widget.h");
    println!("cargo:rerun-if-changed=cpp/timeline_widget.cpp");
    println!("cargo:rerun-if-changed=cpp/timeline_widget.h");
    println!("cargo:rerun-if-changed=cpp/render_settings_widget.cpp");
    println!("cargo:rerun-if-changed=cpp/render_settings_widget.h");
    println!("cargo:rerun-if-changed=cpp/node_graph_widget.cpp");
    println!("cargo:rerun-if-changed=cpp/node_graph_widget.h");
    println!("cargo:rerun-if-changed=cpp/shortcut_registry.cpp");
    println!("cargo:rerun-if-changed=cpp/shortcut_registry.h");
}
