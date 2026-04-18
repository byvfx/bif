use std::collections::HashMap;

use bif_math::{Mat4, Vec3};

use crate::gpu_types::InstanceData;
use crate::ivar_state::RenderMode;
use crate::Renderer;

impl Renderer {
    /// Update timeline animation — wall-clock-driven (call each frame).
    ///
    /// Advances `timeline_state.current_frame` based on wall clock (used by
    /// the old egui-driven playback loop) and re-evaluates animations.
    /// For externally-driven playback (e.g. Qt's QTimer), call
    /// [`Renderer::set_time`] instead.
    pub fn update_animation(&mut self, _delta_time: f32) {
        self.timeline_state.update();
        self.apply_animation_at_current_frame();
    }

    /// Snap animation state to `frame` without advancing wall-clock time.
    ///
    /// Used by Qt's playback + scrubber path (bif_qt), which drives
    /// `current_frame` externally via its own QTimer and timeline slider.
    /// Re-evaluates all animation channels at the new frame and uploads
    /// the GPU buffers so the next paint shows the moved prims.
    pub fn set_time(&mut self, frame: f64) {
        self.timeline_state.current_frame = frame;
        self.apply_animation_at_current_frame();
    }

    /// Re-evaluate every animation channel (transform / vertex /
    /// skinning / camera) at whatever frame `timeline_state.current_frame`
    /// currently holds, and push the results to the GPU.
    ///
    /// Shared by both [`Renderer::update_animation`] (wall-clock) and
    /// [`Renderer::set_time`] (external-drive). Guards against excessive
    /// re-evaluation via the 0.5-frame `last_evaluated_frame` tolerance.
    fn apply_animation_at_current_frame(&mut self) {
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

        // Check if we have any mesh animations (transform, vertex, or skinning)
        let animated_count = self
            .scene
            .instance_animations
            .iter()
            .filter(|opt| opt.as_ref().is_some_and(|anim| anim.is_animated()))
            .count();
        let has_transform_animations = animated_count > 0;
        let has_vertex_animations = !self.scene.vertex_animated_meshes.is_empty();
        let has_skinning = !self.scene.skinned_meshes.is_empty();
        let has_mesh_animations = has_transform_animations || has_vertex_animations || has_skinning;

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

        // Update vertex buffer for meshes with UsdSkel binding (v0.13.5 Phase 3)
        if has_skinning {
            self.update_skinning(eval_frame);
            if self.ivar.ivar_state.mode == RenderMode::Ivar {
                self.invalidate_ivar_scene();
            }
        }

        self.scene.last_evaluated_frame = current_frame;
    }

