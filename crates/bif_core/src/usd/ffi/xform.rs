use super::*;

impl UsdStage {
    /// Get animated transform samples for a camera by path.
    ///
    /// Returns transform samples if the camera has animated transforms.
    pub fn get_camera_xform_samples(
        &self,
        camera_path: &str,
    ) -> UsdBridgeResult<Vec<TransformSample>> {
        let c_path = cstr(camera_path)?;
        let mut samples_ptr: *const UsdBridgeXformSampleRaw = ptr::null();
        let mut count: usize = 0;

        let result = unsafe {
            usd_bridge_get_camera_xform_samples(
                self.raw,
                c_path.as_ptr(),
                &mut samples_ptr,
                &mut count,
            )
        };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        if samples_ptr.is_null() || count == 0 {
            return Ok(Vec::new());
        }

        let samples = unsafe {
            std::slice::from_raw_parts(samples_ptr, count)
                .iter()
                .map(|s| TransformSample {
                    time: s.time,
                    transform: Mat4::from_cols_array(&s.transform),
                })
                .collect()
        };

        Ok(samples)
    }

    /// Get all camera paths in the stage.
    pub fn camera_paths(&self) -> UsdBridgeResult<Vec<String>> {
        let count = self.camera_count()?;
        let mut paths = Vec::with_capacity(count);

        for i in 0..count {
            let mut path_ptr: *const std::ffi::c_char = ptr::null();
            let result = unsafe { usd_bridge_get_camera_path(self.raw, i, &mut path_ptr) };
            if result != UsdBridgeErrorCode::Success {
                return Err(result.into());
            }
            if path_ptr.is_null() {
                continue;
            }
            let path_str = unsafe { CStr::from_ptr(path_ptr) }
                .to_str()
                .unwrap_or("")
                .to_string();
            paths.push(path_str);
        }

        Ok(paths)
    }

    /// Get the number of cameras in the stage.
    pub fn camera_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_camera_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get camera transform at a specific time.
    ///
    /// Returns the interpolated world transform matrix for the camera at the given time.
    pub fn get_camera_xform_at_time(&self, camera_path: &str, time: f64) -> UsdBridgeResult<Mat4> {
        let c_path = cstr(camera_path)?;
        let mut transform = [0.0f32; 16];

        let result = unsafe {
            usd_bridge_get_camera_xform_at_time(
                self.raw,
                c_path.as_ptr(),
                time,
                transform.as_mut_ptr(),
            )
        };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(Mat4::from_cols_array(&transform))
    }

    /// Get camera lens and clipping properties at a specific time.
    pub fn get_camera_properties(
        &self,
        camera_path: &str,
        time: f64,
    ) -> UsdBridgeResult<CameraProperties> {
        let c_path = cstr(camera_path)?;
        let mut props = UsdBridgeCameraPropertiesRaw {
            focal_length: 0.0,
            vertical_aperture: 0.0,
            clip_near: 0.0,
            clip_far: 0.0,
            horizontal_aperture: 0.0,
        };

        let result = unsafe {
            usd_bridge_get_camera_properties(self.raw, c_path.as_ptr(), time, &mut props)
        };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        // SAFETY: no pointers in camera properties raw struct
        Ok(super::ffi_convert::convert_camera_properties(&props))
    }
}
