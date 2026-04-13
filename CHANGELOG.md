# Changelog

All notable changes to BIF will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **v0.13.6 UsdSkelBlendShape — CPU morph target deformation** — blend shapes load from USD, evaluate per-frame, and compose with skinning (shapes applied before LBS).
  - **C++ FFI** — `UsdBridgeBlendShapeTarget` + `UsdBridgeBlendShapeBindingData` structs. `usd_bridge_get_blend_shape_binding_count/get/compute_weights` functions. Dense-expanded in C++ (zero-padded, `pointIndices` scattered at load), shape-order remap built per-mesh vs `UsdSkelAnimation::blendShapes`. `UsdSkelAnimQuery` cached per skeleton for weight eval.
  - **Rust FFI** — `RawBlendShapeTarget`/`RawBlendShapeBindingData` in `ffi_raw.rs`, safe wrappers in `cpp_bridge.rs` (`blend_shape_binding_count`, `get_blend_shape_binding`, `compute_blend_shape_weights`), conversion in `ffi_convert.rs`.
  - **`Mesh::blend_shapes` + `Mesh::bind_normals`** — `BlendShapeTarget` (name, dense offsets, optional normal offsets) and `BlendShapeBinding` (targets + FFI binding index). `bind_normals` snapshot parallels `bind_positions`.
  - **`skinning::apply_blend_shapes()`** — linear delta accumulation: `out[i] += target.offsets[i] * weight` per target. Weights unclamped (USD spec — exaggeration/anti-shapes legal). Normals not renormalized (downstream `skin_normals` handles it).
  - **Pipeline order** — blend shapes applied to `bind_positions` → scratch buffer → fed as input to `skin_positions`/`skin_normals`. Both inline and multi-draw playback paths updated.
  - **Loader** — walks bridge blend shape bindings, matches by mesh path, attaches `BlendShapeBinding`, snapshots `bind_normals`.
  - **6 new unit tests** — passthrough, single@1.0, two@0.5, normal deltas, unclamped weights, shapes+skin composition order.
  - **Test asset** — `test_assets/skel/two_bone_arm.usda` extended with 2 BlendShape prims (`squash`, `twist`) + animated `blendShapeWeights` over frames 0-36.
  - **GPU path stub** — `bif_renderer::gpu_blend_shapes::GpuBlendShapeLayout` reserves data layout for future GPU skinning.
  - **Known limits:** `UsdSkelInbetweenShape` deferred. Normal deltas required on the BlendShape prim for accurate shading — when absent, skinning uses bind-pose normals (documented). GPU path CPU-only (stubs only).

### Changed

- **Architecture refactor campaign closed** — `ARCHITECTURE_REFACTORS.md` and `ARCHITECTURE_REVIEW.md` updated to reflect that all 5 phases (FFI split, cross-platform, node graph eval engine, scene pipeline, renderer hub decomposition) and 7 of 8 prioritized review items have shipped across v0.13.0 → v0.13.5. §10 table now carries a Status column with commit references. `scene_loader.rs` shrinkage logged as a deferred Phase 4.5 follow-up pending v0.14.0 layer-aware rewrite.
- **Node graph extension checklist** — `wiki/architecture/node-graph-system.md` now has a concrete 10-step "Adding a New Node Type" reference card covering the eval engine wiring, persistence round-trip, and `node_dispatch.rs` event handler.

### Fixed

