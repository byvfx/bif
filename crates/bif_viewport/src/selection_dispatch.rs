//! Selection event dispatch — prim selection, transforms, variants, framing.

use crate::ivar_state::RenderMode;
use crate::property_inspector::{reset_property_inspector_cache, PrimProperties, TransformEdit};
use crate::scene_browser::{CompositeProvider, PrimDataProvider, ProceduralPrimKind};
use crate::Renderer;

impl Renderer {
    pub(crate) fn handle_prim_selected(&mut self, prim_path: String) {
        self.selection.selected_prim_path = Some(prim_path.clone());
        // Sync tree visual: expand ancestors so the row is visible, then select.
        self.selection
            .scene_browser_state
            .expand_to_path(&prim_path);
        self.selection.scene_browser_state.select(&prim_path);
        // Map prim path → instance index. Tries in order:
        //  1. exact match (normal case)
        //  2. descendant prefix (clicking a parent Xform when instance is at child mesh)
        //  3. synthetic /BIF/{path}/{idx} fallback (loader left inst.prim_path empty
        //     and resolve_prim_path generated /BIF/{proto_name}/{idx} where proto_name
        //     is often the real USD path)
        self.selection.selected_instance_index = self
            .scene
            .instances
            .prim_paths
            .iter()
            .position(|p| p.as_str() == prim_path.as_str())
            .or_else(|| {
                let prefix = format!("{}/", prim_path);
                self.scene
                    .instances
                    .prim_paths
                    .iter()
                    .position(|p| p.starts_with(&prefix))
            })
            .or_else(|| {
                let synth_prefix = format!("/BIF/{}", prim_path);
                self.scene
                    .instances
                    .prim_paths
                    .iter()
                    .position(|p| p.starts_with(&synth_prefix))
            });
        reset_property_inspector_cache(&self.egui_ctx);
        let stage_guard = self.scene.usd_stage.as_ref().map(|s| s.lock().unwrap());
        let composite = CompositeProvider::new(
            stage_guard.as_deref().map(|s| s as &dyn PrimDataProvider),
            &self.nodes.cached_scene_graph,
        );
        if let Some(info) = composite.get_prim_info(&prim_path) {
            let mut props = PrimProperties::from_display_info(&info);
            if let Some(proc_data) = composite.get_procedural_data(&prim_path) {
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
            // Look up bound material: prim_path → instance → prototype
            if let Some(inst) = self
                .scene
                .working_scene
                .instances()
                .iter()
                .find(|i| i.prim_path.as_ref() == prim_path.as_str())
            {
                if let Some(proto) = self.scene.working_scene.prototypes.get(inst.prototype_id) {
                    if let Some(mat) = &proto.material {
                        props = props.with_material(mat.clone());
                    }
                }
            }
            // Query USD prim attributes and variant sets for inspector display
            // Reuse stage_guard from CompositeProvider (avoid re-locking same Mutex)
            if let Some(ref stage) = stage_guard {
                match stage.get_prim_attributes(&prim_path) {
                    Ok(attrs) if !attrs.is_empty() => {
                        props.usd_attributes = attrs;
                    }
                    Err(e) => {
                        log::debug!("Failed to query attributes for {}: {:?}", prim_path, e);
                    }
                    _ => {}
                }
                // Query variant sets
                if let Ok(set_names) = stage.get_variant_set_names(&prim_path) {
                    for set_name in &set_names {
                        let variants = stage
                            .get_variant_names(&prim_path, set_name)
                            .unwrap_or_default();
                        let selection = stage
                            .get_variant_selection(&prim_path, set_name)
                            .unwrap_or_default();
                        props
                            .variant_sets
                            .push((set_name.clone(), variants, selection));
                    }
                }
            }

            self.selection.selected_prim_properties = Some(props);
        } else {
            self.selection.selected_prim_properties = Some(PrimProperties {
                path: prim_path,
                ..Default::default()
            });
        }
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
                .unwrap()
                .set_variant_selection(&prim_path, &variant_set, &variant_name)
        });
        match set_result {
            Some(Err(e)) => log::error!("Failed to set variant: {:?}", e),
            Some(Ok(())) => {
                // Reload scene after variant change (full rebuild)
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
