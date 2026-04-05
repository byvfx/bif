# BIF Codebase Code Review — 2026-04-05

**Reviewer:** Independent (Maple polecat agent)
**Scope:** bif_math, bif_core, bif_renderer, bif_viewport, bif_viewer, bif_maketx, cpp/usd_bridge, cpp/oiio_bridge, WGSL shaders

---

## Executive Summary

BIF is a well-structured VFX renderer/assembler for a side project at this scale. The Rust codebase is generally clean with good use of `thiserror`, `Result`-based error propagation, extensive doc comments, and meaningful test coverage on the FFI conversion layer and math primitives. The C++ USD bridge is the most complex subsystem and is handled defensively — `unsafe` blocks are narrowly scoped, raw-to-safe conversions are isolated in `ffi_convert.rs`, and the Send/Sync impls for `UsdStage` have detailed documented safety rationale backed by thread_local storage in the C++ side.

The biggest hazards are two substantial dead-code modules (`scene_pipeline.rs`, ~585 lines; `node_graph/eval.rs`, ~725 lines) suppressed with `#![allow(dead_code)]`, debug timing output hardcoded to `true` in the C++ bridge, and a `MAX_INSTANCES` constant defined twice in `lib.rs`. None of these are correctness bugs today, but they signal incomplete refactors that will become maintenance debt. There are also a handful of library-level panics that should become errors.

The WGSL shaders are reasonable in quality. The PBR split-sum IBL implementation in `basic.wgsl` is standard and correct. The adjugate-based normal transform for non-uniform scale is a nice touch. A few precision and correctness notes apply (see Medium findings). The radiance cache's lock-free scheme is architecturally sound for a preview renderer but carries a documented bias.

---

## Findings by Severity

### Critical

*No critical findings.*

---

### High

#### H1 — C++ bridge: `g_log_timing = true` hardcoded, always spams stdout

**File:** `cpp/usd_bridge/usd_bridge.cpp:81`

**Description:**
```cpp
static bool g_log_timing   = true;    // [USD_BRIDGE] timing breakdowns
static bool g_log_variants = true;    // [BIF_TEX] variant selection
```
Both flags are `true`. Every call to `usd_bridge_open_stage` unconditionally prints a full timing breakdown to `std::cout`. `g_log_variants` similarly dumps variant selection to stdout. This pollutes any log capture, interferes with CI (output is interleaved with test output), and is surprising to a user who didn't ask for it.

**Suggested fix:**
```cpp
static bool g_log_timing   = false;
static bool g_log_variants = false;
```
Or, expose them as environment variable overrides (`BIF_LOG_USD_TIMING=1`).

---

#### H2 — Two large modules suppressed entirely with `#![allow(dead_code)]`

**Files:**
- `crates/bif_viewport/src/node_graph/eval.rs:7` (~725 lines)
- `crates/bif_viewport/src/scene_pipeline.rs:6` (~585 lines)

**Description:**
Both modules are committed but fully silenced. `eval.rs` has a comment "TODO: remove once wired into the main loop (Phase 2)"; `scene_pipeline.rs` says "Will be called from scene_loader.rs after wiring." At ~1310 combined lines, this is substantial dead weight. It creates a false impression of completeness, makes it unclear what invariants these modules assume, and means they drift from the actual architecture.

**Suggested fix:**
Either remove them and re-add when the feature lands, or move them behind a feature flag. If they are genuinely planned for the next milestone, a `// NOTE: not yet wired — see MILESTONES.md M31` comment is fine, but still remove `#![allow(dead_code)]` so individual missing pieces stay visible.

---

#### H3 — `MAX_INSTANCES` defined twice in the same file

**File:** `crates/bif_viewport/src/lib.rs:93` and `:700`

**Description:**
```rust
// Line 93
const MAX_INSTANCES: u32 = 100_000;

// Line 700, inside a function
const MAX_INSTANCES: u32 = 100_000;  // shadows the outer const
```
The inner definition shadows the outer one. Both happen to agree, but this is a latent bug: updating one without the other leads to the function using a different limit than the rest of the code. The outer `MAX_INSTANCES` is unused because the inner one takes precedence in that scope.

