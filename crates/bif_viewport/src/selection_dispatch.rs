//! Selection event dispatch — prim selection, transforms, variants, framing.

use bif_core::SceneQuery;

use crate::ivar_state::RenderMode;
use crate::property_inspector::{reset_property_inspector_cache, PrimProperties, TransformEdit};
use crate::scene_browser::{CompositeProvider, PrimDataProvider, ProceduralPrimKind};
use crate::Renderer;

impl Renderer {
    /// Resolve a prim path to an instance index in the viewport's SceneInstances.
    ///
    /// Uses 3-strategy lookup (same logic as `SceneQuery::find_instance_by_prim_path`
    /// but against the viewport's parallel prim_paths array):
    /// 1. Exact match
    /// 2. Descendant prefix (`{path}/...`)
    /// 3. Synthetic `/BIF/` fallback (loader-generated paths)
    fn resolve_instance_index(&self, prim_path: &str) -> Option<usize> {
        self.scene
            .instances
            .prim_paths
            .iter()
            .position(|p| p.as_str() == prim_path)
            .or_else(|| {
                let prefix = format!("{}/", prim_path);
                self.scene
                    .instances
                    .prim_paths
                    .iter()
                    .position(|p| p.starts_with(&prefix))
            })
            .or_else(|| {
                let trimmed = prim_path.strip_prefix('/').unwrap_or(prim_path);
                let synth_prefix = format!("/BIF/{}", trimmed);
                self.scene
                    .instances
                    .prim_paths
                    .iter()
                    .position(|p| p.starts_with(&synth_prefix))
            })
    }

    /// Build prim properties for the property inspector from scene + USD data.
    fn build_prim_properties(
        &self,
        prim_path: &str,
        stage_guard: &Option<std::sync::MutexGuard<'_, bif_core::usd::UsdStage>>,
    ) -> PrimProperties {
        let composite = CompositeProvider::new(
            stage_guard.as_deref().map(|s| s as &dyn PrimDataProvider),
            &self.nodes.cached_scene_graph,
        );
        let Some(info) = composite.get_prim_info(prim_path) else {
            return PrimProperties {
                path: prim_path.to_string(),
                ..Default::default()
            };
        };

        let mut props = PrimProperties::from_display_info(&info);

        // Procedural prim stats
        if let Some(proc_data) = composite.get_procedural_data(prim_path) {
            match &proc_data.kind {
                ProceduralPrimKind::Mesh {
                    vertex_count,
                    triangle_count,
                } => {
                    props = props
                        .with_attribute("Vertices", &vertex_count.to_string())
                        .with_attribute("Triangles", &triangle_count.to_string());
                }
                ProceduralPrimKind::PointInstancer {
                    point_count,
                    prototype_refs,
                } => {
                    props = props.with_attribute("Points", &point_count.to_string());
                    if !prototype_refs.is_empty() {
                        props = props.with_attribute("Prototypes", &prototype_refs.join(", "));
                    }
                }
                ProceduralPrimKind::Scope => {}
            }
        }

        // Look up bound material via SceneQuery: prim_path → instance → prototype
        if let Some(idx) = self
            .scene
            .working_scene
            .find_instance_by_prim_path(prim_path)
        {
            let inst = &self.scene.working_scene.instances()[idx];
            if let Some(mat) = self
                .scene
                .working_scene
                .material_for_prototype(inst.prototype_id)
            {
                props = props.with_material(mat.clone());
            }
        }

        // USD prim attributes and variant sets
        if let Some(ref stage) = stage_guard {
            match stage.get_prim_attributes(prim_path) {
                Ok(attrs) if !attrs.is_empty() => {
                    props.usd_attributes = attrs;
                }
                Err(e) => {
                    log::debug!("Failed to query attributes for {}: {:?}", prim_path, e);
                }
                _ => {}
            }
            // v0.14.0 composition arcs — one entry per layer contributing
            // an opinion on this prim, ordered strongest-first.
            match stage.get_prim_stack(prim_path) {
                Ok(stack) => props.composition_arcs = stack,
                Err(e) => {
                    log::debug!("Failed to query prim stack for {}: {:?}", prim_path, e);
                }
            }
            // v0.14.0 per-attribute opinion traces. Only authored attributes
            // with multi-layer opinions are recorded — unauthored fallbacks
            // have nothing interesting to display, and single-opinion
            // attributes already show their one value inline. Resolve
            // winning-layer index against the scene's layer stack here so
            // the render code can color the dot without another lookup.
            let stack_layers = self.scene.layer_state.as_ref().map(|s| &s.stack);
            for attr in &props.usd_attributes {
                if !attr.is_authored {
                    continue;
                }
                let Ok(sources) = stage.get_attribute_opinions(prim_path, &attr.name) else {
                    continue;
                };
                if sources.len() < 2 {
                    continue;
                }
                let winning_idx = sources
                    .iter()
                    .find(|s| s.is_winning)
                    .or_else(|| sources.first())
                    .and_then(|s| {
                        stack_layers
                            .and_then(|st| st.find_by_identifier(&s.layer_identifier))
                            .map(|(idx, _)| idx)
                    });
                props
                    .opinion_traces
                    .insert(attr.name.clone(), (sources, winning_idx));
            }
            if let Ok(set_names) = stage.get_variant_set_names(prim_path) {
                for set_name in &set_names {
                    let variants = stage
                        .get_variant_names(prim_path, set_name)
                        .unwrap_or_default();
                    let selection = stage
                        .get_variant_selection(prim_path, set_name)
                        .unwrap_or_default();
                    props
                        .variant_sets
                        .push((set_name.clone(), variants, selection));
                }
            }
        }

        props
    }

