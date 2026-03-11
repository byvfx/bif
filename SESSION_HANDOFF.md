# Session Handoff - March 10, 2026

**Last Updated:** Full codebase code review + critical fixes
**Next Milestone:** Tier 2 code review fixes (Embree normals, USD safety)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26 (OIDN denoising) |
| Current | Code review fixes (Tier 1 done, Tier 2-4 planned) |
| Tests | 82 renderer, 41 math, 24 viewport, 27+ bif_core |
| Performance | 60 FPS viewport, 100K instances with LOD |

---

## Recent Work

### Full Codebase Code Review + Fixes (Mar 10, 2026)

4 parallel VFX code review agents reviewed all 6 crates. ~50 issues found.

**Critical fixes applied:**
- Normal transform inverse-transpose in instanced_geometry.rs
- UsdEditLayer::save() double-free prevention
- ControlFlow::Poll → Wait (was burning 100% CPU idle)
- NEE MIS light selection PDF (1/N) correction
- Deleted dead instanced_geometry_bvh.rs (broken UB)

**Quick wins applied:**
- Duplicate gen_f32, O(n²) mesh lookup, #[inline] Aabb::hit
- DEFAULT_FAR_PLANE 100→10000, axis_interval catch-all
- BSDF/PDF default mismatch, .usd routing, CompositeProvider dedup

**Result:** 10 files, +34/-319 lines, all tests pass

---

## Architecture Notes

- **Blue noise default:** BlueNoise is the default sampler mode
- **Pixel filter default:** Box for viewport (fast), Mitchell for batch (quality)
- **Embree normals bug (known):** Shading normals in prototype-local space not transformed by instance inverse-transpose — Tier 2 fix
- **Renderer God object:** ~80 fields, cleanup deferred to Tier 4

---

## Next Steps

1. **Tier 2 Session A:** Embree shading normal transform, viewport normal transform, Embree stride 12→16, Lambertian scatter fix
2. **Tier 2 Session B:** UsdMesh::triangulate bounds check, hardcoded paths → env vars, source_dir, silent parse errors
3. **Tier 3:** ray_color dedup (-150 lines), EXR writer dedup (-300 lines)
4. Continue M29 USD export validation
