---
title: "ADR-004: C++ Bridge for USD"
type: adr
tags: [architecture, usd, ffi]
created: "2026-04-05"
updated: "2026-04-05"
sources: [ARCHITECTURE.md, ARCHITECTURE_REFACTORS.md, ARCHITECTURE_REVIEW.md]
---

# ADR-004: C++ Bridge for USD FFI

## Context

BIF needs to read and write USD files (both text USDA and binary USDC), handle references, payloads, composition arcs, and material networks. USD is a large C++ library (~2M LOC) maintained by Pixar. The question: how should a Rust application interface with it?

BIF started with a pure Rust USDA parser (M0-M11) for text files. At M13, production needs required binary USDC support, reference resolution, and eventually full bidirectional read/write. A C++ FFI bridge was introduced.

## Decision

**Use a C++ bridge built via CMake, linked through Rust's FFI.** The bridge lives in `cpp/usd_bridge/` and is compiled by `bif_core/build.rs`. Rust calls into C-linkage functions that wrap USD C++ API calls.

Architecture:

```text
BIF (Rust) --FFI--> C++ Bridge (cpp/usd_bridge/) --links--> Pixar USD Libraries
```

The bridge was split into three files (Phase 1 refactor, complete):

| File | Contents | Testability |
|------|----------|-------------|
| `ffi_raw.rs` | `#[repr(C)]` structs, `extern "C"` blocks | Not testable without DLLs |
| `ffi_convert.rs` | Pure Rust conversion: raw slices to domain types | Fully testable (44 tests) |
| `cpp_bridge.rs` | `UsdStage` wrapper, public domain types | Needs USD DLLs |

## Alternatives Considered

| Option | Pros | Cons | Verdict |
|--------|------|------|---------|
| **C++ bridge via CMake** | Full USD API access, production-proven pattern (Arnold, Katana use same approach), binary USDC support | Build complexity (requires VS 2022, CMake), Windows-only initially, unsafe FFI boundary | **Chosen** |
| **Pure Rust USD parser** | No FFI, cross-platform, safe | Only handles USDA text, can't do USDC binary, can't resolve references/payloads, would need to reimplement ~2M LOC | Rejected for production (kept for M0-M11 PoC) |
| **usd-rs crate** | Community-maintained bindings | Immature, incomplete API coverage, version lag behind Pixar releases | Rejected |
| **Process bridge (subprocess)** | No FFI, any language | Serialization overhead, latency, complex IPC for interactive use | Rejected |

## Consequences

**Positive:**

- Full access to USD API including USDC binary, composition, references, payloads
- Battle-tested pattern used by major VFX tools
- Phase 1 FFI split made conversion logic testable without C++ DLLs (44 tests in ffi_convert.rs)
- Clean error handling via `UsdBridgeError` enum with `thiserror`
- `Drop` impl on `UsdStage` and `UsdEditLayer` ensures cleanup

**Negative:**

- Build requires Visual Studio 2022 C++ workload + CMake
- Currently Windows-only (Phase 2 cross-platform refactor planned)
- `setup_usd_env.ps1` must be sourced before running bif_core tests
- Tests must run single-threaded (`--test-threads=1`) because USD C++ is not thread-safe
- `unsafe impl Send + Sync for UsdStage` is a soundness hole — should use Mutex (fix recommended)
- Raw pointer slice construction trusts C++ for length — a C++ bug causes undefined behavior in Rust

**Safety Measures:**

- Every FFI call wrapped in safe Rust function that checks error codes
- Typed `UsdBridgeResult<T>` used consistently
- Conversion logic (ffi_convert.rs) is pure Rust, fully tested
- USD libraries provided by user via `PXR_USD_PATH` environment variable (not vendored)

**Planned Expansion:**

- v0.14.0 requires new FFI functions: `SdfLayer` read, `GetEditTarget`, `GetPrimStack`, payload load/unload
- Phase 2 refactor: platform-detect CMake generator, vcpkg triplet, add Linux support

## Related

- [[crate-structure|Crate Structure]] — C++ bridge lives in bif_core
- [[003-hybrid-usd-workflow|ADR 003: Hybrid USD Workflow]] — Layer awareness requires FFI expansion
