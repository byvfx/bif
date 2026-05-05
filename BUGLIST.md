# BUGLIST

Last updated: 2026-05-05

## Active Bugs

- **bif_qt scene browser: residual child-count parity check vs egui/usdview.** Qt now routes through `CompositeProvider` and filters empty child paths at the source, but a real-scene parity pass against usdview is still useful if a visible gap reappears. Noted 2026-04-16; narrowed 2026-04-30.
- OCIO ACES is not working in the viewport (Hill/Narkowicz approx active) full OCIO still needs to be implemented.

- Pre-existing C++ bridge test crashes: `test_load_pointinstancer_external_prototype` (lucy_100_fixed.usda), `test_load_relative_reference_usda` (lucy_100.usda), `test_define_scope_prim` — all crash at `UsdStage::Open` with STATUS_BREAKPOINT. Not caused by recent changes.
- `inst.prim_path` left empty for some USD load paths (observed on lucy.usd) — workaround via synthetic `/BIF/` path fallbacks in selection handler.
- need to see aovs, add aovs, plan this out, i want to implement aovs with ease and flexibility.
- need viewport switch between vulkan and render

## Fixed (since last update)

- v0.16.1 code-review followups swept on `v0.16.1-followups` (2026-04-30): save failures now surface USD/TfError details, shader-swap rollback restores `info:id` after failed input writes, Property Inspector callbacks use `QPointer` guards, modal confirmations defer out of selection-change slots, poisoned stage mutex paths log errors, edit-op negative tests cover locked save / target-switch undo / replace-layer idempotence, `cpp_bridge.rs` uses a shared `cstr(...)` helper, and the export-buffer lifetime contract has regression coverage.
- bif_qt View → Grid toggle added (2026-04-30): the Qt action now drives `DisplaySettings::grid_visible` and the viewport render path skips grid drawing when disabled.
- bif_qt HiDPI downstream scale fixed (2026-04-30): viewport gizmo and selection-outline logical pixel widths now multiply by the display scale factor. Manual synthetic `scale_factor = 2.0` visual verification remains before release.
- bif_qt scene browser empty child paths filtered at the `CompositeProvider` source (2026-04-30), preventing Qt index traversal from losing rows when USD returns empty paths.
- bif_qt: no way to view through USD cameras or standard orthographic views — camera-picker QComboBox added with UsdGeomCamera enumeration + 6 ortho presets + free-fly toggle (commit c835eaf, 2026-04-19).
- bif_qt property inspector opinion dot used prim-level winning layer for all attribute rows — replaced with per-attribute `get_attribute_opinions` call (Tier 1.5, 2026-04-17).
- bif_qt animation playback — `Renderer::set_time` + `on_frame_changed` invokable wired to QTimer (commit 962a3b7, 2026-04-17).
- bif_qt close stage — `reset_scene_state` helper + File → Close Stage (commit 962a3b7, 2026-04-17).
- bif_qt scene browser showed hardcoded demo tree — replaced with `CompositeProvider` traversal (commit 962a3b7, 2026-04-17).
- bif_qt property inspector showed fake attrs — real `get_prim_attributes` + `get_prim_stack` (commit 962a3b7, 2026-04-17).
- bif_qt timeline keyframes hardcoded — real `AnimatedTransform` keyframes (commit 962a3b7, 2026-04-17).
- bif_qt gizmo raycast not wired — `on_prim_pick` invokable + `pick_instance_at` (commit 962a3b7, 2026-04-17).
- Rigid-skinned mesh offset on animated characters — `SkinKind::Rigid` compression in `bif_core/src/usd/loader.rs` assumed `element_size=1, weight=1.0`, collapsing multi-joint rigid bindings (hair at w=0.333, nails at w=0.5) to a single fractional influence. Loader now only uses the compact encoding when `element_size == 1`; multi-joint rigid meshes broadcast through `SkinKind::PerVertex`. New regression test added. Fixed 2026-04-12.
- Dome light from Houdini USD not detected — C++ bridge now checks `UsdLuxDomeLight_1` (new schema), attribute lookup tries `inputs:texture:file` first. Fixed 2026-04-08.
- MaterialX displacement now natively extracted — 3-tier: surface shader input, GetDisplacementOutput() → ND_displacement node, UsdPreviewSurface fallback. Fixed 2026-04-08.
- Camera persistence bug: fixed in v0.13.0-dev (reset viewport/batch camera source on new scene load).
- HDRI properties bug: fixed in v0.13.0-dev (removed `is_loaded` guard, added `hdri_show_background`).

