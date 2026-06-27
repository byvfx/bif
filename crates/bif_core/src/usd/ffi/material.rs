use super::*;

impl UsdStage {
    // ========================================================================
    // Material Data (UsdShade)
    // ========================================================================

    /// Get the number of materials in the stage.
    pub fn material_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_material_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(count)
    }

    /// Get material data by index.
    pub fn get_material(&self, index: usize) -> UsdBridgeResult<UsdMaterialData> {
        let mut raw_data = UsdBridgeMaterialDataRaw {
            path: ptr::null(),
            diffuse_color: [0.5, 0.5, 0.5],
            metallic: 0.0,
            roughness: 0.5,
            specular: 0.5,
            opacity: 1.0,
            transmission: 0.0,
            specular_ior: 1.5,
            emissive_color: [0.0, 0.0, 0.0],
            diffuse_texture: ptr::null(),
            roughness_texture: ptr::null(),
            metallic_texture: ptr::null(),
            normal_texture: ptr::null(),
            emissive_texture: ptr::null(),
            opacity_texture: ptr::null(),
            displacement_texture: ptr::null(),
            displacement_scale: 1.0,
            is_materialx: 0,
        };

        let result = unsafe { usd_bridge_get_material(self.raw, index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("material index {}", index))
                }
                other => other.into(),
            });
        }

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_material(&raw_data) })
    }

    /// Get all materials in the stage.
    pub fn materials(&self) -> UsdBridgeResult<Vec<UsdMaterialData>> {
        let count = self.material_count()?;
        let mut materials = Vec::with_capacity(count);
        for i in 0..count {
            materials.push(self.get_material(i)?);
        }
        Ok(materials)
    }

    // ========================================================================
    // Light Data (UsdLux)
    // ========================================================================

    /// Get the number of lights in the stage.
    pub fn light_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_light_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(count)
    }

    /// Get light data by index.
    pub fn get_light(&self, index: usize) -> UsdBridgeResult<UsdLightData> {
        let mut raw_data = UsdBridgeLightDataRaw {
            path: ptr::null(),
            light_type: UsdBridgeLightType::Distant,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            exposure: 0.0,
            transform: [0.0; 16],
            angle: 0.0,
            radius: 0.0,
            width: 0.0,
            height: 0.0,
            texture_path: ptr::null(),
            length: 0.0,
            shaping_cone_angle: 0.0,
            shaping_cone_softness: 0.0,
            shaping_focus: 0.0,
            shaping_ies_file: ptr::null(),
            light_link_includes: ptr::null(),
            light_link_include_count: 0,
            light_link_excludes: ptr::null(),
            light_link_exclude_count: 0,
        };

        let result = unsafe { usd_bridge_get_light(self.raw, index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("light index {}", index))
                }
                other => other.into(),
            });
        }

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_light(&raw_data) })
    }

    /// Get all lights in the stage.
    pub fn lights(&self) -> UsdBridgeResult<Vec<UsdLightData>> {
        let count = self.light_count()?;
        let mut lights = Vec::with_capacity(count);
        for i in 0..count {
            lights.push(self.get_light(i)?);
        }
        Ok(lights)
    }
}
