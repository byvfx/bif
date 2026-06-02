//! High-level USD scene loading.
//!
//! This module provides the main entry point for loading USD files
//! and converting them to BIF scene graph representation.
//!
//! Supports all USD formats via the C++ bridge:
//! - `.usda` - ASCII text format
//! - `.usdc` - Binary crate format  
//! - `.usd` - Auto-detect format

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use indexmap::IndexMap;

use thiserror::Error;

use crate::mesh::{BlendShapeBinding, BlendShapeTarget, Mesh, SkinBinding, SkinKind};
use crate::point_cloud::{DistributionMethod, PointAttributes, PointCloud};
use crate::scene::{
    AnimatedTransform, CurvesPrim, Light, PointsPrim, Purpose, Scene, TimelineInfo, Transform,
    TransformKeyframe,
};
use crate::usd::cpp_bridge::{UsdBridgeError, UsdLightType, UsdStage};
use crate::usd::layer::PayloadPolicy;
use bif_math::Mat4;

/// Errors that can occur during USD loading.
#[derive(Error, Debug)]
pub enum LoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("USD bridge error: {0}")]
    Bridge(#[from] UsdBridgeError),

    #[error("No geometry found in USD file")]
    NoGeometry,

    #[error("Invalid prototype reference: {0}")]
    InvalidPrototype(String),
}

/// Result type for loading operations.
pub type LoadResult<T> = Result<T, LoadError>;

/// Format a number with comma separators for readability.
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

/// Load a USD file and return a BIF Scene.
///
/// This function supports all USD formats via the C++ bridge:
/// - `.usda` - ASCII text format
/// - `.usdc` - Binary crate format
/// - `.usd` - Auto-detect format
///
/// References are automatically resolved.
///
/// # Example
///
/// ```ignore
/// use bif_core::usd::load_usd;
///
/// let scene = load_usd("scene.usdc")?;
/// println!("Loaded {} instances", scene.instance_count());
/// ```
pub fn load_usd<P: AsRef<Path>>(path: P) -> LoadResult<Scene> {
    let (scene, _stage) = load_usd_with_stage(path)?;
    Ok(scene)
}

/// Load a USD file and return both the BIF Scene and the UsdStage.
///
/// Use this when you need access to the USD stage hierarchy for
/// scene browsing or inspection.
///
/// # Example
///
/// ```ignore
/// use bif_core::usd::load_usd_with_stage;
///
/// let (scene, stage) = load_usd_with_stage("scene.usdc")?;
/// println!("Loaded {} prims", stage.prim_count());
/// ```
pub fn load_usd_with_stage<P: AsRef<Path>>(path: P) -> LoadResult<(Scene, UsdStage)> {
    // Strict variant — first-load callers expect geometry. Preserves the
    // pre-v0.14 contract where an empty scene is a user error (bad USD file,
    // missing geometry, etc.).
    let (scene, stage) = load_usd_with_stage_policy_muted(path, PayloadPolicy::LoadAll, &[])?;
    if scene.prototypes.is_empty() {
        return Err(LoadError::NoGeometry);
    }
    Ok((scene, stage))
}

/// Same as [`load_usd_with_stage`], but applies the given layer mutes after
/// opening the stage and before payloads are loaded — so the C++ bridge's
/// composition + caching pass sees the mutes on its first pass.
///
/// `muted_layer_identifiers` are USD layer identifiers (typically asset paths
/// resolved by [`crate::SceneLayerState::muted`]) to mute. Unknown identifiers
/// are silently ignored — the bridge would TF_WARN but we don't propagate.
pub fn load_usd_with_stage_muted<P: AsRef<Path>>(
    path: P,
    muted_layer_identifiers: &[String],
) -> LoadResult<(Scene, UsdStage)> {
    load_usd_with_stage_policy_muted(path, PayloadPolicy::LoadAll, muted_layer_identifiers)
}

