use super::*;

impl UsdStage {
    // ========================================================================
    // Prim Traversal (Scene Browser Support)
    // ========================================================================

    /// Get the total number of prims in the stage.
    pub fn prim_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_prim_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(count)
    }

    /// Get prim info by index (depth-first traversal order).
    pub fn get_prim_info(&self, index: usize) -> UsdBridgeResult<UsdPrimInfo> {
        let mut raw_info = UsdBridgePrimInfoRaw {
            path: ptr::null(),
            type_name: ptr::null(),
            is_active: 0,
            has_children: 0,
            child_count: 0,
            visibility: 1,
            has_payload: 0,
            is_loaded: 1,
            variant_set_count: 0,
            has_inherits: 0,
            has_specializes: 0,
        };

        let result = unsafe { usd_bridge_get_prim_info(self.raw, index, &mut raw_info) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("prim index {}", index))
                }
                other => other.into(),
            });
        }

        Self::convert_prim_info(&raw_info)
    }

    /// Get prim info by path.
    pub fn get_prim_info_by_path(&self, path: &str) -> UsdBridgeResult<UsdPrimInfo> {
        let c_path = cstr(path)?;
        let mut raw_info = UsdBridgePrimInfoRaw {
            path: ptr::null(),
            type_name: ptr::null(),
            is_active: 0,
            has_children: 0,
            child_count: 0,
            visibility: 1,
            has_payload: 0,
            is_loaded: 1,
            variant_set_count: 0,
            has_inherits: 0,
            has_specializes: 0,
        };

        let result =
            unsafe { usd_bridge_get_prim_info_by_path(self.raw, c_path.as_ptr(), &mut raw_info) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("prim path {}", path))
                }
                other => other.into(),
            });
        }

        Self::convert_prim_info(&raw_info)
    }

    /// Get root prim paths (direct children of pseudo-root).
    pub fn root_prim_paths(&self) -> UsdBridgeResult<Vec<String>> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_root_prim_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        let mut paths = Vec::with_capacity(count);
        for i in 0..count {
            let mut path_ptr: *const std::ffi::c_char = ptr::null();
            let result = unsafe { usd_bridge_get_root_prim_path(self.raw, i, &mut path_ptr) };

            if result != UsdBridgeErrorCode::Success {
                return Err(result.into());
            }

            let path = unsafe {
                if path_ptr.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(path_ptr).to_string_lossy().into_owned()
                }
            };
            paths.push(path);
        }

        Ok(paths)
    }

    /// Get child prim paths for a given parent path.
    ///
    /// Pass "/" or empty string for root prims.
    pub fn child_prim_paths(&self, parent_path: &str) -> UsdBridgeResult<Vec<String>> {
        let c_path = cstr(parent_path)?;

        let mut count: usize = 0;
        let result =
            unsafe { usd_bridge_get_children_count(self.raw, c_path.as_ptr(), &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("parent path {}", parent_path))
                }
                other => other.into(),
            });
        }

        let mut paths = Vec::with_capacity(count);
        for i in 0..count {
            let mut path_ptr: *const std::ffi::c_char = ptr::null();
            let result =
                unsafe { usd_bridge_get_child_path(self.raw, c_path.as_ptr(), i, &mut path_ptr) };

            if result != UsdBridgeErrorCode::Success {
                return Err(result.into());
            }

            let path = unsafe {
                if path_ptr.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(path_ptr).to_string_lossy().into_owned()
                }
            };
            paths.push(path);
        }

        Ok(paths)
    }

    /// Get all prims in the stage (depth-first order).
    pub fn all_prims(&self) -> UsdBridgeResult<Vec<UsdPrimInfo>> {
        let count = self.prim_count()?;
        let mut prims = Vec::with_capacity(count);
        for i in 0..count {
            prims.push(self.get_prim_info(i)?);
        }
        Ok(prims)
    }

    /// Return paths of all UsdGeomCamera prims in the stage.
    pub fn list_camera_prims(&self) -> UsdBridgeResult<Vec<String>> {
        Ok(self
            .all_prims()?
            .into_iter()
            .filter(|p| p.type_name == "Camera")
            .map(|p| p.path)
            .collect())
    }

    /// Helper to convert raw prim info to Rust type.
    fn convert_prim_info(raw: &UsdBridgePrimInfoRaw) -> UsdBridgeResult<UsdPrimInfo> {
        // SAFETY: raw populated by FFI call in caller; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_prim_info(raw) })
    }

    /// Get all attributes for a prim by path.
    pub fn get_prim_attributes(&self, prim_path: &str) -> UsdBridgeResult<Vec<UsdAttributeData>> {
        let c_path = cstr(prim_path)?;

        let mut raw_ptr: *mut UsdBridgeAttributeDataRaw = std::ptr::null_mut();
        let mut count: usize = 0;

        let result = unsafe {
            usd_bridge_get_prim_attributes(self.raw, c_path.as_ptr(), &mut raw_ptr, &mut count)
        };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        if raw_ptr.is_null() || count == 0 {
            return Ok(Vec::new());
        }

        let attributes = unsafe {
            let raw_slice = std::slice::from_raw_parts(raw_ptr, count);
            let attrs: Vec<UsdAttributeData> = raw_slice
                .iter()
                .map(|raw| UsdAttributeData {
                    name: super::ffi_convert::c_str_to_string(raw.name),
                    type_name: super::ffi_convert::c_str_to_string(raw.type_name),
                    value: super::ffi_convert::c_str_to_string(raw.value_str),
                    is_primvar: raw.is_primvar != 0,
                    interpolation: super::ffi_convert::c_str_to_string(raw.interpolation),
                    is_authored: raw.is_authored != 0,
                })
                .collect();
            usd_bridge_free_prim_attributes(raw_ptr, count);
            attrs
        };

        Ok(attributes)
    }

    /// Get all relationships for a prim by path. Targets are resolved
    /// (composed) — the strongest opinion's targets list, after sublayers and
    /// references. Use `get_relationship_opinions` for per-layer authoring.
    pub fn get_prim_relationships(
        &self,
        prim_path: &str,
    ) -> UsdBridgeResult<Vec<UsdRelationshipData>> {
        let c_path = cstr(prim_path)?;

        let mut raw_ptr: *mut UsdBridgeRelationshipDataRaw = std::ptr::null_mut();
        let mut count: usize = 0;

        let result = unsafe {
            usd_bridge_get_prim_relationships(self.raw, c_path.as_ptr(), &mut raw_ptr, &mut count)
        };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        if raw_ptr.is_null() || count == 0 {
            return Ok(Vec::new());
        }

        let relationships = unsafe {
            let raw_slice = std::slice::from_raw_parts(raw_ptr, count);
            let rels: Vec<UsdRelationshipData> = raw_slice
                .iter()
                .map(|raw| {
                    let mut targets = Vec::with_capacity(raw.target_count);
                    if !raw.target_paths.is_null() && raw.target_count > 0 {
                        let tps = std::slice::from_raw_parts(raw.target_paths, raw.target_count);
                        for tp in tps {
                            targets.push(super::ffi_convert::c_str_to_string(*tp));
                        }
                    }
                    UsdRelationshipData {
                        name: super::ffi_convert::c_str_to_string(raw.name),
                        targets,
                        is_authored: raw.is_authored != 0,
                    }
                })
                .collect();
            usd_bridge_free_prim_relationships(raw_ptr, count);
            rels
        };

        Ok(relationships)
    }
}