    /// Re-skin all registered skinned meshes at the given time code.
    ///
    /// For each entry: fetch current joint-skel xforms from the USD stage,
    /// assemble the skinning palette, run CPU LBS against the cached bind
    /// positions, then blit the result into the correct slice of the CPU
    /// vertex buffer and write the buffer to the GPU.
    ///
    /// Two mesh buffer layouts are supported:
    /// - **Single-mesh** (`mesh_ranges == None`): whole buffer belongs to one
    ///   prototype. The positions in `mesh_data.vertices` are in mesh-local
    ///   space (see `MeshData::from_core_mesh`), so skinned positions write
    ///   through directly.
    /// - **Combined multi-mesh** (`mesh_ranges == Some(_)`): buffer built via
    ///   `build_combined_from_instances`, which pre-bakes the instance xform
    ///   into each vertex.
    /// - **Multi-draw** (`multi_draw.enabled`): per-prototype GPU buffers in
    ///   `multi_draw.prototype_gpu_data`. Delegated to `MultiDrawState::update_skinning`.
    ///
    /// Per-skeleton joint xforms are deduped across all entries via a
    /// `HashMap<skel_idx, Vec<Mat4>>` cache built once per call. For scenes
    /// where many prototypes share one skeleton (e.g. HumanFemale's 77
    /// prototypes bound to a single skeleton), this collapses 77 FFI calls
    /// per frame into 1.
    fn update_skinning(&mut self, frame: f64) {
        let stage_mtx = match &self.scene.usd_stage {
            Some(s) => s.clone(),
            None => return,
        };

        // Multi-draw mode: per-prototype GPU vertex buffers live in
        // `multi_draw.prototype_gpu_data`, not in the combined `mesh_data.vertices`.
        // Delegate to the multi-draw update path — same math, different buffer layout.
        // The closure captures a per-call `xform_cache` so each unique skel_idx
        // hits the FFI exactly once per frame.
        if self.multi_draw.enabled {
            let stage = stage_mtx.lock().expect("UsdStage mutex poisoned");
            let mut xform_cache: HashMap<usize, Vec<Mat4>> = HashMap::new();
            let updated = self.multi_draw.update_skinning(
                &self.gpu.queue,
                &mut self.scene.skinned_meshes,
                |skel_idx, f| {
                    if let Some(cached) = xform_cache.get(&skel_idx) {
                        return Some(cached.clone());
                    }
                    let xforms = stage.compute_skel_xforms(skel_idx, f).ok()?;
                    xform_cache.insert(skel_idx, xforms.clone());
                    Some(xforms)
                },
                |binding_idx, f, target_count| {
                    stage
                        .compute_blend_shape_weights(binding_idx, f, target_count)
                        .ok()
                },
                frame,
            );
            drop(stage);
            if updated && self.ivar.ivar_state.mode == RenderMode::Ivar {
                self.invalidate_ivar_scene();
            }
            return;
        }

        let stage = stage_mtx.lock().expect("UsdStage mutex poisoned");

        // Per-skeleton xform cache for the inline (combined-buffer) path.
        // Same dedupe rationale as the multi-draw branch above.
        let mut xform_cache: HashMap<usize, Vec<Mat4>> = HashMap::new();

        // Borrow split: we mutate mesh_data.vertices inside the loop while
        // holding an immutable view of skinned_meshes. Index by index to sidestep.
        let entry_count = self.scene.skinned_meshes.len();
        let mut updated_any = false;

        for entry_idx in 0..entry_count {
            // Compute palette using a short scope so the borrow of skinned_meshes
            // drops before we touch mesh_data.vertices.
            let (palette, mesh_idx, vert_count) = {
                let entry = &self.scene.skinned_meshes[entry_idx];

                // Look up or compute joint-skel xforms for this skeleton.
                // Use the Entry API for the cache miss path so we don't do
                // a double-lookup (clippy::map_entry) and so we keep the
                // FFI failure → `continue` flow on the same control branch.
                if let std::collections::hash_map::Entry::Vacant(slot) =
                    xform_cache.entry(entry.skel_idx)
                {
                    match stage.compute_skel_xforms(entry.skel_idx, frame) {
                        Ok(x) => {
                            slot.insert(x);
                        }
                        Err(e) => {
                            log::debug!(
                                "compute_skel_xforms(skel={}, t={}) failed: {e}; skipping",
                                entry.skel_idx,
                                frame
                            );
                            continue;
                        }
                    }
                }
                let xforms = xform_cache.get(&entry.skel_idx).expect("inserted above");

                let palette = bif_core::skinning::compute_skin_matrices(&entry.skin, xforms);
                if palette.is_empty() {
                    continue;
                }
                (palette, entry.mesh_idx, entry.bind_positions.len())
            };

            // v0.13.6: blend shapes → skinning composition.
            // Apply blend shape deltas to bind_positions into blend_scratch,
            // then feed that as the "bind pose" input to LBS. When no blend
            // shapes are present, skin directly from bind_positions (v0.13.5 path).
            {
                let entry = &mut self.scene.skinned_meshes[entry_idx];
                if entry.skinned_scratch.len() != vert_count {
                    entry.skinned_scratch.resize(vert_count, Vec3::ZERO);
                }

                // Determine input to skinning: blend-deformed or raw bind
                let skin_input = if let Some(ref bs) = entry.blend_shapes {
                    // Fetch weights from C++ bridge at current frame
                    let weights = stage
                        .compute_blend_shape_weights(
                            bs.ffi_binding_idx as usize,
                            frame,
                            bs.targets.len(),
                        )
                        .unwrap_or_default();

                    if entry.blend_scratch_pos.len() != vert_count {
                        entry.blend_scratch_pos.resize(vert_count, Vec3::ZERO);
                    }

                    bif_core::skinning::apply_blend_shapes(
                        bs,
                        &weights,
                        &entry.bind_positions,
                        entry.bind_normals.as_deref(),
                        &mut entry.blend_scratch_pos,
                        entry.blend_scratch_norm.as_deref_mut(),
                    );
                    &entry.blend_scratch_pos as &[Vec3]
                } else {
                    &entry.bind_positions as &[Vec3]
                };

                bif_core::skinning::skin_positions(
                    &entry.skin,
                    skin_input,
                    &palette,
                    &mut entry.skinned_scratch,
                );
            }

            // Write skinned positions back into mesh_data.vertices.
            // Single-mesh mode: whole buffer is this one mesh.
            // Combined mode: use mesh_ranges to find the right slice.
            let mesh_data = &mut self.scene.mesh_data;
            let (range_start, range_len) = if let Some(ref ranges) = mesh_data.mesh_ranges {
                match ranges.iter().find(|r| r.usd_mesh_index == mesh_idx) {
                    Some(r) => (r.vertex_offset as usize, r.vertex_count as usize),
                    None => {
                        log::warn!("No mesh_range for mesh_idx {mesh_idx}; skipping skin update");
                        continue;
                    }
                }
            } else {
                (0, mesh_data.vertices.len())
            };

            if range_len != vert_count {
                log::warn!(
                    "Skinned vertex count mismatch for mesh {mesh_idx}: \
                     bind={vert_count} vs buffer range={range_len}; skipping"
                );
                continue;
            }

            let entry = &self.scene.skinned_meshes[entry_idx];
            for (i, vertex) in mesh_data.vertices[range_start..range_start + range_len]
                .iter_mut()
                .enumerate()
            {
                let p = entry.skinned_scratch[i];
                vertex.position = [p.x, p.y, p.z];
            }
            updated_any = true;
        }

        drop(stage);

        if updated_any {
            self.gpu.queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&self.scene.mesh_data.vertices),
            );
        }
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