**Suggested fix:**
Remove the inner definition and use the module-level constant directly.

---

### Medium

#### M1 — `aabb::axis_interval()` panics in library code on bad input

**File:** `crates/bif_math/src/aabb.rs:47`

**Description:**
```rust
pub fn axis_interval(&self, n: usize) -> Interval {
    match n {
        0 => self.x,
        1 => self.y,
        2 => self.z,
        _ => panic!("axis_interval: invalid axis {n}, expected 0-2"),
    }
}
```
This is a public function in a library crate that panics on out-of-range input. Callers in hot paths (BVH traversal) are trusted, but callers from user code (e.g. scripting, future Python bindings) would get an abort with no recovery.

**Suggested fix:**
Return `Option<Interval>` or use a dedicated `Axis` enum. At minimum, document that it panics:
```rust
/// # Panics
/// Panics if `n > 2`.
```

---

#### M2 — `camera.rs` test uses `panic!` for an unmet assertion in test code

**File:** `crates/bif_math/src/camera.rs:451`

**Description:**
```rust
} else {
    panic!("expected orthographic projection");
}
```
This pattern inside a `#[test]` function is standard Rust test idiom and technically fine. The nit is that `assert!(matches!(...))` or `assert_eq!` with the discriminant is cleaner and produces better failure output. Not a bug, low priority.

---

#### M3 — `usd_bridge_error_message()` pointer lifetime not documented in Rust

**File:** `crates/bif_core/src/usd/cpp_bridge.rs:70-76`

**Description:**
```rust
let msg = unsafe {
    let ptr = usd_bridge_error_message(code);
    if ptr.is_null() {
        "Unknown error".to_string()
    } else {
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
};
```
The C++ signature is `const char* usd_bridge_error_message(UsdBridgeError error)`. The implementation returns a string literal from a switch statement, so the pointer is valid for the program lifetime. However, this is undocumented on the Rust side. If the implementation ever changes to return a heap-allocated string or a thread-local buffer, the code would silently have UB.

**Suggested fix:**
Add a comment: `// SAFETY: usd_bridge_error_message returns a static string literal (switch over enum values). No ownership transfer.`

---

#### M4 — `basic.wgsl` uses simple power-law gamma (1/2.2) instead of piecewise sRGB OETF

**File:** `crates/bif_viewport/src/shaders/basic.wgsl:161-163`

**Description:**
```wgsl
fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return pow(c, vec3(1.0 / 2.2));
}
```
The true sRGB OETF has a linear segment for very dark values (below ~0.0031). The power-law approximation overestimates brightness in very dark zones. For a real-time viewport this is an acceptable approximation for most content, but it will cause a visible mismatch with reference renderers that implement the exact sRGB curve.

**Suggested fix:**
```wgsl
fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let linear_seg = c * 12.92;
    let gamma_seg = 1.055 * pow(c, vec3(1.0 / 2.4)) - vec3(0.055);
    return select(gamma_seg, linear_seg, c < vec3(0.0031308));
}
```
Low visual impact but worth fixing for correctness.

---

#### M5 — `renderer.rs` `linear_to_gamma` uses gamma=2.0 (sqrt), not 2.2

**File:** `crates/bif_renderer/src/renderer.rs:432-438`

**Description:**
```rust
pub fn linear_to_gamma(linear: f32) -> f32 {
    if linear > 0.0 {
        linear.sqrt()
    } else {
        0.0
    }
}
```
This applies gamma=2.0 (square root), not gamma=2.2 or sRGB. The offline renderer uses this for 8-bit LDR output. EXR output is unaffected (written as linear), but any PPM/PNG preview will be slightly too bright. Intentional simplification or oversight is unclear from the code.

**Suggested fix:**
Either rename to `linear_to_gamma2` so the exponent is explicit, or use the standard `pow(x, 1.0/2.2)`.

