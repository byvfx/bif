---
title: Concepts Index
type: index
updated: "2026-04-16"
---

# Concepts

Atomic notes on key technical concepts used in BIF. Each note covers one idea — summary, details, how BIF uses it.

## USD & Scene Description

- [[composition-arcs|Composition Arcs]] — LIVRPS rule for USD opinion resolution
- [[primvars|Primvars]] — Primitive variables and interpolation modes
- [[point-instancer|Point Instancer]] — Efficient geometry instancing in USD
- [[edit-target|Edit Target]] — Directing opinions to specific USD layers

## Rendering & Materials

- [[openpbr|OpenPBR]] — OpenPBR Surface material model
- [[materialx|MaterialX]] — Material exchange standard
- [[ior-fresnel|IOR Fresnel]] — Index of refraction based Fresnel reflectance
- [[bvh|BVH]] — Bounding Volume Hierarchy acceleration structure

## Concurrency & FFI

- [[usdstage-thread-safety|UsdStage Thread Safety]] — Send vs Sync for C++ FFI types, Arc<Mutex> pattern
- [[primdataprovider-trait|PrimDataProvider Trait]] — Abstraction over prim-hierarchy queries; inherent-vs-trait method shadowing gotcha

## Qt Integration

- [[paint-pause-pattern|Paint Pause Pattern]] — Suspend the 16ms render tick around modal dialogs and scene resets
- [[hidpi-dpr-threading|HiDPI DPR Threading]] — `devicePixelRatioF()` from Qt through to `Renderer::scale_factor`

## Tools & Libraries

- [[wgpu]] — Rust WebGPU graphics API implementation
- [[egui-snarl]] — Node graph library for egui

## See Also

- [[usd/_index|USD Section]] — Longer articles on USD topics
- [[rendering/_index|Rendering Section]] — BIF rendering pipeline details
