// Phase 0 spike build script.
//
// Uses `qt-build-utils` for Qt 6 detection (the half of cxx-qt we
// actually need in Phase 0) and `cxx_build` for the Rust<->C++
// bridge. cxx-qt macros are deferred to Phase A when we have
// Rust-side QObjects; using them here would force us to wrap
// QApplication / QMainWindow in Rust prematurely.
//
// Detection: qt-build-utils follows Qt6_DIR / CMAKE_PREFIX_PATH /
// QMAKE. setup_qt_env.ps1 sets all three. If Qt isn't found, this
// fails with a clear message and Phase 0 gate never even compiles.

use qt_build_utils::QtBuild;

fn main() {
    // Qt modules we link: Core, Gui, Widgets (cascade is automatic).
    let mut qt = QtBuild::new(vec!["Core".into(), "Gui".into(), "Widgets".into()]).expect(
        "Qt 6 not found. Source setup_qt_env.ps1 (sets Qt6_DIR + CMAKE_PREFIX_PATH)\n\
             before running cargo build.",
    );

    // Run moc on headers that declare Q_OBJECT (only render_widget.h
    // in the spike). qt-build-utils' moc() returns the generated
    // .cpp file path we append to the cxx build.
    let moc = qt.moc(
        "cpp/render_widget.h",
        qt_build_utils::MocArguments::default(),
    );

    let mut builder = cxx_build::bridge("src/bridge.rs");
    builder
        .file("cpp/render_widget.cpp")
        .file("cpp/app_main.cpp")
        .file(moc.cpp)
        .include("cpp")
        .std("c++17");
    // NOTE: QT_NO_KEYWORDS (used in cxx-qt projects to avoid `slots`
    // colliding with Rust keywords) is intentionally NOT set here —
    // we want `signals:` / `emit` in render_widget.h to work.

    // MSVC + Qt requires /Zc:__cplusplus or Qt's compiler detection
    // bails (it reads __cplusplus to infer C++17). /permissive- and
    // /utf-8 are Qt's recommended MSVC settings.
    if cfg!(target_env = "msvc") {
        builder
            .flag_if_supported("/Zc:__cplusplus")
            .flag_if_supported("/permissive-")
            .flag_if_supported("/utf-8")
            .flag_if_supported("/EHsc");
    }

    for include in qt.include_paths() {
        builder.include(include);
    }

    // Emit Qt lib link directives for cargo *and* feed cflags/defines
    // into our cxx builder. Must be called AFTER all cxx_build config
    // because it may set compile-time defines that the bridge needs.
    qt.cargo_link_libraries(&mut builder);

    builder.compile("bif_qt_spike_bridge");

    println!("cargo:rerun-if-changed=cpp/render_widget.cpp");
    println!("cargo:rerun-if-changed=cpp/render_widget.h");
    println!("cargo:rerun-if-changed=cpp/app_main.cpp");
    println!("cargo:rerun-if-changed=cpp/app_main.h");
    println!("cargo:rerun-if-changed=src/bridge.rs");
}