    pub(crate) fn handle_prim_selected(&mut self, prim_path: String) {
        self.selection.selected_prim_path = Some(prim_path.clone());
        self.selection
            .scene_browser_state
            .expand_to_path(&prim_path);
        self.selection.scene_browser_state.select(&prim_path);
        self.selection.selected_instance_index = self.resolve_instance_index(&prim_path);
        reset_property_inspector_cache(&self.egui_ctx);

        let stage_guard = self
            .scene
            .usd_stage
            .as_ref()
            .map(|s| s.lock().expect("UsdStage mutex poisoned"));
        self.selection.selected_prim_properties =
            Some(self.build_prim_properties(&prim_path, &stage_guard));
    }

    pub(crate) fn handle_transform_edit(&mut self, edit: TransformEdit) {
        if edit.committed {
            self.project.mark_dirty();
            self.push_transform_command(
                edit.instance_index,
                edit.old_transform,
                edit.new_transform,
            );
        } else {
            let idx = edit.instance_index;
            let mat = edit.new_transform.to_matrix();
            if idx < self.scene.instances.current.len() {
                self.scene.instances.current[idx] = mat;
                self.culling.mark_dirty();
                self.update_visible_instances();
                if self.ivar.ivar_state.mode == RenderMode::Ivar {
                    if self.ivar.ivar_state.should_restart() && self.ivar.ivar_state.world.is_some()
                    {
                        self.restart_ivar_at_scale(self.ivar.ivar_state.interaction_scale());
                    }
                    self.ivar.ivar_state.last_interaction_time = Some(std::time::Instant::now());
                }
            }
        }
    }

    pub(crate) fn handle_stage_corrections_changed(&mut self) {
        if let Err(e) = self.reload_working_scene() {
            log::error!("Failed to reload after stage correction toggle: {}", e);
        }
    }

    pub(crate) fn handle_set_keyframe(&mut self, instance_index: u64) {
        self.set_keyframe(instance_index as usize);
    }

    pub(crate) fn handle_export_edit_layer(&mut self, path: std::path::PathBuf) {
        let path_str = path.display().to_string();
        match self.export_edit_layer(&path_str) {
            Ok(()) => log::info!("Edit layer exported to {}", path_str),
            Err(e) => log::error!("Failed to export edit layer: {}", e),
        }
    }

    pub(crate) fn handle_frame_selected(&mut self) {
        if let Some(idx) = self.selection.selected_instance_index {
            if let Some(transform) = self.scene.instances.current.get(idx) {
                let proto_id = self
                    .scene
                    .instances
                    .prototype_ids
                    .get(idx)
                    .copied()
                    .unwrap_or(0);
                let aabb = self
                    .scene
                    .working_scene
                    .prototypes
                    .get(proto_id)
                    .map(|p| p.mesh.bounds);
                let center = aabb.map(|a| a.centroid()).unwrap_or(bif_math::Vec3::ZERO);
                let world_center = (*transform
                    * bif_math::Vec4::new(center.x, center.y, center.z, 1.0))
                .truncate();
                let size = aabb
                    .map(|a| (a.max_point() - a.min_point()).length())
                    .unwrap_or(1.0);
                self.cam.camera.target = world_center;
                self.cam.camera.distance = size * 2.0;
                self.cam.camera.near = size * 0.01;
                self.cam.camera.far = size * 40.0;
                self.cam.camera.update_position_from_angles();
                self.update_camera();
                self.ivar.ivar_state.invalidate_scene();
            }
        }
    }

    pub(crate) fn handle_variant_changed(
        &mut self,
        prim_path: String,
        variant_set: String,
        variant_name: String,
    ) {
        log::info!(
            "Variant changed: {} / {} = {}",
            prim_path,
            variant_set,
            variant_name
        );
        // Lock, set variant, release lock before reload (which needs &mut self)
        let set_result = self.scene.usd_stage.as_ref().map(|s| {
            s.lock()
                .expect("UsdStage mutex poisoned")
                .set_variant_selection(&prim_path, &variant_set, &variant_name)
        });
        match set_result {
            Some(Err(e)) => log::error!("Failed to set variant: {:?}", e),
            Some(Ok(())) => {
                if let Some(ref path) = self.scene.loaded_usd_path {
                    let path = path.clone();
                    if let Err(e) = self.load_usd_scene(std::path::Path::new(&path)) {
                        log::error!("Failed to reload after variant change: {:?}", e);
                    }
                }
            }
            None => {}
        }
    }
}
