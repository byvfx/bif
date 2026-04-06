---
title: "Composition Arcs (LIVRPS)"
type: concept
tags: [usd, composition, scene-description]
created: "2026-04-05"
updated: "2026-04-05"
---

## Summary

Composition arcs are the six mechanisms USD uses to combine scene description from multiple sources into a single composed stage. Their evaluation order is strictly defined by the acronym **LIVRPS**: Local, Inherits, Variants, References, Payload, Specializes. Stronger opinions (earlier in LIVRPS) win over weaker ones during value resolution.

## Details

### The Six Arcs

| Arc | Strength | Purpose |
|-----|----------|---------|
| **L**ocal | Strongest | Direct opinions on the prim in its own layer stack |
| **I**nherits | 2nd | Class-based sharing (like OOP inheritance) |
| **V**ariants | 3rd | Switchable alternatives (LOD, shader sets) |
| **R**eferences | 4th | Compose in another prim/file |
| **P**ayload | 5th | Deferred references (lazy loading) |
| **S**pecializes | Weakest | "Base class" that target can always override |

### Key Rules

- **Strength ordering is absolute** -- a Local opinion always beats an Inherited one, regardless of layer position.
- **Within the same arc type**, layer stack order (sublayer strength) determines the winner.
- **Payloads are references with a gate** -- they can be selectively loaded/unloaded for scene scalability.
- **Inherits vs Specializes**: Inherits are stronger than the arcs they cross; Specializes are weaker. This makes Specializes useful for "fallback" behavior that anything can override.

### Composition as a Tree

Each prim's composed value comes from walking an **index** (PcpPrimIndex) that encodes all arcs. The Pcp (Prim Cache Population) module resolves this at stage-open time.

## In BIF

- **UsdRead node** opens a stage and traverses the already-composed result via the C++ bridge (`bif_core::usd::cpp_bridge`).
- **Export** (`bif_core::usd::export`) writes flat composed data -- composition arcs are not yet round-tripped.
- **Layer-aware editing** (v0.14.0 milestone) will require BIF to understand [[edit-target]] so opinions land on the correct layer, respecting LIVRPS strength.
- The [[point-instancer]] and material binding workflows rely on References internally.

## Related

- [[edit-target]] -- directing opinions to specific layers
- [[primvars]] -- properties that flow through composition
- [[materialx]] -- material definitions that may be referenced across assets
- `docs/usd/composition.md` -- detailed local reference
