use crate::gpu_types::InstanceData;
use crate::ivar_state::RenderMode;
use crate::Renderer;

impl Renderer {
    /// Update timeline animation (call each frame).
    ///
    /// Uses wall-clock time for accurate playback regardless of frame rate.
    pub fn update_animation(&mut self, _delta_time: f32) {
        // Update timeline using wall-clock time (ignores delta_time)
        self.timeline_state.update();

        // Check if frame changed enough to warrant re-evaluation
        // Use larger tolerance (0.5 frame) to avoid excessive updates from rapid redraws
        let current_frame = self.timeline_state.current_frame;
        let frame_tolerance = 0.5;
        let frame_diff = (current_frame - self.scene.last_evaluated_frame).abs();
        if frame_diff < frame_tolerance {
            return; // Not enough change yet
        }

        // Get effective frame (snapped to integer if enabled)
        let eval_frame = self.timeline_state.effective_frame();

        // Sync USD camera if selected (animates camera during playback)
        // Do this BEFORE checking for mesh animations - camera can animate alone
        if let Some(camera_path) = self.cam.selected_usd_camera.clone() {
            self.sync_viewport_to_usd_camera(&camera_path);
        }

        // Sync scene camera if selected (follows instance transform during playback)
        if let crate::ivar_state::CameraSource::SceneCamera(idx) = self.cam.viewport_camera_source {
            self.sync_viewport_to_scene_camera(idx);
        }

        // Check if we have any mesh animations (transform or vertex)
        let animated_count = self
            .scene
            .instance_animations
            .iter()
            .filter(|opt| opt.as_ref().is_some_and(|anim| anim.is_animated()))
            .count();
        let has_transform_animations = animated_count > 0;
        let has_vertex_animations = !self.scene.vertex_animated_meshes.is_empty();
        let has_mesh_animations = has_transform_animations || has_vertex_animations;

        if !has_mesh_animations {
            self.scene.last_evaluated_frame = current_frame;
            return;
        }

        // Evaluate transforms and update GPU buffer
        log::debug!("Evaluating animation at frame {:.1}", eval_frame);
        if has_transform_animations {
            self.evaluate_animation_frame(eval_frame);
        }

        // Update vertex buffer for meshes with vertex animation
        if has_vertex_animations {
            self.update_vertex_animation(eval_frame);
            // Only invalidate Ivar cache when in Ivar mode (avoid overhead during viewport playback)
            if self.ivar.ivar_state.mode == RenderMode::Ivar {
                self.invalidate_ivar_scene();
            }
        }

        self.scene.last_evaluated_frame = current_frame;
    }

    /// Evaluate all animated transforms at the given frame and update GPU buffer.
    fn evaluate_animation_frame(&mut self, frame: f64) {
        let num = self.scene.instances.transforms.len();

        // Reuse per-frame buffers to avoid allocation every frame
        self.scene.anim_instances_buf.clear();
        self.scene.anim_instances_buf.reserve(num);
        self.scene.anim_transforms_buf.clear();
        self.scene.anim_transforms_buf.reserve(num);

        for (i, (base_transform, anim)) in self
            .scene
            .instances
            .transforms
            .iter()
            .zip(self.scene.instance_animations.iter())
            .enumerate()
        {
            let model_matrix = if let Some(anim) = anim {
                // Evaluate animated transform
                let evaluated = anim.evaluate(frame);
                let mat = evaluated.to_matrix();
                // Debug: show first animated instance's transform
                if i == 0 && frame as i32 % 12 == 0 {
                    log::debug!(
                        "Instance {} at frame {}: pos=({:.2}, {:.2}, {:.2})",
                        i,
                        frame,
                        evaluated.translation.x,
                        evaluated.translation.y,
                        evaluated.translation.z
                    );
                }
                mat
            } else {
                // Use static transform
                *base_transform
            };

            self.scene.anim_transforms_buf.push(model_matrix);

            let material_id = self
                .scene
                .instances
                .material_ids
                .get(i)
                .copied()
                .unwrap_or(0);
            self.scene.anim_instances_buf.push(InstanceData {
                model_matrix: model_matrix.to_cols_array_2d(),
                material_id,
                tri_mat_offset: 0,
            });
        }

        // Store evaluated transforms for use by update_visible_instances
        // base transforms stay in instance_transforms for re-evaluation
        std::mem::swap(
            &mut self.scene.instances.current,
            &mut self.scene.anim_transforms_buf,
        );

        // Recompute instance AABBs for frustum culling
        self.culling
            .update_instance_aabbs(&self.scene.instances.current);

        // Invalidate frustum cache
        self.culling.invalidate_frustum();

        // Rebuild instance_groups with animated transforms for multi-draw rendering
        if self.multi_draw.enabled {
            self.multi_draw.rebuild_instance_groups(
                &self.scene.instances.current,
                &self.scene.instances.prototype_ids,
                &self.scene.instances.material_ids,
                &self.scene.instances.purposes,
                self.display_settings.purpose_mode,
            );
        }
    }