---

#### M6 — `AtomicCacheBuffer` relies on x86-64 hardware atomicity assumption

**File:** `crates/bif_renderer/src/radiance_cache.rs:178`

**Description:**
```rust
// On x86-64, aligned `u32` ops are hardware-atomic.
```
The code uses `AtomicU32::from_ptr` to atomically read/write individual f32 fields of `CacheEntry`. The comment documents reliance on x86-64 alignment guarantees. On other architectures (ARM, RISC-V) aligned 32-bit loads/stores are also atomic in practice, but this is not guaranteed by the C++ memory model and should either use `cfg(target_arch)` to limit the platform or document more carefully that this is intentional for the current Windows-only context.

**Suggested fix:**
Add `#[cfg(target_arch = "x86_64")]` or a compile-time assertion, plus a comment about the platform scope.

---

#### M7 — `std::cout` warning line in C++ bridge is unconditional (not behind flag)

**File:** `cpp/usd_bridge/usd_bridge.cpp:945`

**Description:**
```cpp
std::cout << "[USD_BRIDGE] WARNING: faceVarying UV index " << fvIdx
```
This is a bounds-check warning for malformed USD geometry that fires unconditionally (not gated on `g_log_textures`). For a malformed asset this will spam per-triangle messages. Should be throttled or logged once per mesh.

**Suggested fix:**
Gate behind `g_log_textures` or count occurrences and log once per mesh.

---

#### M8 — `EvalMode::OnMouseRelease` still shows "(TODO)" in UI label

**File:** `crates/bif_viewport/src/node_graph/mod.rs:1071`

**Description:**
```rust
EvalMode::OnMouseRelease => "On Release (TODO)",
```
And line 1080:
```rust
"On Release (TODO)",
```
This is visible to users in the UI dropdown. (A convoy bead attempted to fix this but apparently only the `(TODO)` suffix in a different context was addressed.)

**Suggested fix:**
Change to `"On Release"` and add a `log::info!` when selected (the unimplemented feature behavior should degrade gracefully).

---

### Low

#### L1 — `ImageBuffer::get`/`set` use `debug_assert` only — no bounds check in release builds

**File:** `crates/bif_renderer/src/renderer.rs:504-522`

**Description:**
```rust
pub fn get(&self, x: u32, y: u32) -> Color {
    debug_assert!(...);
    self.pixels[(y * self.width + x) as usize]
}
```
In release mode, out-of-bounds access silently reads garbage or panics with an unhelpful index-out-of-bounds message. This is a hot path so `debug_assert` may be intentional for performance, which is fine — just worth documenting.

**Suggested fix:**
Add a `/// # Panics` doc comment noting that OOB panics in debug mode only.

---

#### L2 — `bif_viewer/src/bin/debug_usd_mesh.rs` is a debug binary with `println!` — intentional but undocumented

**File:** `crates/bif_viewer/src/bin/debug_usd_mesh.rs`

**Description:**
The binary is a standalone diagnostic tool, so `println!` is correct here. However, it is compiled into the main `bif_viewer` crate (discovered via `src/bin/`), meaning it ships with every build. Its output format duplicates what `log::info!` could do more consistently.

**Suggested fix:**
Low priority. A comment at the top of the file documenting it as a debug utility would help future contributors understand when/why to use it.

---

#### L3 — Multiple `#[allow(dead_code)]` fields in `OpenPbrSurface`

**File:** `crates/bif_renderer/src/openpbr.rs:36-98`

**Description:**
Many fields of `OpenPbrSurface` carry `#[allow(dead_code)]`. This is expected for an in-progress OpenPBR implementation but over time creates confusion about which parameters actually influence the render.

**Suggested fix:**
Track in a comment which fields correspond to implemented lobes versus planned future lobes. A `#[non_exhaustive]` attribute would prevent downstream code from accidentally relying on the default values.

---

#### L4 — `scene_browser.rs` module-level TODO comment not actionable

