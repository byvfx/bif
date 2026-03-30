# USD Toolset

Command-line tools shipped with USD. All located in `USD_INSTALL_ROOT/bin`.

## File Inspection

### usdcat

Print USD file contents as text (usda format).

```bash
usdcat scene.usd                    # Print to stdout
usdcat scene.usd -o output.usda     # Write to file
usdcat scene.usd --flatten          # Flatten composition (single layer output)
usdcat scene.usd --usdFormat usda   # Force text format for .usd output
usdcat a.usd b.usd -o merged.usda  # Concatenate multiple files
```

Flags:
- `--flatten` — compose all layers into one, resolve all arcs
- `--flattenLayerStack` — flatten sublayers only (keep references/payloads)
- `--skipSourceFileComment` — omit source file comment in output
- `--mask /path` — only output prims under given path(s)

### usdtree

Print prim hierarchy as a tree:

```bash
usdtree scene.usd                   # Full hierarchy
usdtree scene.usd --maxDepth 3      # Limit depth
usdtree scene.usd -a                # Show attributes
usdtree scene.usd -m                # Show metadata
```

### usdview

Interactive graphical scene viewer and inspector:

```bash
usdview scene.usd
usdview scene.usd --complexity high  # Subdivision refinement
usdview scene.usd --renderer GL      # Force renderer
```

Features:
- 3D viewport with orbit/pan/zoom
- Prim browser with property inspector
- Embedded Python interpreter (press `i`)
- Composition inspector
- Layer stack viewer
- Animation playback

Keyboard: `Ctrl++`/`Ctrl+-` = subdivision, `i` = interpreter, `f` = frame selected

## File Editing

### usdedit

Open any USD file in a text editor, save back in original format:

```bash
usdedit scene.usd            # Opens in $EDITOR as .usda text
usdedit scene.usd -n         # No-op mode (don't save changes)
```

Editor lookup order: `USD_EDITOR` → `EDITOR` → emacs → vim → notepad

### usdstitchclips

Combine multiple time-sample files into a single stage with value clips:

```bash
usdstitchclips -o result.usd clip1.usd clip2.usd clip3.usd
usdstitchclips -o result.usd clips/*.usd --templateMetadata --startTimeCode 1 --endTimeCode 100
```

### usdstitch

Merge multiple layers into one (non-clip, just combines opinions):

```bash
usdstitch -o merged.usd layer1.usd layer2.usd
```

## Validation

### usdchecker

Validate USD files against rules and best practices:

```bash
usdchecker scene.usd                # Check for errors
usdchecker scene.usd --arkit        # ARKit compatibility rules
usdchecker scene.usd --strict       # Strict mode
```

Checks include: missing references, invalid schema usage, asset resolution, etc.

### usdcompress / usduncompress (usdz)

```bash
usdzip -r scene.usd -o scene.usdz   # Create usdz package
usdzip --list scene.usdz             # List contents
```

## Asset Resolution

### usdresolve

Test asset path resolution:

```bash
usdresolve asset_path               # Print resolved filesystem path
usdresolve @asset.usd@              # With asset syntax
```

## Diffing

### usddiff

Compare two USD files:

```bash
usddiff a.usd b.usd                 # Text diff of usda representations
```

## Recording

### usdrecord

Render a USD stage to image(s):

```bash
usdrecord scene.usd output.png                      # Single frame
usdrecord scene.usd frame.#.png --frames 1:100      # Frame sequence
usdrecord scene.usd output.png --imageWidth 1920     # Set resolution
usdrecord scene.usd output.png --renderer GL         # Choose renderer
```

## Python Utilities

### usdfixbrokenpixarschemas

Fix deprecated Pixar schema patterns in USD files:

```bash
usdfixbrokenpixarschemas scene.usd
```

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `PXR_PLUGINPATH_NAME` | Additional plugin search paths |
| `PXR_AR_DEFAULT_SEARCH_PATH` | Asset resolution search paths |
| `USD_EDITOR` | Editor for usdedit |
| `TF_DEBUG` | Enable debug output (e.g., `TF_DEBUG=USD_STAGE_OPEN`) |
| `USDC_USE_PREAD` | Performance: use pread for .usdc files |
