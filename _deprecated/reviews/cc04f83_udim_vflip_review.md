# Code Review: cc04f83 — UDIM double V-flip fix

## 1. Critical Issues (must fix)

### Atlas stitching still uses `flipped_row` — CPU/GPU mismatch risk with `transform_uv`

The `build_udim_atlas()` Phase 2 stitching still places tiles using `flipped_row`:

```rust
let flipped_row = (num_rows - 1) - row;
let dest_y = flipped_row * target_h;
```

This means in the atlas pixel buffer, UDIM row 0 (bottom in UV space) is stored at the **bottom** of the image (high pixel-y). The new `transform_uv()` returns `atlas_v = (row + sub_v) / grid_rows`, so for row=0, atlas_v is near 0.0. Then `sample()` computes `y = (1.0 - 0.25) * (height - 1)` = high pixel-y. This **does** land on the flipped_row region, so the math is consistent.

**Verdict: Correct.** The `flipped_row` in stitching + `(row + sub_v)` in `transform_uv` + `(1.0 - v)` in `sample()` compose correctly. Row 0 tiles end up at high dest_y in the buffer, `transform_uv` maps them to low atlas_v, and `sample`'s `1.0 - v` converts low atlas_v back to high pixel-y. All three steps are coherent.

No action needed, but this is subtle enough to warrant a brief inline comment in `build_udim_atlas` explaining *why* the stitching flip is still there after the `transform_uv` fix. Future you will thank past you.

---

## 2. Important Improvements (should fix)

### 2a. `test_udim_sample_tile_color` only tests 2x1 (1-row grid) — the exact case unaffected by this bug

The commit message correctly states "1-row UDIM grids were unaffected." The existing `test_udim_sample_tile_color` is a 2x1 grid — it would have passed both before and after this fix. You need a **2x2 end-to-end sample test** that calls `sample()` (not just `transform_uv`) to verify the full pipeline including the `1.0 - v` flip.

**Suggested test:**

```rust
#[test]
fn test_udim_sample_2x2_grid() {
    // 2x2 atlas: 2px per tile = 4x4 atlas
    // row0: [red, red, green, green]
    // row1: [red, red, green, green]  (top half of row-0 tiles)
    // row2: [blue, blue, white, white]
    // row3: [blue, blue, white, white] (top half of row-1 tiles)
    // But with flipped_row stitching, row-0 tiles go to bottom (rows 2-3),
    // row-1 tiles go to top (rows 0-1).
    let mut pixels = vec![[0.0f32; 4]; 16];
    // Row-1 tiles at top of atlas (pixel rows 0-1):
    // tile 1011 (col=0,row=1) = blue, tile 1012 (col=1,row=1) = white
    pixels[0] = [0.0, 0.0, 1.0, 1.0]; pixels[1] = [0.0, 0.0, 1.0, 1.0];
    pixels[2] = [1.0, 1.0, 1.0, 1.0]; pixels[3] = [1.0, 1.0, 1.0, 1.0];
    pixels[4] = [0.0, 0.0, 1.0, 1.0]; pixels[5] = [0.0, 0.0, 1.0, 1.0];
    pixels[6] = [1.0, 1.0, 1.0, 1.0]; pixels[7] = [1.0, 1.0, 1.0, 1.0];
    // Row-0 tiles at bottom of atlas (pixel rows 2-3):
    // tile 1001 (col=0,row=0) = red, tile 1002 (col=1,row=0) = green
    pixels[8]  = [1.0, 0.0, 0.0, 1.0]; pixels[9]  = [1.0, 0.0, 0.0, 1.0];
    pixels[10] = [0.0, 1.0, 0.0, 1.0]; pixels[11] = [0.0, 1.0, 0.0, 1.0];
    pixels[12] = [1.0, 0.0, 0.0, 1.0]; pixels[13] = [1.0, 0.0, 0.0, 1.0];
    pixels[14] = [0.0, 1.0, 0.0, 1.0]; pixels[15] = [0.0, 1.0, 0.0, 1.0];

    let mut tex = Texture::new(4, 4, pixels, "<udim_2x2>");
    tex.udim_grid_cols = 2;
    tex.udim_grid_rows = 2;
    tex.udim_min_col = 0;
    tex.udim_min_row = 0;

    // tile 1001 center (u=0.5, v=0.5) -> red
    let c = tex.sample(0.5, 0.5);
    assert!((c.x - 1.0).abs() < 0.1, "1001 should be red, got {:?}", c);
    // tile 1012 center (u=1.5, v=1.5) -> white
    let c = tex.sample(1.5, 1.5);
    assert!((c.x - 1.0).abs() < 0.1 && (c.y - 1.0).abs() < 0.1, "1012 should be white, got {:?}", c);
    // tile 1011 center (u=0.5, v=1.5) -> blue
    let c = tex.sample(0.5, 1.5);
    assert!((c.z - 1.0).abs() < 0.1, "1011 should be blue, got {:?}", c);
    // tile 1002 center (u=1.5, v=0.5) -> green
    let c = tex.sample(1.5, 0.5);
    assert!((c.y - 1.0).abs() < 0.1, "1002 should be green, got {:?}", c);
}
```

