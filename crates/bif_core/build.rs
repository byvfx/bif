//! Build script for bif_core.
//!
//! Compiles the USD C++ bridge via CMake and links the resulting library.
//! Optionally compiles the OIIO bridge when the `oiio` feature is enabled.
//! Uses caching to avoid rebuilding when source files haven't changed.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns the vcpkg triplet for the current platform.
fn vcpkg_triplet() -> &'static str {
    if cfg!(target_os = "windows") {
        "x64-windows"
    } else if cfg!(target_os = "macos") {
        "x64-osx"
    } else {
        "x64-linux"
    }
}

/// Returns platform-specific fallback paths for vcpkg root.
fn vcpkg_fallback_paths() -> Vec<String> {
    if cfg!(windows) {
        vec![
            r"G:\__projects\_programming\vcpkg".to_string(),
            r"C:\vcpkg".to_string(),
        ]
    } else {
        let mut paths = vec!["/opt/vcpkg".to_string(), "/usr/local/vcpkg".to_string()];
        if let Ok(home) = env::var("HOME") {
            paths.push(format!("{}/vcpkg", home));
        }
        paths
    }
}

/// Returns the CMake build output subdirectory (MSVC uses Release/, others use .).
fn cmake_build_subdir() -> &'static str {
    if cfg!(windows) {
        "Release"
    } else {
        "."
    }
}

/// Returns CMake generator arguments for the current platform.
fn cmake_generator_args() -> Vec<String> {
    if cfg!(windows) {
        vec![
            "-G".to_string(),
            "Visual Studio 17 2022".to_string(),
            "-A".to_string(),
            "x64".to_string(),
        ]
    } else {
        // Use default generator (Makefiles or Ninja if available)
        vec![]
    }
}

/// Returns vcpkg lib and bin paths for a given root.
fn vcpkg_lib_bin_paths(root: &str) -> (PathBuf, PathBuf) {
    let triplet = vcpkg_triplet();
    let base = PathBuf::from(root).join("installed").join(triplet);
    (base.join("lib"), base.join("bin"))
}

