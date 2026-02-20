# Specification: "Fast as Hell" GI – idTech 8 Inspired Rust/USD Path Tracer

## 1. Project Vision

Implement a high-performance, fully dynamic Global Illumination (GI) system within a Rust-based USD raytracer. This architecture follows the **idTech 8** philosophy: abandoning pre-baked lighting for a **Path Traced Foundation**. It leverages hardware ray tracing and a **Spatially Hashed Radiance Cache (SHARC)** to achieve real-time 60 FPS performance with low sample counts (1-spp).

## 2. Core Constraints & Technologies

* **Language:** Rust (Safety, SIMD, and high-level USD bindings).
* **Scene Source:** USD (Universal Scene Description) as the first-class data model.
* **API:** `wgpu` or `ash` (Vulkan) with Hardware Ray Tracing extensions.
* **Primary Goal:** Decouple visibility from shading using world-space caching.

---

## 3. Implementation Status

### Phase 1: USD-to-GPU Sync (Acceleration Structures) — ✅ Done (M12-M14)

- [x] Instance deduplication (single BLAS per UsdGeomMesh)
- [x] TLAS flattening (world-space transforms via Embree instancing)
- [x] Refit logic (Embree handles rebuild)
- [x] Custom indexing (per-triangle material IDs)

### Phase 2: The SHARC Core — ✅ Done (M23)

- [x] **Radiance Cache Buffer:** 64-shard `RwLock<Vec<CacheEntry>>` (~25 MB for 1M entries)
- [x] **Spatial Hashing:** Bit-perfect 3D hash with normal disambiguation
- [x] **Ray Generation Logic:** Cache READ skips bounces, cache WRITE stores local radiance
- [x] **Russian Roulette:** Throughput-proportional path termination after bounce >= 3
- [x] **Cache Heatmap AOV:** Visualize sample density for diagnostics

### Phase 3: Hardware Efficiency & Divergence — 🎯 Future (M27)

- [ ] Shader Execution Reordering (SER)
- [ ] Opacity Micro-Maps (OMM)
- [ ] Stochastic Light Sampling (ReSTIR)

### Phase 4: Temporal Stability (Denoiser) — 🎯 Future (M26)

- [ ] Velocity Buffers
- [ ] Reprojection Pass
- [ ] Edge-Avoiding Bilateral Filter

---

## 4. Technical Reference

### Rust: Spatial Hash (CPU — implemented in `radiance_cache.rs`)

```rust
pub fn spatial_hash(pos: Vec3, normal: Vec3, cell_size: f32, buffer_size: u32) -> u32 {
    let p = pos / cell_size;
    let ix = p.x.floor() as i32 as u32;
    let iy = p.y.floor() as i32 as u32;
    let iz = p.z.floor() as i32 as u32;
    let n = dominant_axis(normal); // 0..5

    let h = ix.wrapping_mul(73856093)
        ^ iy.wrapping_mul(19349663)
        ^ iz.wrapping_mul(83492791)
        ^ n.wrapping_mul(2654435761);
    h % buffer_size
}
```

### WGSL: Spatial Hash (GPU — for M27)

```wgsl
fn spatial_hash(pos: vec3<f32>, normal: vec3<f32>, cell_size: f32, buffer_size: u32) -> u32 {
    let p = floor(pos / cell_size);
    let ix = bitcast<u32>(i32(p.x));
    let iy = bitcast<u32>(i32(p.y));
    let iz = bitcast<u32>(i32(p.z));
    let n = dominant_axis(normal);

    let h = ix * 73856093u ^ iy * 19349663u ^ iz * 83492791u ^ n * 2654435761u;
    return h % buffer_size;
}
```

### Cache Entry (shared CPU/GPU layout)

```rust
#[repr(C)]
struct CacheEntry {
    radiance_r: f32, radiance_g: f32, radiance_b: f32,
    sample_count: u32, frame_id: u32, _pad: u32,
}  // 24 bytes
```

---

## 5. Implementation Notes

- **Memory Layout:** All GPU-bound structures use `#[repr(C)]` and 8-byte alignment.
- **Concurrency:** 64-shard `RwLock` for CPU cache; GPU version uses `atomicAdd`.
- **Normal disambiguation:** 6 dominant-axis directions prevent floor/ceiling light leak.
- **EMA blending:** Weights recent samples more (good for dynamic scenes).
- **Staleness:** Entries older than `max_age` frames are treated as empty.
- **Debug:** Cache Heatmap AOV visualizes `sample_count` at primary hit.

---

## 6. Verification Methods

- **Cache Heatmap AOV** — Switch AOV channel to `CacheHeatmap`. Green = well-cached regions, black = no cache data. Useful for spotting dead zones (geometry outside hash range, or cell_size too small).
- **Hit Rate / Occupancy Stats** — Live readout in the egui "SHARC Cache" panel. Hit rate = cache reads that returned valid data / total cache reads. Occupancy = non-empty entries / buffer_size. Target: >60% hit rate after warmup.
- **A/B Timing** — Render same scene with cache enabled vs disabled, compare per-SPP wall time. Cache benefit scales with bounce depth and scene complexity.

---

## 7. Settings Reference

| Control | Range | Default | Effect |
|---------|-------|---------|--------|
| Enable/Disable | toggle | enabled | Bypass cache entirely (READ and WRITE) |
| Cell Size | 0.01–10.0 (log slider) | auto from AABB | Spatial resolution of hash grid. Smaller = more detail, more memory pressure |
| Buffer Size | 256K–4M entries | 1M | Total cache slots. Larger = fewer hash collisions, more RAM |
| Min Samples | 1–16 | 4 | Entries with fewer samples return as miss (forces more path tracing early on) |
| Min Bounce Depth | 1–4 | 2 | Bounces before cache READ is attempted. 1 = aggressive caching, 4 = nearly uncached |
| Hit Rate % | read-only | — | Fraction of cache lookups returning valid data |
| Occupancy % | read-only | — | Fraction of buffer slots containing data |

---

## 8. GPU Portability

The CPU SHARC implementation is designed for direct GPU translation at M27:

- **Cache entry layout:** `#[repr(C)]` struct maps 1:1 to a WGSL `struct` (see Section 4 code sample).
- **Spatial hash:** Same large-prime formula in both Rust and WGSL. `i32 as u32` cast in Rust matches `bitcast<u32>(i32(...))` in WGSL.
- **Concurrency model:** CPU uses 64-shard `RwLock` for rayon parallelism. GPU version replaces this with `atomicAdd` on a `storage` buffer — no locks needed.
- **EMA blending:** Identical math; GPU computes `atomicAdd` on `sample_count` and blends `radiance` fields.
- **Target milestone:** M27 (GPU Path Tracing).
