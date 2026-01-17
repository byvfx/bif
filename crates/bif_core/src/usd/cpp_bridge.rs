//! USD C++ Bridge - Rust FFI wrapper.
//!
//! Provides safe Rust bindings to the USD C++ library via the usd_bridge C shim.
//! Supports loading USDA, USD, and USDC files with automatic reference resolution.
//!
//! # Example
//!
//! ```ignore
//! use bif_core::usd::cpp_bridge::UsdStage;
//!
//! let stage = UsdStage::open("scene.usdc")?;
//! for mesh in stage.meshes() {
//!     println!("Mesh: {} with {} vertices", mesh.path, mesh.vertices.len());
//! }
//! ```

use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr;

use bif_math::{Mat4, Vec3};
use thiserror::Error;

// ============================================================================
// FFI Declarations
// ============================================================================

/// Opaque stage handle (matches C struct)
#[repr(C)]
struct UsdBridgeStageRaw {
    _private: [u8; 0],
}

/// Error codes from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum UsdBridgeErrorCode {
    Success = 0,
    NullPointer = 1,
    FileNotFound = 2,
    InvalidStage = 3,
    InvalidPrim = 4,
    OutOfMemory = 5,
    Unknown = 99,
}

/// Mesh data from C API
#[repr(C)]
struct UsdBridgeMeshDataRaw {
    path: *const std::ffi::c_char,
    vertices: *const f32,
    vertex_count: usize,
    indices: *const u32,
    index_count: usize,
    normals: *const f32,
    normal_count: usize,
    uvs: *const f32,
    uv_count: usize,
    transform: [f32; 16],
}

/// Instancer data from C API
#[repr(C)]
struct UsdBridgeInstancerDataRaw {
    path: *const std::ffi::c_char,
    prototype_paths: *const *const std::ffi::c_char,
    prototype_count: usize,
    transforms: *const f32,
    instance_count: usize,
    proto_indices: *const i32,
}

/// Prim info from C API (for scene browser)
#[repr(C)]
struct UsdBridgePrimInfoRaw {
    path: *const std::ffi::c_char,
    type_name: *const std::ffi::c_char,
    is_active: i32,
    has_children: i32,
    child_count: usize,
}

/// Material data from C API (UsdPreviewSurface or MaterialX)
#[repr(C)]
struct UsdBridgeMaterialDataRaw {
    path: *const std::ffi::c_char,
    diffuse_color: [f32; 3],
    metallic: f32,
    roughness: f32,
    specular: f32,
    opacity: f32,
    emissive_color: [f32; 3],
    diffuse_texture: *const std::ffi::c_char,
    roughness_texture: *const std::ffi::c_char,
    metallic_texture: *const std::ffi::c_char,
    normal_texture: *const std::ffi::c_char,
    emissive_texture: *const std::ffi::c_char,
    is_materialx: i32,
}