    /// Update vertex buffer for meshes with vertex animation (deformation).
    fn update_vertex_animation(&mut self, frame: f64) {
        let stage = match &self.scene.usd_stage {
            Some(s) => s.clone(),
            None => return,
        };

        // Multi-draw mode: update per-prototype vertex buffers directly
        // This must come BEFORE mesh_ranges check because multi-draw renders from
        // prototype_gpu_data buffers, not the combined self.vertex_buffer
        if self.multi_draw.enabled {
            let vertex_animated = self.scene.vertex_animated_meshes.clone();
            let stage_guard = stage.lock().expect("UsdStage mutex poisoned");
            self.multi_draw.update_vertex_animation(
                &self.gpu.queue,
                &vertex_animated,
                |mesh_idx, f| stage_guard.get_mesh_vertices_at_time(mesh_idx, f).ok(),
                frame,
            );
            return;
        }

        // Multi-mesh combined buffer: use mesh_ranges to update correct vertex range
        // (only used when NOT in multi-draw mode)
        if let Some(ref ranges) = self.scene.mesh_data.mesh_ranges {
            let mut updated_any = false;
            let stage_guard = stage.lock().expect("UsdStage mutex poisoned");

            for &mesh_idx in &self.scene.vertex_animated_meshes {
                let range = match ranges.iter().find(|r| r.usd_mesh_index == mesh_idx) {
                    Some(r) => r,
                    None => continue,
                };

                let positions = match stage_guard.get_mesh_vertices_at_time(mesh_idx, frame) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let vertex_count = positions.len() / 3;
                if vertex_count != range.vertex_count as usize {
                    log::warn!(
                        "Vertex count mismatch for mesh {}: USD {} vs range {}",
                        mesh_idx,
                        vertex_count,
                        range.vertex_count
                    );
                    continue;
                }

                // Update only this mesh's range
                let start = range.vertex_offset as usize;
                for (i, vertex) in self.scene.mesh_data.vertices[start..start + vertex_count]
                    .iter_mut()
                    .enumerate()
                {
                    vertex.position =
                        [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
                }
                updated_any = true;
            }

            if updated_any {
                self.gpu.queue.write_buffer(
                    &self.vertex_buffer,
                    0,
                    bytemuck::cast_slice(&self.scene.mesh_data.vertices),
                );
            }
            return;
        }

        // Single-mesh fallback (original logic)
        if self.scene.vertex_animated_meshes.len() != 1 {
            return;
        }

        let mesh_idx = self.scene.vertex_animated_meshes[0];
        let stage_guard = stage.lock().expect("UsdStage mutex poisoned");
        if let Ok(positions) = stage_guard.get_mesh_vertices_at_time(mesh_idx, frame) {
            let vertex_count = positions.len() / 3;
            if vertex_count == 0 || vertex_count != self.scene.mesh_data.vertices.len() {
                return;
            }

            for (i, vertex) in self.scene.mesh_data.vertices.iter_mut().enumerate() {
                vertex.position = [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
            }

            self.gpu.queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&self.scene.mesh_data.vertices),
            );
        }
    }
}
