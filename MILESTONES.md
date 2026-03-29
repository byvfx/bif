# BIF Milestones

Roadmap organized by semantic version. Each release is testable, demoable, and gets a devlog + CHANGELOG entry.

[Milestone History](MILESTONES_HISTORY.md) | [Detailed Breakdown](ROADMAP_DETAIL.md) | [Changelog](CHANGELOG.md)

---

## Released

| Version | Theme | Date | Highlights |
|---------|-------|------|------------|
| v0.1.0 | Initial Release | 2026-03-12 | Viewport, instancing, USD C++, Embree, materials, MaterialX, animation, batch render, node graph, scatter, SHARC cache, OIDN denoising |
| v0.11.0 | Ivar Cache | 2026-03-13 | Ivar material cache + pre-warm, Embree indexed geometry |
| v0.12.0 | USD Export | 2026-03-21 | USD export pipeline, OpenPBR migration, subsystem extraction, curves/points import, UDIM atlas |

---

## Upcoming

| Version | Theme | Est. Hours | Key Milestones |
|---------|-------|-----------|----------------|
| **v0.13.0** | **Pipeline Foundation** | — | M29.5, M30, M31 *(in progress)* |
| v0.14.0 | USD Debugging | 35-50h | M32, M33 |
| v0.15.0 | Qt Migration | 50-60h | M28 |
| v0.16.0 | Viewport Performance | 20-30h | M22 |
| v0.17.0 | Context System | 30-40h | M39 |
| v0.18.0 | Scene Authoring | 30-40h | M37, M38 |
| v0.19.0 | MaterialX Authoring | 25-30h | M40 |
| v0.20.0 | GPU Path Tracing | 30-40h | M27 |
| v0.21.0 | Volumes & OpenVDB | 20-30h | M25 |
| v0.22.0 | API & Integration | 40-55h | M35, M34 |
| v0.23.0+ | Framework Extraction | 40+h | M36+ |

**Total estimated:** ~340-445h remaining to 1.0

---

### v0.13.0 — Pipeline Foundation *(in progress)*

M29.5 (UI overhaul), M30 (persistence + eval modes), M31 (per-node viz), unreleased perf fixes. **Last egui feature release.** Ship current work.

### v0.14.0 — USD Debugging

M32 (composition inspector + opinion trace), M33 (debugging tools). "Understand your USD scene." **Last release on egui UI** — logic is UI-agnostic for later Qt port.

### v0.15.0 — Qt Migration

M28 (Qt 6 UI framework). **The pivot — everything after is Qt-native.** Port scene browser, property inspector, node graph, viewport. Largest single release.

### v0.16.0 — Viewport Performance

M22 (Vulkan 1.3, lazy loading, GPU-driven rendering). Foundation for production-scale scenes before authoring tools land.

### v0.17.0 — Context System

M39 (Assembly/Materials/Animation contexts, multi-graph). Built in Qt. Highest architectural risk — touches scene_loader, render, property_inspector.

### v0.18.0 — Scene Authoring

M37 (lights) + M38 (materials + shader graph on context arch). "Create content without external USD." Includes namespace editor.

### v0.19.0 — MaterialX Authoring

M40 (standard_surface graph, XML round-trip, node previews). Built on context system in Materials context.

### v0.20.0 — GPU Path Tracing

M27 (wgpu compute, BVH on GPU, ReSTIR). Fast material preview for authoring workflows.

### v0.21.0 — Volumes & OpenVDB

M25 (fog, smoke, clouds, VDB support). Fills the biggest production content gap.

### v0.22.0 — API & Integration

M35 (API cleanup) then M34 (PyO3 pipeline integration). "Embed BIF in studio pipelines."

### v0.23.0+ — Framework Extraction

M36+ (widget crates, plugin system, DCC connectors). "Reusable VFX framework crates."

---

## Pre-1.0 Hardening

- Error recovery / auto-save
- User-facing error notifications (toast system)
- Undo hardening for all authoring operations
- OCIO color management (ACES/ACEScg)
- User documentation

## 1.0 Criteria

1. Reliable USD round-trip (Houdini → BIF → export → re-import, no data loss)
2. Scene authoring without external tools (lights, materials, scatter, export)
3. Save/load with auto-save and crash recovery
4. Undo/redo for all authoring operations
5. Production batch rendering with denoising
6. GPU path tracing for interactive preview
7. Volume rendering (VDB)
8. Qt-based professional UI
9. User-facing error messages
10. User documentation

---

## Principles

1. One version per release — no partial work
2. Each release must be testable and demoable
3. Each release gets a devlog + CHANGELOG entry
4. v0.15.0 (Qt) is the pivot — everything after is Qt-native
5. Undo commands required for all authoring operations
