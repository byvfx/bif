# USD C++ Integration Setup

This document explains how to set up the USD C++ integration for BIF.

## Overview

BIF uses Pixar's Universal Scene Description (USD) C++ library for:
- Loading binary `.usdc` files
- Resolving file references (`@path.usda@</Prim>`)
- Full USD scene graph traversal

The pure Rust parser (`load_usda()`) remains available for simple `.usda` text files.

## Prerequisites

1. **Visual Studio 2022** with C++ development tools
2. **vcpkg** package manager

## Installation

### 1. Install vcpkg (if not already installed)

```powershell
cd D:\__projects\_programming  # or your preferred location
git clone https://github.com/microsoft/vcpkg.git
cd vcpkg
.\bootstrap-vcpkg.bat
```

### 2. Install USD

```powershell
.\vcpkg.exe install usd:x64-windows
```

> **Note:** This takes 10-40 minutes depending on your machine. USD has many dependencies (TBB, OpenSubdiv, etc.)

### 3. Set Environment Variables

Before running BIF with USD features, you need to set up the environment.

**Option A: Use the helper script (recommended)**

```powershell
. .\setup_usd_env.ps1
```

**Option B: Set manually**

```powershell
$VcpkgRoot = "D:\__projects\_programming\vcpkg"

# Required for USD DLLs
$env:PATH = "$VcpkgRoot\installed\x64-windows\bin;$env:PATH"

# Required for build.rs
$env:VCPKG_ROOT = $VcpkgRoot

# Required for USD plugins (critical!)
$pluginDirs = Get-ChildItem "$VcpkgRoot\installed\x64-windows\bin\usd" -Directory | 
    ForEach-Object { $_.FullName + "\resources" }
$env:PXR_PLUGINPATH_NAME = $pluginDirs -join ";"
```

### 4. Build and Test

```powershell
cargo build --package bif_core
cargo test --package bif_core test_load_usd -- --ignored
```

## Environment Variables Explained

| Variable | Purpose |
|----------|---------|
| `VCPKG_ROOT` | Points to vcpkg installation. Used by `build.rs` to find libraries. |
| `PATH` | Must include USD DLL directory for runtime. |
| `PXR_PLUGINPATH_NAME` | **Critical!** USD uses a plugin architecture. Without this, `UsdStage::Open()` will crash because it can't find file format plugins. |

## Usage

### Load USD files (C++ bridge)

```rust
use bif_core::usd::load_usd;

// Supports .usda, .usdc, and .usd files
// Automatically resolves references
let scene = load_usd("path/to/scene.usdc")?;
```

### Load USDA files (pure Rust)

```rust
use bif_core::usd::load_usda;

// Pure Rust parser, no C++ dependencies
// Only supports text .usda files
let scene = load_usda("path/to/scene.usda")?;
```

## Troubleshooting

### Crash with exit code 0x80000003

**Symptom:** Program crashes immediately when creating `UsdStage`

**Cause:** USD plugins not found

**Solution:** Set `PXR_PLUGINPATH_NAME` correctly (see above)

### LNK1181: cannot open input file 'usd_*.lib'

**Symptom:** Linker error during `cargo build`

**Cause:** vcpkg libraries not found

**Solution:** Set `VCPKG_ROOT` environment variable

### STATUS_DLL_NOT_FOUND

**Symptom:** Test or binary won't run

**Cause:** USD DLLs not in PATH

**Solution:** Add vcpkg bin directory to PATH

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     Rust Application                     │
├─────────────────────────────────────────────────────────┤
│  load_usda() (pure Rust)  │  load_usd() (C++ bridge)    │
├───────────────────────────┼─────────────────────────────┤
│  usd/parser.rs            │  usd/cpp_bridge.rs (FFI)    │
│  (Rust USDA parser)       │           │                 │
│                           │           ▼                 │
│                           │  cpp/usd_bridge/            │
│                           │  (C++ shim, extern "C")     │
│                           │           │                 │
│                           │           ▼                 │
│                           │  Pixar USD C++ Library      │
└───────────────────────────┴─────────────────────────────┘
```

## Files

| File | Purpose |
|------|---------|
| `cpp/usd_bridge/usd_bridge.h` | C API declarations |
| `cpp/usd_bridge/usd_bridge.cpp` | C++ implementation wrapping USD |
| `cpp/usd_bridge/CMakeLists.txt` | CMake build config |
| `crates/bif_core/build.rs` | Cargo build script (runs CMake) |
| `crates/bif_core/src/usd/cpp_bridge.rs` | Rust FFI wrapper (~500 LOC) |
| `setup_usd_env.ps1` | PowerShell environment setup script |