#[link(name = "usd_bridge")]
extern "C" {
    fn usd_bridge_error_message(error: UsdBridgeErrorCode) -> *const std::ffi::c_char;

    fn usd_bridge_open_stage(
        path: *const std::ffi::c_char,
        out_stage: *mut *mut UsdBridgeStageRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_close_stage(stage: *mut UsdBridgeStageRaw);

    fn usd_bridge_get_mesh_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_instancer_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_mesh(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeMeshDataRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_instancer(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeInstancerDataRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_export_stage(
        stage: *const UsdBridgeStageRaw,
        path: *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    // Prim traversal APIs (for scene browser)
    fn usd_bridge_get_prim_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_prim_info(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_info: *mut UsdBridgePrimInfoRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_root_prim_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_root_prim_path(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_path: *mut *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_children_count(
        stage: *const UsdBridgeStageRaw,
        parent_path: *const std::ffi::c_char,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_child_path(
        stage: *const UsdBridgeStageRaw,
        parent_path: *const std::ffi::c_char,
        index: usize,
        out_path: *mut *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_prim_info_by_path(
        stage: *const UsdBridgeStageRaw,
        path: *const std::ffi::c_char,
        out_info: *mut UsdBridgePrimInfoRaw,
    ) -> UsdBridgeErrorCode;

    // Material APIs
    fn usd_bridge_get_material_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_material(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeMaterialDataRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_mesh_material_path(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        out_path: *mut *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors from USD bridge operations.
#[derive(Error, Debug)]
pub enum UsdBridgeError {
    #[error("Null pointer passed to USD bridge")]
    NullPointer,

    #[error("USD file not found: {0}")]
    FileNotFound(String),

    #[error("Invalid USD stage handle")]
    InvalidStage,

    #[error("Invalid prim or index: {0}")]
    InvalidPrim(String),

    #[error("Out of memory")]
    OutOfMemory,

    #[error("USD bridge error: {0}")]
    Unknown(String),

    #[error("Path contains invalid UTF-8")]
    InvalidPath,
}

impl From<UsdBridgeErrorCode> for UsdBridgeError {
    fn from(code: UsdBridgeErrorCode) -> Self {
        match code {
            UsdBridgeErrorCode::Success => unreachable!("Success is not an error"),
            UsdBridgeErrorCode::NullPointer => UsdBridgeError::NullPointer,
            UsdBridgeErrorCode::FileNotFound => UsdBridgeError::FileNotFound(String::new()),
            UsdBridgeErrorCode::InvalidStage => UsdBridgeError::InvalidStage,
            UsdBridgeErrorCode::InvalidPrim => UsdBridgeError::InvalidPrim(String::new()),
            UsdBridgeErrorCode::OutOfMemory => UsdBridgeError::OutOfMemory,
            UsdBridgeErrorCode::Unknown => {
                let msg = unsafe {
                    let ptr = usd_bridge_error_message(code);
                    if ptr.is_null() {
                        "Unknown error".to_string()
                    } else {
                        CStr::from_ptr(ptr).to_string_lossy().into_owned()
                    }
                };
                UsdBridgeError::Unknown(msg)
            }
        }
    }
}

pub type UsdBridgeResult<T> = Result<T, UsdBridgeError>;

// ============================================================================
// Safe Rust Types
// ============================================================================

/// Mesh data extracted from USD.
#[derive(Clone, Debug)]
pub struct UsdMeshData {
    /// Prim path in the USD hierarchy
    pub path: String,

    /// Vertex positions
    pub vertices: Vec<Vec3>,

    /// Triangle indices
    pub indices: Vec<u32>,

    /// Vertex normals (optional)
    pub normals: Option<Vec<Vec3>>,

    /// UV coordinates (optional, from primvars:st)
    pub uvs: Option<Vec<[f32; 2]>>,

    /// World transform matrix
    pub transform: Mat4,
}

/// Point instancer data extracted from USD.
#[derive(Clone, Debug)]
pub struct UsdInstancerData {
    /// Prim path in the USD hierarchy
    pub path: String,

    /// Paths to prototype prims
    pub prototype_paths: Vec<String>,

    /// Instance transforms (world space)
    pub transforms: Vec<Mat4>,

    /// Prototype index for each instance
    pub proto_indices: Vec<i32>,
}

/// Prim info for scene hierarchy browsing.
#[derive(Clone, Debug)]
pub struct UsdPrimInfo {
    /// Prim path (e.g., "/World/Mesh")
    pub path: String,

    /// Type name (e.g., "Mesh", "Xform", "PointInstancer")
    pub type_name: String,

    /// Whether prim is active in composed scene
    pub is_active: bool,

    /// Whether prim has children
    pub has_children: bool,

    /// Number of direct children
    pub child_count: usize,
}

/// Material data extracted from USD (UsdPreviewSurface or MaterialX).
#[derive(Clone, Debug)]
pub struct UsdMaterialData {
    /// Material prim path (e.g., "/World/Looks/Material_0")
    pub path: String,

    /// Diffuse/albedo color (RGB, 0-1)
    pub diffuse_color: Vec3,

    /// Metallic factor (0=dielectric, 1=metal)
    pub metallic: f32,

    /// Roughness factor (0=smooth, 1=rough)
    pub roughness: f32,

    /// Specular factor
    pub specular: f32,

    /// Opacity (0=transparent, 1=opaque)
    pub opacity: f32,

    /// Emissive color (RGB)
    pub emissive_color: Vec3,

    /// Path to diffuse texture (if any)
    pub diffuse_texture: Option<String>,

    /// Path to roughness texture (if any)
    pub roughness_texture: Option<String>,

    /// Path to metallic texture (if any)
    pub metallic_texture: Option<String>,

    /// Path to normal map texture (if any)
    pub normal_texture: Option<String>,

    /// Path to emissive texture (if any)
    pub emissive_texture: Option<String>,

    /// True if material is from MaterialX, false for UsdPreviewSurface
    pub is_materialx: bool,
}

// ============================================================================
// UsdStage - Safe Wrapper
// ============================================================================

/// A USD stage opened via the C++ bridge.
///
/// Automatically closes the stage when dropped.
pub struct UsdStage {
    raw: *mut UsdBridgeStageRaw,
}

// UsdStage is Send because the underlying C++ code is thread-safe for reading
unsafe impl Send for UsdStage {}

impl UsdStage {
    /// Open a USD stage from a file path.
    ///
    /// Supports `.usda` (text), `.usdc` (binary), and `.usd` (auto-detect) formats.
    /// References are automatically resolved.
    pub fn open<P: AsRef<Path>>(path: P) -> UsdBridgeResult<Self> {
        // Convert to absolute path - USD C++ library may not handle relative paths well
        let abs_path = std::fs::canonicalize(path.as_ref())
            .map_err(|_| UsdBridgeError::FileNotFound(path.as_ref().display().to_string()))?;

        // Convert to forward slashes for USD compatibility
        let path_str = abs_path.to_str().ok_or(UsdBridgeError::InvalidPath)?;
        // Remove Windows extended path prefix if present (\\?\)
        let path_str = path_str.strip_prefix(r"\\?\").unwrap_or(path_str);

        let c_path = CString::new(path_str).map_err(|_| UsdBridgeError::InvalidPath)?;

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

    /// Get the number of mesh prims in the stage.
    pub fn mesh_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_mesh_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(count)
    }

    /// Get the number of point instancer prims in the stage.
    pub fn instancer_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_instancer_count(self.raw, &mut count) };

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
            transform: [0.0; 16],
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

        // Convert to Rust types
        let path = unsafe {
            if raw_data.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw_data.path).to_string_lossy().into_owned()
            }
        };

        // Convert vertices (flat f32 array to Vec<Vec3>)
        let vertices = unsafe {
            if raw_data.vertices.is_null() || raw_data.vertex_count == 0 {
                Vec::new()
            } else {
                let slice =
                    std::slice::from_raw_parts(raw_data.vertices, raw_data.vertex_count * 3);
                slice
                    .chunks_exact(3)
                    .map(|chunk| Vec3::new(chunk[0], chunk[1], chunk[2]))
                    .collect()
            }
        };

        // Convert indices
        let indices = unsafe {
            if raw_data.indices.is_null() || raw_data.index_count == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(raw_data.indices, raw_data.index_count).to_vec()
            }
        };

        // Convert normals (optional)
        let normals = unsafe {
            if raw_data.normals.is_null() || raw_data.normal_count == 0 {
                None
            } else {
                let slice = std::slice::from_raw_parts(raw_data.normals, raw_data.normal_count * 3);
                Some(
                    slice
                        .chunks_exact(3)
                        .map(|chunk| Vec3::new(chunk[0], chunk[1], chunk[2]))
                        .collect(),
                )
            }
        };

        // Convert UVs (optional)
        let uvs = unsafe {
            if raw_data.uvs.is_null() || raw_data.uv_count == 0 {
                None
            } else {
                let slice = std::slice::from_raw_parts(raw_data.uvs, raw_data.uv_count * 2);
                Some(
                    slice
                        .chunks_exact(2)
                        .map(|chunk| [chunk[0], chunk[1]])
                        .collect(),
                )
            }
        };

        // Convert transform (column-major f32[16] to Mat4)
        let transform = Mat4::from_cols_array(&raw_data.transform);

        Ok(UsdMeshData {
            path,
            vertices,
            indices,
            normals,
            uvs,
            transform,
        })
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

        // Convert path
        let path = unsafe {
            if raw_data.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw_data.path).to_string_lossy().into_owned()
            }
        };

        // Convert prototype paths
        let prototype_paths = unsafe {
            if raw_data.prototype_paths.is_null() || raw_data.prototype_count == 0 {
                Vec::new()
            } else {
                let ptrs =
                    std::slice::from_raw_parts(raw_data.prototype_paths, raw_data.prototype_count);
                ptrs.iter()
                    .map(|&ptr| {
                        if ptr.is_null() {
                            String::new()
                        } else {
                            CStr::from_ptr(ptr).to_string_lossy().into_owned()
                        }
                    })
                    .collect()
            }
        };

        // Convert transforms (flat f32 array to Vec<Mat4>)
        let transforms = unsafe {
            if raw_data.transforms.is_null() || raw_data.instance_count == 0 {
                Vec::new()
            } else {
                let slice =
                    std::slice::from_raw_parts(raw_data.transforms, raw_data.instance_count * 16);
                slice
                    .chunks_exact(16)
                    .map(|chunk| {
                        let mut arr = [0.0f32; 16];
                        arr.copy_from_slice(chunk);
                        Mat4::from_cols_array(&arr)
                    })
                    .collect()
            }
        };

        // Convert proto indices
        let proto_indices = unsafe {
            if raw_data.proto_indices.is_null() || raw_data.instance_count == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(raw_data.proto_indices, raw_data.instance_count).to_vec()
            }
        };

        Ok(UsdInstancerData {
            path,
            prototype_paths,
            transforms,
            proto_indices,
        })
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

    /// Get all instancers in the stage.
    pub fn instancers(&self) -> UsdBridgeResult<Vec<UsdInstancerData>> {
        let count = self.instancer_count()?;
        let mut instancers = Vec::with_capacity(count);
        for i in 0..count {
            instancers.push(self.get_instancer(i)?);
        }
        Ok(instancers)
    }

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
            emissive_color: [0.0, 0.0, 0.0],
            diffuse_texture: ptr::null(),
            roughness_texture: ptr::null(),
            metallic_texture: ptr::null(),
            normal_texture: ptr::null(),
            emissive_texture: ptr::null(),
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

        // Convert path
        let path = unsafe {
            if raw_data.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw_data.path).to_string_lossy().into_owned()
            }
        };

        // Helper to convert optional texture path
        let texture_path = |ptr: *const std::ffi::c_char| -> Option<String> {
            if ptr.is_null() {
                None
            } else {
                let s = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
        };

        Ok(UsdMaterialData {
            path,
            diffuse_color: Vec3::new(
                raw_data.diffuse_color[0],
                raw_data.diffuse_color[1],
                raw_data.diffuse_color[2],
            ),
            metallic: raw_data.metallic,
            roughness: raw_data.roughness,
            specular: raw_data.specular,
            opacity: raw_data.opacity,
            emissive_color: Vec3::new(
                raw_data.emissive_color[0],
                raw_data.emissive_color[1],
                raw_data.emissive_color[2],
            ),
            diffuse_texture: texture_path(raw_data.diffuse_texture),
            roughness_texture: texture_path(raw_data.roughness_texture),
            metallic_texture: texture_path(raw_data.metallic_texture),
            normal_texture: texture_path(raw_data.normal_texture),
            emissive_texture: texture_path(raw_data.emissive_texture),
            is_materialx: raw_data.is_materialx != 0,
        })
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

    /// Get all materials in the stage.
    pub fn materials(&self) -> UsdBridgeResult<Vec<UsdMaterialData>> {
        let count = self.material_count()?;
        let mut materials = Vec::with_capacity(count);
        for i in 0..count {
            materials.push(self.get_material(i)?);
        }
        Ok(materials)
    }

    /// Export the stage to a file.
    ///
    /// Format is determined by file extension: `.usda`, `.usdc`, or `.usd`.
    pub fn export<P: AsRef<Path>>(&self, path: P) -> UsdBridgeResult<()> {
        let path_str = path.as_ref().to_str().ok_or(UsdBridgeError::InvalidPath)?;
        let c_path = CString::new(path_str).map_err(|_| UsdBridgeError::InvalidPath)?;

        let result = unsafe { usd_bridge_export_stage(self.raw, c_path.as_ptr()) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(())
    }

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
        let c_path = CString::new(path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut raw_info = UsdBridgePrimInfoRaw {
            path: ptr::null(),
            type_name: ptr::null(),
            is_active: 0,
            has_children: 0,
            child_count: 0,
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
        let c_path = CString::new(parent_path).map_err(|_| UsdBridgeError::InvalidPath)?;

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

    /// Helper to convert raw prim info to Rust type.
    fn convert_prim_info(raw: &UsdBridgePrimInfoRaw) -> UsdBridgeResult<UsdPrimInfo> {
        let path = unsafe {
            if raw.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw.path).to_string_lossy().into_owned()
            }
        };

        let type_name = unsafe {
            if raw.type_name.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw.type_name).to_string_lossy().into_owned()
            }
        };

        Ok(UsdPrimInfo {
            path,
            type_name,
            is_active: raw.is_active != 0,
            has_children: raw.has_children != 0,
            child_count: raw.child_count,
        })
    }
}

impl Drop for UsdStage {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                usd_bridge_close_stage(self.raw);
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        // Verify error conversion doesn't panic
        let _ = UsdBridgeError::from(UsdBridgeErrorCode::NullPointer);
        let _ = UsdBridgeError::from(UsdBridgeErrorCode::FileNotFound);
        let _ = UsdBridgeError::from(UsdBridgeErrorCode::InvalidStage);
    }

    // Integration tests require USD to be installed
    // Run with: cargo test --features usd-integration-tests
}
