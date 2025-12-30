# Houdini USD Export Guide

**Last Updated:** December 30, 2025

This guide covers best practices for exporting USD files from Houdini for use in BIF.

---

## Normals: Point vs Vertex

**Critical:** BIF expects **point normals**, not vertex normals.

### The Issue

- **Point Normals** (`N` on points): One normal per vertex, shared across faces. Exports cleanly to USD.
- **Vertex Normals** (`N` on vertices): One normal per face-corner. Can cause orientation issues and inverted normals in BIF.

### The Fix

Before USD export, use an **Attribute Promote** SOP:

| Setting | Value |
|---------|-------|
| Original Name | `N` |
| Original Class | `Vertex` |
| New Class | `Point` |
| Promotion Method | `Average` |

### Quick VEX Alternative

```vex
// In a Point Wrangle SOP (before export)
v@N = normalize(v@N);
```

---

## Winding Order

BIF renders with **clockwise front-face winding** (`FrontFace::Cw`) to match Houdini/USD conventions.

If your model appears inside-out:
1. Check the `Reverse` SOP to flip normals
2. Ensure consistent winding with `Facet` SOP → Unique Points → Compute Normals

---

## Recommended Export Settings

### USD ROP / SOP Export

| Setting | Recommended Value |
|---------|-------------------|
| Format | `.usda` (text, for debugging) or `.usdc` (binary, smaller) |
| Export Normals | ✅ Enabled |
| Normal Attribute | `N` (point class) |
| Subdivision | `none` (for mesh preview) |

### For Large Scenes

- Use **PointInstancer** for repeated geometry (BIF supports this)
- Keep prototype geometry in a single Mesh prim
- Instance transforms via `positions`, `orientations`, `scales`

---

## Testing Your Export

1. Export to `.usda` (text format)
2. Open in text editor and verify:
   - `primvars:normals` exists
   - `primvars:normals:interpolation = "vertex"` or `"faceVarying"`
   - Point positions look correct
3. Load in BIF: `cargo run -p bif_viewer -- --usda your_file.usda`

---

## Troubleshooting

### Inverted/Wrong Normals
- **Cause:** Vertex normals instead of point normals
- **Fix:** Attribute Promote → Point class

### Model Looks Inside-Out
- **Cause:** Winding order mismatch
- **Fix:** Reverse SOP or check USD export orientation settings

### Missing Normals
- **Cause:** No `N` attribute on geometry
- **Fix:** Add `Normal` SOP before export, set to Point normals

### Broken Shading (Faceted)
- **Cause:** Hard edges or per-face normals
- **Fix:** Use `Facet` SOP with Cusp Angle for smooth normals, then promote to points
