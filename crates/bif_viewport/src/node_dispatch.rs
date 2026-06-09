//! Node graph event dispatch — handles all NodeGraphEvent variants.

use std::collections::{HashMap, HashSet};

use crate::ivar_state::RenderMode;
use crate::node_graph::{GraphNodeId, NodeGraphEvent, SceneNode};
use crate::Renderer;

impl Renderer {
    /// Handle a single node graph event (USD loading, render start, export, etc.).
    pub(crate) fn handle_node_graph_event(&mut self, event: NodeGraphEvent) {
        match event {
            NodeGraphEvent::LoadUsdFile { path, node_id } => {
                log::info!("Node graph: Loading USD file: {}", path);

                // Remove old prototypes from this node (reload case)
                let had_protos = self
                    .nodes
                    .node_outputs
                    .get(&node_id)
                    .is_some_and(|o| !o.proto_ids.is_empty());
                self.execute(crate::SceneCmd::RemoveNodeProtos { node: node_id });
                if had_protos {
                    // GC orphaned materials so new load starts with clean offsets
                    self.scene.working_scene.compact_materials();
                }

                self.nodes.materials_dirty = true;
                let proto_offset = self.scene.working_scene.prototype_count();
                match self.load_usd_scene(&path) {
                    Ok(()) => {
                        // Track which prototypes this UsdRead node owns
                        let new_proto_count = self.scene.working_scene.prototype_count();
                        let proto_ids: Vec<usize> = (proto_offset..new_proto_count).collect();
                        if !proto_ids.is_empty() {
                            log::info!("UsdRead {:?} owns protos {:?}", node_id, proto_ids);
                            self.execute(crate::SceneCmd::RecordProtos {
                                node: node_id,
                                proto_ids,
                            });
                        }
                        self.nodes.node_graph_state.mark_node_loaded(&path);
                        log::info!("USD file loaded successfully: {}", path);
                    }
                    Err(e) => {
                        log::error!("Failed to load USD file: {}", e);
                        self.nodes
                            .node_graph_state
                            .mark_node_error(&path, e.to_string());
                    }
                }
            }
            NodeGraphEvent::StartRender { spp } => {
                log::info!("Node graph: Starting render with {} SPP", spp);
                self.ivar.ivar_state.samples_per_pixel = spp;
                self.ivar.ivar_state.mode = RenderMode::Ivar;
                self.ivar.ivar_state.current_scale = 1;
                self.ivar.ivar_state.last_interaction_time = None;
                self.start_ivar_render();
            }
            NodeGraphEvent::ConvertTexturesToTx => {
                #[cfg(feature = "oiio")]
                {
                    let paths = self.collect_material_texture_paths();
                    if paths.is_empty() {
                        self.nodes
                            .node_graph_state
                            .mark_tx_conversion_complete("No textures".into());
                    } else {
                        log::info!("Converting {} textures to .tx", paths.len());
                        self.environment
                            .start_tx_conversion(paths, self.scene.texture_base_dir.clone());
                    }
                }
                #[cfg(not(feature = "oiio"))]
                {
                    self.nodes
                        .node_graph_state
                        .mark_tx_conversion_complete("OIIO not available".into());
                }
            }
            NodeGraphEvent::ClearTxCache => {
                #[cfg(feature = "oiio")]
                {
                    let paths = self.collect_material_texture_paths();
                    let cache = match self.scene.texture_base_dir.as_deref() {
                        Some(dir) => bif_core::texture::TextureCache::with_base_dir(dir),
                        None => bif_core::texture::TextureCache::new(),
                    };
                    let removed = cache.clear_tx_cache(&paths);
                    let status = format!("{} .tx files cleared", removed);
                    self.nodes
                        .node_graph_state
                        .mark_tx_conversion_complete(status);
                }
                #[cfg(not(feature = "oiio"))]
                {
                    self.nodes
                        .node_graph_state
                        .mark_tx_conversion_complete("OIIO not available".into());
                }
            }
            NodeGraphEvent::LoadHdri {
                path,
                rotation,
                intensity,
                show_background,
            } => {
                log::info!("Node graph: Loading HDRI (async): {}", path);
                self.nodes.node_graph_state.mark_hdri_loading(&path);
                self.environment.start_hdri_load(
                    std::path::Path::new(&path),
                    rotation,
                    intensity,
                    show_background,
                );
            }
            NodeGraphEvent::UpdateHdriParams {
                rotation,
                intensity,
                show_background,
            } => {
                let rotation_rad = rotation.to_radians();
                self.update_environment_params(intensity, rotation_rad, show_background);
                // Restart Ivar so CPU path tracer reflects updated params.
                // Throttled: slider drag fires every frame, avoid excessive cancel+spawn.
                if self.ivar.ivar_state.mode == RenderMode::Ivar
                    && self.ivar.ivar_state.world.is_some()
                    && self.ivar.ivar_state.should_restart()
                {
                    self.restart_ivar_at_scale(self.ivar.ivar_state.current_scale);
                }
            }
            NodeGraphEvent::CreatePrimitive {
                kind,
                size,
                node_id,
            } => {
                log::info!("Node graph: Creating {:?} primitive (size={})", kind, size);
                let snarl_id: egui_snarl::NodeId = node_id.into();

                // Read prim_path from the Primitive node for export naming
                let prim_path =
                    if let crate::node_graph::SceneNode::Primitive { ref prim_path, .. } =
                        &self.nodes.node_graph_state.snarl[snarl_id]
                    {
                        Some(prim_path.clone())
                    } else {
                        None
                    };

                // Remove old prototype if re-creating (e.g. size change)
                self.execute(crate::SceneCmd::RemoveNodeProtos { node: node_id });

                match self.load_primitive(kind, size) {
                    Ok(proto_id) => {
                        // Update prototype name from node's prim_path (used during export)
                        if let Some(ref pp) = prim_path {
                            if let Some(proto) =
                                self.scene.working_scene.prototypes.get_mut(proto_id)
                            {
                                std::sync::Arc::make_mut(proto).name = pp.as_str().into();
                            }
                        }
                        self.execute(crate::SceneCmd::RecordProtos {
                            node: node_id,
                            proto_ids: vec![proto_id],
                        });

                        // Recursively dirty all downstream nodes
                        crate::node_graph::propagate_dirty(
                            snarl_id,
                            &mut self.nodes.node_graph_state.snarl,
                        );

                        if let Err(e) = self.reload_working_scene() {
                            log::error!("Failed to reload after primitive create: {}", e);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to create primitive: {}", e);
                    }
                }
            }
            NodeGraphEvent::ScatterPointsCompute { node_id, params } => {
                log::info!(
                    "Node graph: Scatter Points {:?}, count={}, seed={}",
                    params.source,
                    params.count,
                    params.seed
                );
                let snarl_id: egui_snarl::NodeId = node_id.into();

                // Remove previous cloud for this node (if regenerating).
                // Also clears the scatter-surface mapping (rebuilt in reload).
                self.execute(crate::SceneCmd::RemoveNodeCloud { node: node_id });

                let cloud = match params.source {
                    bif_core::PointSource::Surface => {
                        let scatter_mesh_idx = params.target_proto_id.unwrap_or(0);
                        if let Some(proto) =
                            self.scene.working_scene.prototypes.get(scatter_mesh_idx)
                        {
                            let mesh = proto.mesh.clone();
                            let mesh_transform = self
                                .scene
                                .working_scene
                                .instances()
                                .iter()
                                .find(|i| i.prototype_id == scatter_mesh_idx)
                                .map(|i| i.model_matrix())
                                .unwrap_or(bif_math::Mat4::IDENTITY);

                            let config = bif_core::scatter::ScatterConfig {
                                count: (params.count).min(params.max_point_limit) as usize,
                                min_distance: params.min_distance,
                                seed: params.seed,
                                align_to_normal: params.align_to_normal,
                                scale_range: (params.scale_min, params.scale_max),
                                rotation_range: params.rotation_range.to_radians(),
                            };

                            let mut cloud = bif_core::scatter::scatter_on_surface(
                                &mesh,
                                &mesh_transform,
                                params.scatter_mode,
                                &config,
                                vec![scatter_mesh_idx],
                            );

                            if params.relax_iterations > 0 {
                                bif_core::scatter::repulsion_relax(
                                    &mut cloud.positions,
                                    params.relax_iterations,
                                    params.scale_radii,
                                    params.max_relax_radius,
                                    Some((&mesh, &mesh_transform)),
                                );
                            }

                            // Track surface proto for hiding (rebuilt in reload_working_scene)
                            self.nodes
                                .node_scatter_surface_map
                                .insert(node_id, scatter_mesh_idx);

                            Some(cloud)
                        } else {
                            log::warn!(
                                "Scatter: no mesh at proto_id {} to scatter on",
                                scatter_mesh_idx
                            );
                            None
                        }
                    }
                    bif_core::PointSource::Grid => {
                        let config = bif_core::scatter::ScatterConfig {
                            seed: params.seed,
                            scale_range: (params.scale_min, params.scale_max),
                            rotation_range: params.rotation_range.to_radians(),
                            ..Default::default()
                        };
                        let mut cloud = bif_core::scatter::generate_grid_points(
                            params.grid_size,
                            params.grid_spacing,
                            params.max_point_limit,
                            &config,
                        );
                        if params.relax_iterations > 0 {
                            bif_core::scatter::repulsion_relax(
                                &mut cloud.positions,
                                params.relax_iterations,
                                params.scale_radii,
                                params.max_relax_radius,
                                None,
                            );
                        }
                        Some(cloud)
                    }
                    bif_core::PointSource::Sphere => {
                        let config = bif_core::scatter::ScatterConfig {
                            seed: params.seed,
                            scale_range: (params.scale_min, params.scale_max),
                            rotation_range: params.rotation_range.to_radians(),
                            ..Default::default()
                        };
                        let mut cloud = bif_core::scatter::generate_sphere_points(
                            params.sphere_radius,
                            params.count,
                            params.sphere_on_surface,
                            params.max_point_limit,
                            &config,
                        );
                        if params.relax_iterations > 0 {
                            bif_core::scatter::repulsion_relax(
                                &mut cloud.positions,
                                params.relax_iterations,
                                params.scale_radii,
                                params.max_relax_radius,
                                None,
                            );
                        }
                        Some(cloud)
                    }
                };

                if let Some(cloud) = cloud {
                    let pt_count = cloud.positions.len();
                    self.execute(crate::SceneCmd::AddCloud {
                        node: node_id,
                        cloud: Box::new(cloud),
                    });
                    self.execute(crate::SceneCmd::UploadPointPreview);

                    // Auto-enable point preview and sync color/size from node
                    self.point_preview.visible = true;
                    if let crate::node_graph::SceneNode::ScatterPoints {
                        point_size,
                        point_color,
                        ..
                    } = &self.nodes.node_graph_state.snarl[snarl_id]
                    {
                        self.point_preview.point_size = *point_size;
                        self.point_preview.color = *point_color;
                        self.point_preview_params_dirty = true;
                    }

                    log::info!("Scatter Points complete: {} points", pt_count);

                    // Reload scene to hide scatter surface geometry
                    if let Err(e) = self.reload_working_scene() {
                        log::error!("Failed to reload after scatter: {}", e);
                    }

                    // Recursively dirty all downstream nodes
                    crate::node_graph::propagate_dirty(
                        snarl_id,
                        &mut self.nodes.node_graph_state.snarl,
                    );
                }
            }
            NodeGraphEvent::PointInstancerCompute {
                node_id,
                points_source_node,
                proto_source_node,
            } => {
                let snarl_id: egui_snarl::NodeId = node_id.into();
                // Resolve cloud ID from scatter node
                let cloud_id = self
                    .nodes
                    .node_outputs
                    .get(&points_source_node)
                    .and_then(|o| o.cloud_id);
                // Resolve first prototype ID from primitive/USD node
                let proto_id = self
                    .nodes
                    .node_outputs
                    .get(&proto_source_node)
                    .map(|o| &o.proto_ids)
                    .and_then(|ids| ids.first().copied());

                // Helper: mark compute failed on the node (prevents infinite retry)
                let mark_compute_failed =
                    |snarl: &mut egui_snarl::Snarl<crate::node_graph::SceneNode>,
                     nid: egui_snarl::NodeId| {
                        if let crate::node_graph::SceneNode::PointInstancer {
                            is_computing,
                            compute_failed,
                            ..
                        } = &mut snarl[nid]
                        {
                            *is_computing = false;
                            *compute_failed = true;
                        }
                    };

                // Read PointInstancer node's prim_path for export
                let instancer_prim_path =
                    if let crate::node_graph::SceneNode::PointInstancer { ref prim_path, .. } =
                        &self.nodes.node_graph_state.snarl[snarl_id]
                    {
                        Some(prim_path.clone())
                    } else {
                        None
                    };

                match (cloud_id, proto_id) {
                    (Some(cid), Some(pid)) => {
                        // Update cloud name from PointInstancer prim_path (used during export)
                        if let Some(ref prim_path) = instancer_prim_path {
                            if let Some(cloud) = self
                                .scene
                                .working_scene
                                .point_clouds
                                .iter_mut()
                                .find(|c| c.id == cid)
                            {
                                cloud.name = prim_path.clone();
                                // TODO: multi-prototype instancing not yet supported,
                                // using first proto only. See node_outputs proto_ids .first().
                                cloud.prototype_ids = vec![pid];
                                self.execute(crate::SceneCmd::MarkSceneGraphDirty);
                            }
                        }

                        // Find cloud in working scene by ID
                        let cloud = self
                            .scene
                            .working_scene
                            .point_clouds
                            .iter()
                            .find(|c| c.id == cid);

                        if let Some(cloud) = cloud {
                            let pt_count = cloud.positions.len();
                            let expanded = cloud.expand_with_prototype(pid);
                            let inst_count = expanded.len();
                            self.nodes.instancer_results.insert(node_id, expanded);

                            // Reload scene to rebuild GPU buffers with instancer instances
                            if let Err(e) = self.reload_working_scene() {
                                log::error!("Failed to reload after instancing: {}", e);
                            }

                            // Update node UI state
                            if let crate::node_graph::SceneNode::PointInstancer {
                                instance_count,
                                is_instanced,
                                is_computing,
                                compute_failed,
                                ..
                            } = &mut self.nodes.node_graph_state.snarl[snarl_id]
                            {
                                *instance_count = inst_count;
                                *is_instanced = true;
                                *is_computing = false;
                                *compute_failed = false;
                            }

                            log::info!(
                                "Point Instancer: {} points x proto {} = {} instances",
                                pt_count,
                                pid,
                                inst_count
                            );
                        } else {
                            log::warn!("Point Instancer: cloud {} not found in scene", cid);
                            mark_compute_failed(&mut self.nodes.node_graph_state.snarl, snarl_id);
                        }
                    }
                    (None, _) => {
                        log::warn!(
                            "Point Instancer: no cloud for source node {:?}",
                            points_source_node
                        );
                        mark_compute_failed(&mut self.nodes.node_graph_state.snarl, snarl_id);
                    }
                    (_, None) => {
                        log::warn!(
                            "Point Instancer: no prototype for source node {:?}",
                            proto_source_node
                        );
                        mark_compute_failed(&mut self.nodes.node_graph_state.snarl, snarl_id);
                    }
                }
            }
            NodeGraphEvent::InstancerInvalidate { node_id } => {
                if self.nodes.instancer_results.remove(&node_id).is_some() {
                    if let Err(e) = self.reload_working_scene() {
                        log::error!("Failed to reload after instancer invalidate: {}", e);
                    }
                    log::info!("Instancer {:?} invalidated", node_id);
                }
            }
            NodeGraphEvent::PointPreviewUpdate {
                node_id,
                point_size,
                point_color,
            } => {
                // All scatter nodes share one PointPreviewRenderer — last emitter wins.
                // node_id kept for diagnostics; per-node rendering is a future task.
                log::trace!(
                    "PointPreviewUpdate from {:?}: size={}, color={:?}",
                    node_id,
                    point_size,
                    point_color,
                );
                self.point_preview.point_size = point_size;
                self.point_preview.color = point_color;
                self.point_preview_params_dirty = true;
            }
            NodeGraphEvent::ExportUsd {
                node_id,
                output_path,
                as_sublayer,
                export_root,
            } => {
                let snarl_id: egui_snarl::NodeId = node_id.into();
                // Collect authored prims, graft prefix, and source USD path from upstream
                let (authored_prims, graft_prefix, upstream_usd_path) =
                    collect_export_context(snarl_id, &self.nodes.node_graph_state.snarl);
                let node_xform_overrides = collect_node_xform_overrides(
                    snarl_id,
                    &self.nodes.node_graph_state.snarl,
                    &self.nodes.node_outputs,
                    self.scene.working_scene.instances(),
                );
                // Auto-enable sublayer when upstream UsdRead exists
                let effective_as_sublayer = as_sublayer || upstream_usd_path.is_some();
                let config = bif_core::ExportConfig {
                    output_path: output_path.clone(),
                    source_usd_path: upstream_usd_path.or(self.scene.loaded_usd_path.clone()),
                    as_sublayer: effective_as_sublayer,
                    export_root,
                    authored_prims,
                    graft_prefix,
                    stage_metadata: self.scene.working_scene.stage_metadata.clone(),
                    hidden_prim_paths: Vec::new(),
                };
                let mut export_edit_state = self.scene.edit_state.clone();
                for (instance_index, transform) in node_xform_overrides {
                    let base = export_edit_state
                        .transform_overrides
                        .get(&instance_index)
                        .cloned()
                        .unwrap_or(transform.base_transform);
                    let final_matrix = transform.node_matrix * base.to_matrix();
                    export_edit_state.transform_overrides.insert(
                        instance_index,
                        bif_core::Transform::from_matrix(final_matrix),
                    );
                }

                let export_result = bif_core::usd::export::export_scene(
                    &self.scene.working_scene,
                    &export_edit_state,
                    &self.scene.instances.prim_paths,
                    &config,
                )
                .map_err(|e| anyhow::anyhow!("Export failed: {}", e));

                match export_result {
                    Ok(result) => {
                        let status = format!("{}", result);
                        log::info!("USD export: {}", status);
                        // Update node status
                        if let SceneNode::UsdExport {
                            is_exported,
                            last_result,
                            ..
                        } = &mut self.nodes.node_graph_state.snarl[snarl_id]
                        {
                            *is_exported = true;
                            *last_result = Some(status);
                        }
                    }
                    Err(e) => {
                        let err_msg = format!("{}", e);
                        log::error!("USD export failed: {}", err_msg);
                        if let SceneNode::UsdExport {
                            is_exported,
                            last_result,
                            ..
                        } = &mut self.nodes.node_graph_state.snarl[snarl_id]
                        {
                            *is_exported = false;
                            *last_result = Some(format!("Error: {}", err_msg));
                        }
                    }
                }
            }
            NodeGraphEvent::XformChanged { node_id } => {
                let snarl_id: egui_snarl::NodeId = node_id.into();
                if let SceneNode::Xform { is_applied, .. } =
                    &mut self.nodes.node_graph_state.snarl[snarl_id]
                {
                    *is_applied = true;
                }
                if let Err(e) = self.reload_working_scene() {
                    log::error!("Failed to reload after xform change: {}", e);
                }
            }
            NodeGraphEvent::UsdPrimCreate { node_id } => {
                let snarl_id: egui_snarl::NodeId = node_id.into();
                if let SceneNode::UsdPrim { is_created, .. } =
                    &mut self.nodes.node_graph_state.snarl[snarl_id]
                {
                    *is_created = true;
                }
                self.execute(crate::SceneCmd::MarkSceneGraphDirty);
            }
            NodeGraphEvent::GraftBranchesCompute { node_id } => {
                let snarl_id: egui_snarl::NodeId = node_id.into();
                if let SceneNode::GraftBranches { is_computed, .. } =
                    &mut self.nodes.node_graph_state.snarl[snarl_id]
                {
                    *is_computed = true;
                }
                self.execute(crate::SceneCmd::MarkSceneGraphDirty);
            }
            NodeGraphEvent::SetDisplayNode(id) => {
                // Toggle: clicking the same node clears display
                if self.nodes.node_graph_state.display_node == Some(id) {
                    self.nodes.node_graph_state.display_node = None;
                } else {
                    self.nodes.node_graph_state.display_node = Some(id);
                }
                if let Err(e) = self.reload_working_scene() {
                    log::error!("Failed to reload after display change: {}", e);
                }
            }
            NodeGraphEvent::SelectNode(_) => {
                // Selection handled in render_node_graph
            }
            NodeGraphEvent::DeleteNode(node_id) => {
                // Snapshot what this node owns before tearing it down, so we
                // preserve the "only re-upload preview if a cloud existed" and
                // "only compact if protos existed" behavior.
                let had_cloud = self
                    .nodes
                    .node_outputs
                    .get(&node_id)
                    .and_then(|o| o.cloud_id)
                    .is_some();
                let had_protos = self
                    .nodes
                    .node_outputs
                    .get(&node_id)
                    .is_some_and(|o| !o.proto_ids.is_empty());

                // Clean up scatter cloud (+ surface mapping) and refresh preview.
                self.execute(crate::SceneCmd::RemoveNodeCloud { node: node_id });
                if had_cloud {
                    self.execute(crate::SceneCmd::UploadPointPreview);
                    log::info!("Deleted scatter node {:?} → cloud removed", node_id);
                }

                // Clean up instancer results
                if self.nodes.instancer_results.remove(&node_id).is_some() {
                    log::info!("Deleted instancer node {:?}", node_id);
                }

                // Clean up prototypes owned by this node
                self.execute(crate::SceneCmd::RemoveNodeProtos { node: node_id });
                if had_protos {
                    // GC orphaned materials left behind by removed prototypes
                    self.scene.working_scene.compact_materials();
                    self.nodes.materials_dirty = true;
                }

                // Always reload after deletion — Xform/display-flag changes
                // need scene rebuild even if no protos were directly owned
                if let Err(e) = self.reload_working_scene() {
                    log::error!("Failed to reload after deletion: {}", e);
                }

                // If no loaded UsdRead nodes remain, clear USD stage state
                let has_usd_read = self
                    .nodes
                    .node_graph_state
                    .snarl
                    .node_ids()
                    .any(|(_, node)| {
                        matches!(
                            node,
                            SceneNode::UsdRead {
                                is_loaded: true,
                                ..
                            }
                        )
                    });
                if !has_usd_read {
                    self.scene.usd_stage = None;
                    self.scene.loaded_usd_path = None;
                    self.selection.scene_browser_state =
                        crate::scene_browser::SceneBrowserState::new();
                    self.selection.selected_prim_path = None;
                    self.selection.selected_prim_properties = None;
                }
            }
            NodeGraphEvent::CacheToggleBypass { node_id } => {
                log::info!("Cache node {:?} bypass toggled", node_id);
            }
            NodeGraphEvent::CacheClear { node_id } => {
                log::info!("Cache node {:?} cleared", node_id);
            }
            NodeGraphEvent::CookNode { node_id } => {
                // Re-dispatch as the proper compute event by reading node state
                let snarl_id: egui_snarl::NodeId = node_id.into();
                let event = match &self.nodes.node_graph_state.snarl[snarl_id] {
                    SceneNode::Primitive { kind, size, .. } => {
                        Some(NodeGraphEvent::CreatePrimitive {
                            kind: *kind,
                            size: *size,
                            node_id,
                        })
                    }
                    SceneNode::ScatterPoints {
                        source,
                        count,
                        max_point_limit,
                        seed,
                        scatter_mode,
                        min_distance,
                        align_to_normal,
                        grid_size,
                        grid_spacing,
                        sphere_radius,
                        sphere_on_surface,
                        relax_iterations,
                        scale_radii,
                        max_relax_radius,
                        scale_min,
                        scale_max,
                        rotation_range,
                        ..
                    } => Some(NodeGraphEvent::ScatterPointsCompute {
                        node_id,
                        params: crate::node_graph::ScatterPointsParams {
                            source: *source,
                            count: *count,
                            max_point_limit: *max_point_limit,
                            seed: *seed,
                            scatter_mode: *scatter_mode,
                            min_distance: *min_distance,
                            align_to_normal: *align_to_normal,
                            grid_size: *grid_size,
                            grid_spacing: *grid_spacing,
                            sphere_radius: *sphere_radius,
                            sphere_on_surface: *sphere_on_surface,
                            relax_iterations: *relax_iterations,
                            scale_radii: *scale_radii,
                            max_relax_radius: *max_relax_radius,
                            scale_min: *scale_min,
                            scale_max: *scale_max,
                            rotation_range: *rotation_range,
                            target_proto_id: None,
                        },
                    }),
                    SceneNode::PointInstancer { .. } => {
                        // Need upstream node IDs from connections
                        let in0 = self
                            .nodes
                            .node_graph_state
                            .snarl
                            .in_pin(egui_snarl::InPinId {
                                node: snarl_id,
                                input: 0,
                            });
                        let in1 = self
                            .nodes
                            .node_graph_state
                            .snarl
                            .in_pin(egui_snarl::InPinId {
                                node: snarl_id,
                                input: 1,
                            });
                        match (in0.remotes.first(), in1.remotes.first()) {
                            (Some(pts), Some(proto)) => {
                                Some(NodeGraphEvent::PointInstancerCompute {
                                    node_id,
                                    points_source_node: GraphNodeId::from(pts.node),
                                    proto_source_node: GraphNodeId::from(proto.node),
                                })
                            }
                            _ => None,
                        }
                    }
                    SceneNode::Xform { .. } => Some(NodeGraphEvent::XformChanged { node_id }),
                    SceneNode::UsdPrim { .. } => Some(NodeGraphEvent::UsdPrimCreate { node_id }),
                    _ => None,
                };
                if let Some(e) = event {
                    self.handle_node_graph_event(e);
                }
            }
        }
    }
}

/// Canonicalize a path for USD (strips Windows `\\?\` prefix).
///
/// Falls back to the original path with a warning if canonicalize fails
/// (e.g. file doesn't exist yet).
pub(crate) fn canonicalize_for_usd(path: &str) -> String {
    let result = std::fs::canonicalize(path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| {
            log::warn!(
                "canonicalize failed for '{}': {}, using original path",
                path,
                e
            );
            path.to_string()
        });
    result.strip_prefix(r"\\?\").unwrap_or(&result).to_string()
}

struct NodeXformOverride {
    node_matrix: bif_math::Mat4,
    base_transform: bif_core::Transform,
}

fn collect_node_xform_overrides(
    export_node: egui_snarl::NodeId,
    snarl: &egui_snarl::Snarl<SceneNode>,
    node_outputs: &HashMap<GraphNodeId, crate::node_graph::NodeOutputs>,
    instances: &[bif_core::Instance],
) -> HashMap<usize, NodeXformOverride> {
    let active_nodes = crate::node_graph::collect_upstream_nodes(export_node, snarl);
    let mut xforms = Vec::new();

    for &node_id in &active_nodes {
        if let SceneNode::Xform {
            translate,
            rotate,
            scale,
            ..
        } = &snarl[node_id]
        {
            let upstream = collect_xform_input_upstream(node_id, snarl);
            if upstream.is_empty() {
                continue;
            }
            let depth = upstream
                .iter()
                .filter(|&&uid| matches!(snarl[uid], SceneNode::Xform { .. }))
                .count();
            xforms.push((node_id, *translate, *rotate, *scale, upstream, depth));
        }
    }

    xforms.sort_by_key(|(_, _, _, _, _, depth)| *depth);

    let mut overrides = HashMap::new();
    for (_, translate, rotate, scale, upstream, _) in xforms {
        let node_matrix = node_xform_matrix(translate, rotate, scale);
        if node_matrix.abs_diff_eq(bif_math::Mat4::IDENTITY, 1e-7) {
            continue;
        }

        let affected_proto_ids: HashSet<usize> = node_outputs
            .iter()
            .filter(|(node_id, _)| {
                let snarl_id: egui_snarl::NodeId = (**node_id).into();
                upstream.contains(&snarl_id)
            })
            .flat_map(|(_, o)| o.proto_ids.iter().copied())
            .collect();

        for (instance_index, instance) in instances.iter().enumerate() {
            if !affected_proto_ids.contains(&instance.prototype_id) {
                continue;
            }
            overrides
                .entry(instance_index)
                .and_modify(|entry: &mut NodeXformOverride| {
                    entry.node_matrix = node_matrix * entry.node_matrix;
                })
                .or_insert_with(|| NodeXformOverride {
                    node_matrix,
                    base_transform: instance.transform,
                });
        }
    }

    overrides
}

fn collect_xform_input_upstream(
    xform_node: egui_snarl::NodeId,
    snarl: &egui_snarl::Snarl<SceneNode>,
) -> HashSet<egui_snarl::NodeId> {
    let in_pin = snarl.in_pin(egui_snarl::InPinId {
        node: xform_node,
        input: 0,
    });
    in_pin
        .remotes
        .first()
        .map(|remote| crate::node_graph::collect_upstream_nodes(remote.node, snarl))
        .unwrap_or_default()
}

fn node_xform_matrix(translate: [f32; 3], rotate: [f32; 3], scale: [f32; 3]) -> bif_math::Mat4 {
    let rotation = bif_math::Quat::from_euler(
        bif_math::EulerRot::XYZ,
        rotate[0].to_radians(),
        rotate[1].to_radians(),
        rotate[2].to_radians(),
    );
    bif_math::Mat4::from_scale_rotation_translation(
        bif_math::Vec3::new(scale[0], scale[1], scale[2]),
        rotation,
        bif_math::Vec3::new(translate[0], translate[1], translate[2]),
    )
}

/// Walk upstream from an export node and collect AuthoredPrims, graft prefix,
/// and the source USD path from an upstream UsdRead node.
// TODO: move to node_graph/ops.rs — pure function of (NodeId, &Snarl), no Renderer dependency
pub(crate) fn collect_export_context(
    export_node: egui_snarl::NodeId,
    snarl: &egui_snarl::Snarl<SceneNode>,
) -> (Vec<bif_core::AuthoredPrim>, Option<String>, Option<String>) {
    let upstream = crate::node_graph::collect_upstream_nodes(export_node, snarl);
    let mut authored_prims = Vec::new();
    let mut graft_prefix: Option<String> = None;
    let mut source_usd_path: Option<String> = None;

    for &nid in &upstream {
        match &snarl[nid] {
            SceneNode::UsdPrim {
                prim_path,
                prim_type,
                kind,
                specifier,
                ..
            } => {
                authored_prims.push(bif_core::AuthoredPrim {
                    path: prim_path.clone(),
                    prim_type: *prim_type,
                    kind: *kind,
                    specifier: *specifier,
                });
            }
            SceneNode::GraftBranches {
                destination_path, ..
            } => {
                graft_prefix = Some(destination_path.clone());
            }
            SceneNode::UsdRead {
                file_path,
                is_loaded: true,
                ..
            } if !file_path.is_empty() => {
                source_usd_path = Some(canonicalize_for_usd(file_path));
            }
            _ => {}
        }
    }

    (authored_prims, graft_prefix, source_usd_path)
}