/// Same as [`load_usd_with_stage_muted`], but lets callers decide whether
/// payloads are populated eagerly (`LoadAll`) or left unloaded (`LoadNone`)
/// after layer mutes are applied.
pub fn load_usd_with_stage_policy_muted<P: AsRef<Path>>(
    path: P,
    payload_policy: PayloadPolicy,
    muted_layer_identifiers: &[String],
) -> LoadResult<(Scene, UsdStage)> {
    let path = path.as_ref();
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed");

    let load_start = Instant::now();

    // Open stage via C++ bridge (LoadNone — hierarchy only, no geometry yet)
    let stage_start = Instant::now();
    let stage = UsdStage::open(path)?;

    // v0.14.0 — apply caller-supplied layer mutes BEFORE payload load so the
    // first composition pass sees the mute state. Without this, the C++
    // bridge caches geometry under the un-muted composition and any later
    // mute call leaves stale data in `bridge->meshes` until file reload.
    for identifier in muted_layer_identifiers {
        if let Err(e) = stage.set_layer_muted(identifier, true) {
            log::debug!("pre-load mute on {identifier} ignored: {e:?}");
        }
    }

    // Apply the requested payload policy after the mute set is in place so
    // the first composition pass sees the final layer+payload configuration.
    let prim_count = match payload_policy {
        PayloadPolicy::LoadAll => stage.load_payloads()?,
        PayloadPolicy::LoadNone => 0,
    };
    log::info!("Stage loaded: {} prims ({payload_policy:?})", prim_count);
    let stage_time = stage_start.elapsed();

    let mut scene = Scene::new(name);
    let mut prototype_map: IndexMap<String, usize> = IndexMap::new();

    // Extract stage metadata (metersPerUnit, upAxis)
    if let Ok(meta) = stage.get_stage_metadata() {
        log::info!("Stage metadata: {}", meta);
        scene.stage_metadata = Some(meta);
    }

    // Extract timeline metadata
    if let Ok(timeline_data) = stage.get_timeline() {
        if timeline_data.has_authored_time_range {
            scene.timeline = Some(TimelineInfo {
                start_frame: timeline_data.start_time_code,
                end_frame: timeline_data.end_time_code,
                fps: timeline_data.frames_per_second,
            });
            log::info!(
                "Timeline: frames {:.0}-{:.0} @ {:.0} fps",
                timeline_data.start_time_code,
                timeline_data.end_time_code,
                timeline_data.frames_per_second
            );
        }
    }

    // Mesh deduplication: (vertex_count, index_count, first_vertex_hash) -> proto_id
    // This handles referenced meshes that appear multiple times with different transforms
    let mut mesh_dedup: HashMap<(usize, usize, u64), usize> = HashMap::new();

    // Precompute inverse bind matrices per skeleton so per-mesh skin population
    // only needs a cheap HashMap lookup. `bind_transforms` are world-space at bind
    // time; inverting once at load amortizes the cost across every subsequent
    // skinning eval. Skipped silently when there are no skeletons on the stage.
    let skel_inv_binds: HashMap<String, Vec<Mat4>> = {
        let mut map = HashMap::new();
        if let Ok(count) = stage.skeleton_count() {
            for i in 0..count {
                if let Ok(skel) = stage.get_skeleton(i) {
                    let inv: Vec<Mat4> = skel.bind_transforms.iter().map(|m| m.inverse()).collect();
                    map.insert(skel.path, inv);
                }
            }
        }
        map
    };

    // Load all meshes as prototypes (with deduplication).
    // All purposes are loaded; filtering happens at viewport culling time.
    // Visibility filtering ALSO happens later via hidden_prim_paths +
    // reload_instance_visibility, NOT at load time — skipping invisible
    // meshes here would mean the prototype is never created, so the
    // eye-icon toggle has nothing to un-hide on reopen of a file with a
    // persisted `visibility = "invisible"` opinion.
    let mesh_start = Instant::now();
    let meshes = stage.meshes()?;
    // Stopgap visibility-aware memory report — v0.16.2 stopped skipping
    // invisible meshes at load, so hidden geometry now lives in CPU
    // prototype arrays. Surface the cost so it's visible until v0.18
    // lands deferred GPU upload.
    {
        let mut invisible_count = 0usize;
        let mut invisible_bytes = 0usize;
        for m in meshes.iter() {
            if !m.visible {
                invisible_count += 1;
                invisible_bytes += m.vertices.len() * std::mem::size_of::<bif_math::Vec3>();
                invisible_bytes += m.indices.len() * std::mem::size_of::<u32>();
                if let Some(n) = &m.normals {
                    invisible_bytes += n.len() * std::mem::size_of::<bif_math::Vec3>();
                }
                if let Some(uvs) = &m.uvs {
                    invisible_bytes += uvs.len() * std::mem::size_of::<[f32; 2]>();
                }
            }
        }
        if invisible_count > 0 {
            log::info!(
                "Invisible prototypes loaded: {} meshes (~{:.1} MB CPU). \
                 Deferred GPU upload is v0.18 work.",
                invisible_count,
                invisible_bytes as f64 / (1024.0 * 1024.0),
            );
        }
    }
    for (mesh_idx, mesh_data) in meshes.iter().enumerate() {
        // Hash from references — no clone until we know it's unique
        let vertex_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            mesh_data.vertices.len().hash(&mut hasher);
            mesh_data.indices.len().hash(&mut hasher);
            let vlen = mesh_data.vertices.len();
            let sample_count = vlen.min(50);
            for i in 0..sample_count {
                let idx = if sample_count <= 1 {
                    0
                } else {
                    i * (vlen - 1) / (sample_count - 1)
                };
                if let Some(v) = mesh_data.vertices.get(idx) {
                    v.x.to_bits().hash(&mut hasher);
                    v.y.to_bits().hash(&mut hasher);
                    v.z.to_bits().hash(&mut hasher);
                }
            }
            let ilen = mesh_data.indices.len();
            let idx_sample_count = ilen.min(50);
            for i in 0..idx_sample_count {
                let idx = if idx_sample_count <= 1 {
                    0
                } else {
                    i * (ilen - 1) / (idx_sample_count - 1)
                };
                if let Some(&val) = mesh_data.indices.get(idx) {
                    val.hash(&mut hasher);
                }
            }
            // Hash sampled normals for stronger dedup
            if let Some(normals) = mesh_data.normals.as_ref().filter(|n| !n.is_empty()) {
                let nlen = normals.len();
                let n_samples = nlen.min(20);
                for i in 0..n_samples {
                    let idx = if n_samples <= 1 {
                        0
                    } else {
                        i * (nlen - 1) / (n_samples - 1)
                    };
                    if let Some(n) = normals.get(idx) {
                        n.x.to_bits().hash(&mut hasher);
                        n.y.to_bits().hash(&mut hasher);
                        n.z.to_bits().hash(&mut hasher);
                    }
                }
            }
            hasher.finish()
        };
        let dedup_key = (
            mesh_data.vertices.len(),
            mesh_data.indices.len(),
            vertex_hash,
        );

        let proto_id = if let Some(&existing_id) = mesh_dedup.get(&dedup_key) {
            // Mesh already exists, reuse prototype
            existing_id
        } else {
            // New unique mesh — clone data only for unique meshes
            let mut mesh = Mesh::new_with_materials(
                mesh_data.vertices.clone(),
                mesh_data.indices.clone(),
                mesh_data.normals.clone(),
                mesh_data.uvs.clone(),
                mesh_data.face_material_ids.clone(),
            );
            mesh.ensure_normals();

            // Store subdivision data for Embree subd geometry
            mesh.subdivision_scheme = mesh_data.subdivision_scheme;
            mesh.face_vertex_counts = mesh_data.face_vertex_counts.clone();
            mesh.polygon_indices = mesh_data.face_vertex_indices.clone();
            mesh.crease_indices = mesh_data.crease_indices.clone();
            mesh.crease_lengths = mesh_data.crease_lengths.clone();
            mesh.crease_sharpnesses = mesh_data.crease_sharpnesses.clone();
            mesh.vertices_orig = mesh_data.vertices_orig.clone();
            mesh.facevarying_uvs = mesh_data.facevarying_uvs.clone();
            mesh.facevarying_uv_indices = mesh_data.facevarying_uv_indices.clone();
            mesh.display_color = mesh_data.display_color.clone();

            // UsdSkel binding (v0.13.5 Phase 1). Silently no-op for non-skinned
            // meshes: the C++ bridge returns INVALID_PRIM, which maps to Err here.
            // Snapshots positions into `bind_positions` before any skinning pass
            // mutates them.
            if let Ok(skin_data) = stage.get_skin_binding(mesh_idx) {
                if let Some(inv_binds) = skel_inv_binds.get(&skin_data.skeleton_path) {
                    // Pull log values from skin_data BEFORE the move into SkinBinding
                    // so we don't have to .unwrap() the Option<SkinBinding> twice.
                    let log_skel_path = skin_data.skeleton_path.clone();
                    let log_element_size = skin_data.element_size;
                    let log_kind = if skin_data.is_rigid {
                        "rigid"
                    } else {
                        "per-vertex"
                    };

                    // Build the variant based on the C++ bridge's `is_rigid` flag.
                    //
                    // `SkinKind::Rigid` is a compact encoding for the common case:
                    // accessory mesh (eye, shoe, button) bound to a SINGLE joint
                    // with weight 1.0. It stores one `(joint_idx, weight)` pair
                    // and the skinning inner loop becomes one matrix multiply per
                    // vertex — saves hundreds of KB per mesh vs broadcasting the
                    // influence block across every post-split vertex.
                    //
                    // However, USD's `UsdSkelSkinningQuery::IsRigidlyDeformed()`
                    // is broader than "single joint": it returns true for any
                    // mesh whose binding is per-prim rather than per-vertex. That
                    // includes multi-bone uniform bindings like hair (3 head/neck
                    // bones weighted 1/3 each, same for every vertex) and
                    // fingernails (2 finger-tip bones weighted 1/2 each). For
                    // those we must fall back to the per-vertex layout — taking
                    // only `joint_indices[0]` + `joint_weights[0]` would keep the
                    // first bone at fractional weight and collapse the mesh
                    // toward that bone's origin (v0.13.5.2 bug: hair on
                    // HumanFemale.walk.usd rendered at 1/3 of its correct
                    // position because weight 0.333 scaled every vertex).
                    let is_rigid_compact = skin_data.is_rigid && skin_data.element_size == 1;
                    let kind = if is_rigid_compact {
                        let first_raw = skin_data.joint_indices.first().copied();
                        let joint_idx = first_raw
                            .filter(|&i| i >= 0)
                            .map(|i| i as u32)
                            .unwrap_or(u32::MAX);
                        let weight = skin_data.joint_weights.first().copied().unwrap_or(1.0);
                        SkinKind::Rigid { joint_idx, weight }
                    } else if skin_data.is_rigid {
                        // Multi-joint rigid: C++ bridge stored a single authored
                        // block of `element_size` entries. Broadcast here to
                        // match the per-vertex layout the skinning kernel
                        // expects — one block per post-split vertex.
                        let elem_size = skin_data.element_size;
                        let n_verts = mesh.positions.len();
                        let src_ji = &skin_data.joint_indices;
                        let src_jw = &skin_data.joint_weights;
                        let mut joint_indices = Vec::with_capacity(n_verts * elem_size);
                        let mut joint_weights = Vec::with_capacity(n_verts * elem_size);
                        for _ in 0..n_verts {
                            for k in 0..elem_size {
                                let i = src_ji.get(k).copied().unwrap_or(-1);
                                let w = src_jw.get(k).copied().unwrap_or(0.0);
                                joint_indices.push(if i < 0 { u32::MAX } else { i as u32 });
                                joint_weights.push(w);
                            }
                        }
                        SkinKind::PerVertex {
                            joint_indices,
                            joint_weights,
                            element_size: elem_size,
                        }
                    } else {
                        // Per-vertex binding: C++ already broadcast to
                        // `post_vert * element_size` entries. -1 sentinels mean
                        // "no skeleton mapping" — convert to u32::MAX so the
                        // skinning kernel's bounds check drops the influence
                        // instead of silently pulling joint 0.
                        SkinKind::PerVertex {
                            joint_indices: skin_data
                                .joint_indices
                                .iter()
                                .map(|&i| if i < 0 { u32::MAX } else { i as u32 })
                                .collect(),
                            joint_weights: skin_data.joint_weights,
                            element_size: skin_data.element_size,
                        }
                    };

                    mesh.bind_positions = Some(mesh.positions.clone());
                    mesh.skin = Some(SkinBinding {
                        skeleton_path: skin_data.skeleton_path,
                        kind,
                        geom_bind_transform: skin_data.geom_bind_transform,
                        inv_bind_matrices: inv_binds.clone(),
                    });
                    log::debug!(
                        "Mesh {} bound to skeleton {} ({log_kind}, {} influences/vertex)",
                        mesh_data.path,
                        log_skel_path,
                        log_element_size
                    );
                } else {
                    log::warn!(
                        "Mesh {} references skeleton {} but no matching skeleton cached",
                        mesh_data.path,
                        skin_data.skeleton_path
                    );
                }
            }

            // UsdSkel blend shapes (v0.13.6). Walk the bridge's blend shape
            // bindings and attach matching ones to this mesh. Populates
            // `mesh.blend_shapes`, snapshots `bind_normals`, and ensures
            // `bind_positions` exists even for shapes-only meshes (no skin).
            {
                let bs_count = stage.blend_shape_binding_count().unwrap_or(0);
                for bs_idx in 0..bs_count {
                    let Ok(bs_data) = stage.get_blend_shape_binding(bs_idx) else {
                        continue;
                    };
                    if bs_data.mesh_path != mesh_data.path {
                        continue;
                    }
                    if bs_data.targets.is_empty() {
                        continue;
                    }

                    // Convert UsdBlendShapeTarget → mesh::BlendShapeTarget
                    let targets: Vec<BlendShapeTarget> = bs_data
                        .targets
                        .into_iter()
                        .map(|t| BlendShapeTarget {
                            name: t.name,
                            offsets: t.offsets,
                            normal_offsets: t.normal_offsets,
                        })
                        .collect();

                    // Ensure bind_positions exists (may already be set by skin path)
                    if mesh.bind_positions.is_none() {
                        mesh.bind_positions = Some(mesh.positions.clone());
                    }
                    // Snapshot bind normals for blend shape normal deltas
                    if mesh.bind_normals.is_none() {
                        mesh.bind_normals = mesh.normals.clone();
                    }

                    let target_count = targets.len();
                    mesh.blend_shapes = Some(BlendShapeBinding {
                        mesh_path: bs_data.mesh_path,
                        targets,
                        ffi_binding_idx: bs_idx as u32,
                    });

                    log::info!(
                        "Mesh {} blend shapes: {} targets",
                        mesh_data.path,
                        target_count
                    );
                    break; // one binding per mesh
                }
            }

            let mesh_arc = Arc::new(mesh);
            let proto_id = scene.add_prototype(mesh_arc, mesh_data.path.clone());
            mesh_dedup.insert(dedup_key, proto_id);
            prototype_map.insert(mesh_data.path.clone(), proto_id);
            // Map parent Xform path → same proto_id (common PointInstancer pattern:
            // prototype targets are Xforms wrapping a single child mesh).
            // NOTE: multi-mesh prototypes (Xform with >1 child mesh) will only
            // map to the first child — compound prototypes not yet supported.
            if let Some(parent_end) = mesh_data.path.rfind('/') {
                let parent_path = &mesh_data.path[..parent_end];
                if !parent_path.is_empty() {
                    if let Some(&existing_id) = prototype_map.get(parent_path) {
                        if existing_id != proto_id {
                            log::warn!(
                                "Multi-mesh prototype: '{}' already mapped to proto {}, \
                                 skipping proto {} from '{}'",
                                parent_path,
                                existing_id,
                                proto_id,
                                mesh_data.path
                            );
                        }
                    } else {
                        prototype_map.insert(parent_path.to_string(), proto_id);
                    }
                }
            }
            proto_id
        };

        // Add an instance with this mesh's world transform.
        //
        // For skinned meshes (v0.13.5), override with the SkelRoot's world
        // transform. The skinning math produces vertices in skel-local space,
        // so the instance matrix needs to re-anchor them at the SkelRoot, not
        // at the individual mesh prim. Without this, sub-Xform offsets inside
        // the SkelRoot (e.g. buttons translated to the chest) get applied
        // twice — once via geom_bind_transform during skinning, once via the
        // mesh's own world matrix — and the mesh ends up at 2x the offset.
        //
        // We also track `is_skinned` so we can SKIP per-frame transform
        // keyframe animation for the instance: skinned meshes get all their
        // per-frame motion from the joint deformation pass, and applying the
        // mesh prim's own animated xform on top would re-introduce the same
        // double-application that the static override is fixing.
        let (transform, is_skinned) = if let Ok(skin_data) = stage.get_skin_binding(mesh_idx) {
            if skel_inv_binds.contains_key(&skin_data.skeleton_path) {
                (
                    Transform::from_matrix(skin_data.skel_root_world_xform),
                    true,
                )
            } else {
                (Transform::from_matrix(mesh_data.transform), false)
            }
        } else {
            (Transform::from_matrix(mesh_data.transform), false)
        };

        // Check for animation data. Skinned meshes deliberately skip the
        // per-frame keyframe path — see the rationale on the transform block
        // above.
        let animation = if !is_skinned && scene.timeline.is_some() {
            match stage.get_mesh_animation(mesh_idx) {
                Ok(anim_data) => {
                    log::debug!(
                        "Mesh {} (idx {}): {} xform samples",
                        mesh_data.path,
                        mesh_idx,
                        anim_data.xform_samples.len()
                    );
                    if anim_data.xform_samples.is_empty() {
                        None
                    } else {
                        let keyframes: Vec<TransformKeyframe> = anim_data
                            .xform_samples
                            .iter()
                            .map(|sample| TransformKeyframe {
                                time: sample.time,
                                transform: Transform::from_matrix(sample.transform),
                            })
                            .collect();
                        log::debug!(
                            "Mesh {} has {} animation keyframes",
                            mesh_data.path,
                            keyframes.len()
                        );
                        Some(AnimatedTransform::with_keyframes(transform, keyframes))
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to get animation for mesh {}: {:?}",
                        mesh_data.path,
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        let inst_idx = if let Some(anim) = animation {
            scene.add_animated_instance_with_path(proto_id, transform, anim, mesh_data.path.clone())
        } else {
            scene.add_instance_with_path(proto_id, transform, mesh_data.path.clone())
        };

        scene.set_instance_purpose(inst_idx, mesh_data.purpose.into());
    }
    let mesh_time = mesh_start.elapsed();
    let total_verts: usize = meshes.iter().map(|m| m.vertices.len()).sum();

    // Fetch native instances early (processed after material binding below)
    let native_instances = stage.native_instances().unwrap_or_default();

    // Load materials
    let material_start = Instant::now();
    let usd_materials = stage.materials().unwrap_or_default();
    let mut material_map: IndexMap<String, usize> = IndexMap::new();
    let mut materialx_count = 0;

    for mat_data in &usd_materials {
        if mat_data.is_materialx {
            materialx_count += 1;
            log::debug!(
                "MaterialX material: {} (base_color={:?}, metalness={:.2}, roughness={:.2})",
                mat_data.path,
                mat_data.base_color,
                mat_data.base_metalness,
                mat_data.specular_roughness
            );
        }
        let material = crate::scene::Material {
            name: mat_data.path.clone().into(),
            base_color: mat_data.base_color,
            base_metalness: mat_data.base_metalness,
            specular_roughness: mat_data.specular_roughness,
            emission_color: mat_data.emission_color,
            emission_luminance: 0.0,
            transmission_weight: mat_data.transmission_weight,
            geometry_opacity: mat_data.geometry_opacity,
            specular_weight: mat_data.specular_weight,
            specular_ior: mat_data.specular_ior,
            base_color_texture: mat_data.base_color_texture.as_deref().map(Arc::from),
            specular_roughness_texture: mat_data
                .specular_roughness_texture
                .as_deref()
                .map(Arc::from),
            base_metalness_texture: mat_data.base_metalness_texture.as_deref().map(Arc::from),
            normal_texture: mat_data.normal_texture.as_deref().map(Arc::from),
            emission_texture: mat_data.emission_texture.as_deref().map(Arc::from),
            geometry_opacity_texture: mat_data.geometry_opacity_texture.as_deref().map(Arc::from),
            displacement_texture: mat_data.displacement_texture.as_deref().map(Arc::from),
            displacement_scale: mat_data.displacement_scale,
            source_dir: path.parent().map(|p| p.to_path_buf()),
            double_sided: false,
        };
        let mat_id = scene.add_material(material);
        material_map.insert(mat_data.path.clone(), mat_id);
    }

    if materialx_count > 0 {
        log::info!("Loaded {} MaterialX materials", materialx_count);
    }

    // Bind materials to prototypes via mesh material paths
    let mut bound_count = 0usize;
    for (mesh_idx, mesh_data) in meshes.iter().enumerate() {
        if let Ok(Some(mat_path)) = stage.get_mesh_material_path(mesh_idx) {
            if let Some(&mat_id) = material_map.get(&mat_path) {
                if let Some(&proto_id) = prototype_map.get(&mesh_data.path) {
                    if let Some(proto) = scene.prototypes.get(proto_id) {
                        let mut updated_proto: crate::scene::Prototype = (**proto).clone();
                        let mat = &scene.materials[mat_id];
                        updated_proto.material = Some(mat.clone());
                        scene.prototypes[proto_id] = Arc::new(updated_proto);
                        bound_count += 1;
                    }
                }
            }
        }
    }
    log::info!(
        "Bound {} / {} meshes to materials",
        bound_count,
        meshes.len()
    );

    // Build bridge-material-index → scene-material-index lookup
    let bridge_mat_to_scene: Vec<Option<usize>> = usd_materials
        .iter()
        .map(|m| material_map.get(&m.path).copied())
        .collect();

    // Process native instances now that materials are bound
    let native_start = Instant::now();
    let mut override_proto_cache: HashMap<(usize, usize), usize> = HashMap::new();

    for native_inst in &native_instances {
        if let Some(mesh_data) = meshes.get(native_inst.proto_mesh_idx) {
            if let Some(&proto_id) = prototype_map.get(&mesh_data.path) {
                let transform = Transform::from_matrix(native_inst.transform);
                let prim_path = format!("{}/native_{}", mesh_data.path, scene.instance_count());

                let target_proto = if native_inst.material_override_idx >= 0 {
                    let bridge_idx = native_inst.material_override_idx as usize;
                    if let Some(Some(scene_mat_idx)) = bridge_mat_to_scene.get(bridge_idx) {
                        let override_mat = &scene.materials[*scene_mat_idx];
                        let needs_override = match &scene.prototypes[proto_id].material {
                            Some(proto_mat) => !Arc::ptr_eq(proto_mat, override_mat),
                            None => true,
                        };
                        if needs_override {
                            let key = (proto_id, bridge_idx);
                            if let Some(&cached) = override_proto_cache.get(&key) {
                                cached
                            } else {
                                let mut cloned = (*scene.prototypes[proto_id]).clone();
                                cloned.material = Some(override_mat.clone());
                                cloned.name = format!(
                                    "{}/mat_{}",
                                    cloned.name, scene.materials[*scene_mat_idx].name
                                )
                                .into();
                                let new_id = scene.prototypes.len();
                                cloned.id = new_id;
                                scene.prototypes.push(Arc::new(cloned));
                                override_proto_cache.insert(key, new_id);
                                new_id
                            }
                        } else {
                            proto_id
                        }
                    } else {
                        proto_id
                    }
                } else {
                    proto_id
                };

                let inst_idx = scene.add_instance_with_path(target_proto, transform, prim_path);
                scene.set_instance_purpose(inst_idx, native_inst.purpose.into());
            }
        }
    }
    let native_time = native_start.elapsed();
    if !native_instances.is_empty() {
        log::info!(
            "Native instances: {} ({} overrides, {:.1}ms)",
            native_instances.len(),
            override_proto_cache.len(),
            native_time.as_secs_f64() * 1000.0
        );
    }

    let material_time = material_start.elapsed();

    // Free bulk mesh geometry from C++ cache — Rust now owns all mesh data.
    // Keeps vertices/indices/paths for animation queries via USD stage.
    stage.free_mesh_geometry_cache();

    // Load lights (UsdLux)
    let light_start = Instant::now();
    let usd_lights = stage.lights().unwrap_or_default();
    for light_data in &usd_lights {
        // Extract direction/position from transform
        // For directional lights, -Z axis is the light direction
        // For point lights, the translation is the position
        let transform = light_data.transform;

        let light = match light_data.light_type {
            UsdLightType::Distant => {
                // Direction is -Z axis of the transform (forward direction)
                let direction = -bif_math::Vec3::new(
                    transform.col(2).x,
                    transform.col(2).y,
                    transform.col(2).z,
                )
                .normalize();
                Light::Distant {
                    direction,
                    color: light_data.color,
                    intensity: light_data.intensity,
                    angle: light_data.angle,
                }
            }
            UsdLightType::Sphere => {
                // Position is the translation component
                let position =
                    bif_math::Vec3::new(transform.col(3).x, transform.col(3).y, transform.col(3).z);
                Light::Point {
                    position,
                    color: light_data.color,
                    intensity: light_data.intensity,
                    radius: light_data.radius,
                }
            }
            UsdLightType::Rect => Light::Rect {
                transform,
                color: light_data.color,
                intensity: light_data.intensity,
                width: light_data.width,
                height: light_data.height,
            },
            UsdLightType::Dome => {
                // Extract Y-axis rotation from transform's Z basis vector
                let z_basis = bif_math::Vec3::new(transform.col(2).x, 0.0, transform.col(2).z);
                let rotation = z_basis.x.atan2(z_basis.z); // radians
                Light::Dome {
                    rotation,
                    intensity: light_data.intensity,
                    texture_path: light_data.texture_path.as_deref().map(Arc::from),
                }
            }
            UsdLightType::Cylinder => {
                // Map cylinder to point light (approximate — BIF doesn't have cylinder light yet)
                let position =
                    bif_math::Vec3::new(transform.col(3).x, transform.col(3).y, transform.col(3).z);
                Light::Point {
                    position,
                    color: light_data.color,
                    intensity: light_data.intensity,
                    radius: light_data.radius,
                }
            }
            UsdLightType::Disk => {
                // Map disk to rect light (approximate — BIF doesn't have disk light yet)
                let diameter = light_data.radius * 2.0;
                Light::Rect {
                    transform,
                    color: light_data.color,
                    intensity: light_data.intensity,
                    width: diameter,
                    height: diameter,
                }
            }
        };

        scene.lights.push(light);
        log::debug!(
            "Loaded light: {} ({:?}, intensity={})",
            light_data.path,
            light_data.light_type,
            light_data.intensity
        );
    }
    let light_time = light_start.elapsed();

    if !usd_lights.is_empty() {
        log::info!("Loaded {} lights", usd_lights.len());
    }

    // Load curves (UsdGeomBasisCurves)
    let curves_data = stage.curves().unwrap_or_default();
    for curve_data in &curves_data {
        scene.curves.push(CurvesPrim {
            path: curve_data.path.clone(),
            points: curve_data.points.clone(),
            widths: curve_data.widths.clone(),
            curve_vertex_counts: curve_data.curve_vertex_counts.clone(),
            curve_type: curve_data.curve_type,
            basis: curve_data.basis,
            wrap: curve_data.wrap,
            transform: curve_data.transform,
            // TODO: read purpose from C++ bridge (needs curves purpose in FFI)
            purpose: Purpose::Default,
        });
    }
    if !curves_data.is_empty() {
        log::info!("Loaded {} curves prims", curves_data.len());
    }

    // Load points (UsdGeomPoints)
    let points_data = stage.points().unwrap_or_default();
    for pt_data in &points_data {
        scene.points_prims.push(PointsPrim {
            path: pt_data.path.clone(),
            positions: pt_data.positions.clone(),
            widths: pt_data.widths.clone(),
            normals: pt_data.normals.clone(),
            ids: pt_data.ids.clone(),
            transform: pt_data.transform,
            // TODO: read purpose from C++ bridge (needs points purpose in FFI)
            purpose: Purpose::Default,
        });
    }
    if !points_data.is_empty() {
        log::info!("Loaded {} points prims", points_data.len());
    }

    log::info!(
        "Loaded {} unique prototypes from {} meshes, {} materials",
        scene.prototype_count(),
        meshes.len(),
        scene.material_count()
    );

    // Load point instancers as PointClouds
    let instancer_start = Instant::now();
    let instancers = stage.instancers()?;
    for (instancer_idx, instancer_data) in instancers.iter().enumerate() {
        // Resolve prototypes
        let proto_ids: Vec<usize> = instancer_data
            .prototype_paths
            .iter()
            .filter_map(|path| prototype_map.get(path).copied())
            .collect();

        if proto_ids.is_empty() {
            log::warn!(
                "Instancer {} has no resolvable prototypes",
                instancer_data.path
            );
            continue;
        }

        // Extract positions from instancer transforms
        let positions: Vec<bif_math::Vec3> = instancer_data
            .transforms
            .iter()
            .map(|mat| bif_math::Vec3::new(mat.col(3).x, mat.col(3).y, mat.col(3).z))
            .collect();

        // Extract per-point scale and orientation from transforms
        let mut scales = Vec::with_capacity(positions.len());
        let mut orientations = Vec::with_capacity(positions.len());
        for mat in &instancer_data.transforms {
            let (s, r, _) = mat.to_scale_rotation_translation();
            scales.push(s);
            orientations.push(r);
        }

        let proto_indices: Vec<u32> = instancer_data
            .proto_indices
            .iter()
            .take(positions.len())
            .map(|&idx| idx as u32)
            .collect();

        let cloud = PointCloud {
            id: scene.point_clouds.len(),
            name: instancer_data.path.clone(),
            positions,
            attributes: PointAttributes {
                scales: Some(scales),
                orientations: Some(orientations),
                proto_indices,
                ids: None,
            },
            prototype_ids: proto_ids,
            transform: Transform::default(),
            distribution: DistributionMethod::UsdPointInstancer {
                path: instancer_data.path.clone(),
            },
            invisible_ids: instancer_data.invisible_ids.clone(),
        };

        log::info!(
            "PointInstancer '{}': {} points, {} protos -> PointCloud",
            instancer_data.path,
            cloud.point_count(),
            cloud.prototype_ids.len()
        );

        // Expand immediately so instances appear (animation handled below)
        let expanded = cloud.expand();

        // Get animation data for instancer if timeline exists. We swallow the
        // error so the rest of the stage still loads — but log it so the user
        // can see when AllocTooLarge skipped a giant instancer animation
        // (see ffi_guard::MAX_ALLOC_BYTES).
        let instancer_anim = if scene.timeline.is_some() {
            match stage.get_instancer_animation(instancer_idx) {
                Ok(anim) => Some(anim),
                Err(err) => {
                    log::warn!("skipping animation for instancer {instancer_idx}: {err}");
                    None
                }
            }
        } else {
            None
        };

        // Build set of invisible instance IDs for fast lookup
        let invisible_set: std::collections::HashSet<i64> =
            instancer_data.invisible_ids.iter().copied().collect();

        for (i, inst) in expanded.into_iter().enumerate() {
            // Skip invisible instances (from UsdGeomPointInstancer::GetInvisibleIdsAttr)
            if invisible_set.contains(&(i as i64)) {
                continue;
            }

            // Build animation for this instance if available
            let animation = instancer_anim.as_ref().and_then(|anim| {
                if anim.time_samples.is_empty() || i >= anim.instance_count {
                    return None;
                }

                let keyframes: Vec<TransformKeyframe> = anim
                    .time_samples
                    .iter()
                    .enumerate()
                    .filter_map(|(time_idx, &time)| {
                        anim.transforms.get(time_idx).and_then(|instances| {
                            instances.get(i).map(|mat| TransformKeyframe {
                                time,
                                transform: Transform::from_matrix(*mat),
                            })
                        })
                    })
                    .collect();

                if keyframes.is_empty() {
                    None
                } else {
                    Some(AnimatedTransform::with_keyframes(inst.transform, keyframes))
                }
            });

            let prim_path = format!("{}/i{}", instancer_data.path, i);
            if let Some(anim) = animation {
                scene.add_animated_instance_with_path(
                    inst.prototype_id,
                    inst.transform,
                    anim,
                    prim_path,
                );
            } else {
                scene.add_instance_with_path(inst.prototype_id, inst.transform, prim_path);
            }
        }

        scene.add_point_cloud(cloud);
    }
    let instancer_time = instancer_start.elapsed();

    // Empty scenes are legitimate when the caller supplied a mute set
    // that strips the def-providing layer — composition resolves to
    // zero prims. `load_usd_with_stage` (strict variant) re-adds the
    // `NoGeometry` check at its own layer; callers that tolerate empty
    // (mute-driven reloads) use this permissive path directly.

    // Log timing breakdown
    let total_time = load_start.elapsed();
    let instance_count: usize = instancers.iter().map(|i| i.transforms.len()).sum();
    log::info!(
        "USD Load: {} ({} meshes, {} verts, {} materials, {} instances)",
        path.display(),
        meshes.len(),
        format_number(total_verts),
        scene.material_count(),
        format_number(instance_count)
    );
    log::info!("  Stage open: {:>7.1}ms", stage_time.as_secs_f64() * 1000.0);
    log::info!("  Meshes:     {:>7.1}ms", mesh_time.as_secs_f64() * 1000.0);
    log::info!(
        "  Materials:  {:>7.1}ms",
        material_time.as_secs_f64() * 1000.0
    );
    log::info!(
        "  Lights:     {:>7.1}ms ({} lights)",
        light_time.as_secs_f64() * 1000.0,
        scene.lights.len()
    );
    log::info!(
        "  Instancers: {:>7.1}ms",
        instancer_time.as_secs_f64() * 1000.0
    );
    log::info!("  Total:      {:>7.1}ms", total_time.as_secs_f64() * 1000.0);

    // Apply CPU vertex displacement (UsdPreviewSurface `displacement` input).
    // Modifies meshes in place via Arc::make_mut — both viewport and Ivar see
    // the displaced positions automatically. Skipped silently for meshes
    // without a displacement texture.
    let displaced = crate::usd::displacement::apply_displacement_to_scene(&mut scene);
    if displaced > 0 {
        log::info!("  Displaced:  {} mesh(es)", displaced);
    }

    Ok((scene, stage))
}

/// Load a USDA file via the C++ bridge.
///
/// This is an alias for `load_usd()` — kept for backwards compatibility.
pub fn load_usda<P: AsRef<Path>>(path: P) -> LoadResult<Scene> {
    load_usd(path)
}

/// Load USDA from a string (writes to temp file, loads via C++ bridge).
///
/// Useful for testing. Adds `#usda 1.0` header if missing.
pub fn load_usda_from_string(
    content: &str,
    name: &str,
    _base_dir: Option<PathBuf>,
) -> LoadResult<Scene> {
    let content = if content.trim_start().starts_with("#usda") {
        content.to_string()
    } else {
        format!("#usda 1.0\n(\n)\n\n{}", content)
    };
    let temp_path = std::env::temp_dir().join(format!("bif_usda_{}.usda", name));
    std::fs::write(&temp_path, &content)?;
    let result = load_usd(&temp_path);
    let _ = std::fs::remove_file(&temp_path);
    result
}

// SceneBuilder removed — all loading goes through C++ bridge.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_simple_mesh() {
        let usda = r#"
def Mesh "Triangle" {
    point3f[] points = [(0, 0, 0), (1, 0, 0), (0.5, 1, 0)]
    int[] faceVertexCounts = [3]
    int[] faceVertexIndices = [0, 1, 2]
}
"#;

        let scene = match load_usda_from_string(usda, "test_simple", None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        assert_eq!(scene.prototype_count(), 1);
        assert_eq!(scene.instance_count(), 1);
    }

    #[test]
    fn test_load_mesh_with_normals() {
        let usda = r#"
def Mesh "Triangle" {
    point3f[] points = [(0, 0, 0), (1, 0, 0), (0.5, 1, 0)]
    normal3f[] normals = [(0, 0, 1), (0, 0, 1), (0, 0, 1)]
    int[] faceVertexCounts = [3]
    int[] faceVertexIndices = [0, 1, 2]
}
"#;

        let scene = match load_usda_from_string(usda, "test_normals", None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        assert!(scene.prototypes[0].mesh.has_normals());
    }

    #[test]
    fn test_load_point_instancer() {
        let usda = r#"
def PointInstancer "Grid" {
    int[] protoIndices = [0, 0, 0, 0]
    point3f[] positions = [(0, 0, 0), (2, 0, 0), (0, 0, 2), (2, 0, 2)]

    def Mesh "Proto" {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0.5, 1, 0)]
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
    }
}
"#;

        let scene = match load_usda_from_string(usda, "test_instancer", None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        assert!(scene.prototype_count() >= 1);
        assert!(scene.instance_count() >= 1);
    }

    #[test]
    fn test_load_transformed_mesh() {
        let usda = r#"
def Xform "World" {
    double3 xformOp:translate = (10, 0, 0)
    uniform token[] xformOpOrder = ["xformOp:translate"]

    def Mesh "Cube" {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
        int[] faceVertexCounts = [4]
        int[] faceVertexIndices = [0, 1, 2, 3]
    }
}
"#;

        let scene = match load_usda_from_string(usda, "test_xform", None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - USD bridge unavailable: {e}");
                return;
            }
        };

        assert_eq!(scene.prototype_count(), 1);
        assert_eq!(scene.instance_count(), 1);
    }

    // ========================================================================
    // Integration tests for C++ bridge (require USD to be installed)
    // Run with: cargo test --package bif_core -- --ignored
    // ========================================================================

    /// Helper to get test asset path (works from any working directory)
    fn test_asset_path(relative: &str) -> std::path::PathBuf {
        // Get the crate root via CARGO_MANIFEST_DIR or use relative path
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
        let crate_root = std::path::Path::new(&manifest_dir);
        // Go up to workspace root
        let workspace_root = crate_root.parent().unwrap().parent().unwrap();
        workspace_root.join(relative)
    }

    #[test]
    #[ignore = "requires USD C++ library installed"]
    fn test_load_usd_cube() {
        // Load cube.usda via C++ bridge
        let path = test_asset_path("assets/ref_test/cube.usda");
        let scene = super::load_usd(&path).unwrap();

        assert_eq!(scene.prototype_count(), 1, "Should have 1 prototype (cube)");
        assert_eq!(scene.instance_count(), 1, "Should have 1 instance");

        // Cube has 6 faces × 2 triangles = 12 triangles
        assert_eq!(scene.total_triangle_count(), 12);
    }

    #[test]
    #[ignore = "requires USD C++ library installed"]
    fn test_load_usd_with_references() {
        // Load ref_test.usda which references cube.usda
        let path = test_asset_path("assets/ref_test/ref_test.usda");
        let scene = super::load_usd(&path).unwrap();

        // Should have resolved the reference and loaded 2 cube instances
        assert!(
            scene.prototype_count() >= 1,
            "Should have at least 1 prototype"
        );
        assert!(
            scene.instance_count() >= 2,
            "Should have at least 2 instances (2 referenced cubes)"
        );
    }

    // test_usda_and_cpp_bridge_produce_same_mesh removed — Rust parser eliminated,
    // both paths now use C++ bridge.

    /// v0.13.5 Phase 1: load the two_bone_arm fixture via the full loader path
    /// and verify skin data flows into Mesh.skin / Mesh.bind_positions.
    #[test]
    fn test_load_two_bone_arm_mesh_skin() {
        let path = "../../test_assets/skel/two_bone_arm.usda";
        let scene = match super::load_usd(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - could not load fixture: {e}");
                return;
            }
        };

        assert_eq!(scene.prototype_count(), 1, "expected 1 prototype");
        let proto = &scene.prototypes[0];
        let mesh = &proto.mesh;

        // Bind pose snapshot matches current positions (no skinning applied yet)
        let skin = mesh.skin.as_ref().expect("mesh should have a skin binding");
        let bind_positions = mesh
            .bind_positions
            .as_ref()
            .expect("mesh should have bind_positions snapshot");

        assert_eq!(bind_positions.len(), 8, "8 vertices in two-bone-arm box");
        assert_eq!(mesh.positions.len(), 8);
        // At load, positions must equal bind_positions (no deformation yet)
        for (p, bp) in mesh.positions.iter().zip(bind_positions.iter()) {
            assert!((*p - *bp).length() < 1e-5);
        }

        // SkinBinding sanity. The fixture has per-vertex influences (not rigid),
        // so we expect a `PerVertex` variant.
        assert_eq!(skin.skeleton_path, "/Root/Character/Skel");
        let crate::mesh::SkinKind::PerVertex {
            joint_indices,
            joint_weights,
            element_size,
        } = &skin.kind
        else {
            panic!(
                "expected PerVertex variant for two_bone_arm fixture, got {:?}",
                skin.kind
            );
        };
        assert_eq!(*element_size, 1);
        assert_eq!(joint_indices.len(), 8);
        assert_eq!(joint_weights.len(), 8);
        assert_eq!(&joint_indices[..4], &[0, 0, 0, 0]);
        assert_eq!(&joint_indices[4..], &[1, 1, 1, 1]);

        // Two joints → two inverse-bind matrices
        assert_eq!(skin.inv_bind_matrices.len(), 2);
        // Joint 0 inv-bind is identity (bind was identity)
        let inv0 = skin.inv_bind_matrices[0];
        assert!(
            (inv0 - bif_math::Mat4::IDENTITY)
                .to_cols_array()
                .iter()
                .all(|&x| x.abs() < 1e-5),
            "joint 0 inv-bind should be identity"
        );
        // Joint 1 inv-bind is translate(0, -1, 0) (bind translated +1 Y)
        let inv1 = skin.inv_bind_matrices[1];
        assert!(
            (inv1.w_axis.y - (-1.0)).abs() < 1e-5,
            "joint 1 inv-bind translation.y should be -1.0, got {}",
            inv1.w_axis.y
        );
    }
}