fn main() {
    // Build OIIO bridge if feature enabled
    #[cfg(feature = "oiio")]
    build_oiio_bridge();

    // Find vcpkg root with USD installed (toolchain + pxr headers must exist)
    let triplet = vcpkg_triplet();
    let vcpkg_root = env::var("VCPKG_ROOT")
        .ok()
        .into_iter()
        .chain(vcpkg_fallback_paths())
        .find(|root| {
            Path::new(root)
                .join("scripts/buildsystems/vcpkg.cmake")
                .exists()
                && Path::new(root)
                    .join(format!("installed/{}/include/pxr/pxr.h", triplet))
                    .exists()
        });

    if vcpkg_root.is_none() {
        println!("cargo:warning=vcpkg with USD not found, skipping USD bridge build");
        return;
    }
    let vcpkg_root = vcpkg_root.unwrap();
    let vcpkg_toolchain = format!("{}/scripts/buildsystems/vcpkg.cmake", vcpkg_root);

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
        build_usd_bridge(&cpp_dir, &build_dir, &vcpkg_toolchain);
    } else {
        println!("cargo:warning=USD bridge up to date, skipping CMake");
    }

    // Link the USD bridge library
    println!(
        "cargo:rustc-link-search=native={}",
        build_dir.join(cmake_build_subdir()).display()
    );
    println!("cargo:rustc-link-lib=static=usd_bridge");

    // Link USD libraries from vcpkg
    if let Ok(vcpkg_root) = env::var("VCPKG_ROOT") {
        let (lib_path, bin_path) = vcpkg_lib_bin_paths(&vcpkg_root);
        println!("cargo:rustc-link-search=native={}", lib_path.display());
        println!("cargo:rustc-link-search=native={}", bin_path.display());
    } else {
        for root in vcpkg_fallback_paths() {
            let (lib_path, bin_path) = vcpkg_lib_bin_paths(&root);
            if lib_path.exists() {
                println!("cargo:rustc-link-search=native={}", lib_path.display());
                println!("cargo:rustc-link-search=native={}", bin_path.display());
                break;
            }
        }
    }

    // USD core libraries (order matters for linking)
    let usd_libs = [
        "usd_usdRender", // For render settings
        "usd_usdSkel",   // For skeleton/skinning
        "usd_usdVol",    // For volumes
        "usd_usdLux",    // For lights
        "usd_usdShade",  // For materials/shaders
        "usd_usdGeom",
        "usd_usd",
        "usd_kind", // For UsdModelAPI kind tokens
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

    // Platform system libraries
    if cfg!(windows) {
        println!("cargo:rustc-link-lib=ws2_32");
        println!("cargo:rustc-link-lib=dbghelp");
        println!("cargo:rustc-link-lib=shlwapi");
        println!("cargo:rustc-link-lib=advapi32");
    } else {
        println!("cargo:rustc-link-lib=stdc++");
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

    if cfg!(windows) {
        // Visual Studio 2022 bundled CMake
        let vs_cmake = r"C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe";
        if Path::new(vs_cmake).exists() {
            return vs_cmake.to_string();
        }

        let fallbacks = [
            r"C:\Program Files\CMake\bin\cmake.exe",
            r"C:\Program Files (x86)\CMake\bin\cmake.exe",
        ];
        for path in fallbacks {
            if Path::new(path).exists() {
                return path.to_string();
            }
        }
    } else {
        let fallbacks = ["/usr/bin/cmake", "/usr/local/bin/cmake", "/snap/bin/cmake"];
        for path in fallbacks {
            if Path::new(path).exists() {
                return path.to_string();
            }
        }
    }

    panic!("CMake not found. Install CMake or add it to PATH.");
}

/// Build the USD bridge using CMake.
fn build_usd_bridge(cpp_dir: &Path, build_dir: &Path, toolchain: &str) {
    // Create build directory
    fs::create_dir_all(build_dir).expect("Failed to create build directory");

    // Find CMake
    let cmake = find_cmake();

    // CMake configure
    let mut configure_args = vec![
        "-S".to_string(),
        cpp_dir.to_str().unwrap().to_string(),
        "-B".to_string(),
        ".".to_string(),
    ];
    configure_args.extend(cmake_generator_args());
    configure_args.push(format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain));
    configure_args.push("-DCMAKE_BUILD_TYPE=Release".to_string());
    // Bridges consume pre-installed vcpkg packages. Force classic mode so the
    // toolchain doesn't flip to manifest mode on discovering the repo-root
    // vcpkg.json (which redirects find_package to an empty build-dir prefix).
    configure_args.push("-DVCPKG_MANIFEST_MODE=OFF".to_string());

    let configure_status = Command::new(&cmake)
        .current_dir(build_dir)
        .args(&configure_args)
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

// ============================================================================
// OIIO Bridge Build (feature-gated)
// ============================================================================

#[cfg(feature = "oiio")]
fn build_oiio_bridge() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = Path::new(&manifest_dir).parent().unwrap().parent().unwrap();
    let cpp_dir = workspace_root.join("cpp").join("oiio_bridge");
    let out_dir = env::var("OUT_DIR").unwrap();
    let build_dir = Path::new(&out_dir).join("oiio_bridge_build");

    // Output library path
    let lib_path = if cfg!(windows) {
        build_dir.join("Release").join("oiio_bridge.lib")
    } else {
        build_dir.join("liboiio_bridge.a")
    };

    // Source files to track for changes
    let source_files = vec![
        cpp_dir.join("oiio_bridge.cpp"),
        cpp_dir.join("oiio_bridge.h"),
        cpp_dir.join("CMakeLists.txt"),
    ];

    // Emit rerun-if-changed for all source files
    for src in &source_files {
        println!("cargo:rerun-if-changed={}", src.display());
    }

    // Check if rebuild is needed
    let needs_rebuild = needs_cmake_rebuild(&lib_path, &source_files);

    if needs_rebuild {
        println!("cargo:warning=Building OIIO bridge via CMake...");
        build_oiio_bridge_cmake(&cpp_dir, &build_dir);
    } else {
        println!("cargo:warning=OIIO bridge up to date, skipping CMake");
    }

    // Link the OIIO bridge library
    println!(
        "cargo:rustc-link-search=native={}",
        build_dir.join(cmake_build_subdir()).display()
    );
    println!("cargo:rustc-link-lib=static=oiio_bridge");

    // Link OIIO libraries from vcpkg. Capture the resolved lib dir so we can
    // probe for version-dependent import-lib names below.
    let vcpkg_lib_dir = if let Ok(vcpkg_root) = env::var("VCPKG_ROOT") {
        let (lib_path, bin_path) = vcpkg_lib_bin_paths(&vcpkg_root);
        println!("cargo:rustc-link-search=native={}", lib_path.display());
        println!("cargo:rustc-link-search=native={}", bin_path.display());
        Some(lib_path)
    } else {
        let mut found = None;
        for root in vcpkg_fallback_paths() {
            let (lib_path, bin_path) = vcpkg_lib_bin_paths(&root);
            if lib_path.exists() {
                println!("cargo:rustc-link-search=native={}", lib_path.display());
                println!("cargo:rustc-link-search=native={}", bin_path.display());
                found = Some(lib_path);
                break;
            }
        }
        found
    };

    // Link OpenImageIO and its dependencies. Names are given as version-less
    // bases; `resolve_lib_name` tolerates the `-<major>_<minor>` suffixes that
    // OpenEXR/Imath/etc. carry and that change across vcpkg baselines (e.g.
    // `OpenEXR` -> `OpenEXR-3_4`). Exact match wins, so today's names are
    // emitted unchanged — this only kicks in when the baseline moves.
    let oiio_libs = [
        "OpenImageIO",
        "OpenImageIO_Util",
        "OpenEXR",
        "OpenEXRCore",
        "OpenEXRUtil",
        "Imath",
        "IlmThread",
        "Iex",
        "tiff",
        "jpeg",
        "libpng16",
    ];

    for base in oiio_libs {
        let name = vcpkg_lib_dir
            .as_deref()
            .map(|dir| resolve_lib_name(dir, base))
            .unwrap_or_else(|| base.to_string());
        println!("cargo:rustc-link-lib={}", name);
    }

    // zlib's import-lib name varies by vcpkg baseline: older ports ship
    // `zlib.lib`, newer ones ship `z.lib`. Link whichever is present.
    let zlib = vcpkg_lib_dir.as_deref().map(zlib_link_name).unwrap_or("z");
    println!("cargo:rustc-link-lib={}", zlib);

    // C++ standard library
    if cfg!(windows) {
        // MSVC links automatically
    } else {
        println!("cargo:rustc-link-lib=stdc++");
    }
}

