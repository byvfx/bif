# USD FAQ & Common Gotchas

## File Formats

### What's the difference between .usda, .usdc, and .usd?

| Extension | Format | Use |
|-----------|--------|-----|
| `.usda` | Text (ASCII) | Human-readable, debugging, hand-editing |
| `.usdc` | Binary (crate) | Production — fast, compact, memory-mappable |
| `.usd` | Either | Determined by content or `USD_DEFAULT_FILE_FORMAT` env var |
| `.usdz` | Zip archive | Delivery — bundles USD + textures, uncompressed |

Best practice: Use `.usd` extension in references (lets pipeline switch between text/binary without changing references).

### Isn't USD just another file format?

No. USD is a **composition engine + scenegraph platform**. File interchange is one capability, but the key differentiators are:
- **Composition engine** — weaves many files into a single scenegraph via arcs (references, sublayers, variants, etc.)
- **High-performance scenegraph** — efficient traversal, value resolution, change notification
- **Non-destructive editing** — overrides in separate layers, never modifying source assets

## Composition

### Why doesn't composition consider time?

For scalability. If composition depended on time, the stage would need to be recomposed every frame, which is prohibitively expensive for large scenes. Instead, composition is time-independent — the structure is fixed, and only attribute values vary with time.

### What's the difference between an "over" and a "typeless def"?

- **`over`**: Speculative opinions. The prim only appears in traversal if a `def` exists at that path from another composition source.
- **Typeless `def`**: A concrete prim with no schema type. It **does** appear in traversal even without opinions from elsewhere.

```usda
over "Ghost" { }       # Won't appear unless def'd elsewhere
def "Concrete" { }     # Always appears (typeless but concrete)
def Xform "Typed" { }  # Appears with Xform schema applied
```

### How does "prepend" vs "append" work for composition arcs?

```usda
def "Prim" (
    prepend references = @strong.usd@    # Stronger
    append references = @weak.usd@       # Weaker
)
```

`prepend` (default) adds to the front of the list (stronger). `append` adds to the end (weaker). This applies to references, payloads, inherits, specializes, and variantSets.

### Why do local opinions beat variant opinions?

By LIVRPS ordering, Local > Variants. If you author a value directly on a prim (local opinion), it will always beat any value from a variant selection. To let variants take effect, `Clear()` the local opinion.

## Instancing

### Native vs Point Instancing — when to use which?

| Feature | Native Instancing | Point Instancing |
|---------|-------------------|------------------|
| Setup | `instanceable = true` on prim | `UsdGeomPointInstancer` |
| Scale | ~100s of instances | ~1000s to millions |
| Per-instance data | Limited (only via composition) | Rich (positions, orientations, scales, primvars) |
| Editability | Read-only beneath instance | Data-driven (modify arrays) |
| Use case | Repeated referenced assets | Crowds, vegetation, particles |

### Can I edit prims inside a native instance?

Not directly. The subtree beneath an instanceable prim is shared and read-only. To customize per-instance:
1. Use composition arcs (variants, overrides in stronger layers)
2. Switch to point instancing for per-instance transforms
3. Remove `instanceable = true` to break sharing (at memory cost)

## Layers & Stages

### How does layer caching work?

Sdf maintains a global registry of opened layers by identifier. `FindOrOpen()` returns the cached layer if already open. The registry holds weak references — if no strong reference remains, the layer is released. UsdStage holds strong refs to all composed layers.

### What are anonymous layers for?

In-memory-only layers with no file backing. Useful for:
- Session layers (temporary overrides during interactive editing)
- Procedurally generated scene description
- Scratch layers for undo/redo systems

Cannot be `Save()`d — must `Export()` to a file explicitly.

### What's a session layer?

A layer that sits above the root layer in composition strength. Used for transient, non-persistent opinions (viewport overrides, selection state, etc.). Not saved with `stage.Save()` — must explicitly save with `stage.SaveSessionLayers()`.

## Common Pitfalls

1. **Forgetting `subdivisionScheme = "none"`** — Meshes default to Catmull-Clark subdivision. Your "polygonal" mesh will look smoother than expected.

2. **Not setting `defaultPrim`** — References without an explicit target prim path will fail if `defaultPrim` isn't set on the referenced layer.

3. **Authoring on the wrong layer** — Check `stage.GetEditTarget()`. Opinions go to whichever layer is the current edit target.

4. **Time samples from wrong layer** — First layer with *any* time sample for an attribute wins *all* time samples. Can't merge animation from multiple layers on the same attribute.

5. **Relationship targets not remapped** — Actually they are! USD automatically remaps relationship targets when namespaces change through referencing. This is a feature, not a bug.

6. **Material bindings need MaterialBindingAPI** — Prims must `apply` the `MaterialBindingAPI` schema for bindings to be respected by renderers.

7. **Transform order matters** — `xformOpOrder` defines the multiplication order. Common mistake: ops authored but not listed in xformOpOrder are ignored.

8. **Payload loading state** — If a stage is opened with `LoadNone`, payloaded content won't be visible until explicitly loaded. Use `stage.Load()` or `stage.LoadAll()`.

9. **Primvar interpolation mismatch** — Array size must match the interpolation mode. E.g., `faceVarying` primvar on a quad mesh needs 4 elements per face, not per vertex.

10. **Token vs String** — Use `token` for enum-like values and identifiers (interned, fast comparison). Use `string` for freeform text. Don't use string where token is expected.
