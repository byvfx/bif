use super::*;

impl UsdStage {
    /// Open a USD stage from a file path.
    ///
    /// Supports `.usda` (text), `.usdc` (binary), and `.usd` (auto-detect) formats.
    /// References are automatically resolved.
    pub fn open<P: AsRef<Path>>(path: P) -> UsdBridgeResult<Self> {
        // Convert to absolute path - USD C++ library may not handle relative paths well
        let abs_path = std::fs::canonicalize(path.as_ref())
            .map_err(|_| UsdBridgeError::FileNotFound(path.as_ref().display().to_string()))?;

        let path_str = abs_path.to_str().ok_or(UsdBridgeError::InvalidPath)?;

        // Handle Windows extended path prefixes from canonicalize():
        // - Local paths: \\?\C:\... -> C:\...
        // - UNC paths: \\?\UNC\server\share\... -> \\server\share\...
        let path_str = if let Some(unc_path) = path_str.strip_prefix(r"\\?\UNC\") {
            // UNC path - convert back to standard \\server\share format
            format!(r"\\{}", unc_path)
        } else if let Some(local_path) = path_str.strip_prefix(r"\\?\") {
            // Local extended path - just strip the prefix
            local_path.to_string()
        } else {
            path_str.to_string()
        };

        let c_path = cstr(path_str.as_str())?;

        let mut raw: *mut UsdBridgeStageRaw = ptr::null_mut();

        let result = unsafe { usd_bridge_open_stage(c_path.as_ptr(), &mut raw) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::FileNotFound => {
                    UsdBridgeError::FileNotFound(path_str.to_string())
                }
                other => other.into(),
            });
        }

        Ok(Self { raw })
    }

    /// Export the stage to a file.
    ///
    /// Format is determined by file extension: `.usda`, `.usdc`, or `.usd`.
    pub fn export<P: AsRef<Path>>(&self, path: P) -> UsdBridgeResult<()> {
        let path_str = path.as_ref().to_str().ok_or(UsdBridgeError::InvalidPath)?;
        let c_path = cstr(path_str)?;

        let result = unsafe { usd_bridge_export_stage(self.raw, c_path.as_ptr()) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(())
    }

    // ========================================================================
    // Stage Metadata
    // ========================================================================

    /// Get stage metadata (metersPerUnit, upAxis).
    pub fn get_stage_metadata(&self) -> UsdBridgeResult<UsdStageMetadata> {
        let mut raw = UsdBridgeStageMetadataRaw {
            meters_per_unit: 1.0,
            up_axis: UsdBridgeUpAxisRaw::Y,
            time_codes_per_second: 24.0,
        };

        let result = unsafe { usd_bridge_get_stage_metadata(self.raw, &mut raw) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        // SAFETY: no pointers in stage metadata raw struct
        Ok(super::ffi_convert::convert_stage_metadata(&raw))
    }

    // ========================================================================
    // Timeline / Animation
    // ========================================================================

    /// Get timeline metadata from the stage.
    ///
    /// Returns start/end time codes, FPS, and whether the stage has authored time range.
    pub fn get_timeline(&self) -> UsdBridgeResult<UsdTimelineData> {
        let mut raw_data = UsdBridgeTimelineDataRaw {
            start_time_code: 0.0,
            end_time_code: 0.0,
            frames_per_second: 24.0,
            has_authored_time_range: 0,
        };

        let result = unsafe { usd_bridge_get_timeline(self.raw, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        // SAFETY: no pointers in timeline raw struct
        Ok(super::ffi_convert::convert_timeline(&raw_data))
    }

    // ========================================================================
    // Payload Load/Unload
    // ========================================================================

    /// Load a prim's payload content.
    pub fn load_payload(&self, prim_path: &str) -> UsdBridgeResult<()> {
        let c_path = cstr(prim_path)?;
        // Safety: load_payload invalidates caches in C++ (mutable through const pointer is ok
        // because the C++ side handles internal mutability via non-const stage member)
        let code = unsafe { usd_bridge_load_payload(self.raw as *mut _, c_path.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Unload a prim's payload to free memory.
    pub fn unload_payload(&self, prim_path: &str) -> UsdBridgeResult<()> {
        let c_path = cstr(prim_path)?;
        // SAFETY: same as load_payload — mutates C++ state through a const pointer; caller must
        // hold the Arc<Mutex<UsdStage>> guard to ensure exclusive access.
        let code = unsafe { usd_bridge_unload_payload(self.raw as *mut _, c_path.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Free bulk mesh geometry cache (normals, UVs, subdivision data) after Rust
    /// has copied it. Keeps vertices/indices/paths for animation queries.
    pub fn free_mesh_geometry_cache(&self) {
        if !self.raw.is_null() {
            unsafe { usd_bridge_free_mesh_geometry(self.raw) }
        }
    }

    /// Load all payloads and cache mesh/material/animation data.
    /// Stage opens with LoadNone (hierarchy only); call this to populate geometry.
    /// Returns prim count.
    pub fn load_payloads(&self) -> UsdBridgeResult<usize> {
        let mut prim_count: usize = 0;
        let result = unsafe { usd_bridge_load_payloads(self.raw, &mut prim_count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(prim_count)
    }
}

// ============================================================================
// Layer-Aware Stage (v0.14.0)
// ============================================================================
//
// Read-only inspection of the stage's layer stack, prim stacks, and
// per-attribute opinion sources, plus layer muting and payload-policy-aware
// stage opening. No write paths — editing lands in v0.16.

impl UsdStage {
    /// Open a stage with an explicit payload-loading policy.
    ///
    /// `LoadAll` opens and resolves every payload eagerly (default USD
    /// behavior). `LoadNone` opens hierarchy only; payloads load lazily via
    /// [`load_payload`](Self::load_payload) or [`load_payloads`](Self::load_payloads).
    pub fn open_with_policy<P: AsRef<Path>>(
        path: P,
        policy: PayloadPolicy,
    ) -> UsdBridgeResult<Self> {
        let abs_path = std::fs::canonicalize(path.as_ref())
            .map_err(|_| UsdBridgeError::FileNotFound(path.as_ref().display().to_string()))?;

        let path_str = abs_path.to_str().ok_or(UsdBridgeError::InvalidPath)?;

        // Mirror UsdStage::open's Windows extended-path handling.
        let path_str = if let Some(unc_path) = path_str.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{}", unc_path)
        } else if let Some(local_path) = path_str.strip_prefix(r"\\?\") {
            local_path.to_string()
        } else {
            path_str.to_string()
        };

        let c_path = cstr(path_str.as_str())?;

        let mut raw: *mut UsdBridgeStageRaw = ptr::null_mut();
        let code = unsafe {
            usd_bridge_open_stage_with_policy(
                c_path.as_ptr(),
                payload_policy_to_raw(policy),
                &mut raw,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if raw.is_null() {
            return Err(UsdBridgeError::InvalidStage);
        }
        Ok(Self { raw })
    }

    /// Get the stage's sublayer stack (root + recursive sublayers).
    ///
    /// Read-only snapshot. If sublayers change (mute/unmute, reload), call
    /// this again to refresh.
    pub fn get_layer_stack(&self) -> UsdBridgeResult<LayerStack> {
        let mut raw_stack: *mut UsdBridgeLayerStackRaw = ptr::null_mut();
        let code = unsafe { usd_bridge_stage_get_layer_stack(self.raw, &mut raw_stack) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let stack = unsafe { convert_layer_stack_ptr(raw_stack) };
        unsafe { usd_bridge_layer_stack_free(raw_stack) };
        Ok(stack)
    }

    /// Get the current edit target — the layer where new opinions would be
    /// authored. Informational in v0.14.0 (read-only release).
    pub fn get_edit_target(&self) -> UsdBridgeResult<EditTarget> {
        let mut raw_target = UsdBridgeEditTargetRaw {
            layer_identifier: ptr::null(),
        };
        let code = unsafe { usd_bridge_stage_get_edit_target(self.raw, &mut raw_target) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let target = unsafe { convert_edit_target(&raw_target) };
        unsafe { usd_bridge_edit_target_free(&mut raw_target) };
        Ok(target)
    }

    /// Mute or unmute a layer by its authored identifier. Triggers stage
    /// recomposition — any cached prim data should be refreshed.
    pub fn set_layer_muted(&self, identifier: &str, muted: bool) -> UsdBridgeResult<()> {
        let c_id = cstr(identifier)?;
        let code = unsafe {
            usd_bridge_stage_mute_layer(
                self.raw as *mut _,
                c_id.as_ptr(),
                if muted { 1 } else { 0 },
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn save_layer(&self, identifier: &str) -> UsdBridgeResult<()> {
        let c_id = cstr(identifier)?;
        let mut error_ptr: *const std::ffi::c_char = ptr::null();
        let code = unsafe { usd_bridge_layer_save(self.raw, c_id.as_ptr(), &mut error_ptr) };
        if code != UsdBridgeErrorCode::Success {
            if !error_ptr.is_null() {
                let message = unsafe { CStr::from_ptr(error_ptr).to_string_lossy().into_owned() };
                if !message.is_empty() {
                    return Err(UsdBridgeError::SaveFailed(message));
                }
            }
            return Err(code.into());
        }
        Ok(())
    }

    pub fn layer_permission_to_edit(&self, identifier: &str) -> UsdBridgeResult<bool> {
        let c_id = cstr(identifier)?;
        let mut can_edit = 0;
        let code =
            unsafe { usd_bridge_layer_permission_to_edit(self.raw, c_id.as_ptr(), &mut can_edit) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(can_edit != 0)
    }

    pub fn export_layer_as_string(&self, identifier: &str) -> UsdBridgeResult<String> {
        let c_id = cstr(identifier)?;
        let mut text_ptr: *const std::ffi::c_char = ptr::null();
        let code =
            unsafe { usd_bridge_layer_export_as_string(self.raw, c_id.as_ptr(), &mut text_ptr) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if text_ptr.is_null() {
            return Ok(String::new());
        }
        Ok(unsafe { CStr::from_ptr(text_ptr).to_string_lossy().into_owned() })
    }

    pub fn import_layer_from_string(&self, identifier: &str, text: &str) -> UsdBridgeResult<()> {
        let c_id = cstr(identifier)?;
        let c_text = cstr(text)?;
        let code = unsafe {
            usd_bridge_layer_import_from_string(self.raw, c_id.as_ptr(), c_text.as_ptr())
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn parse_usda(text: &str) -> UsdBridgeResult<()> {
        let c_text = cstr(text)?;
        let code = unsafe { usd_bridge_parse_usda(c_text.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn layer_get_attr_value(
        &self,
        identifier: &str,
        prim_path: &str,
        attr_name: &str,
    ) -> UsdBridgeResult<Option<String>> {
        let c_id = cstr(identifier)?;
        let c_path = cstr(prim_path)?;
        let c_attr = cstr(attr_name)?;
        let mut value_ptr: *const std::ffi::c_char = ptr::null();
        let code = unsafe {
            usd_bridge_layer_get_attr_value(
                self.raw,
                c_id.as_ptr(),
                c_path.as_ptr(),
                c_attr.as_ptr(),
                &mut value_ptr,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if value_ptr.is_null() {
            return Ok(None);
        }
        Ok(Some(unsafe {
            CStr::from_ptr(value_ptr).to_string_lossy().into_owned()
        }))
    }

    pub fn write_layer_xform(
        &self,
        identifier: &str,
        prim_path: &str,
        time: f64,
        matrix_16: &[f32; 16],
    ) -> UsdBridgeResult<()> {
        let c_id = cstr(identifier)?;
        let c_path = cstr(prim_path)?;
        let code = unsafe {
            usd_bridge_layer_write_xform(
                self.raw as *mut _,
                c_id.as_ptr(),
                c_path.as_ptr(),
                time,
                matrix_16.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Author a visibility opinion on `identifier`. `time = None` uses
    /// `UsdTimeCode::Default()` (the only path bif exercises in v0.16);
    /// `time = Some(t)` writes at that time-sample. Visibility-animation
    /// is on the v0.20+ roadmap — `EditOperation::Visibility` does not
    /// yet plumb a time, so production callers pass `None`.
    pub fn write_layer_visibility(
        &self,
        identifier: &str,
        prim_path: &str,
        visible: bool,
        time: Option<f64>,
    ) -> UsdBridgeResult<()> {
        let c_id = cstr(identifier)?;
        let c_path = cstr(prim_path)?;
        // Sentinel < 0.0 → C++ side resolves to `UsdTimeCode::Default()`.
        // Matches the convention in `usd_bridge_layer_write_xform`.
        let time_value = time.unwrap_or(-1.0);
        let code = unsafe {
            usd_bridge_layer_write_visibility(
                self.raw as *mut _,
                c_id.as_ptr(),
                c_path.as_ptr(),
                if visible { 1 } else { 0 },
                time_value,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn bind_layer_material(
        &self,
        identifier: &str,
        prim_path: &str,
        material_path: &str,
    ) -> UsdBridgeResult<()> {
        let c_id = cstr(identifier)?;
        let c_prim = cstr(prim_path)?;
        let c_mat = cstr(material_path)?;
        let code = unsafe {
            usd_bridge_layer_bind_material(
                self.raw as *mut _,
                c_id.as_ptr(),
                c_prim.as_ptr(),
                c_mat.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn set_layer_shader_input(
        &self,
        identifier: &str,
        shader_path: &str,
        input_name: &str,
        value_type: &str,
        value: &str,
    ) -> UsdBridgeResult<()> {
        let c_id = cstr(identifier)?;
        let c_shader = cstr(shader_path)?;
        let c_input = cstr(input_name)?;
        let c_type = cstr(value_type)?;
        let c_value = cstr(value)?;
        let code = unsafe {
            usd_bridge_layer_set_shader_input(
                self.raw as *mut _,
                c_id.as_ptr(),
                c_shader.as_ptr(),
                c_input.as_ptr(),
                c_type.as_ptr(),
                c_value.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Author the `info:id` token attribute on `shader_path` for the
    /// shading-model swap dropdown. C4b-3.
    pub fn set_layer_shader_id(
        &self,
        layer_identifier: &str,
        shader_path: &str,
        shader_id: &str,
    ) -> UsdBridgeResult<()> {
        let c_id = cstr(layer_identifier)?;
        let c_shader = cstr(shader_path)?;
        let c_value = cstr(shader_id)?;
        let code = unsafe {
            usd_bridge_layer_set_shader_id(
                self.raw as *mut _,
                c_id.as_ptr(),
                c_shader.as_ptr(),
                c_value.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Read the surface shader's `info:id` for the material bound to
    /// `prim_path`. Empty when nothing bound. C4b-3.
    pub fn get_bound_shader_id(&self, prim_path: &str) -> UsdBridgeResult<String> {
        let c_path = cstr(prim_path)?;
        let mut out_id: *const std::os::raw::c_char = std::ptr::null();
        let code =
            unsafe { usd_bridge_prim_get_bound_shader_id(self.raw, c_path.as_ptr(), &mut out_id) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if out_id.is_null() {
            return Ok(String::new());
        }
        Ok(unsafe { CStr::from_ptr(out_id) }
            .to_string_lossy()
            .into_owned())
    }

    /// Enumerate the surface shader inputs of the material bound to
    /// `prim_path`. Returns `(shader_path, inputs)` where inputs are
    /// `(name, type, value)` triples. `shader_path` is empty when no
    /// surface shader is bound. Inputs vec is empty when no material
    /// is bound. C4b-1.
    pub fn get_bound_material_inputs(
        &self,
        prim_path: &str,
    ) -> UsdBridgeResult<(String, Vec<BoundMaterialInput>)> {
        let c_path = cstr(prim_path)?;
        let mut out_text: *const std::os::raw::c_char = std::ptr::null();
        let mut out_shader: *const std::os::raw::c_char = std::ptr::null();
        let code = unsafe {
            usd_bridge_prim_get_bound_material_inputs(
                self.raw,
                c_path.as_ptr(),
                &mut out_text,
                &mut out_shader,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }

        let shader_path = if out_shader.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(out_shader) }
                .to_string_lossy()
                .into_owned()
        };
        let text = if out_text.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(out_text) }
                .to_string_lossy()
                .into_owned()
        };

        let mut inputs = Vec::new();
        for line in text.lines() {
            if line.is_empty() {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let name = parts.next().unwrap_or("").to_string();
            let type_name = parts.next().unwrap_or("").to_string();
            let value = parts.next().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            inputs.push(BoundMaterialInput {
                name,
                type_name,
                value,
            });
        }
        Ok((shader_path, inputs))
    }

    pub fn set_layer_permission_to_edit(
        &self,
        identifier: &str,
        permission_to_edit: bool,
    ) -> UsdBridgeResult<()> {
        let c_id = cstr(identifier)?;
        let code = unsafe {
            usd_bridge_layer_set_permission_to_edit(
                self.raw,
                c_id.as_ptr(),
                if permission_to_edit { 1 } else { 0 },
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Get a layer's time offset + scale as authored on the root layer's
    /// sublayer reference list. Returns identity `(0.0, 1.0)` if the layer
    /// isn't a direct sublayer of the root.
    pub fn get_layer_offset(&self, identifier: &str) -> UsdBridgeResult<LayerOffset> {
        let c_id = cstr(identifier)?;
        let mut raw_offset = UsdBridgeLayerOffsetRaw {
            offset: 0.0,
            scale: 1.0,
        };
        let code = unsafe { usd_bridge_layer_get_offset(self.raw, c_id.as_ptr(), &mut raw_offset) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(convert_layer_offset(&raw_offset))
    }

    /// Get the full prim stack — every layer that authors an opinion on the
    /// prim, ordered strongest-first. Returns empty `Vec` if the prim has no
    /// authored opinions (shouldn't happen for a composed prim).
    pub fn get_prim_stack(&self, prim_path: &str) -> UsdBridgeResult<Vec<PrimStackEntry>> {
        let c_path = cstr(prim_path)?;
        let mut raw_stack: *mut UsdBridgePrimStackRaw = ptr::null_mut();
        let code =
            unsafe { usd_bridge_prim_get_prim_stack(self.raw, c_path.as_ptr(), &mut raw_stack) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let entries = unsafe { convert_prim_stack_ptr(raw_stack) };
        unsafe { usd_bridge_prim_stack_free(raw_stack) };
        Ok(entries)
    }

    /// Get the opinion stack for a single attribute — one entry per layer
    /// contributing an opinion. The entry with `is_winning = true` is the
    /// value the composed stage sees.
    pub fn get_attribute_opinions(
        &self,
        prim_path: &str,
        attr_name: &str,
    ) -> UsdBridgeResult<Vec<OpinionSource>> {
        let c_path = cstr(prim_path)?;
        let c_attr = cstr(attr_name)?;
        let mut raw_opinions: *mut UsdBridgeAttributeOpinionsRaw = ptr::null_mut();
        let code = unsafe {
            usd_bridge_attr_get_opinion_sources(
                self.raw,
                c_path.as_ptr(),
                c_attr.as_ptr(),
                &mut raw_opinions,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let sources = unsafe { convert_attribute_opinions_ptr(raw_opinions) };
        unsafe { usd_bridge_opinions_free(raw_opinions) };
        Ok(sources)
    }

    /// Get the opinion stack for a single relationship — one entry per layer
    /// authoring an opinion. Each source's `value_display` is the comma-joined
    /// target path list authored at that layer (or "<no opinion>" / "(empty)").
    pub fn get_relationship_opinions(
        &self,
        prim_path: &str,
        rel_name: &str,
    ) -> UsdBridgeResult<Vec<OpinionSource>> {
        let c_path = cstr(prim_path)?;
        let c_rel = cstr(rel_name)?;
        let mut raw_opinions: *mut UsdBridgeAttributeOpinionsRaw = ptr::null_mut();
        let code = unsafe {
            usd_bridge_rel_get_opinion_sources(
                self.raw,
                c_path.as_ptr(),
                c_rel.as_ptr(),
                &mut raw_opinions,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let sources = unsafe { convert_attribute_opinions_ptr(raw_opinions) };
        unsafe { usd_bridge_opinions_free(raw_opinions) };
        Ok(sources)
    }
}