/// Returns the cargo link name for zlib based on which import lib vcpkg
/// installed: older ports produce `zlib.lib` (link name `zlib`), newer ports
/// produce `z.lib` (link name `z`).
#[cfg(feature = "oiio")]
fn zlib_link_name(lib_dir: &Path) -> &'static str {
    // Non-Windows always links `-lz` (libz.a/.so); the .lib probe is Windows-only.
    if !cfg!(windows) {
        return "z";
    }
    if lib_dir.join("zlib.lib").exists() {
        "zlib"
    } else if lib_dir.join("z.lib").exists() {
        "z"
    } else {
        // Neither present — likely a stale VCPKG_ROOT. Warn so the misconfig is
        // visible instead of failing later with a confusing "cannot find z.lib".
        println!(
            "cargo:warning=zlib import lib not found in {} (expected zlib.lib or z.lib); defaulting to 'z'",
            lib_dir.display()
        );
        "z"
    }
}

/// Resolve a vcpkg lib's cargo link name from a version-less base, tolerating
/// the `-<major>_<minor>` suffixes that OpenEXR/Imath/etc. carry and that change
/// across vcpkg baselines (e.g. `OpenEXR` -> `OpenEXR-3_4`). Exact (unversioned)
/// match wins, so today's installed names are emitted unchanged; the glob only
/// kicks in when the baseline moves. Falls back to `base` if nothing matches.
#[cfg(feature = "oiio")]
fn resolve_lib_name(lib_dir: &Path, base: &str) -> String {
    // Per-platform file shape: Windows `Foo.lib` / `Foo-3_4.lib`; unix
    // `libFoo.{a,so,dylib}` / `libFoo-3_4.{...}`. The cargo link name is the
    // stem minus the unix `lib` prefix and the extension.
    let (lead, exts): (&str, &[&str]) = if cfg!(windows) {
        ("", &["lib"][..])
    } else {
        ("lib", &["a", "so", "dylib"][..])
    };

    // Fast path: exact unversioned name present -> preserve current behavior.
    for ext in exts {
        if lib_dir.join(format!("{lead}{base}.{ext}")).exists() {
            return base.to_string();
        }
    }

    // Versioned: `<lead><base>-<digits/underscores>.<ext>`. The all-digits guard
    // means `OpenEXR` matches `OpenEXR-3_4` but NOT `OpenEXRCore-3_4`.
    if let Ok(entries) = fs::read_dir(lib_dir) {
        let vprefix = format!("{lead}{base}-");
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let fname = fname.to_string_lossy();
            let Some(stem) = exts
                .iter()
                .find_map(|e| fname.strip_suffix(&format!(".{e}")))
            else {
                continue;
            };
            if let Some(version) = stem.strip_prefix(&vprefix) {
                if !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '_') {
                    return stem.strip_prefix(lead).unwrap_or(stem).to_string();
                }
            }
        }
    }

    base.to_string()
}

