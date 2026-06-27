use super::*;

impl UsdStage {
    /// Get variant set names for a prim.
    ///
    /// # Safety note
    /// C++ returns pointers to thread-local strings — we copy immediately via
    /// `to_string_lossy().into_owned()`. Never store the raw pointer across calls.
    pub fn get_variant_set_names(&self, prim_path: &str) -> UsdBridgeResult<Vec<String>> {
        let c_path = cstr(prim_path)?;
        let mut count: usize = 0;
        let code =
            unsafe { usd_bridge_get_variant_set_count(self.raw, c_path.as_ptr(), &mut count) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let mut names = Vec::with_capacity(count);
        for i in 0..count {
            let mut name_ptr: *const std::ffi::c_char = ptr::null();
            let code = unsafe {
                usd_bridge_get_variant_set_name(self.raw, c_path.as_ptr(), i, &mut name_ptr)
            };
            if code != UsdBridgeErrorCode::Success {
                continue;
            }
            if !name_ptr.is_null() {
                names.push(unsafe { CStr::from_ptr(name_ptr).to_string_lossy().into_owned() });
            }
        }
        Ok(names)
    }

    /// Get variant names within a variant set.
    pub fn get_variant_names(
        &self,
        prim_path: &str,
        variant_set: &str,
    ) -> UsdBridgeResult<Vec<String>> {
        let c_path = cstr(prim_path)?;
        let c_set = cstr(variant_set)?;
        let mut count: usize = 0;
        let code = unsafe {
            usd_bridge_get_variant_count(self.raw, c_path.as_ptr(), c_set.as_ptr(), &mut count)
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let mut names = Vec::with_capacity(count);
        for i in 0..count {
            let mut name_ptr: *const std::ffi::c_char = ptr::null();
            let code = unsafe {
                usd_bridge_get_variant_name(
                    self.raw,
                    c_path.as_ptr(),
                    c_set.as_ptr(),
                    i,
                    &mut name_ptr,
                )
            };
            if code != UsdBridgeErrorCode::Success {
                continue;
            }
            if !name_ptr.is_null() {
                names.push(unsafe { CStr::from_ptr(name_ptr).to_string_lossy().into_owned() });
            }
        }
        Ok(names)
    }

    /// Get current variant selection for a variant set.
    pub fn get_variant_selection(
        &self,
        prim_path: &str,
        variant_set: &str,
    ) -> UsdBridgeResult<String> {
        let c_path = cstr(prim_path)?;
        let c_set = cstr(variant_set)?;
        let mut sel_ptr: *const std::ffi::c_char = ptr::null();
        let code = unsafe {
            usd_bridge_get_variant_selection(
                self.raw,
                c_path.as_ptr(),
                c_set.as_ptr(),
                &mut sel_ptr,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if sel_ptr.is_null() {
            return Ok(String::new());
        }
        Ok(unsafe { CStr::from_ptr(sel_ptr).to_string_lossy().into_owned() })
    }

    /// Set variant selection (triggers re-composition, invalidates caches).
    pub fn set_variant_selection(
        &self,
        prim_path: &str,
        variant_set: &str,
        variant_name: &str,
        layer_identifier: &str,
    ) -> UsdBridgeResult<()> {
        let c_path = cstr(prim_path)?;
        let c_set = cstr(variant_set)?;
        let c_name = cstr(variant_name)?;
        let c_layer = cstr(layer_identifier)?;
        let code = unsafe {
            usd_bridge_set_variant_selection(
                self.raw as *mut _,
                c_path.as_ptr(),
                c_set.as_ptr(),
                c_name.as_ptr(),
                c_layer.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }
}
