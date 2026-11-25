---
title: "Bounding Volume Hierarchy (BVH)"
type: concept
tags: [rendering, acceleration-structure, ray-tracing]
created: "2026-04-05"
updated: "2026-04-05"
---

## Summary

A Bounding Volume Hierarchy is a tree-based acceleration structure that enables efficient ray-scene intersection testing. Instead of testing every ray against every primitive (O(n) per ray), the BVH organizes primitives into a hierarchy of axis-aligned bounding boxes (AABBs), allowing O(log n) rejection of large groups of primitives.

## Details

### Structure

- **Leaf nodes** contain a small number of primitives (typically 1-8).
- **Internal nodes** contain an AABB that bounds all children, plus left/right child pointers.
- **Construction** typically uses top-down recursive splitting: pick an axis, sort/partition primitives by centroid, recurse.

### Build Strategies

| Strategy | Quality | Build Speed | Notes |
|----------|---------|-------------|-------|
| Median split | Medium | Fast | Sort on longest axis, split at median |
| SAH (Surface Area Heuristic) | High | Slower | Minimizes expected traversal cost |
| LBVH (Linear) | Lower | Very fast | Morton code sorting, GPU-friendly |
| HLBVH | High | Fast | Hybrid: LBVH top, SAH bottom |

### Traversal

Ray traversal tests the ray against each node's AABB. If the ray misses the box, the entire subtree is skipped. For shadow rays, any-hit traversal can terminate early.

### BVH vs Other Structures

- **Uniform grid**: Better for evenly distributed scenes, bad for clustered geometry.
- **KD-tree**: Tighter spatial partition but harder to build/update.
- **BVH**: Best general-purpose choice -- handles dynamic scenes, simple to implement, cache-friendly with flattened arrays.

## In BIF

BIF implements a BVH in `crates/bif_renderer/src/bvh.rs`:

- Uses a **median-split** strategy on the longest AABB axis.
- `BvhNode` is an enum: `Branch { left, right, bbox }`, `Leaf { objects, bbox }`, `Empty`.
- Leaf threshold: `LEAF_MAX_SIZE = 4` primitives.
- Ported from Brandon's Go raytracer with Rust-idiomatic enhancements (enum dispatch instead of trait objects for nodes).
- Used alongside Embree (`bif_renderer::embree`) which provides its own hardware-optimized BVH for production rendering.

## Related

- [[openpbr]] -- materials evaluated after BVH intersection finds a hit
- [[ior-fresnel]] -- Fresnel evaluated at hit points found by BVH traversal
- [[wgpu]] -- GPU-side rendering uses rasterization, not BVH (BVH is CPU raytracer path)
