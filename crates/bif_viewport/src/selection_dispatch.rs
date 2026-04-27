//! Selection event dispatch — prim selection, transforms, variants, framing.

use bif_core::SceneQuery;
use bif_math::Vec3;

use crate::gizmo::{axis_direction, axis_name, hit_test_axis, GizmoAxis};
use crate::ivar_state::RenderMode;
use crate::property_inspector::{PrimProperties, TransformEdit};
use crate::scene_browser::{CompositeProvider, PrimDataProvider, ProceduralPrimKind};
use crate::Renderer;

impl Renderer {
    /// Resolve a prim path to an instance index in the viewport's SceneInstances.
    ///
    /// Uses layered lookup against the viewport's parallel prim_paths array:
    /// 1. Exact match
    /// 2. Synthetic `/BIF/` display-path match
    /// 3. Descendant prefix (`{path}/...`) for parent Xform selection
    /// 4. Immediate-parent match for mesh children backed by parent Xform instances
    fn resolve_instance_index(&self, prim_path: &str) -> Option<usize> {
        let query = normalize_prim_path(prim_path);
        let instance_paths = &self.scene.instances.prim_paths;
        let has_transform = |idx: usize| self.scene.instances.current.get(idx).is_some();

        instance_paths
            .iter()
            .enumerate()
            .position(|(idx, p)| has_transform(idx) && normalize_prim_path(p) == query)
            .or_else(|| {
                instance_paths.iter().enumerate().position(|(idx, p)| {
                    has_transform(idx) && normalize_instance_display_path(p) == query
                })
            })
            .or_else(|| {
                instance_paths.iter().enumerate().position(|(idx, p)| {
                    has_transform(idx)
                        && path_is_descendant_of(&normalize_instance_display_path(p), &query)
                })
            })
            .or_else(|| {
                instance_paths.iter().enumerate().position(|(idx, p)| {
                    has_transform(idx)
                        && immediate_parent(&query)
                            .is_some_and(|parent| parent == normalize_instance_display_path(p))
                })
            })
    }

    fn selected_instance_transform(&self) -> Option<(usize, bif_core::Transform)> {
        let idx = self.selection.selected_instance_index?;
        let mat = self.scene.instances.current.get(idx).copied()?;
        Some((idx, bif_core::Transform::from_matrix(mat)))
    }

    pub(crate) fn selected_gizmo_origin(&self) -> Option<Vec3> {
        let idx = self.selection.selected_instance_index?;
        let mat = self.scene.instances.current.get(idx).copied()?;
        let proto_id = self
            .scene
            .instances
            .prototype_ids
            .get(idx)
            .copied()
            .unwrap_or(0);
        let local_center = self
            .scene
            .working_scene
            .prototypes
            .get(proto_id)
            .map(|p| p.mesh.bounds.centroid())
            .unwrap_or(Vec3::ZERO);
        Some(
            (mat * bif_math::Vec4::new(local_center.x, local_center.y, local_center.z, 1.0))
                .truncate(),
        )
    }

    pub fn has_transform_gizmo(&self) -> bool {
        self.selected_gizmo_origin().is_some()
    }

    fn hover_transform_gizmo_axis(&self, screen_x: f32, screen_y: f32) -> Option<GizmoAxis> {
        let origin = self.selected_gizmo_origin()?;
        Some(hit_test_axis(
            &self.cam.camera,
            origin,
            self.viewport_rect(),
            (screen_x, screen_y),
        ))
    }

    pub fn begin_transform_gizmo_drag(
        &mut self,
        screen_x: f32,
        screen_y: f32,
    ) -> Option<&'static str> {
        let (_, transform) = self.selected_instance_transform()?;
        let origin = self.selected_gizmo_origin()?;
        let axis = self.hover_transform_gizmo_axis(screen_x, screen_y)?;
        if axis == GizmoAxis::None {
            self.selection.gizmo_state.hovered_axis = GizmoAxis::None;
            return None;
        }