This is the most important missing piece. The `test_transform_uv_2x2_grid` test validates the UV math in isolation but does NOT exercise the `sample() -> (1.0 - v) -> get_pixel()` pipeline that was the actual bug site.

### 2b. 1 GB atlas allocation with no warning log at the budget cap

The pixel budget guard computes scale and downsizes, but the log message only fires when `target_w != raw_max_w`. If the per-tile cap doesn't trigger but the total pixel cap does, you still get the log (since target_w changes). Good. However, consider logging the actual memory at the budget cap separately so artists see "UDIM atlas capped to 850 MB" rather than having to mentally compute `tile_size * grid * 16`.

---

## 3. Suggestions (consider)

### 3a. Atlas limits are reasonable but document the memory implication

- `MAX_IVAR_UDIM_TILE_SIZE = 4096`: Standard for ACES 4K textures. Good.
- `MAX_IVAR_ATLAS_SIZE = 8192`: Allows 2x2 grid of 4K tiles. Good.
- `MAX_IVAR_ATLAS_PIXELS = 67_108_864` (64M pixels = 1 GB at f32x4): This is aggressive for a side project but reasonable for VFX. A 10x10 UDIM grid of 4K tiles would need 1.6 billion pixels before capping — the guard will kick in and downscale to ~810px/tile, which is fine.

One concern: the `vec![[0.0f32; 4]; (atlas_w * atlas_h) as usize]` allocation at line ~660 will attempt a single 1 GB contiguous allocation. On Windows with default heap, this can fail silently or OOM-kill. Consider adding a fallback or at least a `log::warn!` when the atlas exceeds 256 MB.

### 3b. Doc comment still references `basic.wgsl lines 281-305`

The `transform_uv` doc says "using the same math as `basic.wgsl` lines 281-305" but the `find` for wgsl UDIM handling returned no results. Either the wgsl file doesn't have UDIM UV transform logic (the GPU uses `textureSample` natively with the atlas layout), or the line numbers have drifted. Verify and update or remove this reference.

### 3c. `sample_channel` also uses `(1.0 - v)` — confirmed consistent

`sample_channel()` calls `transform_uv()` then applies `(1.0 - v)` identically to `sample()`. The fix correctly applies to both paths since they share `transform_uv`. Good.

### 3d. Consider edge-case: non-zero `udim_min_col`/`udim_min_row`

The test only covers `min_col=0, min_row=0`. Production UDIMs can start at arbitrary tiles (e.g., tiles 1021-1032 for a character's head). A test with `min_col=2, min_row=1` would catch offset indexing bugs.

---

## 4. Questions/Challenges

**Q1: Have you visually verified the PaperScroll.usd UDIM render (mentioned in SESSION_HANDOFF)?**
The math looks correct on paper, but UDIM bugs are notoriously visual. A before/after screenshot comparison of a multi-row UDIM asset is essential.

**Q2: Why keep `flipped_row` in atlas stitching but remove it from `transform_uv`?**
This is the correct approach (atlas pixel layout is top-down, UV convention is bottom-up, single `1.0-v` bridges them). But the asymmetry between GPU viewport stitching (also uses `flipped_row`) and CPU `transform_uv` (no longer flips) means someone reading the code has to hold two conventions in their head. A one-line comment in `build_udim_atlas` like `// Row flip: atlas stores row-0 at bottom of pixel buffer; transform_uv returns UV-space (0=bottom), sample() applies 1.0-v` would prevent the next person from "fixing" this back.

**Q3: The viewport path (`bif_viewport/src/render.rs`) also builds a UDIM atlas with `flipped_row` — does the GPU shader's `textureSample` expect the same convention?**
The `find` for UDIM-related wgsl code returned empty. If the GPU shader just uses raw UVs with `textureSample`, it relies on the atlas being laid out with row-0 at the bottom of the texture (which the viewport's `flipped_row` achieves since GPU textures are typically origin-top-left). Confirm this is the case.

---

## Summary

The V-flip fix is **mathematically correct**. The old code applied two V-inversions (`flipped_row + (1.0 - sub_v)` in `transform_uv`, then `(1.0 - v)` in `sample`), which cancelled for 1-row grids but produced wrong tile ordering for multi-row grids. The new code returns standard UV-space coordinates, letting `sample()`'s single flip do the right thing.

**Must-do:** Add a 2x2 end-to-end `sample()` test (not just `transform_uv`). The existing 2x1 test was blind to this exact bug class.

**Should-do:** Add a comment in `build_udim_atlas` explaining the relationship between atlas pixel layout and `transform_uv` conventions. Add a `log::warn!` for atlas allocations exceeding 256 MB.

| Aspect | Verdict |
|---|---|
| V-flip correctness | Pass |
| GPU/CPU convention match | Pass (needs wgsl verification) |
| Atlas limit values | Reasonable for VFX |
| Downscale-instead-of-error | Good improvement |
| Test coverage | **Insufficient** for multi-row grids |
| Documentation | Needs comment in build_udim_atlas |
