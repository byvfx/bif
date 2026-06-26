use super::*;

impl UsdStage {
    // ========================================================================
    // Mesh / Geometry
    // ========================================================================

    /// Get the number of mesh prims in the stage.
    pub fn mesh_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_mesh_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(count)
    }

    /// Get mesh data by index.
    pub fn get_mesh(&self, index: usize) -> UsdBridgeResult<UsdMeshData> {
        let mut raw_data = UsdBridgeMeshDataRaw {
            path: ptr::null(),
            vertices: ptr::null(),
            vertex_count: 0,
            indices: ptr::null(),
            index_count: 0,
            normals: ptr::null(),
            normal_count: 0,
            uvs: ptr::null(),
            uv_count: 0,
            face_material_ids: ptr::null(),
            triangle_count: 0,
            transform: [0.0; 16],
            purpose: UsdBridgePurposeRaw::Default,
            is_instance_proxy: 0,
            visibility: 1,
            double_sided: 0,
            subdivision_scheme: ptr::null(),
            normals_interpolation: 0,
            display_color: ptr::null(),
            display_color_count: 0,
            display_opacity: 1.0,
            resets_xform_stack: 0,
            face_vertex_counts: ptr::null(),
            face_count: 0,
            face_vertex_indices: ptr::null(),
            face_vertex_index_count: 0,
            crease_indices: ptr::null(),
            crease_index_count: 0,
            crease_lengths: ptr::null(),
            crease_length_count: 0,
            crease_sharpnesses: ptr::null(),
            crease_sharpness_count: 0,
            vertices_orig: ptr::null(),
            vertex_count_orig: 0,
            facevarying_uvs: ptr::null(),
            facevarying_uv_count: 0,
            facevarying_uv_indices: ptr::null(),
            facevarying_uv_index_count: 0,
        };

        let result = unsafe { usd_bridge_get_mesh(self.raw, index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("mesh index {}", index))
                }
                other => other.into(),
            });
        }

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_mesh(&raw_data) })
    }

    /// Get all meshes in the stage.
    pub fn meshes(&self) -> UsdBridgeResult<Vec<UsdMeshData>> {
        let count = self.mesh_count()?;
        let mut meshes = Vec::with_capacity(count);
        for i in 0..count {
            meshes.push(self.get_mesh(i)?);
        }
        Ok(meshes)
    }

    /// Get the material path bound to a mesh.
    pub fn get_mesh_material_path(&self, mesh_index: usize) -> UsdBridgeResult<Option<String>> {
        let mut path_ptr: *const std::ffi::c_char = ptr::null();
        let result =
            unsafe { usd_bridge_get_mesh_material_path(self.raw, mesh_index, &mut path_ptr) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("mesh index {}", mesh_index))
                }
                other => other.into(),
            });
        }

        if path_ptr.is_null() {
            return Ok(None);
        }

        let path = unsafe { CStr::from_ptr(path_ptr).to_string_lossy().into_owned() };
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }

    // ========================================================================
    // Primvar Query
    // ========================================================================

    /// Get user primvars for a mesh (excludes built-in st, normals, displayColor).
    pub fn get_mesh_primvars(&self, mesh_index: usize) -> UsdBridgeResult<Vec<UsdPrimvarData>> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_mesh_primvar_count(self.raw, mesh_index, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        let mut primvars = Vec::with_capacity(count);
        for i in 0..count {
            let mut raw = UsdBridgePrimvarDataRaw {
                name: ptr::null(),
                primvar_type: UsdBridgePrimvarTypeRaw::Float,
                interpolation: UsdBridgePrimvarInterpolationRaw::Vertex,
                float_data: ptr::null(),
                int_data: ptr::null(),
                element_count: 0,
            };

            let result = unsafe { usd_bridge_get_mesh_primvar(self.raw, mesh_index, i, &mut raw) };
            if result != UsdBridgeErrorCode::Success {
                continue;
            }

            // SAFETY: raw populated by FFI call above; pointers valid while stage is open
            primvars.push(unsafe { super::ffi_convert::convert_primvar(&raw) });
        }

        Ok(primvars)
    }

    // ========================================================================
    // Timeline / Animation — Mesh
    // ========================================================================

    /// Get animation data for a mesh by index.
    ///
    /// Returns transform samples if the mesh has animated transforms.
    pub fn get_mesh_animation(&self, mesh_index: usize) -> UsdBridgeResult<UsdAnimatedMeshData> {
        let mut raw_data = UsdBridgeAnimatedMeshDataRaw {
            mesh_index: 0,
            xform_samples: ptr::null(),
            xform_sample_count: 0,
        };

        let result = unsafe { usd_bridge_get_mesh_animation(self.raw, mesh_index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("mesh index {}", mesh_index))
                }
                other => other.into(),
            });
        }

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_mesh_animation(&raw_data) })
    }

    // ========================================================================
    // Vertex Animation (Point Deformation)
    // ========================================================================

    /// Check if a mesh has animated vertices (point deformation).
    ///
    /// Returns the time samples if animated, or empty vec if static.
    pub fn get_mesh_vertex_animation_times(&self, mesh_index: usize) -> UsdBridgeResult<Vec<f64>> {
        let mut info = UsdBridgeVertexAnimationInfoRaw {
            has_animated_vertices: 0,
            time_sample_count: 0,
            time_samples: ptr::null(),
        };

        let result =
            unsafe { usd_bridge_get_mesh_vertex_animation_info(self.raw, mesh_index, &mut info) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        if info.has_animated_vertices == 0 || info.time_samples.is_null() {
            return Ok(Vec::new());
        }

        let times = unsafe {
            std::slice::from_raw_parts(info.time_samples, info.time_sample_count).to_vec()
        };

        Ok(times)
    }

    /// Get mesh vertices at a specific time.
    ///
    /// For animated meshes, this queries the USD stage for interpolated vertices.
    /// For static meshes, returns the cached vertices.
    pub fn get_mesh_vertices_at_time(
        &self,
        mesh_index: usize,
        time: f64,
    ) -> UsdBridgeResult<Vec<f32>> {
        let mut vertices_ptr: *const f32 = ptr::null();
        let mut vertex_count: usize = 0;

        let result = unsafe {
            usd_bridge_get_mesh_vertices_at_time(
                self.raw,
                mesh_index,
                time,
                &mut vertices_ptr,
                &mut vertex_count,
            )
        };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        if vertices_ptr.is_null() || vertex_count == 0 {
            return Ok(Vec::new());
        }

        // Copy the vertices (C++ uses a temporary buffer that may be reused)
        let n = super::ffi_guard::checked_mul_count(vertex_count, 3, "mesh.vertices_at_time")?;
        let _ = super::ffi_guard::checked_alloc_count::<f32>(n, "mesh.vertices_at_time")?;
        let vertices = unsafe { std::slice::from_raw_parts(vertices_ptr, n).to_vec() };

        Ok(vertices)
    }

    // ========================================================================
    // BasisCurves
    // ========================================================================

    /// Get the number of BasisCurves prims.
    pub fn curves_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_curves_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get curves data by index.
    pub fn get_curves(&self, index: usize) -> UsdBridgeResult<UsdCurvesData> {
        let mut raw = UsdBridgeCurvesDataRaw {
            path: ptr::null(),
            points: ptr::null(),
            point_count: 0,
            widths: ptr::null(),
            width_count: 0,
            curve_vertex_counts: ptr::null(),
            curve_count: 0,
            curve_type: UsdBridgeCurveTypeRaw::Linear,
            basis: UsdBridgeCurveBasisRaw::Bezier,
            wrap: UsdBridgeCurveWrapRaw::Nonperiodic,
            transform: [0.0; 16],
        };

        let result = unsafe { usd_bridge_get_curves(self.raw, index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_curves(&raw) })
    }

    /// Get all BasisCurves prims.
    pub fn curves(&self) -> UsdBridgeResult<Vec<UsdCurvesData>> {
        let count = self.curves_count()?;
        let mut curves = Vec::with_capacity(count);
        for i in 0..count {
            curves.push(self.get_curves(i)?);
        }
        Ok(curves)
    }

    // ========================================================================
    // Volumes
    // ========================================================================

    /// Get volume count.
    pub fn volume_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_volume_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get volume data by index.
    pub fn get_volume(&self, index: usize) -> UsdBridgeResult<UsdVolumeData> {
        let mut raw = UsdBridgeVolumeDataRaw {
            path: ptr::null(),
            vdb_file_path: ptr::null(),
            field_name: ptr::null(),
            transform: [0.0; 16],
        };
        let result = unsafe { usd_bridge_get_volume(self.raw, index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_volume(&raw) })
    }

    /// Get all volumes.
    pub fn volumes(&self) -> UsdBridgeResult<Vec<UsdVolumeData>> {
        let count = self.volume_count()?;
        let mut vols = Vec::with_capacity(count);
        for i in 0..count {
            vols.push(self.get_volume(i)?);
        }
        Ok(vols)
    }

    // ========================================================================
    // UsdGeomPoints
    // ========================================================================

    /// Get the number of UsdGeomPoints prims.
    pub fn points_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_points_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get points data by index.
    pub fn get_points(&self, index: usize) -> UsdBridgeResult<UsdPointsData> {
        let mut raw = UsdBridgePointsDataRaw {
            path: ptr::null(),
            positions: ptr::null(),
            point_count: 0,
            widths: ptr::null(),
            width_count: 0,
            normals: ptr::null(),
            normal_count: 0,
            ids: ptr::null(),
            id_count: 0,
            transform: [0.0; 16],
        };

        let result = unsafe { usd_bridge_get_points(self.raw, index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_points(&raw) })
    }

    /// Get all UsdGeomPoints prims.
    pub fn points(&self) -> UsdBridgeResult<Vec<UsdPointsData>> {
        let count = self.points_count()?;
        let mut pts = Vec::with_capacity(count);
        for i in 0..count {
            pts.push(self.get_points(i)?);
        }
        Ok(pts)
    }

    // ========================================================================
    // Skeleton
    // ========================================================================

    /// Get skeleton count.
    pub fn skeleton_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_skeleton_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get skeleton data by index.
    pub fn get_skeleton(&self, index: usize) -> UsdBridgeResult<UsdSkeletonData> {
        let mut raw = UsdBridgeSkeletonDataRaw {
            path: ptr::null(),
            joint_paths: ptr::null(),
            joint_count: 0,
            bind_transforms: ptr::null(),
            rest_transforms: ptr::null(),
        };
        let result = unsafe { usd_bridge_get_skeleton(self.raw, index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_skeleton(&raw) })
    }

    /// Get skin binding for a mesh (if it has UsdSkelBindingAPI).
    pub fn get_skin_binding(&self, mesh_index: usize) -> UsdBridgeResult<UsdSkinBindingData> {
        let mut raw = UsdBridgeSkinBindingDataRaw {
            mesh_path: ptr::null(),
            skeleton_path: ptr::null(),
            joint_indices: ptr::null(),
            joint_indices_count: 0,
            joint_weights: ptr::null(),
            joint_weights_count: 0,
            joint_indices_element_size: 0,
            geom_bind_transform: [0.0; 16],
            skel_root_world_xform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            is_rigid: 0,
        };
        let result = unsafe { usd_bridge_get_skin_binding(self.raw, mesh_index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_skin_binding(&raw) })
    }

    /// Compute joint-skel-space transforms at a specific USD time code.
    ///
    /// Returns one `Mat4` per joint in the skeleton, in the skeleton's
    /// joint order (matches `UsdSkeletonData::joint_paths`). Phase 3 feeds
    /// these to `bif_core::skinning::compute_skin_matrices` to build the
    /// palette for CPU LBS.
    ///
    /// Cheap when called repeatedly at different times — the underlying
    /// `UsdSkelSkeletonQuery` is cached inside the C++ bridge's `UsdSkelCache`,
    /// so only the per-time topology walk + matrix compose runs each call.
    pub fn compute_skel_xforms(
        &self,
        skel_index: usize,
        time_code: f64,
    ) -> UsdBridgeResult<Vec<Mat4>> {
        let skel = self.get_skeleton(skel_index)?;
        let joint_count = skel.joint_paths.len();
        if joint_count == 0 {
            return Ok(Vec::new());
        }

        let mut buf: Vec<f32> = vec![0.0; joint_count * 16];
        let result = unsafe {
            usd_bridge_compute_skel_skin_xforms(
                self.raw,
                skel_index,
                time_code,
                buf.as_mut_ptr(),
                buf.len(),
            )
        };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        let mut out = Vec::with_capacity(joint_count);
        for i in 0..joint_count {
            let mut arr = [0.0f32; 16];
            arr.copy_from_slice(&buf[i * 16..(i + 1) * 16]);
            out.push(Mat4::from_cols_array(&arr));
        }
        Ok(out)
    }

    // ========================================================================
    // Blend Shapes
    // ========================================================================

    /// Get the number of meshes with blend shape bindings.
    pub fn blend_shape_binding_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_blend_shape_binding_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get blend shape binding data by index.
    pub fn get_blend_shape_binding(&self, index: usize) -> UsdBridgeResult<UsdBlendShapeBinding> {
        let mut raw = UsdBridgeBlendShapeBindingDataRaw {
            targets: ptr::null(),
            target_count: 0,
            mesh_prim_path: ptr::null(),
        };
        let result = unsafe { usd_bridge_get_blend_shape_binding(self.raw, index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_blend_shape_binding(&raw) })
    }

    /// Compute blend shape weights at a specific USD time code.
    ///
    /// Returns one weight per target in the binding's target order (matches
    /// `UsdBlendShapeBinding::targets`). Shapes not driven by the animation
    /// get weight 0.
    pub fn compute_blend_shape_weights(
        &self,
        binding_index: usize,
        time_code: f64,
        target_count: usize,
    ) -> UsdBridgeResult<Vec<f32>> {
        if target_count == 0 {
            return Ok(Vec::new());
        }

        let mut buf: Vec<f32> = vec![0.0; target_count];
        let result = unsafe {
            usd_bridge_compute_blend_shape_weights(
                self.raw,
                binding_index,
                time_code,
                buf.as_mut_ptr(),
                buf.len(),
            )
        };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(buf)
    }
}
