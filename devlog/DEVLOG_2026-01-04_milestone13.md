# Devlog: Milestone 13 - USD C++ Integration

**Date:** January 4, 2026
**Duration:** ~4 hours (including vcpkg build time)
**Status:** ✅ Complete

## Overview

Added support for USDC binary files and USD file references by integrating Pixar's USD C++ library via FFI.

## Implementation

### 1. vcpkg USD Installation

```powershell
.\vcpkg.exe install usd:x64-windows  # ~12 minutes
```

USD 25.11 installed with TBB and other dependencies.

### 2. C++ Bridge (`cpp/usd_bridge/`)

Created a C++ shim with `extern "C"` functions for Rust FFI:

- `usd_bridge_open_stage()` - Open USD/USDA/USDC files
- `usd_bridge_close_stage()` - Clean up
- `usd_bridge_get_mesh()` - Extract mesh data
- `usd_bridge_get_instancer()` - Extract point instancer data
- `usd_bridge_export_stage()` - Export to USD

The implementation caches mesh and instancer data on first access for efficient FFI transfer.

### 3. CMake Integration (`build.rs`)

Created a `build.rs` that:
- Finds CMake (checks VS2022 bundled path)
- Configures with vcpkg toolchain
- Builds static library with caching
- Links all required USD libraries

### 4. Rust FFI Wrapper (`cpp_bridge.rs`)

~500 LOC of safe Rust wrapper:
- `UsdStage` - Handle to open stage, auto-closes on drop
- `UsdMeshData` - Mesh vertices, indices, normals, transform
- `UsdInstancerData` - Instancer transforms and prototype paths
- Full error handling with `UsdBridgeError` enum

## Key Discoveries

### 1. Plugin Path Required

USD uses a plugin architecture. Without `PXR_PLUGINPATH_NAME`, `UsdStage::CreateInMemory()` crashes!

```powershell
$env:PXR_PLUGINPATH_NAME = (Get-ChildItem "$VcpkgRoot\bin\usd" -Directory | 
    ForEach-Object { $_.FullName + "\resources" }) -join ";"
```

### 2. USD 25.11 API Changes

`GetForwardedTargets()` now takes an output parameter:

```cpp
// Old (USD 24.x)
SdfPathVector paths = rel.GetForwardedTargets();

// New (USD 25.x)
SdfPathVector paths;
rel.GetForwardedTargets(&paths);
```

### 3. vcpkg Library Layout

USD import libs are in `bin/` not `lib/`:

```
installed/x64-windows/
├── bin/           # DLLs AND import libs (.lib)
│   ├── usd_ar.dll
│   ├── usd_ar.lib  # Import lib here, not in lib/!
│   └── usd/        # Plugin directories
├── lib/           # Only TBB and other static libs
```

## Testing

Three integration tests verify the bridge works:

```powershell
. .\setup_usd_env.ps1
cargo test --package bif_core test_load_usd -- --ignored
```

- `test_load_usd_cube` - Load simple mesh
- `test_load_usd_with_references` - Verify reference resolution
- `test_usda_and_cpp_bridge_produce_same_mesh` - Compare Rust vs C++ parsing

## Files Added

| File | Purpose | LOC |
|------|---------|-----|
| `cpp/usd_bridge/usd_bridge.h` | C API declarations | ~100 |
| `cpp/usd_bridge/usd_bridge.cpp` | C++ implementation | ~330 |
| `cpp/usd_bridge/CMakeLists.txt` | Build config | ~50 |
| `crates/bif_core/build.rs` | CMake automation | ~210 |
| `crates/bif_core/src/usd/cpp_bridge.rs` | Rust FFI | ~500 |
| `setup_usd_env.ps1` | Environment setup | ~20 |
| `USD_SETUP.md` | Documentation | ~150 |

## Architecture

```
┌────────────────────────────────────────────────┐
│               Rust Application                  │
├─────────────────────┬──────────────────────────┤
│  load_usda()        │  load_usd()              │
│  (pure Rust)        │  (C++ bridge)            │
├─────────────────────┼──────────────────────────┤
│  usd/parser.rs      │  usd/cpp_bridge.rs       │
│                     │       │ FFI              │
│                     │       ▼                  │
│                     │  usd_bridge.cpp          │
│                     │       │ Link             │
│                     │       ▼                  │
│                     │  Pixar USD Library       │
└─────────────────────┴──────────────────────────┘
```

## Time Breakdown

- vcpkg USD install: ~12 min (automated build)
- C++ bridge implementation: ~1 hour
- Rust FFI wrapper: ~1 hour
- Debugging plugin path issue: ~1.5 hours
- Documentation and cleanup: ~0.5 hours

## Lessons Learned

1. **Read the debug output** - USD crashes silently without plugins
2. **Check API changelogs** - USD 25.x changed several APIs
3. **vcpkg layouts vary** - Don't assume lib/ contains all libs
4. **Environment matters** - Runtime needs different setup than build

## Next Steps (Milestone 14)

- UsdPreviewSurface material parsing
- Texture file loading
- Material assignment to meshes
