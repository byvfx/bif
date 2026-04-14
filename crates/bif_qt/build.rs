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
            cc.include("cpp");
            cc.std("c++17");

            if cfg!(target_env = "msvc") {
                cc.flag_if_supported("/Zc:__cplusplus");
                cc.flag_if_supported("/permissive-");
                cc.flag_if_supported("/utf-8");
                cc.flag_if_supported("/EHsc");
            }
        })
        .build();

    println!("cargo:rerun-if-changed=src/main_window.rs");
    println!("cargo:rerun-if-changed=src/app.rs");
    println!("cargo:rerun-if-changed=cpp/window_builder.cpp");
    println!("cargo:rerun-if-changed=cpp/window_builder.h");
}
