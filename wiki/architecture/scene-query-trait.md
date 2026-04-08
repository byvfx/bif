---
title: "SceneQuery Trait"
type: article
tags: [architecture, traits, scene]
created: "2026-04-07"
updated: "2026-04-07"
---

## Summary

`SceneQuery` is a read-only trait in `bif_core` that abstracts access to `Scene` fields. It enables future alternative implementations (e.g., `LayerAwareScene` for M32 opinion trace) without changing viewport code.

## Design

### 15 Methods

- **Prototypes:** `prototype_count`, `prototype(id)`, `prototypes()`
- **Instances:** `instance_count`, `instances()`, `instance_animations()`, `get_instance(idx)`
- **Materials:** `material_count`, `material(id)`, `materials()`
- **Collections:** `cameras()`, `point_clouds()`, `lights()`, `timeline()`, `stage_metadata()`
- **Derived:** `total_triangle_count()`, `has_animation()`, `find_instance_by_prim_path(path)`, `material_for_prototype(id)`

### Key Method: find_instance_by_prim_path

3-strategy lookup matching USD path resolution behavior:
1. Exact match on `Instance.prim_path`
2. Descendant prefix match (`{path}/...`) — clicking parent Xform
3. Synthetic `/BIF/{path}` fallback — loader-generated paths (strips leading `/`)

### Default Method: material_for_prototype

Reads `Prototype.material` directly. Override in layer-aware implementations where material bindings come from opinion overrides.

## Usage

```rust
use bif_core::SceneQuery;

// Use through trait for read-only access
let q: &dyn SceneQuery = &scene;
if let Some(idx) = q.find_instance_by_prim_path("/World/Cube") {
    let mat = q.material_for_prototype(q.instances()[idx].prototype_id);
}
```

## Files

- `crates/bif_core/src/scene_query.rs` — trait + impl + 9 tests
- `crates/bif_viewport/src/selection_dispatch.rs` — first real consumer

## See Also

- [[Architecture Review]] — SceneQuery is item #6
- `ARCHITECTURE_REVIEW.md` section 9 — inter-crate boundary issues
