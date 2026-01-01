// Build script for linking Embree library
//
// Uses vcpkg-installed Embree 4 on Windows.
// Install via: vcpkg install embree[geometry-triangle,geometry-instance]:x64-windows

fn main() {
    // vcpkg integration handles the linking automatically on Windows
    // but we need to tell Cargo the library name

    // Embree 4 library name (vcpkg installs embree4.lib)
    println!("cargo:rustc-link-lib=embree4");

    // On Windows with vcpkg integration, vcpkg.exe automatically adds
    // the correct link paths via MSBuild integration. For manual builds:
    if let Ok(vcpkg_root) = std::env::var("VCPKG_ROOT") {
        let lib_path = format!("{}\\installed\\x64-windows\\lib", vcpkg_root);
        println!("cargo:rustc-link-search=native={}", lib_path);
    }

    // Rebuild if this build script changes
    println!("cargo:rerun-if-changed=build.rs");
}
