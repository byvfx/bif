use super::*;

impl UsdEditLayer {
    /// Create a new edit layer at the given output path.
    pub fn create(output_path: &str) -> UsdBridgeResult<Self> {
        let c_path = cstr(output_path)?;
        let mut raw: *mut UsdBridgeEditLayerRaw = std::ptr::null_mut();
        let code = unsafe { usd_bridge_create_edit_layer(c_path.as_ptr(), &mut raw) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(Self { raw })
    }

    /// Write a transform opinion at the given prim path and time.
    ///
    /// Use `time = -1.0` for default (static) time.
    pub fn write_xform(
        &mut self,
        prim_path: &str,
        time: f64,
        matrix: &bif_math::Mat4,
    ) -> UsdBridgeResult<()> {
        let c_path = cstr(prim_path)?;
        let cols = matrix.to_cols_array();
        let code = unsafe {
            usd_bridge_write_xform_opinion(self.raw, c_path.as_ptr(), time, cols.as_ptr())
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Add a sublayer to this edit layer (for composing over original USD).
    pub fn add_sublayer(&mut self, sublayer_path: &str) -> UsdBridgeResult<()> {
        let c_path = cstr(sublayer_path)?;
        let code = unsafe { usd_bridge_edit_layer_add_sublayer(self.raw, c_path.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Add a reference on a prim.
    pub fn add_reference(
        &mut self,
        prim_path: &str,
        reference_file: &str,
        reference_prim_path: Option<&str>,
    ) -> UsdBridgeResult<()> {
        let c_prim = cstr(prim_path)?;
        let c_file = cstr(reference_file)?;
        let c_ref_prim = reference_prim_path.map(cstr).transpose()?;
        let ref_prim_ptr = c_ref_prim
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(ptr::null());
        let code = unsafe {
            usd_bridge_edit_layer_add_reference(
                self.raw,
                c_prim.as_ptr(),
                c_file.as_ptr(),
                ref_prim_ptr,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Set the default prim on the stage.
    pub fn set_default_prim(&mut self, prim_path: &str) -> UsdBridgeResult<()> {
        let c_path = cstr(prim_path)?;
        let code = unsafe { usd_bridge_edit_layer_set_default_prim(self.raw, c_path.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write a PointInstancer prim from a point cloud.
    pub fn write_point_instancer(
        &mut self,
        prim_path: &str,
        cloud: &crate::point_cloud::PointCloud,
        proto_paths: &[String],
    ) -> UsdBridgeResult<()> {
        let c_prim = cstr(prim_path)?;

        // Flatten positions to f32 array
        let positions: Vec<f32> = cloud
            .positions
            .iter()
            .flat_map(|p| [p.x, p.y, p.z])
            .collect();
        let count = cloud.positions.len();

        // Validate parallel arrays match positions length
        if let Some(ref orients) = cloud.attributes.orientations {
            if orients.len() != count {
                return Err(UsdBridgeError::InvalidPrim(format!(
                    "orientations len {} != positions len {}",
                    orients.len(),
                    count
                )));
            }
        }
        if let Some(ref s) = cloud.attributes.scales {
            if s.len() != count {
                return Err(UsdBridgeError::InvalidPrim(format!(
                    "scales len {} != positions len {}",
                    s.len(),
                    count
                )));
            }
        }
        if cloud.attributes.proto_indices.len() != count {
            return Err(UsdBridgeError::InvalidPrim(format!(
                "proto_indices len {} != positions len {}",
                cloud.attributes.proto_indices.len(),
                count
            )));
        }

        // Flatten orientations (wxyz) if available
        let orientations: Option<Vec<f32>> =
            cloud.attributes.orientations.as_ref().map(|orients| {
                orients
                    .iter()
                    .flat_map(|q| {
                        // glam Quat is (x, y, z, w) internally, USD wants (w, x, y, z)
                        [q.w, q.x, q.y, q.z]
                    })
                    .collect()
            });

        // Flatten scales if available
        let scales: Option<Vec<f32>> = cloud
            .attributes
            .scales
            .as_ref()
            .map(|s| s.iter().flat_map(|v| [v.x, v.y, v.z]).collect());

        // Proto indices (convert u32 -> i32, checked)
        let proto_indices: Vec<i32> = cloud
            .attributes
            .proto_indices
            .iter()
            .map(|&i| {
                i32::try_from(i).map_err(|_| {
                    UsdBridgeError::InvalidPrim(format!("proto_index {} overflows i32", i))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Build C string array for prototype paths
        let c_proto_paths: Vec<CString> = proto_paths
            .iter()
            .map(|p| cstr(p.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        let c_proto_ptrs: Vec<*const std::ffi::c_char> =
            c_proto_paths.iter().map(|c| c.as_ptr()).collect();

        let code = unsafe {
            usd_bridge_write_point_instancer(
                self.raw,
                c_prim.as_ptr(),
                positions.as_ptr(),
                orientations
                    .as_ref()
                    .map(|o| o.as_ptr())
                    .unwrap_or(ptr::null()),
                scales.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null()),
                proto_indices.as_ptr(),
                count,
                c_proto_ptrs.as_ptr(),
                c_proto_ptrs.len(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write a UsdGeomMesh prim from a `Mesh`.
    pub fn write_mesh(&mut self, prim_path: &str, mesh: &crate::mesh::Mesh) -> UsdBridgeResult<()> {
        let c_path = cstr(prim_path)?;

        let points: Vec<f32> = mesh
            .positions
            .iter()
            .flat_map(|p| [p.x, p.y, p.z])
            .collect();

        let normals_flat: Vec<f32> = mesh
            .normals
            .as_ref()
            .map(|ns| ns.iter().flat_map(|n| [n.x, n.y, n.z]).collect())
            .unwrap_or_default();

        let uvs_flat: Vec<f32> = mesh
            .uvs
            .as_ref()
            .map(|uvs| uvs.iter().flat_map(|uv| [uv[0], uv[1]]).collect())
            .unwrap_or_default();

        let code = unsafe {
            usd_bridge_write_mesh(
                self.raw,
                c_path.as_ptr(),
                points.as_ptr(),
                mesh.positions.len(),
                mesh.indices.as_ptr(),
                mesh.indices.len(),
                if normals_flat.is_empty() {
                    ptr::null()
                } else {
                    normals_flat.as_ptr()
                },
                normals_flat.len() / 3,
                if uvs_flat.is_empty() {
                    ptr::null()
                } else {
                    uvs_flat.as_ptr()
                },
                uvs_flat.len() / 2,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Define or override a prim at the given path.
    pub fn define_prim(
        &mut self,
        path: &str,
        prim_type: UsdPrimType,
        specifier: UsdSpecifier,
    ) -> UsdBridgeResult<()> {
        let c_path = cstr(path)?;
        let type_name = prim_type.as_usd_type_name();
        let c_type = cstr(type_name)?;
        let spec_raw = match specifier {
            UsdSpecifier::Define => UsdBridgeSpecifierRaw::Define,
            UsdSpecifier::Over => UsdBridgeSpecifierRaw::Over,
        };
        let code =
            unsafe { usd_bridge_define_prim(self.raw, c_path.as_ptr(), c_type.as_ptr(), spec_raw) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Set the model kind on a prim (must already exist).
    pub fn set_prim_kind(&mut self, path: &str, kind: UsdKind) -> UsdBridgeResult<()> {
        let c_path = cstr(path)?;
        let kind_raw = match kind {
            UsdKind::None => UsdBridgeKindRaw::None,
            UsdKind::Component => UsdBridgeKindRaw::Component,
            UsdKind::Group => UsdBridgeKindRaw::Group,
            UsdKind::Assembly => UsdBridgeKindRaw::Assembly,
            UsdKind::Subcomponent => UsdBridgeKindRaw::Subcomponent,
        };
        let code = unsafe { usd_bridge_set_prim_kind(self.raw, c_path.as_ptr(), kind_raw) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Add a payload arc on a prim.
    pub fn add_payload(
        &mut self,
        prim_path: &str,
        asset_path: &str,
        target_path: Option<&str>,
    ) -> UsdBridgeResult<()> {
        let c_prim = cstr(prim_path)?;
        let c_asset = cstr(asset_path)?;
        let c_target = target_path.and_then(|s| CString::new(s).ok());
        let code = unsafe {
            usd_bridge_edit_layer_add_payload(
                self.raw,
                c_prim.as_ptr(),
                c_asset.as_ptr(),
                c_target.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write a material (UsdPreviewSurface + OpenPBR MaterialX).
    #[allow(clippy::too_many_arguments)]
    pub fn write_material(
        &mut self,
        mat_path: &str,
        material: &crate::scene::Material,
    ) -> UsdBridgeResult<()> {
        let c_path = cstr(mat_path)?;
        let diffuse = [
            material.base_color.x,
            material.base_color.y,
            material.base_color.z,
        ];
        let emissive = [
            material.emission_color.x,
            material.emission_color.y,
            material.emission_color.z,
        ];

        let diffuse_tex = material
            .base_color_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let roughness_tex = material
            .specular_roughness_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let metallic_tex = material
            .base_metalness_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let normal_tex = material
            .normal_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let emissive_tex = material
            .emission_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());

        let code = unsafe {
            usd_bridge_write_material(
                self.raw,
                c_path.as_ptr(),
                diffuse.as_ptr(),
                material.base_metalness,
                material.specular_roughness,
                material.specular_weight,
                material.geometry_opacity,
                emissive.as_ptr(),
                diffuse_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                roughness_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                metallic_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                normal_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                emissive_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                material.specular_ior,
                material.transmission_weight,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Bind a material to a prim.
    pub fn bind_material(&mut self, prim_path: &str, material_path: &str) -> UsdBridgeResult<()> {
        let c_prim = cstr(prim_path)?;
        let c_mat = cstr(material_path)?;
        let code = unsafe { usd_bridge_bind_material(self.raw, c_prim.as_ptr(), c_mat.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write visibility attribute on a prim.
    pub fn write_visibility(&mut self, prim_path: &str, visible: bool) -> UsdBridgeResult<()> {
        let c_path = cstr(prim_path)?;
        let code = unsafe {
            usd_bridge_write_visibility(self.raw, c_path.as_ptr(), if visible { 1 } else { 0 })
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Set stage metadata (metersPerUnit, upAxis, timeCodesPerSecond).
    pub fn set_stage_metadata(&mut self, metadata: &UsdStageMetadata) -> UsdBridgeResult<()> {
        let up_axis = match metadata.up_axis {
            UpAxis::Z => 1,
            UpAxis::Y => 0,
        };
        let code = unsafe {
            usd_bridge_set_stage_metadata(
                self.raw,
                metadata.meters_per_unit,
                up_axis,
                metadata.time_codes_per_second,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write a UsdGeomCamera prim. Call multiple times at different `time` for animation.
    #[allow(clippy::too_many_arguments)]
    pub fn write_camera(
        &mut self,
        path: &str,
        props: &CameraProperties,
        time: f64,
        transform: &Mat4,
    ) -> UsdBridgeResult<()> {
        let c_path = cstr(path)?;
        let xform = transform.to_cols_array();
        let code = unsafe {
            usd_bridge_write_camera(
                self.raw,
                c_path.as_ptr(),
                props.focal_length,
                props.horizontal_aperture,
                props.vertical_aperture,
                props.clip_near,
                props.clip_far,
                time,
                xform.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write a UsdLux light prim with type-specific properties.
    #[allow(clippy::too_many_arguments)]
    pub fn write_light(&mut self, light: &UsdLightData) -> UsdBridgeResult<()> {
        let c_path = cstr(light.path.as_str())?;
        let color = [light.color.x, light.color.y, light.color.z];
        let xform = light.transform.to_cols_array();
        let light_type = match light.light_type {
            UsdLightType::Distant => UsdBridgeLightType::Distant,
            UsdLightType::Sphere => UsdBridgeLightType::Sphere,
            UsdLightType::Rect => UsdBridgeLightType::Rect,
            UsdLightType::Dome => UsdBridgeLightType::Dome,
            UsdLightType::Cylinder => UsdBridgeLightType::Cylinder,
            UsdLightType::Disk => UsdBridgeLightType::Disk,
        };
        let tex_path = light
            .texture_path
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());

        let code = unsafe {
            usd_bridge_write_light(
                self.raw,
                c_path.as_ptr(),
                light_type,
                color.as_ptr(),
                light.intensity,
                0.0, // exposure already baked into intensity on read
                xform.as_ptr(),
                light.angle,
                light.radius,
                light.width,
                light.height,
                light.length,
                tex_path.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                light.shaping.cone_angle,
                light.shaping.cone_softness,
                light.shaping.focus,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write UsdRenderSettings prim.
    pub fn write_render_settings(
        &mut self,
        path: &str,
        resolution: (u32, u32),
        camera_path: Option<&str>,
        pixel_aspect_ratio: f32,
    ) -> UsdBridgeResult<()> {
        let c_path = cstr(path)?;
        let c_cam = camera_path.and_then(|s| CString::new(s).ok());
        let code = unsafe {
            usd_bridge_write_render_settings(
                self.raw,
                c_path.as_ptr(),
                resolution.0 as i32,
                resolution.1 as i32,
                c_cam.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                pixel_aspect_ratio,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write a GeomSubset child prim for per-face material assignment.
    pub fn write_geom_subset(
        &mut self,
        mesh_path: &str,
        subset_name: &str,
        face_indices: &[i32],
        material_path: Option<&str>,
    ) -> UsdBridgeResult<()> {
        let c_mesh = cstr(mesh_path)?;
        let c_name = cstr(subset_name)?;
        let c_mat = material_path.and_then(|s| CString::new(s).ok());
        let code = unsafe {
            usd_bridge_write_geom_subset(
                self.raw,
                c_mesh.as_ptr(),
                c_name.as_ptr(),
                face_indices.as_ptr(),
                face_indices.len(),
                c_mat.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write invisibleIds attribute on a PointInstancer prim.
    pub fn write_invisible_ids(
        &mut self,
        instancer_path: &str,
        ids: &[i64],
    ) -> UsdBridgeResult<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let c_path = cstr(instancer_path)?;
        let code = unsafe {
            usd_bridge_write_invisible_ids(self.raw, c_path.as_ptr(), ids.as_ptr(), ids.len())
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Save the edit layer to disk. Drop frees the C++ handle.
    pub fn save(self) -> UsdBridgeResult<()> {
        let code = unsafe { usd_bridge_save_edit_layer(self.raw) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        // Drop runs after this, calling usd_bridge_free_edit_layer
        Ok(())
    }
}