// ============================================================================
// CollectionAPI (v0.16.5)
// ============================================================================

/// Authored info for one UsdCollectionAPI instance applied to a prim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsdCollectionInfo {
    pub name: String,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    /// "expandPrims" | "expandPrimsAndProperties" | "explicitOnly"
    pub expansion_rule: String,
    pub include_root: bool,
}

impl UsdStage {
    /// List the CollectionAPI instance names applied to a prim.
    pub fn list_collections(&self, prim_path: &str) -> UsdBridgeResult<Vec<String>> {
        let c_path = cstr(prim_path)?;
        let mut raw_ptr: *mut *mut std::ffi::c_char = std::ptr::null_mut();
        let mut count: usize = 0;
        let code = unsafe {
            usd_bridge_list_collections(self.raw, c_path.as_ptr(), &mut raw_ptr, &mut count)
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if raw_ptr.is_null() || count == 0 {
            return Ok(Vec::new());
        }
        let names = unsafe {
            let slice = std::slice::from_raw_parts(raw_ptr, count);
            let names: Vec<String> = slice
                .iter()
                .map(|&p| super::ffi_convert::c_str_to_string(p as *const _))
                .collect();
            usd_bridge_free_string_list(raw_ptr, count);
            names
        };
        Ok(names)
    }

    /// Read authored includes/excludes/expansion rule for a single collection.
    pub fn get_collection_info(
        &self,
        prim_path: &str,
        coll_name: &str,
    ) -> UsdBridgeResult<UsdCollectionInfo> {
        let c_path = cstr(prim_path)?;
        let c_name = cstr(coll_name)?;
        let mut raw_ptr: *mut UsdBridgeCollectionInfoRaw = std::ptr::null_mut();
        let code = unsafe {
            usd_bridge_get_collection_info(self.raw, c_path.as_ptr(), c_name.as_ptr(), &mut raw_ptr)
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if raw_ptr.is_null() {
            return Err(UsdBridgeError::InvalidPrim(prim_path.to_string()));
        }
        let info = unsafe {
            let r = &*raw_ptr;
            let read_array = |arr: *const *const std::ffi::c_char, n: usize| -> Vec<String> {
                if arr.is_null() || n == 0 {
                    return Vec::new();
                }
                std::slice::from_raw_parts(arr, n)
                    .iter()
                    .map(|&p| super::ffi_convert::c_str_to_string(p))
                    .collect()
            };
            let result = UsdCollectionInfo {
                name: super::ffi_convert::c_str_to_string(r.name),
                includes: read_array(r.includes, r.includes_count),
                excludes: read_array(r.excludes, r.excludes_count),
                expansion_rule: super::ffi_convert::c_str_to_string(r.expansion_rule),
                include_root: r.include_root != 0,
            };
            usd_bridge_collection_info_free(raw_ptr);
            result
        };
        Ok(info)
    }

    /// Compute the fully-resolved member set (UsdCollectionAPI::ComputeIncludedPaths).
    pub fn compute_collection_members(
        &self,
        prim_path: &str,
        coll_name: &str,
    ) -> UsdBridgeResult<Vec<String>> {
        let c_path = cstr(prim_path)?;
        let c_name = cstr(coll_name)?;
        let mut raw_ptr: *mut *mut std::ffi::c_char = std::ptr::null_mut();
        let mut count: usize = 0;
        let code = unsafe {
            usd_bridge_compute_collection_members(
                self.raw,
                c_path.as_ptr(),
                c_name.as_ptr(),
                &mut raw_ptr,
                &mut count,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if raw_ptr.is_null() || count == 0 {
            return Ok(Vec::new());
        }
        let paths = unsafe {
            let slice = std::slice::from_raw_parts(raw_ptr, count);
            let paths: Vec<String> = slice
                .iter()
                .map(|&p| super::ffi_convert::c_str_to_string(p as *const _))
                .collect();
            usd_bridge_free_string_list(raw_ptr, count);
            paths
        };
        Ok(paths)
    }

    /// Apply a new CollectionAPI(coll_name) to a prim. Idempotent.
    pub fn apply_collection(&self, prim_path: &str, coll_name: &str) -> UsdBridgeResult<()> {
        let c_path = cstr(prim_path)?;
        let c_name = cstr(coll_name)?;
        let code = unsafe {
            usd_bridge_collection_apply(self.raw as *mut _, c_path.as_ptr(), c_name.as_ptr())
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Add a target to the includes (is_include=true) or excludes (false) of
    /// a collection. Authored at the stage's current edit target.
    pub fn collection_add_target(
        &self,
        prim_path: &str,
        coll_name: &str,
        target_path: &str,
        is_include: bool,
    ) -> UsdBridgeResult<()> {
        let c_path = cstr(prim_path)?;
        let c_name = cstr(coll_name)?;
        let c_target = cstr(target_path)?;
        let code = unsafe {
            usd_bridge_collection_add_target(
                self.raw as *mut _,
                c_path.as_ptr(),
                c_name.as_ptr(),
                c_target.as_ptr(),
                if is_include { 1 } else { 0 },
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Remove a target from a collection's includes/excludes.
    pub fn collection_remove_target(
        &self,
        prim_path: &str,
        coll_name: &str,
        target_path: &str,
        is_include: bool,
    ) -> UsdBridgeResult<()> {
        let c_path = cstr(prim_path)?;
        let c_name = cstr(coll_name)?;
        let c_target = cstr(target_path)?;
        let code = unsafe {
            usd_bridge_collection_remove_target(
                self.raw as *mut _,
                c_path.as_ptr(),
                c_name.as_ptr(),
                c_target.as_ptr(),
                if is_include { 1 } else { 0 },
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Set the expansion rule. Valid: "expandPrims", "expandPrimsAndProperties", "explicitOnly".
    pub fn collection_set_expansion_rule(
        &self,
        prim_path: &str,
        coll_name: &str,
        rule: &str,
    ) -> UsdBridgeResult<()> {
        let c_path = cstr(prim_path)?;
        let c_name = cstr(coll_name)?;
        let c_rule = cstr(rule)?;
        let code = unsafe {
            usd_bridge_collection_set_expansion_rule(
                self.raw as *mut _,
                c_path.as_ptr(),
                c_name.as_ptr(),
                c_rule.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }
}