**File:** `crates/bif_viewport/src/scene_browser.rs:13`

**Description:**
```rust
//! # TODOs
```
The TODOs block at the top of the module is good for planning but several items are vague. They should either reference a MILESTONES.md entry or be turned into `// TODO(M31):` style comments inline.

---

#### L5 — `bif_maketx` has no tests

**File:** `crates/bif_maketx/src/main.rs`

**Description:**
The crate is a thin binary wrapper, so unit tests are difficult, but integration behavior (conversion of an image file to `.tx` format) is untested. A single smoke test verifying the exit code with a known-good input would catch regressions.

---

#### L6 — `node_dispatch.rs` multi-prototype instancing silently falls back

**File:** `crates/bif_viewport/src/node_dispatch.rs:405`

**Description:**
```rust
// TODO: multi-prototype instancing not yet supported,
```
The fallback behavior is undocumented to the user — they get silently wrong output if they provide a multi-prototype instancer. This should log a warning.

---

## Positive Observations

**FFI conversion architecture:** Splitting raw FFI structs (`ffi_raw.rs`), safe conversion functions (`ffi_convert.rs`), and the bridge API (`cpp_bridge.rs`) into separate files is excellent. The conversion functions are pure, testable without C++ DLLs, and have extensive tests (`test_convert_mesh_*`, etc.).

**Safety documentation:** `unsafe impl Send + Sync for UsdStage` has one of the most thorough safety comments in the codebase (6 numbered points with mechanism-level justification). Same quality of documentation for `EmbreeScene` and `AtomicCacheBuffer`.

**Error handling:** `UsdBridgeError` with `thiserror`, `UsdBridgeResult<T>`, and proper propagation through `?` operator. No panics in the FFI path itself.

**WGSL shader quality:** `basic.wgsl` correctly computes the adjugate matrix for normal transform under non-uniform scale (lines 130-132). This is frequently done wrong (using the inverse transpose, which for column-major requires an additional transpose). The Cook-Torrance BRDF, split-sum IBL, and ACES tonemapping are correctly assembled.

**Test coverage in critical paths:** `ffi_convert.rs` tests cover mesh, instancer, material, light, primvar, skeleton, and timeline conversions extensively. `bif_math` has full coverage of AABB, camera, ray, and interval math. `bif_renderer` covers the path tracer core, material models, and Embree integration.

**USD bridge parallelism:** The C++ bridge uses `tbb::parallel_for` / `pxr::WorkParallelForN` for mesh extraction with a timings mutex for safe aggregation. This is clean and correct.

**`radiance_cache.rs`:** The SHARC-inspired lock-free EMA blending approach is well-documented. Comments are honest about the bias (stores emission+NEE only, no indirect) and the workaround (progressive passes blend it out).

---

## Overall Grade Per Crate

| Crate | Grade | Notes |
|---|---|---|
| **bif_math** | A | Clean, fully tested, no unsafe, minor panic in `axis_interval` public API |
| **bif_core** | A- | Excellent FFI layering and test coverage; minor issues with `g_log_timing` on C++ side |
| **bif_renderer** | A- | Path tracer is principled and correct; gamma= 2.0 vs 2.2 discrepancy; `#[allow(dead_code)]` fields in OpenPBR |
| **bif_viewport** | B+ | Good overall; two dead modules (~1310 lines) suppressed systemically; `MAX_INSTANCES` duplication; `(TODO)` in UI label |
| **bif_viewer** | B | Mostly entry-point glue; debug binary ships with main crate; `-D warnings` compliance unclear |
| **bif_maketx** | B | Thin wrapper, no tests, but in scope of a solo side project |
| **cpp/usd_bridge** | B+ | Solid USD API usage; `g_log_timing=true` and unconditional UV warning are the main issues |
| **cpp/oiio_bridge** | Not reviewed in depth | Surface-level check shows no obvious issues |
| **WGSL shaders** | A- | PBR math is correct; approximate sRGB encoding; otherwise clean |