#[cfg(feature = "oiio")]
fn build_oiio_bridge_cmake(cpp_dir: &Path, build_dir: &Path) {
    // Create build directory
    fs::create_dir_all(build_dir).expect("Failed to create OIIO build directory");

    // Find CMake
    let cmake = find_cmake();

    // Find vcpkg toolchain file
    let vcpkg_root = env::var("VCPKG_ROOT").unwrap_or_else(|_| {
        vcpkg_fallback_paths()
            .into_iter()
            .find(|p| {
                Path::new(p)
                    .join("scripts/buildsystems/vcpkg.cmake")
                    .exists()
            })
            .unwrap_or_else(|| panic!("VCPKG_ROOT not set and no vcpkg found in fallback paths"))
    });
    let toolchain = format!("{}/scripts/buildsystems/vcpkg.cmake", vcpkg_root);

    // CMake configure
    let mut configure_args = vec![
        "-S".to_string(),
        cpp_dir.to_str().unwrap().to_string(),
        "-B".to_string(),
        ".".to_string(),
    ];
    configure_args.extend(cmake_generator_args());
    configure_args.push(format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain));
    configure_args.push("-DCMAKE_BUILD_TYPE=Release".to_string());
    // Bridges consume pre-installed vcpkg packages. Force classic mode so the
    // toolchain doesn't flip to manifest mode on discovering the repo-root
    // vcpkg.json (which redirects find_package to an empty build-dir prefix).
    configure_args.push("-DVCPKG_MANIFEST_MODE=OFF".to_string());

    let configure_status = Command::new(&cmake)
        .current_dir(build_dir)
        .args(&configure_args)
        .status()
        .expect("Failed to run cmake configure for OIIO bridge");

    if !configure_status.success() {
        panic!("CMake configure failed for OIIO bridge");
    }

    // CMake build
    let build_status = Command::new(&cmake)
        .current_dir(build_dir)
        .args(["--build", ".", "--config", "Release", "--parallel"])
        .status()
        .expect("Failed to run cmake build for OIIO bridge");

    if !build_status.success() {
        panic!("CMake build failed for OIIO bridge");
    }
}