        let state = &mut self.selection.gizmo_state;
        state.hovered_axis = axis;
        state.active_axis = axis;
        state.is_dragging = true;
        state.drag_start_screen = (screen_x, screen_y);
        state.drag_start_world = origin;
        state.drag_world_delta = 0.0;
        state.drag_start_transform = Some(transform);
        Some(axis_name(axis))
    }

    pub fn update_transform_gizmo_drag(&mut self, screen_x: f32, screen_y: f32) -> bool {
        if !self.selection.gizmo_state.is_dragging {
            if let Some(axis) = self.hover_transform_gizmo_axis(screen_x, screen_y) {
                self.selection.gizmo_state.hovered_axis = axis;
                return axis != GizmoAxis::None;
            }
            self.selection.gizmo_state.hovered_axis = GizmoAxis::None;
            return false;
        }

        let Some((idx, _)) = self.selected_instance_transform() else {
            self.selection.gizmo_state.reset();
            return false;
        };
        let state = self.selection.gizmo_state;
        let Some(start_transform) = state.drag_start_transform else {
            self.selection.gizmo_state.reset();
            return false;
        };
        let Some(axis_dir) = axis_direction(state.active_axis) else {
            self.selection.gizmo_state.reset();
            return false;
        };

        let delta = crate::gizmo::compute_drag_delta(
            (screen_x, screen_y),
            state.drag_start_screen,
            &self.cam.camera,
            state.active_axis,
            state.drag_start_world,
            self.viewport_rect(),
        );
        let mut new_transform = start_transform;
        new_transform.translation = start_transform.translation + axis_dir * delta;
        self.selection.gizmo_state.drag_world_delta = delta;
        self.handle_transform_edit(TransformEdit {
            instance_index: idx,
            old_transform: start_transform,
            new_transform,
            committed: false,
        });
        true
    }

    pub fn end_transform_gizmo_drag(&mut self, screen_x: f32, screen_y: f32) -> bool {
        if !self.selection.gizmo_state.is_dragging {
            self.selection.gizmo_state.hovered_axis = GizmoAxis::None;
            return false;
        }

        self.update_transform_gizmo_drag(screen_x, screen_y);
        let Some((idx, final_transform)) = self.selected_instance_transform() else {
            self.selection.gizmo_state.reset();
            return false;
        };
        let Some(start_transform) = self.selection.gizmo_state.drag_start_transform else {
            self.selection.gizmo_state.reset();
            return false;
        };

        self.handle_transform_edit(TransformEdit {
            instance_index: idx,
            old_transform: start_transform,
            new_transform: final_transform,
            committed: true,
        });
        self.selection.gizmo_state.reset();
        true
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

        let stage_guard = self
            .scene
            .usd_stage
            .as_ref()
            .map(|s| s.lock().expect("UsdStage mutex poisoned"));
        self.selection.selected_prim_properties =
            Some(self.build_prim_properties(&prim_path, &stage_guard));
        if let Some(origin) = self.selected_gizmo_origin() {
            log::info!(
                "Transform gizmo ready for {prim_path}: instance={:?}, origin=({:.3},{:.3},{:.3})",
                self.selection.selected_instance_index,
                origin.x,
                origin.y,
                origin.z
            );
        } else {
            let sample = self
                .scene
                .instances
                .prim_paths
                .iter()
                .take(8)
                .enumerate()
                .map(|(idx, path)| format!("{idx}:{path}"))
                .collect::<Vec<_>>()
                .join(", ");
            log::info!(
                "No transform gizmo instance resolved for {prim_path}; instance_paths=[{sample}]"
            );
        }
    }

    pub(crate) fn handle_transform_edit(&mut self, edit: TransformEdit) {
        if edit.committed {
            let can_author_usd = self.scene.usd_stage.is_some() && self.scene.layer_state.is_some();
            if can_author_usd {
                if let Some(key) = self
                    .instance_to_opinion_key(edit.instance_index, bif_core::usd::AttrSlot::Xform)
                {
                    let op = bif_core::usd::EditOperation::Transform {
                        key,
                        before: Some(edit.old_transform.to_matrix().to_cols_array()),
                        after: edit.new_transform.to_matrix().to_cols_array(),
                    };
                    match self.apply_usd_edit(op) {
                        Ok(_) => {
                            let idx = edit.instance_index;
                            let mat = edit.new_transform.to_matrix();
                            if idx < self.scene.instances.current.len() {
                                self.scene.instances.current[idx] = mat;
                                self.culling.mark_dirty();
                                self.update_visible_instances();
                                if let Some(pick_scene) = &self.pick_scene {
                                    pick_scene.update_instance_transform(idx, &mat);
                                }
                            }
                            if self.ivar.ivar_state.mode == RenderMode::Ivar {
                                self.invalidate_ivar_scene();
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to author USD transform edit: {e}");
                        }
                    }
                    return;
                }
            }
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

    /// Swap the surface shader's `info:id` to a new shading model
    /// and best-effort remap inputs whose name has a known analogue
    /// in the target model. The whole sequence — id swap + remap —
    /// is wrapped in `begin_group`/`end_group` so a single Ctrl+Z
    /// reverts the entire swap as one undo step. Returns the list
    /// of source-only inputs that won't round-trip (lossy params).
    /// C4b-3.
    pub fn dispatch_swap_shading_model(
        &mut self,
        prim_path: &str,
        target_model: &str,
    ) -> anyhow::Result<Vec<String>> {
        let stage_arc = self
            .scene
            .usd_stage
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no USD stage loaded"))?;
        if self.scene.layer_state.is_none() {
            return Err(anyhow::anyhow!("no scene layer state"));
        }
        let normalized = normalize_prim_path(prim_path);

        // Resolve the surface shader path + current id under the lock.
        let (shader_path, current_id, current_inputs) = {
            let stage = stage_arc
                .lock()
                .map_err(|_| anyhow::anyhow!("UsdStage mutex poisoned"))?;
            let (shader_path, inputs) = stage
                .get_bound_material_inputs(&normalized)
                .map_err(|e| anyhow::anyhow!("get bound material: {e:?}"))?;
            let current_id = stage.get_bound_shader_id(&normalized).unwrap_or_default();
            (shader_path, current_id, inputs)
        };
        if shader_path.is_empty() {
            return Err(anyhow::anyhow!("no surface shader bound"));
        }

        // Best-effort name map between OpenPBR and UsdPreviewSurface.
        let map_name = |from: &str| -> Option<&'static str> {
            match (current_id.as_str(), target_model, from) {
                ("OpenPBR", "UsdPreviewSurface", "base_color") => Some("diffuseColor"),
                ("OpenPBR", "UsdPreviewSurface", "specular_roughness") => Some("roughness"),
                ("OpenPBR", "UsdPreviewSurface", "base_metalness") => Some("metallic"),
                ("OpenPBR", "UsdPreviewSurface", "specular_ior") => Some("ior"),
                ("OpenPBR", "UsdPreviewSurface", "emission_color") => Some("emissiveColor"),
                ("UsdPreviewSurface", "OpenPBR", "diffuseColor") => Some("base_color"),
                ("UsdPreviewSurface", "OpenPBR", "roughness") => Some("specular_roughness"),
                ("UsdPreviewSurface", "OpenPBR", "metallic") => Some("base_metalness"),
                ("UsdPreviewSurface", "OpenPBR", "ior") => Some("specular_ior"),
                ("UsdPreviewSurface", "OpenPBR", "emissiveColor") => Some("emission_color"),
                _ => None,
            }
        };
        // Inputs the target model can't represent at all — surfaced
        // to the UI for the lossy QMessageBox warning.
        let lossy = |from: &str| -> bool {
            match (current_id.as_str(), target_model) {
                ("OpenPBR", "UsdPreviewSurface") => matches!(
                    from,
                    "subsurface_weight"
                        | "subsurface_color"
                        | "subsurface_radius"
                        | "subsurface_radius_scale"
                        | "subsurface_scatter_anisotropy"
                        | "transmission_weight"
                        | "transmission_color"
                        | "transmission_depth"
                        | "transmission_scatter"
                        | "transmission_dispersion_scale"
                        | "transmission_dispersion_abbe_number"
                        | "coat_weight"
                        | "coat_color"
                        | "coat_roughness"
                        | "coat_anisotropy"
                        | "coat_rotation"
                        | "coat_ior"
                        | "coat_darkening"
                ),
                _ => false,
            }
        };

        let mut dropped: Vec<String> = current_inputs
            .iter()
            .filter(|i| lossy(&i.name))
            .map(|i| i.name.clone())
            .collect();
        // Fallback: if mapping didn't recognize current_id, mark all
        // inputs as dropped so the UI surfaces something useful.
        if dropped.is_empty() && current_id != target_model {
            dropped = current_inputs
                .iter()
                .filter(|i| map_name(&i.name).is_none())
                .map(|i| i.name.clone())
                .collect();
        }

        let layer_state = self
            .scene
            .layer_state
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no scene layer state"))?;

        layer_state.edit_history.begin_group("swap shading model");

        // 1. Author the new info:id.
        let id_op = bif_core::usd::EditOperation::SetShaderId {
            key: bif_core::usd::OpinionKey::new(
                shader_path.clone(),
                bif_core::usd::AttrSlot::ShaderId,
            ),
            before: Some(current_id.clone()),
            after: target_model.to_string(),
        };
        let stage_locked = stage_arc
            .lock()
            .map_err(|_| anyhow::anyhow!("UsdStage mutex poisoned"))?;
        let _ = layer_state
            .edit_history
            .apply_and_record(&stage_locked, id_op)
            .map_err(|e| anyhow::anyhow!("set shader id: {e:?}"))?;

        // 2. Best-effort remap of mapped inputs (preserves authored
        //    values under the new model's analogous input name).
        for input in &current_inputs {
            let Some(mapped) = map_name(&input.name) else {
                continue;
            };
            let after = match parse_shader_value_for_dispatch(&input.type_name, &input.value) {
                Some(v) => v,
                None => continue,
            };
            let op = bif_core::usd::EditOperation::MaterialParamOverride {
                key: bif_core::usd::OpinionKey::new(
                    shader_path.clone(),
                    bif_core::usd::AttrSlot::ShaderInput {
                        shader_path: shader_path.clone(),
                        name: mapped.to_string(),
                    },
                ),
                before: None,
                after,
            };
            let _ = layer_state.edit_history.apply_and_record(&stage_locked, op);
        }

        layer_state.edit_history.end_group();
        layer_state.mark_working_layer_dirty(true);
        Ok(dropped)
    }

    /// Validate `new_text` as USDA, then atomically replace the
    /// contents of the layer at `layer_id` with it. The pre-replace
    /// text is captured as `before` so undo restores byte-equivalent
    /// text. C4b-2 — USDA panel Apply path.
    pub fn dispatch_replace_layer_contents(
        &mut self,
        layer_id: &str,
        new_text: &str,
    ) -> anyhow::Result<String> {
        let stage_arc = self
            .scene
            .usd_stage
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no USD stage loaded"))?;
        if self.scene.layer_state.is_none() {
            return Err(anyhow::anyhow!("no scene layer state"));
        }
        // Validate before authoring so a bad parse never lands a
        // partial change on the live stage.
        bif_core::usd::UsdStage::parse_usda(new_text)
            .map_err(|e| anyhow::anyhow!("USDA parse failed: {e:?}"))?;

        let before = stage_arc
            .lock()
            .ok()
            .and_then(|stage| stage.export_layer_as_string(layer_id).ok())
            .unwrap_or_default();

        let op =
            bif_core::usd::EditOperation::replace_layer(layer_id, before, new_text.to_string());
        self.apply_usd_edit(op)
    }

    /// Author a `MaterialAssign` opinion on the working layer.
    /// Records one undo step. C4b-1.
    pub fn dispatch_material_assign(
        &mut self,
        prim_path: &str,
        material_path: &str,
    ) -> anyhow::Result<String> {
        if self.scene.usd_stage.is_none() || self.scene.layer_state.is_none() {
            return Err(anyhow::anyhow!("no stage/layer state"));
        }
        let normalized = normalize_prim_path(prim_path);
        let op = bif_core::usd::EditOperation::MaterialAssign {
            key: bif_core::usd::OpinionKey::new(
                normalized,
                bif_core::usd::AttrSlot::MaterialBinding,
            ),
            before: None,
            after: material_path.to_string(),
        };
        self.apply_usd_edit(op)
    }

    /// Author a `MaterialParamOverride` opinion on the working layer.
    /// `before` is supplied by the caller (cache holds the pre-edit
    /// value). C4b-1.
    pub fn dispatch_material_param_override(
        &mut self,
        shader_path: &str,
        input_name: &str,
        before: Option<bif_core::usd::ShaderValue>,
        after: bif_core::usd::ShaderValue,
    ) -> anyhow::Result<String> {
        if self.scene.usd_stage.is_none() || self.scene.layer_state.is_none() {
            return Err(anyhow::anyhow!("no stage/layer state"));
        }
        let op = bif_core::usd::EditOperation::MaterialParamOverride {
            key: bif_core::usd::OpinionKey::new(
                shader_path,
                bif_core::usd::AttrSlot::ShaderInput {
                    shader_path: shader_path.to_string(),
                    name: input_name.to_string(),
                },
            ),
            before,
            after,
        };
        self.apply_usd_edit(op)
    }

    /// Author a working-layer visibility opinion for `prim_path` and
    /// record it on the C4a edit history (one undo step per call).
    /// Mirrors `handle_transform_edit` but for the Visibility slot —
    /// `before` is read from the live composed stage, `after` is the
    /// caller-supplied target. No-op when no USD stage / layer state.
    pub fn dispatch_visibility(&mut self, prim_path: &str, after: bool) -> anyhow::Result<String> {
        let stage_arc = self
            .scene
            .usd_stage
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no USD stage loaded"))?;
        if self.scene.layer_state.is_none() {
            return Err(anyhow::anyhow!("no scene layer state"));
        }
        let normalized = normalize_prim_path(prim_path);
        let before = stage_arc
            .lock()
            .ok()
            .and_then(|stage| stage.get_prim_info_by_path(&normalized).ok())
            .map(|info| info.visible);
        let op = bif_core::usd::EditOperation::Visibility {
            key: bif_core::usd::OpinionKey::new(normalized, bif_core::usd::AttrSlot::Visibility),
            before,
            after,
        };
        self.apply_usd_edit(op)
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
        let before = self.scene.usd_stage.as_ref().and_then(|s| {
            s.lock()
                .ok()
                .and_then(|stage| stage.get_variant_selection(&prim_path, &variant_set).ok())
        });
        let op = bif_core::usd::EditOperation::VariantSelect {
            key: bif_core::usd::OpinionKey::new(
                prim_path.clone(),
                bif_core::usd::AttrSlot::VariantSelection {
                    vset: variant_set.clone(),
                },
            ),
            before,
            after: variant_name.clone(),
        };
        let set_result = if self.scene.usd_stage.is_some() && self.scene.layer_state.is_some() {
            Some(self.apply_usd_edit(op).map(|_| ()))
        } else {
            None
        };
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

fn normalize_prim_path(path: &str) -> String {
    let trimmed = path.trim().trim_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn normalize_instance_display_path(path: &str) -> String {
    normalize_prim_path(&denormalize_synthetic_instance_path(path))
}

fn denormalize_synthetic_instance_path(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix("/BIF/") {
        if let Some(last_slash) = stripped.rfind('/') {
            let suffix = &stripped[last_slash + 1..];
            if suffix.parse::<usize>().is_ok() {
                return stripped[..last_slash].to_string();
            }
        }
        return stripped.to_string();
    }
    path.to_string()
}

fn path_is_descendant_of(path: &str, ancestor: &str) -> bool {
    ancestor != "/" && path.starts_with(&format!("{ancestor}/"))
}

fn immediate_parent(path: &str) -> Option<String> {
    let path = normalize_prim_path(path);
    let slash = path.rfind('/')?;
    if slash == 0 {
        return None;
    }
    Some(path[..slash].to_string())
}

/// Mirror of `bif_qt::parse_shader_value`. Used by the shading-model
/// swap remap. Defined here so the bif_viewport dispatcher doesn't
/// take a UI-crate dependency. C4b-3.
fn parse_shader_value_for_dispatch(
    type_name: &str,
    value: &str,
) -> Option<bif_core::usd::ShaderValue> {
    use bif_core::usd::ShaderValue;
    match type_name {
        "float" => value.parse::<f32>().ok().map(ShaderValue::Float),
        "double" => value.parse::<f64>().ok().map(ShaderValue::Double),
        "int" => value.parse::<i32>().ok().map(ShaderValue::Int),
        "bool" => match value.to_ascii_lowercase().as_str() {
            "true" | "1" => Some(ShaderValue::Bool(true)),
            "false" | "0" => Some(ShaderValue::Bool(false)),
            _ => None,
        },
        "token" => Some(ShaderValue::Token(value.to_string())),
        "string" => Some(ShaderValue::String(value.to_string())),
        "color3f" | "float3" => {
            let parts: Vec<&str> = value.split(',').map(str::trim).collect();
            if parts.len() != 3 {
                return None;
            }
            let r = parts[0].parse::<f32>().ok()?;
            let g = parts[1].parse::<f32>().ok()?;
            let b = parts[2].parse::<f32>().ok()?;
            if type_name == "color3f" {
                Some(ShaderValue::Color3f([r, g, b]))
            } else {
                Some(ShaderValue::Vec3f([r, g, b]))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod selection_resolver_tests {
    use super::{
        immediate_parent, normalize_instance_display_path, normalize_prim_path,
        path_is_descendant_of,
    };

    #[test]
    fn normalizes_synthetic_instance_paths_to_display_paths() {
        assert_eq!(
            normalize_instance_display_path("/BIF/cube/mesh_0/12"),
            "/cube/mesh_0"
        );
    }

    #[test]
    fn parent_selection_matches_descendant_instance_path() {
        assert!(path_is_descendant_of("/ground/mesh_0", "/ground"));
    }

    #[test]
    fn mesh_child_selection_can_match_parent_instance_path() {
        assert_eq!(immediate_parent("/cube/mesh_0").as_deref(), Some("/cube"));
    }

    #[test]
    fn normalization_restores_leading_slash() {
        assert_eq!(normalize_prim_path("cube/mesh_0"), "/cube/mesh_0");
    }
}