## Investigate & Validate

### Performance & Profiling

- Implement metrics tracking (time, memory, throughput).
- Compare bif vs usdview on reference scenes (OpenUSD docs plus custom assets).
- Review [OpenUSD v25 performance guidance](https://openusd.org/release/ref_performance_metrics.html) and audit compliance.
- Plan upgrade path to USD v26 after stabilization.
- Benchmark scene load and playback on SSD.
- Investigate RenderMan denoising integration.

### USD & Pipeline Research

- Investigate rigid-body animation flow into USD, then into bif for viewport/rendering (Houdini-style RBD procedural workflow).
- Keep selective prim loading simple for artists, with optional deeper controls.
- Validate subdivision surface behavior with self-authored assets.
- Verify proxy material fallback behavior (use display color when no material is bound).
- Check depth usage and expose useful controls in UI.
- Compare with Claude the Houdini USD nodes and what we could use, same as Katana. Let's build something simple but elegant.

### Architecture Notes

- Use GetBracketingTimeSamples instead of GetTimeSamples for large-clip performance.
- Extract evalTime logic into a helper.
- Consider exposing resolved evalTime to Rust for timeline scrubbing.
- Double-check USD schema compliance (custom vs standard).

### Scene Assembly Open Questions

- Goal: simple top layer for artists, optional deep layer for power users.
- Open design questions: UI paradigm, prim workflow, export and save patterns.
- RBD integration planning notes are in ./claude/plans.

## v0.16.0 Dogfood Test List (2026-04-28)

Run after `. .\setup_qt_env.ps1; . .\setup_usd_env.ps1; cargo run -p bif_viewer`. Each item: action → check viewport/save → reopen saved file in external `usdview`.

### C4b-Carry-1 — Visibility toggle
1. Select prim → uncheck `Visible` in Property Inspector header → viewport hides it.
    - visibility not working, lets have the eyes control visibility instead. remove checkbox lets also carry this design element to the layer stack. we can also make a node that controls this.
2. Ctrl+Z → comes back. Ctrl+Y → hides again.
    - this doesnt work, though the action is logged in the info.
3. Ctrl+S → close stage → reopen → still hidden.
    - see above anwser
4. Open saved working layer in `usdview` → confirm `token visibility = "invisible"` opinion.
    - over all saving layers seems to work. i tested this with our test asset root.usda
5. Edge case: toggle visibility on a prim with inherited-only visibility (no authored opinion). Should still write a working-layer opinion.
    - lets just move over to the eyes control visibility instead. remove checkbox then we can re-test.
### C4b-Carry-2 / C4b-2 — USDA Source dock
6. `View → USDA Source` opens a dock at the bottom showing the active edit-target layer.
    - this works
7. Switch edit-target via the layer stack panel → dock auto-refreshes (if editor isn't focused).
    - works
8. Type valid `over "/World/Cube" { token visibility = "invisible" }` → Apply → viewport updates, undo works.
    - doesnt do anything, even simple color changes dont seem take. 
9. Type bad syntax (e.g. unclosed brace) → Apply → red status label shows error, no dispatch happens.
    -works
10. Apply valid edit → Ctrl+Z → editor still shows the new text but stage reverted; refocus elsewhere → editor reloads to current.
    - doesnt work
11. Save → reopen in `usdview` → only authored opinions show, not whole composed scene.
    - we will need to collaborate on this. 
12. Edge case: paste a multi-prim USDA block → Apply → all opinions land in one undo step.
    - doesnt work

### C4b-1 — Material Sheet
13. Select prim with bound material → Material Sheet tab populates with grouped sections (Base, Specular, Transmission, Subsurface, Coat, Emission, Geometry, Other).
    - this all shows up

14. Drag/edit a `roughness` `QDoubleSpinBox` → press Enter → status bar reports change, viewport updates.
    - i can adjust the values but no changes in the viewport.
15. Click `base_color` swatch → `QColorDialog` opens with current sRGB color → pick new color → swatch + line-edit update with linear triple → opinion authored.
    - color picker opens and changes the color in the swatch and line-edit. but the colors dont show in the viewport, and i dont think the opinion is being authored.
16. Toggle a `bool` input → records one undo step.
    - undo doesnt work.
17. Edit a `token` / `string` input → editingFinished records.
    - cant find where this would be. 
18. Ctrl+Z → reverts last edit. Each parameter edit = exactly one undo step.
    - undo doesnt work. 
19. Bind material: select unbound prim → `Bind…` → enter `/Materials/Red` → viewport reflects binding → Ctrl+S → reopen → binding present.
    - dialog comes up and i can name where the input prim would be,  i think all this should really live in the node graph when applying / editing existing materials. lets plan this out because there could be some long term issues.  the scene graph doesnt update after you type in the path.
20. Edge case: select prim with no bound material → Material Sheet shows "Click Bind…" hint, no editors.
    - this is good to go.
21. Edge case: select Material prim itself → Material Sheet should degrade gracefully.
    - this isnt working in fact i dont think the syncing between viewport and scene graph with the material sheet. some times it will refresh when i unselect and re-reselect.
22. Color sanity: pick pure red in `QColorDialog` → check saved layer for `(1, 0, 0)` (linear) — confirms sRGB→linear conversion.
    - this is good to go.

### C4b-3 — Shading model swap
23. Select prim bound to UsdPreviewSurface → Material Sheet header dropdown shows `UsdPreviewSurface`.
    - this is good to go.

24. Switch to `OpenPBR` → confirmation `QMessageBox` fires → accept → swap lands.
    - this is good to go.
25. Single Ctrl+Z reverts entire mswap (id + remap) in one shot.
    - undo doesnt work.
26. Confirm best-effort remap: `diffuseColor` → `base_color`, `roughness` → `specular_roughness`, `metallic` → `base_metalness`, `ior` → `specular_ior`, `emissiveColor` → `emission_color`.
27. Switch from OpenPBR to UsdPreviewSurface with authored Subsurface/Transmission/Coat values → lossy `QMessageBox` lists drops by name.
28. Cancel swap dialog → dropdown reverts to current id without authoring.
29. Save → reopen in `usdview` → confirm `info:id = "OpenPBR"` (or target) in saved layer.
30. Edge case: pick the same model that's already set → no-op, no undo step recorded.

### End-to-end / regression
- i feel there are too many current issues here to really run the rest of the tests.
31. Open `test_assets/layers/root.usda` → make one edit of each kind (transform via gizmo, visibility, material bind, material param, variant, shading swap, USDA dock edit) → Ctrl+S → close → reopen in BIF → all edits present.
32. Open saved working layer alone in `usdview` → confirm it contains *only* authored overs, not full composed stage (layer-discipline check).
33. Run saved file through `usdchecker` → should pass.
34. Stress: chain 20 quick visibility/material edits → mash Ctrl+Z 20 times → all revert cleanly.
35. Multi-layer: switch edit-target between two writable sublayers → edit one → switch → edit other → Ctrl+S each → reopen → opinions land on right layers.
36. Window-title dirty bit: any edit makes title show `[*]`; Ctrl+S clears it.
37. Watch for known-flaky `test_should_restart_no_render` — unrelated but worth a sanity sweep.

### Known rough edges (deferred to v0.17)
- USDA parse errors don't show line/col yet — generic message only.
- No real-time keystroke parse in USDA dock — Apply-only.
- No drag-drop material binding (use `Bind…` button).
- File-watcher won't prompt if external tool edits the same file mid-session.
- `cpp_bridge.rs` still 4kloc monolith.
