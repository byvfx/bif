use super::*;

impl UsdStage {
    /// Get the number of point instancer prims in the stage.
    pub fn instancer_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_instancer_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(count)
    }

    /// Get instancer data by index.
    pub fn get_instancer(&self, index: usize) -> UsdBridgeResult<UsdInstancerData> {
        let mut raw_data = UsdBridgeInstancerDataRaw {
            path: ptr::null(),
            prototype_paths: ptr::null(),
            prototype_count: 0,
            transforms: ptr::null(),
            instance_count: 0,
            proto_indices: ptr::null(),
            velocities: ptr::null(),
            velocity_count: 0,
            angular_velocities: ptr::null(),
            angular_velocity_count: 0,
            invisible_ids: ptr::null(),
            invisible_id_count: 0,
        };

        let result = unsafe { usd_bridge_get_instancer(self.raw, index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("instancer index {}", index))
                }
                other => other.into(),
            });
        }

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_instancer(&raw_data) })
    }

    /// Get all instancers in the stage.
    pub fn instancers(&self) -> UsdBridgeResult<Vec<UsdInstancerData>> {
        let count = self.instancer_count()?;
        let mut instancers = Vec::with_capacity(count);
        for i in 0..count {
            instancers.push(self.get_instancer(i)?);
        }
        Ok(instancers)
    }

    // ========================================================================
    // Native Instances (instanceable=true)
    // ========================================================================

    /// Get the number of native instances (from instanceable=true prims).
    pub fn native_instance_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_native_instance_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get native instance data by index.
    pub fn get_native_instance(&self, index: usize) -> UsdBridgeResult<UsdNativeInstance> {
        let mut raw_data = UsdNativeInstanceDataRaw {
            proto_mesh_idx: 0,
            transform: [0.0; 16],
            material_override_idx: -1,
            purpose: 0,
        };

        let result = unsafe { usd_bridge_get_native_instance(self.raw, index, &mut raw_data) };
        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("native instance index {}", index))
                }
                other => other.into(),
            });
        }

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_native_instance(&raw_data) })
    }

    /// Get all native instances.
    pub fn native_instances(&self) -> UsdBridgeResult<Vec<UsdNativeInstance>> {
        let count = self.native_instance_count()?;
        let mut instances = Vec::with_capacity(count);
        for i in 0..count {
            instances.push(self.get_native_instance(i)?);
        }
        Ok(instances)
    }

    /// Get animation data for an instancer by index.
    ///
    /// Returns per-instance transforms at each time sample.
    pub fn get_instancer_animation(
        &self,
        instancer_index: usize,
    ) -> UsdBridgeResult<UsdAnimatedInstancerData> {
        let mut raw_data = UsdBridgeAnimatedInstancerDataRaw {
            instancer_index: 0,
            time_samples: ptr::null(),
            time_sample_count: 0,
            instance_count: 0,
            transforms: ptr::null(),
        };

        let result =
            unsafe { usd_bridge_get_instancer_animation(self.raw, instancer_index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("instancer index {}", instancer_index))
                }
                other => other.into(),
            });
        }

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        unsafe { super::ffi_convert::convert_instancer_animation(&raw_data) }
    }
}
