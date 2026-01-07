//! Build script for bif_core.
//!
//! Compiles the USD C++ bridge via CMake and links the resulting library.
//! Uses caching to avoid rebuilding when source files haven't changed.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Paths
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = Path::new(&manifest_dir).parent().unwrap().parent().unwrap();
    let cpp_dir = workspace_root.join("cpp").join("usd_bridge");
    let out_dir = env::var("OUT_DIR").unwrap();
    let build_dir = Path::new(&out_dir).join("usd_bridge_build");

    // Output library path
    let lib_path = if cfg!(windows) {
        build_dir.join("Release").join("usd_bridge.lib")
    } else {
        build_dir.join("libusd_bridge.a")
    };

    // Source files to track for changes
    let source_files = vec![
        cpp_dir.join("usd_bridge.cpp"),
        cpp_dir.join("usd_bridge.h"),
        cpp_dir.join("CMakeLists.txt"),
    ];

    // Emit rerun-if-changed for all source files
    for src in &source_files {
        println!("cargo:rerun-if-changed={}", src.display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    // Check if rebuild is needed
    let needs_rebuild = needs_cmake_rebuild(&lib_path, &source_files);

    if needs_rebuild {
        println!("cargo:warning=Building USD bridge via CMake...");
        build_usd_bridge(&cpp_dir, &build_dir);
    } else {
        println!("cargo:warning=USD bridge up to date, skipping CMake");
    }

    // Link the USD bridge library
    println!(
        "cargo:rustc-link-search=native={}",
        build_dir.join("Release").display()
    );
    println!("cargo:rustc-link-lib=static=usd_bridge");

    // Link USD libraries from vcpkg
    // Note: vcpkg puts USD import libs (.lib) in bin/ folder alongside DLLs
    if let Ok(vcpkg_root) = env::var("VCPKG_ROOT") {
        let lib_path = format!("{}\\installed\\x64-windows\\lib", vcpkg_root);
        let bin_path = format!("{}\\installed\\x64-windows\\bin", vcpkg_root);
        println!("cargo:rustc-link-search=native={}", lib_path);
        println!("cargo:rustc-link-search=native={}", bin_path);
    } else {
        // Try to find vcpkg in common locations
        let possible_vcpkg_paths = vec![
            (
                "D:\\__projects\\_programming\\vcpkg\\installed\\x64-windows\\lib",
                "D:\\__projects\\_programming\\vcpkg\\installed\\x64-windows\\bin",
            ),
            (
                "C:\\vcpkg\\installed\\x64-windows\\lib",
                "C:\\vcpkg\\installed\\x64-windows\\bin",
            ),
        ];
        for (lib_path, bin_path) in possible_vcpkg_paths {
            if Path::new(lib_path).exists() {
                println!("cargo:rustc-link-search=native={}", lib_path);
                println!("cargo:rustc-link-search=native={}", bin_path);
                break;
            }
        }
    }

    // USD core libraries (order matters for linking)
    let usd_libs = [
        "usd_usdGeom",
        "usd_usd",
        "usd_sdf",
        "usd_tf",
        "usd_gf",
        "usd_vt",
        "usd_arch",
        "usd_trace",
        "usd_work",
        "usd_plug",
        "usd_ar",
        "usd_js",
        "usd_pcp",
    ];

    for lib in usd_libs {
        println!("cargo:rustc-link-lib={}", lib);
    }

    // TBB (required by USD)
    println!("cargo:rustc-link-lib=tbb12");

    // Windows system libraries
    if cfg!(windows) {
        println!("cargo:rustc-link-lib=ws2_32");
        println!("cargo:rustc-link-lib=dbghelp");
        println!("cargo:rustc-link-lib=shlwapi");
        println!("cargo:rustc-link-lib=advapi32");
    }
}

/// Check if CMake rebuild is needed by comparing timestamps.
fn needs_cmake_rebuild(lib_path: &Path, source_files: &[PathBuf]) -> bool {
    // If library doesn't exist, need to build
    let lib_mtime = match fs::metadata(lib_path) {
        Ok(meta) => match meta.modified() {
            Ok(time) => time,
            Err(_) => return true,
        },
        Err(_) => return true,
    };

    // Check if any source file is newer than the library
    for src in source_files {
        if let Ok(meta) = fs::metadata(src) {
            if let Ok(src_mtime) = meta.modified() {
                if src_mtime > lib_mtime {
                    return true;
                }
            }
        }
    }

    false
}

/// Find CMake executable path.
fn find_cmake() -> String {
    // Check if cmake is in PATH first
    if Command::new("cmake").arg("--version").output().is_ok() {
        return "cmake".to_string();
    }

    // Visual Studio 2022 bundled CMake
    let vs_cmake = "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\Common7\\IDE\\CommonExtensions\\Microsoft\\CMake\\CMake\\bin\\cmake.exe";
    if Path::new(vs_cmake).exists() {
        return vs_cmake.to_string();
    }

    // Fallback locations
    let fallbacks = [
        "C:\\Program Files\\CMake\\bin\\cmake.exe",
        "C:\\Program Files (x86)\\CMake\\bin\\cmake.exe",
    ];

    for path in fallbacks {
        if Path::new(path).exists() {
            return path.to_string();
        }
    }

    panic!("CMake not found. Install CMake or add it to PATH.");
}

/// Build the USD bridge using CMake.
fn build_usd_bridge(cpp_dir: &Path, build_dir: &Path) {
    // Create build directory
    fs::create_dir_all(build_dir).expect("Failed to create build directory");

    // Find CMake
    let cmake = find_cmake();

    // Find vcpkg toolchain file
    let vcpkg_root = env::var("VCPKG_ROOT")
        .unwrap_or_else(|_| "D:\\__projects\\_programming\\vcpkg".to_string());
    let toolchain = format!("{}/scripts/buildsystems/vcpkg.cmake", vcpkg_root);

    // CMake configure
    let configure_status = Command::new(&cmake)
        .current_dir(build_dir)
        .args([
            "-S",
            cpp_dir.to_str().unwrap(),
            "-B",
            ".",
            "-G",
            "Visual Studio 17 2022",
            "-A",
            "x64",
            &format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain),
            "-DCMAKE_BUILD_TYPE=Release",
        ])
        .status()
        .expect("Failed to run cmake configure");

    if !configure_status.success() {
        panic!("CMake configure failed");
    }

    // CMake build
    let build_status = Command::new(&cmake)
        .current_dir(build_dir)
        .args(["--build", ".", "--config", "Release", "--parallel"])
        .status()
        .expect("Failed to run cmake build");

    if !build_status.success() {
        panic!("CMake build failed");
    }
}