- **Rigid-skinned mesh offset on multi-joint rigid bindings** — `SkinKind::Rigid` compression in `crates/bif_core/src/usd/loader.rs` assumed `element_size == 1, weight == 1.0`, but USD's `UsdSkelSkinningQuery::IsRigidlyDeformed()` also covers meshes with uniform per-prim multi-bone influence. Taking only `joint_indices[0]` + `joint_weights[0]` for hair (3 head/neck bones at w=0.333 each) and fingernails (2 tip bones at w=0.5 each) collapsed every vertex by the fractional weight, visually shrinking the mesh toward its first bone's origin. Loader now gates the compact `SkinKind::Rigid` encoding on `element_size == 1` only; multi-joint rigid meshes broadcast their single authored influence block across post-split vertices and flow through `SkinKind::PerVertex`. Affected: HumanFemale hair, fingernails on accessory meshes. New regression test `skinning::tests::rigid_matches_pervertex_single_influence` locks `SkinKind::Rigid{J, 1.0}` to match equivalent `SkinKind::PerVertex{[J;N],[1.0;N], 1}`. Bug was pre-existing since v0.13.5.2.
- **`persistence.rs` path-relativization tests now cross-platform** — `path_relativization_*` tests used to hardcode `D:\\projects\\...` literals. Rewrote with `#[cfg(windows)]` / `#[cfg(not(windows))]` constants so the tests exercise the same logic on both targets. `sample_project()` `file_path` dropped its `D:\\` prefix (round-trip serde test doesn't hit the filesystem).

## [0.13.5] - 2026-04-10

### Added

- **v0.13.5 UsdSkel import + CPU linear blend skinning** — skinned characters load, render at bind pose, and deform per-frame when scrubbing the timeline.
  - **C++ bridge refactor** — `cache_skeleton_data()` now uses `UsdSkelCache` + `UsdSkelSkeletonQuery` + `UsdSkelSkinningQuery` via `UsdSkelRoot::ComputeSkelBindings`, replacing raw attribute reads. Persistent `skel_cache` member on `UsdBridgeStage` enables per-time-code eval without re-populating. Fixed a latent SSO-related UAF in `joint_path_ptrs` fixup.
  - **New FFI** — `usd_bridge_compute_skel_skin_xforms(stage, skel_idx, time_code, out, capacity)` evaluates cached SkeletonQuery at a time code and writes joint-skel transforms into a caller-allocated buffer. Rust wrapper `UsdStage::compute_skel_xforms(skel_idx, t)`.
  - **`bif_core::skinning` module** — `compute_skin_matrices` assembles the palette (`joint_skel * inv_bind * geom_bind`). `skin_positions` does weighted blend per vertex. `skin_normals` uses inverse-transpose 3×3 so non-uniform-scale joints produce correct normals. Out-of-range joint indices are skipped (no panic) on malformed skins.
  - **`Mesh::skin` + `Mesh::bind_positions`** — new `SkinBinding` struct stores skeleton path, joint indices/weights, element size, geom-bind-transform, and pre-inverted bind matrices. Loader populates these via `stage.get_skin_binding(mesh_idx)` + precomputed inv-binds per skeleton.
  - **Viewport `update_skinning(frame)`** — extends `update_animation` hot path. For each `SkinnedMeshEntry`, evaluates joint xforms, runs CPU LBS, writes positions into `mesh_data.vertices` (single-mesh or combined-buffer mode) and re-uploads via `queue.write_buffer`. Multi-draw mode flagged as v0.13.5 limitation (one-time warning, deferred).
  - **Scene registration** — scene loader scans loaded prototypes and registers skinned ones in `SceneManager::skinned_meshes`, snapshotting `bind_positions`, `SkinBinding`, and a per-entry scratch buffer so the hot path is allocation-free.
  - **Test fixture** — `test_assets/skel/two_bone_arm.usda`: 2-joint rig, 8-vertex box, deterministic bind pose (identity + T(0,1,0)), used by cpp_bridge, loader, and skinning round-trip tests. `usdchecker` clean.
  - **14 new tests** — 4 cpp_bridge (`test_load_two_bone_arm_skel`, `test_compute_skel_xforms_at_time`, `test_compute_skel_xforms_animated_character` with HumanFemale), 2 loader (`test_load_two_bone_arm_mesh_skin`), 8 skinning unit tests.
  - **Validation asset** — Pixar's HumanFemale UsdSkel example at `assets/UsdSkelExamples/HumanFemale/HumanFemale.walk.usd`. Anim eval test asserts joint motion over the authored 101-129 time range (max delta 39.5).
  - **Multi-draw skinning** — `MultiDrawState::update_skinning` writes skinned positions to per-prototype GPU vertex buffers (the path used for scenes with >1 prototype). Mirrors `update_vertex_animation`'s structure: build palette, run LBS into a per-entry scratch, write each prototype's `vertex_buffer`. Required to make characters with many parts (e.g. HumanFemale's 77 prototypes) actually deform.
  - **Per-mesh joint-order remap** — when a skinned mesh authors a custom `skel:joints` primvar (subset/reordering of the skeleton's joint order), `ComputeJointInfluences` returns indices into the mesh's local order, not the skeleton's. The bridge now builds a mesh-local→skel-global lookup map and remaps each influence index. Without this, body parts pull transforms from the wrong joints and the mesh explodes.
  - **UV-seam joint expansion** — subdivision meshes with `faceVarying` UVs duplicate vertices across UV seams in `mesh.positions` (e.g. 32890→35268), but `ComputeJointInfluences` returns one block per ORIGINAL vertex. The bridge now walks `vertex_index_map[split_idx → orig_idx]` and copies each vertex's influence block to all its post-split duplicates. Without this, every UV-seam vertex collapsed to `Vec3::ZERO`.
  - **Rigidly-deformed mesh broadcast** — meshes without per-vertex `jointIndices` (hair, buttons, teeth, eyelashes — bound to a single joint with `IsRigidlyDeformed()`) get a single influence block from `ComputeJointInfluences`. The bridge now broadcasts that block to every post-split vertex so the Rust hot path doesn't bounds-check out and drop the mesh to origin.
  - **SkelRoot world transform override** — for skinned meshes, the loader now uses the SkelRoot's world transform as the static instance matrix instead of the mesh prim's own world transform. Without this, mesh prims sitting under sub-Xforms (e.g. buttons translated to the chest) would double-apply the offset since the offset is also in `geomBindTransform`. New `skel_root_world_xform[16]` field on `UsdBridgeSkinBindingData` propagates the SkelRoot's xform from the C++ bridge.
  - **Skip per-frame xform animation for skinned meshes** — meshes with skin bindings now bypass the `AnimatedTransform` keyframe path. All per-frame motion comes from the joint deformation pass; applying the prim's animated xform on top would re-introduce the double-application that the SkelRoot override fixes.
  - Scoped out: blend shapes (→ v0.13.6), per-instance matrix re-baking for multiple non-identity instances of the same skinned prototype.

## [0.13.0] - 2026-04-09

### Removed

- **Welcome overlay** — removed centered "Open USD File..." dialog from empty viewport (still accessible via File menu)

### Added

- **Wireframe selection overlay** — `POLYGON_MODE_LINE` pipeline renders selected prim wireframe over the solid pass. `VariantChanged` and `FrameSelected` `AppEvent` variants for UI→renderer dispatch.
- **Qt UI spec §17-25** — context menus, multi-select, undo/redo feedback, long-op progress tiers, reduced-motion accessibility, scene tree filter/search, error state badges, workspace layout storage (global + per-project TOML), `cxx-qt` binding decision. Click targets 24px→32px rows; node labels 10px→12px.
- **CPU vertex displacement** — Post-load pass samples heightmap per vertex and offsets positions along normals (USD convention: 0.5 neutral, scale factor). `displacement.rs` with bilinear sampling, sync `image` crate loader (PNG/JPG/EXR/TIF), `Mesh::recompute_bounds()` for correct framing. Works in both viewport (wgpu) and Ivar (Embree) — same displaced positions. C++ bridge MaterialX fallback reads UsdPreviewSurface displacement when MaterialX extraction skips it. 14 unit tests.
- **Displacement texture pipeline** — UsdPreviewSurface `displacement` input + scale extracted in C++ bridge, flows through FFI to Material struct. Foundation for CPU vertex displacement.
- **Display color + shading mode** — `primvars:displayColor` flows from C++ bridge through Mesh to vertex color. Viewport shader uses it as fallback when no texture. `ShadingMode` enum (Textured/DisplayColor) with GPU uniform and UI dropdown in Display settings.
- **USD prim attribute inspector** — "Attributes" tab in property panel shows all prim attributes and primvars with types, values, and interpolation modes. C++ bridge `usd_bridge_get_prim_attributes()` enumerates by path. Arrays show count, scalars show value. Primvars color-coded with interpolation indicator.
- **Subdivision surface rendering** — Full Catmull-Clark subdivision via Embree 4. `SubdivInfo` preserves original polygon topology through MeshData pipeline. `vertices_orig` FFI passes pre-UV-split positions from C++ bridge. `rtcInterpolate` computes smooth limit-surface normals (dPdu×dPdv). Tessellation rate 8 for BVH accuracy. Fixed `RTCBufferType` enum values (Face=16, EdgeCreaseIndex=18, EdgeCreaseWeight=19). Test asset: `pig_subDivCrease_test.usd`.
- **Two-sided viewport lighting** — Viewport shader auto-flips normals facing away from camera, fixing dark surfaces on meshes with inconsistent winding.
- **HDRI show_background for Ivar** — `hdri_show_background` field on IvarState/RenderConfig. Camera rays respect toggle (solid bg when off), bounced rays always sample HDRI for correct lighting.
- **Obsidian knowledge base** — `wiki/` vault with 42 articles (architecture, USD, rendering, concepts, ADRs, UI/UX), 4 templates, LLM-optimized indexes. 92 devlog entries get `## Wiki Links` backlink sections. `bif-commit` skill updated to maintain wiki on each commit.
- **Markdown linting** — `.markdownlint.json` config + all 235 `.md` files linted/fixed. `pre-commit` framework with `markdownlint-fix` and `cargo fmt` runs on every commit.

### Added

- **Native MaterialX displacement** — 3-tier extraction: surface shader `displacement` input, `GetDisplacementOutput()` → `ND_displacement_float/vector3` node traversal (scale + texture), UsdPreviewSurface companion fallback. No longer requires manual UsdPreviewSurface wiring.
- **SceneQuery viewport migration** — `build_scene_graph_cache()` now takes `&dyn SceneQuery` instead of `&Scene`. Enables future LayerAwareScene drop-in for v0.14.0.
- **SceneQuery trait** — read-only query API in bif_core abstracting Scene field access. 15 methods covering prototypes, instances, materials, cameras, lights, timeline, metadata. `find_instance_by_prim_path` encapsulates the 3-strategy prim path lookup (exact, prefix, synthetic /BIF/ fallback). 9 tests. Enables future LayerAwareScene for M32 opinion trace without viewport changes.
- **Dispatch split** — extracted `render_dispatch.rs`, `selection_dispatch.rs`, `project_dispatch.rs` from monolithic `dispatch_events()` in render.rs. 20 AppEvent match arms → individual handler methods following `node_dispatch.rs` pattern. `dispatch_events()` is now a thin router.

### Fixed

- **Code review hardening (v0.13.0 ship prep)** — Validated faceVarying UV indices (count match, non-negative, in-bounds) with fallback to per-vertex path. Removed misleading top-level `displacement` input check in `extract_materialx_properties()` that could clobber scale on standard_surface materials. Added `checked_mul` overflow guard in FFI conversion. Added `ND_displacement_vector3` diagnostic warning (downstream only handles scalar). Flipped `get_light_attr` lookup order to try `inputs:` prefix first (new schema default).
- **Subdiv faceVarying UVs** — Full pipeline: C++ bridge preserves raw faceVarying UV data before vertex split, flows through FFI to Embree which sets up dual topology (vertex + faceVarying) via `rtcSetGeometryTopologyCount`/`rtcSetGeometryVertexAttributeTopology`. Fixes broken textures on subdivision surfaces.
- **Houdini DomeLight_1 not detected** — C++ bridge now checks `UsdLuxDomeLight_1` (new USD Lux schema). Attribute lookup tries `inputs:texture:file` first (new schema), falls back to `texture:file` (old schema).
- **UsdStage thread-safety soundness hole** — removed `unsafe impl Sync for UsdStage`, wrapped in `Arc<Mutex<UsdStage>>` across 10 files. Prevents potential data races from concurrent C++ stage access (set_variant_selection mutates through `&self`). Batch render and animation paths lock before each FFI call.
- **Deadlock in handle_prim_selected** — stage_guard held lock, then re-locked same non-reentrant Mutex. Now reuses existing guard.
- **Synthetic /BIF/ path double-slash bug** — `find_instance_by_prim_path` and `resolve_instance_index` now strip leading `/` before prepending `/BIF/`.
- **Mutex lock diagnostics** — all 19 `.lock().unwrap()` sites replaced with `.lock().expect("UsdStage mutex poisoned")` for actionable crash messages.
- **Animation lock granularity** — lock-per-iteration in vertex animation loop → lock-once-before-loop.

### Changed

- **Embree feature-gated** — `bif_renderer` Embree dependency behind `embree` feature (default on). Enables Linux CI without Embree linking. `cargo check -p bif_renderer --no-default-features` now passes.
- **Linux CI expanded** — `check-linux` job now runs clippy + tests for `bif_renderer --no-default-features` (97 non-Embree tests).
- **setup_usd_env.sh** — added `bin/usd` plugin directory scan for parity with PS1 script (MaterialX plugin may land in either `bin/usd` or `lib/usd`).
- **Identity pivot** — BIF reframed as "USD Orchestration Tool" (layer-aware editing + procedural assembly + rendering). Docs updated: README, BIF_USD_WORKFLOW, SESSION_HANDOFF, CLAUDE.md.
- **Drive migration D: → G:** — vcpkg/OIDN paths updated in `build.rs`, `setup_usd_env.ps1`, `CLAUDE.md`. Repo relocated to `G:\__projects\_programming\rust\bif`.
- **Qt UI design spec** — Consolidated UI_DESIGN.md as authoritative pre-implementation spec (16 sections, ~730 lines). 14 Stitch mockups across 2 batches. Vertical code split layout variant, Bjorn asset manager, active layer safety system, opinion encoding table, command palette details, canonical component specs, workspace configs. UX Architect + UX Researcher reviews conducted and incorporated.
- **Stitch UI mockups** — 2 batches of Google Stitch-generated mockups covering Assembly (3 variants), Lighting (2 + command palette), Materials (2 + node graph safety), Render (2 + catalog), and First Launch onboarding screen. Obsidian Graphite "Quiet Confidence" design system.

- **MaterialX file format support** — usdMtlx plugin detection with startup diagnostic, `resolve_mtlx_input()` follows Material interface connections for scalar values, deep descendant shader search by `info:id`, refactored extraction into shared helper. `setup_usd_env.ps1` scans both `bin/usd` and `lib/usd` for plugin resources. Enables loading external `.mtlx` references (OpenPBR Shader Playground pattern). Requires `vcpkg install usd[materialx]:x64-windows`.
- **Power-weighted light sampling** — `LightList` now selects lights proportional to emitted power via CDF instead of uniform 1/N. `Light` trait gains `power()` method. Foundation for future hierarchical light tree (v0.20.0). 9 new tests.
- **Max Depth UI slider** — interactive Ivar path tracer now exposes max bounce depth (1–32) in the render settings panel, with restart on change
- **Material roughness trait** — `Material::roughness()` method (default 1.0) implemented for Metal and OpenPbrSurface, used by SHARC cache skip logic
- **SHARC cache roughness skip** — radiance cache reads/writes skip surfaces with roughness < 0.1 to prevent blurred reflections on glossy/mirror materials
- **GitHub Pages site** — mdBook-based dev diary + manual at byvfx.github.io/bif. Auto-deploys on push via `scripts/generate-site.sh` (copies devlog/docs, generates SUMMARY.md) + GitHub Actions workflow. Manual sections: getting started, architecture, USD reference, changelog.
- **FFI bridge split** — extracted `ffi_raw.rs` (898 lines, raw C types + extern block) and `ffi_convert.rs` (2,054 lines, 17 conversion functions + 44 tests) from monolithic `cpp_bridge.rs`. Conversion logic now testable without C++ DLLs. Phase 1 of architecture deepening plan.
- **Architecture refactors plan** — `ARCHITECTURE_REFACTORS.md` documenting 5-phase plan: FFI split, Linux support, node graph eval engine, scene pipeline, renderer decomposition. 45-65 new tests targeted.
- **Cross-platform build foundation** — platform-detect CMake generator, vcpkg triplet, lib paths in `build.rs`. Linux CI job (bif_math + bif_renderer). `setup_usd_env.sh` for Linux/macOS. Windows `process::exit(0)` guarded with `#[cfg(windows)]`.
- **Node graph eval engine** — `node_graph/eval.rs` with `collect_auto_compute_events()` pure function + 19 tests. Decouples auto-compute decisions from egui rendering (fixes nodes-scrolled-out-of-view bug). Phase 3 of architecture deepening.
- **Scene pipeline extraction** — `scene_pipeline.rs` with 5 pure functions (prim path resolution, material lookup, instance expansion, world bounds, axis correction) + 16 tests. Testable without wgpu. Phase 4 of architecture deepening.
- **Renderer dispatch decomposition** — extracted `handle_node_graph_event()` (726 lines) + 2 helpers into `node_dispatch.rs`. render.rs `dispatch_events()` becomes thin router. Phase 5 of architecture deepening.
- **ffi_convert wiring** — 17 UsdStage `get_*` methods now delegate to `ffi_convert::convert_*()`. `cpp_bridge.rs` reduced from 4,542 to 2,824 lines (38% reduction). Code review fixes: dedup `resolve_prim_path`, `panic!` → `unreachable!`, `log::warn` on fallbacks.
- **USD performance metrics harness** (`bif_perf`) — new `benchmarks/` crate with modular Metric trait, 7 metrics (StageOpen, PayloadLoad, MeshExtract, MaterialLoad, PrimTraversal, StageClose, FullLoad), scene registry with tier filtering, terminal/YAML/CSV reporters, CLI (`run`, `list`, `audit`). Based on USD `ref_performance_metrics.html` methodology (N iterations, warmup, min/max/mean/median/p95).
- **USD best-practices audit** — AuditCheck trait with 6 checks from maxperf.html: binary format, payload usage, prim count, instance usage, Alembic detection, layer count (skip). CLI `audit` subcommand with pass/warn/fail/skip output.
- **External measurement targets** — usdview (Python/pxr subprocess) and Houdini (hython subprocess) targets for cross-tool USD load time comparison
- **Benchmark comparison** — `compare` subcommand loads two YAML result files, shows per-metric delta_ms/delta_%/FASTER/SLOWER/~same
- **Historical results storage** — `--save` flag auto-saves YAML to `benchmarks/results/` with timestamp + `latest_{target}.yaml`
- **Asset download helper** — `download` subcommand shows missing official assets with download URLs (Kitchen Set, ALab, Moore Lane)
- **Per-tile UDIM loading** — UdimTileSet/UdimGridLayout types in bif_core, per-tile sampling (CPU+GPU), contiguous texture array blocks with shader tile offset. Eliminates atlas stitching (~3s/set). Unified CPU/GPU path ready for material editor.
- **Box SubDiv crease test assets** — `box_subDivCrease_test.usd/usda` for subdivision surface crease weight validation.
- **Kilo config** — `kilo.jsonc` with MCP context-mode plugin and bash/skill permission presets.
- **Site SUMMARY.md** — updated mdBook nav index covering full devlog history (Jan 2025 – Apr 2026).

### Fixed

- **Selection outline rendering** — replaced broken `PolygonMode::Line` + `shading_mode` `queue.write_buffer` hack (DX12 depth bias + write ordering made lines invisible) with a normal-expanded back-face silhouette pipeline (`outline.wgsl`) and a dedicated `wireframe_cam_bind_group`. Clean silhouette outline (no internal edges) on selected prims.
- **Bidirectional tree ↔ viewport selection sync** — viewport click → tree row highlights (via new `select_at_screen()` + `denormalize_synthetic_path()`), tree click → outline appears on corresponding mesh (prefix + synthetic `/BIF/{path}/{idx}` fallbacks in `PrimSelected` handler). Tree auto-expands ancestors so selected rows become visible. Click on empty viewport space deselects; clicks on UI panels preserve selection.
- **Camera persistence bug** — reset viewport/batch camera source on new scene load; stale USD camera from previous scene no longer persists
- **HDRI background toggle** — removed `is_loaded` guard on UpdateHdriParams so params propagate for auto-loaded DomeLight HDRIs; background now correctly hides when unchecked
- **PointInstancer time-sampled data** — C++ bridge now falls back to stage startTimeCode or first time sample when Default yields empty arrays (fixes Pixar PointInstancedMedCity.usd and similar files with no default values)
- **PointInstancer Xform prototype resolution** — prototype_map now includes parent Xform paths so instancer targets like `/Prototypes/proto_0` resolve to child mesh `/Prototypes/proto_0/mesh_0`
- **bif_perf code review fixes** — stable Rust compat (`count % 2` over nightly `is_multiple_of`), safe `u64::try_from` for duration stats, sample stddev (N-1), metadata surfaced in reports, iterations>=1 guard, CSV field escaping, `CARGO_MANIFEST_DIR` workspace root, `serde_yml` replacing deprecated `serde_yaml`, removed unused `csv` dep
- **Audit review fixes** — AlembicUsageCheck→Skip (prim paths don't contain file refs), InstanceUsageCheck now includes native instances, PayloadUsageCheck uses root prim count, run_audit runs path-only checks before payload load, `result()` helper on AuditCheck trait, 5 unit tests

### Changed

- **Tree browser selection visuals** — removed node-source row tinting (green/blue); only the selected row paints a background. Selection color now uses `from_rgba_unmultiplied(74, 144, 217, 75)` (was premultiplied which produced near-additive blending against dark panel). `selectable_label` passed `false` to avoid double-painting on top of manual row bg.
- **Documentation test count sync** — Updated test counts to 516 total (was stale 160+/400+). Per-crate: bif_math (74), bif_core (163), bif_renderer (111), bif_viewport (149), bif_viewer (19). Phase 1 FFI split marked complete in ARCHITECTURE_REFACTORS.md.
- **Async texture loading for all paths** — working scene rebuild and legacy loader now use async placeholders + streaming instead of blocking sync load. Viewport interactive immediately on scene load.

### Fixed

- **OIDN denoiser using geometric normals** — switched to shading normals for better edge preservation on normal-mapped surfaces
- **Zombie process on close** — `process::exit(0)` after event loop prevents native DLL teardown deadlock on Windows
- **Unsaved changes dialog not showing** — `mark_dirty()` added to gizmo drag, undo, redo, keyframe operations
- **Save dialog hidden behind window** — store window Arc in Renderer, hide main window while rfd MessageDialog shows
- Shader `tex_offset` comment for future UDIM texture slots (roughness, normal, etc.)
- Deduplicate `find_udim_tiles` calls — `prepare_texture_placeholders` passes expanded paths to async loader
- Removed vestigial `udim_grid_*` fields from `Texture` struct (16 bytes/texture savings)
- Clean up stale `.bif_cache/udim/` directories on scene load
- Panic guard: bounds check in `upload_streamed_texture` + views/textures sync in `prepare_texture_placeholders`
- UDIM capacity check before allocating — skip sets that won't fit in texture array
- Validate UDIM ID range (1001-1200) in `UdimGridLayout::from_tiles`
- `#[must_use]` on `grid_slots()`, negative UV test, clamping behavior documented

### Removed

- UDIM atlas stitching, disk cache (UdimCacheMeta, cache dir/key/load/save/clear), scale_pixels_box, ClearUdimCache UI, serde_json dep from bif_core
- `create_gpu_textures_for_scene` sync loader (replaced by async path)

- **M31: Per-node scene graph visualization** — source_node tagging on ProceduralPrim, prim count `[N]` badges on node headers, Scene/Node tab bar with NodeFilteredProvider for upstream-filtered browsing, row highlighting for selected node's prims in full scene browser. 4 new tests.
- **M30 Phase 6: Cache node** — SceneNode::Cache with bypass toggle, visual indicators (Cached/Stale/Bypassed), property inspector, CacheToggleBypass/CacheClear events. Data serialization deferred.
- **M30 Phase 5: eval modes** — Auto/Manual/OnMouseRelease with dirty node tracking, eval mode ComboBox toolbar, Cook All/Cook Selected buttons, dirty visual indicators, CookNode event for deferred compute dispatch
- **M30 Phases 3-4: File menu + save/load UI** — File > New/Open/Save/SaveAs with Ctrl+N/O/S/Shift+S, Recent Files submenu, dirty tracking on node/transform events, dynamic title bar with unsaved indicator, unsaved-changes prompt on close/new/open, extract/apply project for full state round-trip
- **M30 Phase 2: ProjectFile persistence** — save/load .bif (bincode) + .bifa (JSON), CameraData snapshot, relative path resolution, RecentFiles (8 max), ProjectState (dirty flag + window title), EvalMode enum, format versioning. 10 unit tests.
- **M30 Phase 1: serde foundation** — Serialize/Deserialize derives on all types needed for .bif/.bifa persistence across 4 crates (SceneNode, GraphNodeId, BatchRenderSettings, Camera types, USD enums, renderer configs). egui-snarl serde feature enabled, bincode added. Round-trip tests for all 10 node variants + Snarl graph.
- **M29.5 UI overhaul** — centralized theme system (theme.rs), scene browser promoted to primary left panel, viewport stats overlay, File/View/Render menu bar with Ctrl+O, node params moved from show_body() to property inspector (all 10 types), welcome screen on empty state, Unicode prim icons replacing emoji, tooltips on all controls, node selection via header click with accent highlight
- **VNDF GGX sampling** — Heitz 2018 visible normal distribution sampling replaces NDF sampling for 2-4x convergence on rough metals at grazing angles
- **GraphNodeId newtype** — framework-agnostic node ID decouples node graph evaluation from egui_snarl, preparing for M30 persistence and Qt migration
- **GpuMaterialState / GpuTextureState** — extracted 12 GPU fields from Renderer into focused sub-structs
- **types.rs** — moved PurposeMode, DisplaySettings, UsdLoadStatus, AsyncChannels, SceneInstances out of lib.rs
- **State mutation convention** — documented direct-mutation vs EventBus patterns in render.rs
- **set_instance_purpose(index)** — add_instance returns index; replaces fragile set_last_instance_purpose API
- **Stage::Load() eager population** — 8.3x USD loading speedup (8.8s→1s on Glasses.usd) by forcing eager USD composition
- **GPU buffer size guards** — cap triangle material, vertex, and index buffers to device limits with placeholder fallback (prevents crash on 342M vert scenes)
- **Chunked texture loading** — load 16 textures at a time (was all-at-once), paced GPU uploads (32/frame)
- **Texture count warning** — log when >511 textures exceed viewport GPU slot limit
- **Texture backpressure** — `sync_channel(32)` prevents unbounded RAM growth on 500+ texture scenes
- **Adaptive texture downscale** — auto 512px for 200+ textures, 1024px for 50+ (background thread downscale before channel send)
- **MAX_VIEWPORT_TEXTURES** — raised 512→2048 for production scenes
- **Free C++ mesh cache** — `usd_bridge_free_mesh_geometry()` frees normals/UVs/subdivision after Rust copy (~8GB on 335M vert scenes)
- **Texture streaming progress** — periodic log of loaded/total count
- **LoadNone deferred payloads** — `UsdStage::Open(LoadNone)` opens hierarchy only; `load_payloads()` loads geometry on demand
- **UDIM tile downscale before stitch** — tiles downscaled to adaptive size before atlas assembly (was full-res → OOM)
- **Pick scene size guard** — skip Embree pick BVH for >50M tris (prevents 25GB OOM)
- **C++ debug log flags** — `g_log_textures`, `g_log_timing`, `g_log_variants` toggle output sections
- **Parallel UV seam split** — 3-pass `cache_stage_data()` refactor using USD `WorkParallelForN`; per-mesh geometry extraction in parallel with per-thread `UsdGeomXformCache`
- **Viewport .tx texture cache** — viewport prefers pre-converted .tx files over source JPG/PNG via `resolve_tx_path`; auto-triggers background .tx conversion on scene load
- **Parallel .tx conversion** — `convert_textures_to_tx` uses rayon for concurrent subprocess spawning (~4x speedup)
- **Parallel Ivar texture pre-warm** — `pre_warm_parallel` loads all textures concurrently before material build (9s→2.7s on 13 network textures)
- **Clear .tx cache** — UI button to delete cached .tx files for current scene
- **UDIM .tx fallback** — tile discovery checks for .tx variant when source file missing
- **OpenPBR MaterialX import** — C++ bridge recognizes `ND_open_pbr_surface` with fallback input names (`base_metalness`, `geometry_normal`, `geometry_opacity`)
- **Shading normal AOV** — `Ns` layer in EXR + "Shading Normal" in viewport AOV dropdown; shows normal-mapped normals vs geometric `N`

### Removed

- Dead `show_ui` toggle (field + early return, never wired to keybinding)
- Emoji prim type icons (replaced with colored Unicode geometric shapes)
- 35+ inline Color32 literals (replaced with theme constants)

### Fixed

- **M30 review fixes (13 items)** — SavePromptResult enum fixes "Yes" = silent cancel data loss; eval_mode now persists round-trip; reset_project clears scene/instances/stage/selection; cached recent files (was per-frame disk I/O); pub FORMAT_VERSION const; Cache node dirty propagation; path canonicalization for recent files; compute nodes marked dirty on project load; OnMouseRelease labeled TODO; SelectNode no longer sets dirty flag; bincode fragility documented; title bar cached; save_recent_files logs warnings
- **OpenPBR energy conservation** — diffuse attenuated by (1-F_specular) to prevent energy creation at grazing angles
- **Shadow ray shading normal** — offset uses shading normal instead of geometric normal, fixing dark bands with normal maps
- **Distant light angle units** — convert degrees→radians in constructor, fix cos_max formula for correct soft shadows
- **Normal matrix zero-scale guard** — fallback to identity for degenerate transforms (prevents NaN on hidden USD instances)
- **SHARC cache NaN guard** — filter non-finite values from lock-free cache torn reads
- **Point light falloff** — use max() instead of additive epsilon for correct near-light energy
- **OpenPBR is_delta()** — any transmission with roughness<0.001 treated as delta (saves wasted shadow rays)
- **Crease data validation** — validate index/sharpness counts before Embree FFI
- **NaN guards** — HDR direction_to_uv zero-length, texture sample non-finite UV inputs
- **UsdBridgeError Success** — safe fallback instead of unreachable!() panic
- **Box filter boundary** — half-open interval avoids double-counting at bucket edges
- **Embree Drop safety** — documented field-order invariant preventing use-after-free
- **Node graph unwrap** — let-else pattern match prevents potential panic on disconnect
- **Mesh dedup hash** — 10→50 vertex/index samples + normal hashing reduces collision risk
- **u32 overflow in triangle count display** — use u64 for large scene stats (607M tris × 13K instances)
- **Normals lost on meshes without UVs** — deferred normals copy was inside UV block; meshes with normals but no UVs got flat shading
- **UV seam split hash collisions** — PairHash uses bit mixing instead of MSVC identity hash

### Changed

- **Copy on Transform** — derive Copy on Transform struct, removing redundant .clone() calls across codebase
- **HdrImage::downscale_to_max_dim** — returns Option<Self> to avoid cloning when no downscale needed
- **Prototype::bounds removed** — redundant field, use mesh.bounds directly
- **IBL Vec3 ops** — replaced local [f32;3] math helpers with glam Vec3 operations
- **Orthonormal basis dedup** — light.rs uses bif_math::build_orthonormal_basis instead of local copy
- **HDRI pole clamp** — resolution-dependent half-texel clamp replaces fixed epsilon
- **SHARC TOCTOU race** — documented known lock-free EMA blend race condition
- **Deferred normals copy** — skip 289ms wasted copy when UV seam split rebuilds normals
- **Bulk vertex/normal copy** — `assign()` replaces push_back loops in C++ bridge
- **UV seam split** — `std::map` → `std::unordered_map` (O(log n) → O(1))
- **Deferred Rust clones** — mesh dedup hash from references, clone only unique meshes
- **Disable Ivar prewarm** — materials built on-demand at render time (saves 8+ GB RAM on large scenes)
- **UDIM atlas cap** — 64MP → 32MP (1GB → 512MB max per atlas)
- **Remove texture upload clone** — skip `tex.data.clone()` when no downscale needed

## [0.12.0] - 2026-03-21

### Added

- **Purpose filtering** — USD purpose attr (render/proxy/guide) toggle in viewport; C++ bridge `compute_inherited_purpose()` with hierarchy walk for instance proxies
- **Purpose enum** — `Purpose` type on `Instance` with per-instance filtering in combined mesh build
- **Native instance purpose** — own purpose from scene hierarchy (not inherited from prototype mesh)
- **Material diagnostics** — debug-level Ivar material/texture logging (`RUST_LOG=bif_renderer=debug`)
- **USD export** — stage metadata, materials (UsdPreviewSurface + OpenPBR MaterialX), lights (4 types), cameras, visibility, GeomSubsets, invisible_ids roundtrip
- **Curves/Points import** — UsdGeomBasisCurves and UsdGeomPoints via C++ bridge with viewport preview
- **Bound Material inspector** — property inspector shows OpenPBR params for selected prim
- **Implicit geometry** — C++ bridge tessellates UsdGeomSphere/UsdGeomCube with dedup + native instances
- **DomeLight** — auto-HDRI, rotation, color temperature via Tanner Helland
- **Glass/transmission** — extract from MaterialX/UsdPreviewSurface, Snell's law refraction + TIR
- **USD spec compliance** — 15 FFI fields (visibility, doubleSided, subdivisionScheme, velocities, cameras, lights)
- **Embree subdivision** — Catmull-Clark from USD polygon topology + crease data
- **UDIM texture atlas** — probe/stitch pipeline for Ivar + viewport with memory caps
- **Blue noise sampling** — Cranley-Patterson rotation, pixel reconstruction filters
- **EventBus** — typed `AppEvent` enum replacing 23 string-keyed temp-data slots
- **Subsystem extraction** — SceneManager, SelectionManager, CameraState, IvarContext, NodeGraphContext
- **50+ new tests** — bif_math, bif_renderer, bif_viewer

### Fixed

- **UsdPreviewSurface specular** — `specularColor` was averaged to `specular_weight`, breaking dielectrics; now always 1.0 (IOR/Fresnel-controlled)
- **Ivar double-filtering** — purpose filter applied twice causing material index misalignment
- **OIIO mip buffer overflow** — `read_image()` always passed `miplevel=0` into smaller mip buffers
- **UDIM double V-flip** — `transform_uv()` returned pixel-space causing second flip in `sample()`
- **Memory leak** — `UsdEditLayer::save()` nulled pointer preventing Drop from freeing C++ handle
- **SHARC race** — two threads CAS-increment but only one's radiance survived
- **Point light specular ring** — MIS power heuristic crushed delta light specular peak
- **Shadow ray self-intersection** — offset along surface normal prevents acne
- **Ivar texture paths** — resolve relative paths via `material.source_dir`
- **Material dedup** — instance proxy materials cached once per prototype
- **Instance proxy binding** — resolve `bound_material_path` during traversal
- **Backface culling** — `FrontFace::Cw` → `FrontFace::Ccw` for USD rightHanded convention

### Changed

- **OpenPBR migration** — Disney Principled BSDF → OpenPBR Surface v1.1 across all 6 crates
- Renderer decomposed from ~75 fields into sub-structs
- `render()` (2,695 lines) split into 6 phase methods
- `node_graph.rs` (2,272 lines) split into module directory
- Winding convention CW → CCW throughout
- `ControlFlow::Poll` → `Wait` (was burning 100% CPU idle)
- Texture loading ~25-50x faster (raw u8 path, GPU mipmaps, async streaming)

### Removed

- Rust USDA parser (all USD loading via C++ bridge)
- Dead `instanced_geometry_bvh.rs` (274 lines, broken UB)
- Stub LayerStack/Composition property inspector tabs (will return in M30+)

## [0.11.0] - 2026-03-13

### Added

- Ivar material cache + pre-warm (background texture/material loading)
- Embree indexed geometry path (shared vertices, parallel hit data)
- `MeshData::extract_positions/normals/uvs()` SOA helpers
- Shared `build_materials()` helper

### Performance

- Ivar subsequent builds: 6.7s → ~47ms (cached materials)
- Embree indexed geometry: 37s → 47ms BVH build

## [0.1.0] - 2026-03-12

Initial versioned release. Viewport rendering, instancing, USD C++ bridge, Embree ray tracing, materials, MaterialX, animation, batch render, node graph, scatter, SHARC cache, OIDN denoising.
