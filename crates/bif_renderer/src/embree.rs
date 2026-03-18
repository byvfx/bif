// ! Embree 4 integration for high-performance ray tracing with two-level BVH.
//!
//! Manual FFI bindings to Intel Embree 4 library, avoiding bindgen dependency.
//! Only includes the minimal API needed for instanced geometry rendering.

use crate::{
    hittable::{HitRecord, Hittable},
    openpbr::OpenPbrSurface,
    Ray,
};
use bif_math::{Aabb, Interval, Mat3, Mat4, Vec3};
use rayon::prelude::*;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during Embree scene creation.
#[derive(Debug, Error)]
pub enum EmbreeError {
    #[error("Embree device creation failed - ensure embree4.dll is in PATH")]
    DeviceCreation,
    #[error("Embree device error: code {0}")]
    DeviceError(i32),
    #[error("Scene creation failed")]
    SceneCreation,
    #[error("Geometry creation failed")]
    GeometryCreation,
    #[error("Buffer setup failed: {0}")]
    BufferSetup(String),
    #[error("No materials provided - at least one material required")]
    NoMaterials,
}

// ============================================================================
// Embree FFI - imported from shared module
// ============================================================================

use crate::embree_ffi::*;

// ============================================================================
// Helper Functions
// ============================================================================

impl RTCRayHit {
    fn from_ray(ray: &Ray, ray_t: Interval) -> Self {
        Self {
            ray: RTCRay {
                org_x: ray.origin().x,
                org_y: ray.origin().y,
                org_z: ray.origin().z,
                tnear: ray_t.min,

                dir_x: ray.direction().x,
                dir_y: ray.direction().y,
                dir_z: ray.direction().z,
                time: ray.time(),

                tfar: ray_t.max,
                mask: 0xFFFFFFFF,
                id: 0,
                flags: 0,
            },
            hit: RTCHit {
                ng_x: 0.0,
                ng_y: 0.0,
                ng_z: 0.0,
                u: 0.0,
                v: 0.0,
                prim_id: RTC_INVALID_GEOMETRY_ID,
                geom_id: RTC_INVALID_GEOMETRY_ID,
                inst_id: [RTC_INVALID_GEOMETRY_ID],
            },
        }
    }
}

// ============================================================================
// EmbreeScene - Two-Level BVH for Instanced Geometry
// ============================================================================

/// High-performance instanced geometry using Intel Embree.
///
/// Uses two-level BVH:
/// - Top level: Instance transforms (O(log I) where I = instance count)
/// - Bottom level: Prototype mesh triangles (O(log P) where P = primitive count)
///
/// Performance: O(log I + log P) vs O(I × log P) for instance-aware BVH
///
/// # Example
/// ```ignore
/// let vertices = mesh.extract_triangle_vertices();
/// let uvs = mesh.extract_triangle_uvs();
/// let normals = mesh.extract_triangle_normals();
/// let transforms = vec![Mat4::IDENTITY; 1000];
/// let materials = vec![Arc::new(OpenPbrSurface::default())];
/// let tri_mat_ids = vec![0u32; vertices.len()];
///
/// let scene = EmbreeScene::new(&vertices, &uvs, &normals, transforms, materials, &tri_mat_ids);
/// ```
/// Subdivision surface data for Embree geometry creation.
pub struct SubdivData<'a> {
    /// Face vertex counts (polygon sizes)
    pub face_vertex_counts: &'a [i32],
    /// Polygon vertex indices (not triangulated)
    pub polygon_indices: &'a [i32],
    /// Crease edge vertex index pairs
    pub crease_indices: &'a [i32],
    /// Crease chain lengths
    pub crease_lengths: &'a [i32],
    /// Crease sharpnesses (one per chain)
    pub crease_sharpnesses: &'a [f32],
}

pub struct EmbreeScene {
    device: RTCDevice,
    scene: RTCScene,
    prototype_scene: RTCScene, // Must stay alive while instances reference it!
    materials: Vec<Arc<OpenPbrSurface>>,
    triangle_material_ids: Vec<u32>,

    // Keep vertex, index, and transform data alive (Embree holds pointers to this)
    _vertex_data: Vec<f32>,
    _index_data: Vec<u32>,
    _transform_data: Vec<[f32; 16]>,

    // Keep subdivision data alive (Embree holds pointers)
    _face_data: Vec<u32>,
    _subd_index_data: Vec<u32>,
    _crease_index_data: Vec<u32>,
    _crease_weight_data: Vec<f32>,

    // Per-triangle UV, normal, and tangent data for interpolation (3 entries per triangle).
    // Used by the old `new()` path. Empty when using indexed path.
    uv_data: Vec<[f32; 2]>,
    normal_data: Vec<[f32; 3]>,
    tangent_data: Vec<[f32; 3]>,

    // Per-vertex data for indexed path (lookup via _index_data in hit()).
    // When non-empty, these are used instead of per-triangle uv_data/normal_data.
    per_vertex_uvs: Vec<[f32; 2]>,
    per_vertex_normals: Vec<[f32; 3]>,

    // Per-instance inverse-transpose Mat3 for correct normal transformation
    normal_matrices: Vec<Mat3>,

    // For debugging/stats
    instance_count: usize,
    triangle_count: usize,
}

impl EmbreeScene {
    /// Try to create Embree scene, returns None if Embree unavailable or error occurs.
    ///
    /// Prefer `try_from_indexed()` for better perf with shared vertices.
    pub fn try_new(
        vertices: &[[Vec3; 3]],
        uvs: &[[[f32; 2]; 3]],
        normals: &[[[f32; 3]; 3]],
        transforms: Vec<Mat4>,
        materials: Vec<Arc<OpenPbrSurface>>,
        triangle_material_ids: &[u32],
    ) -> Option<Self> {
        match Self::new(
            vertices,
            uvs,
            normals,
            transforms,
            materials,
            triangle_material_ids,
        ) {
            Ok(scene) => Some(scene),
            Err(e) => {
                log::warn!("Embree scene creation failed: {}", e);
                None
            }
        }
    }

    /// Create Embree scene with instanced geometry.
    ///
    /// # Arguments
    /// * `vertices` - Triangle vertices as flat array of Vec3 triplets
    /// * `uvs` - Per-triangle UV coordinates (3 UVs per triangle)
    /// * `normals` - Per-triangle vertex normals (3 normals per triangle)
    /// * `transforms` - Instance transforms (local-to-world matrices)
    /// * `materials` - Materials for the scene (indexed by triangle_material_ids)
    /// * `triangle_material_ids` - Per-triangle material index into materials vec
    ///
    /// # Errors
    /// Returns `EmbreeError` if device/scene creation fails or materials is empty.
    ///
    /// Prefer `from_indexed()` for better perf with shared vertices.
    pub fn new(
        vertices: &[[Vec3; 3]],
        uvs: &[[[f32; 2]; 3]],
        normals: &[[[f32; 3]; 3]],
        transforms: Vec<Mat4>,
        materials: Vec<Arc<OpenPbrSurface>>,
        triangle_material_ids: &[u32],
    ) -> Result<Self, EmbreeError> {
        let total_start = Instant::now();

        if materials.is_empty() {
            return Err(EmbreeError::NoMaterials);
        }

        unsafe {
            // 1. Create Embree device
            let t0 = Instant::now();
            let device = rtcNewDevice(std::ptr::null());
            if device.is_null() {
                return Err(EmbreeError::DeviceCreation);
            }
            let err = rtcGetDeviceError(device);
            if err != 0 {
                rtcReleaseDevice(device);
                return Err(EmbreeError::DeviceError(err));
            }
            let device_ms = t0.elapsed().as_secs_f64() * 1000.0;

            // 2. Create scene for prototype mesh
            let prototype_scene = rtcNewScene(device);
            if prototype_scene.is_null() {
                rtcReleaseDevice(device);
                return Err(EmbreeError::SceneCreation);
            }

            // 3. Flatten triangles into vertex + index arrays (unindexed path)
            let t0 = Instant::now();
            let mut vertex_data = Vec::with_capacity(vertices.len() * 12);
            let mut index_data = Vec::with_capacity(vertices.len() * 3);

            for (tri_idx, tri) in vertices.iter().enumerate() {
                vertex_data.extend_from_slice(&[tri[0].x, tri[0].y, tri[0].z, 0.0]);
                vertex_data.extend_from_slice(&[tri[1].x, tri[1].y, tri[1].z, 0.0]);
                vertex_data.extend_from_slice(&[tri[2].x, tri[2].y, tri[2].z, 0.0]);
                let base_idx = (tri_idx * 3) as u32;
                index_data.push(base_idx);
                index_data.push(base_idx + 1);
                index_data.push(base_idx + 2);
            }
            let flatten_ms = t0.elapsed().as_secs_f64() * 1000.0;

            // 4. Create triangle mesh geometry
            let t0 = Instant::now();
            let geom = rtcNewGeometry(device, RTCGeometryType::Triangle);
            if geom.is_null() {
                rtcReleaseScene(prototype_scene);
                rtcReleaseDevice(device);
                return Err(EmbreeError::GeometryCreation);
            }

            rtcSetSharedGeometryBuffer(
                geom,
                RTCBufferType::Vertex as u32,
                0,
                RTCFormat::Float3 as u32,
                vertex_data.as_ptr() as *const std::ffi::c_void,
                0,
                16,
                vertex_data.len() / 4,
            );

            let err = rtcGetDeviceError(device);
            if err != 0 {
                rtcReleaseGeometry(geom);
                rtcReleaseScene(prototype_scene);
                rtcReleaseDevice(device);
                return Err(EmbreeError::BufferSetup(format!(
                    "vertex buffer: error {}",
                    err
                )));
            }

            rtcSetSharedGeometryBuffer(
                geom,
                RTCBufferType::Index as u32,
                0,
                RTCFormat::UInt3 as u32,
                index_data.as_ptr() as *const std::ffi::c_void,
                0,
                12,
                vertices.len(),
            );

            let err = rtcGetDeviceError(device);
            if err != 0 {
                rtcReleaseGeometry(geom);
                rtcReleaseScene(prototype_scene);
                rtcReleaseDevice(device);
                return Err(EmbreeError::BufferSetup(format!(
                    "index buffer: error {}",
                    err
                )));
            }

            rtcCommitGeometry(geom);
            rtcAttachGeometry(prototype_scene, geom);

            let err = rtcGetDeviceError(device);
            if err != 0 {
                let err_msg = match err {
                    1 => "RTC_ERROR_UNKNOWN",
                    2 => "RTC_ERROR_INVALID_ARGUMENT",
                    3 => "RTC_ERROR_INVALID_OPERATION",
                    4 => "RTC_ERROR_OUT_OF_MEMORY",
                    5 => "RTC_ERROR_UNSUPPORTED_CPU",
                    6 => "RTC_ERROR_CANCELLED",
                    _ => "UNKNOWN_ERROR",
                };
                rtcReleaseScene(prototype_scene);
                rtcReleaseDevice(device);
                return Err(EmbreeError::BufferSetup(format!(
                    "attach geometry: {} ({}), verts={}, tris={}",
                    err,
                    err_msg,
                    vertices.len() * 3,
                    vertices.len()
                )));
            }

            rtcReleaseGeometry(geom);
            rtcCommitScene(prototype_scene);
            let bvh_ms = t0.elapsed().as_secs_f64() * 1000.0;

            // 5. Create top-level scene with instances
            let t0 = Instant::now();
            let scene = rtcNewScene(device);
            if scene.is_null() {
                rtcReleaseScene(prototype_scene);
                rtcReleaseDevice(device);
                return Err(EmbreeError::SceneCreation);
            }

            let transform_data: Vec<[f32; 16]> =
                transforms.iter().map(|t| t.to_cols_array()).collect();

            for transform_array in &transform_data {
                let inst_geom = rtcNewGeometry(device, RTCGeometryType::Instance);
                if inst_geom.is_null() {
                    log::warn!("Failed to create instance geometry");
                    continue;
                }
                rtcSetGeometryInstancedScene(inst_geom, prototype_scene);
                rtcSetGeometryTransform(
                    inst_geom,
                    0,
                    RTCFormat::Float4x4ColumnMajor as u32,
                    transform_array.as_ptr(),
                );
                rtcCommitGeometry(inst_geom);
                rtcAttachGeometry(scene, inst_geom);
                rtcReleaseGeometry(inst_geom);
            }

            rtcCommitScene(scene);
            let instance_ms = t0.elapsed().as_secs_f64() * 1000.0;

            // 6. Build per-triangle hit data
            let t0 = Instant::now();
            let mut uv_data = Vec::with_capacity(vertices.len() * 3);
            let mut normal_data = Vec::with_capacity(vertices.len() * 3);
            for tri_idx in 0..vertices.len() {
                if tri_idx < uvs.len() {
                    uv_data.push(uvs[tri_idx][0]);
                    uv_data.push(uvs[tri_idx][1]);
                    uv_data.push(uvs[tri_idx][2]);
                } else {
                    uv_data.push([0.0, 0.0]);
                    uv_data.push([0.0, 0.0]);
                    uv_data.push([0.0, 0.0]);
                }
                if tri_idx < normals.len() {
                    normal_data.push(normals[tri_idx][0]);
                    normal_data.push(normals[tri_idx][1]);
                    normal_data.push(normals[tri_idx][2]);
                } else {
                    normal_data.push([0.0, 1.0, 0.0]);
                    normal_data.push([0.0, 1.0, 0.0]);
                    normal_data.push([0.0, 1.0, 0.0]);
                }
            }

            let mut tangent_data = Vec::with_capacity(vertices.len());
            for tri_idx in 0..vertices.len() {
                let uv0 = if tri_idx < uvs.len() {
                    uvs[tri_idx][0]
                } else {
                    [0.0, 0.0]
                };
                let uv1 = if tri_idx < uvs.len() {
                    uvs[tri_idx][1]
                } else {
                    [1.0, 0.0]
                };
                let uv2 = if tri_idx < uvs.len() {
                    uvs[tri_idx][2]
                } else {
                    [0.0, 1.0]
                };

                let edge1 = vertices[tri_idx][1] - vertices[tri_idx][0];
                let edge2 = vertices[tri_idx][2] - vertices[tri_idx][0];
                let duv1 = [uv1[0] - uv0[0], uv1[1] - uv0[1]];
                let duv2 = [uv2[0] - uv0[0], uv2[1] - uv0[1]];

                let det = duv1[0] * duv2[1] - duv2[0] * duv1[1];
                let tangent = if det.abs() > 1e-8 {
                    let r = 1.0 / det;
                    let t = (edge1 * duv2[1] - edge2 * duv1[1]) * r;
                    let len = t.length();
                    if len > 1e-8 {
                        (t / len).into()
                    } else {
                        [1.0, 0.0, 0.0]
                    }
                } else {
                    [1.0, 0.0, 0.0]
                };

                tangent_data.push(tangent);
            }
            let hitdata_ms = t0.elapsed().as_secs_f64() * 1000.0;

            let tri_mat_ids = if triangle_material_ids.is_empty() {
                vec![0u32; vertices.len()]
            } else {
                triangle_material_ids.to_vec()
            };

            let t0 = Instant::now();
            let normal_matrices: Vec<Mat3> = transforms
                .iter()
                .map(|t| Mat3::from_mat4(*t).inverse().transpose())
                .collect();
            let normat_ms = t0.elapsed().as_secs_f64() * 1000.0;

            let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;
            log::info!(
                "EmbreeScene::new() timing: total={:.1}ms (device={:.1}, flatten={:.1}, bvh={:.1}, instances={:.1}, hitdata={:.1}, normats={:.1}) | {} tris, {} instances, {} verts(unindexed)",
                total_ms, device_ms, flatten_ms, bvh_ms, instance_ms, hitdata_ms, normat_ms,
                vertices.len(), transforms.len(), vertex_data.len() / 4
            );

            Ok(Self {
                device,
                scene,
                prototype_scene,
                materials,
                triangle_material_ids: tri_mat_ids,
                _vertex_data: vertex_data,
                _index_data: index_data,
                _transform_data: transform_data,
                _face_data: vec![],
                _subd_index_data: vec![],
                _crease_index_data: vec![],
                _crease_weight_data: vec![],
                uv_data,
                normal_data,
                tangent_data,
                per_vertex_uvs: vec![],
                per_vertex_normals: vec![],
                normal_matrices,
                instance_count: transforms.len(),
                triangle_count: vertices.len(),
            })
        }
    }

    /// Create Embree scene from indexed mesh data (shared vertices).
    ///
    /// Much faster than `new()` for meshes with shared vertices — avoids
    /// unindexing and feeds Embree the original index buffer directly.
    /// Per-triangle hit data is built in parallel via rayon.
    ///
    /// # Arguments
    /// * `positions` - Shared vertex positions (vertex_count entries)
    /// * `normals` - Per-vertex normals (vertex_count entries)
    /// * `uvs` - Per-vertex UVs (vertex_count entries)
    /// * `indices` - Triangle indices (tri_count * 3 entries)
    /// * `transforms` - Instance transforms
    /// * `materials` - Materials indexed by triangle_material_ids
    /// * `triangle_material_ids` - Per-triangle material index
    /// * `subd` - Optional subdivision surface data
    #[allow(clippy::too_many_arguments)]
    pub fn from_indexed(
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        transforms: Vec<Mat4>,
        materials: Vec<Arc<OpenPbrSurface>>,
        triangle_material_ids: &[u32],
        subd: Option<&SubdivData<'_>>,
    ) -> Result<Self, EmbreeError> {
        let total_start = Instant::now();

        if materials.is_empty() {
            return Err(EmbreeError::NoMaterials);
        }

        let tri_count = indices.len() / 3;

        // Validate index buffer bounds upfront
        if let Some(&max_idx) = indices.iter().max() {
            let max = max_idx as usize;
            if max >= positions.len() {
                log::warn!(
                    "Index buffer contains index {} but only {} positions — OOB entries will use defaults",
                    max, positions.len()
                );
            }
            if max >= normals.len() {
                log::warn!(
                    "Index buffer contains index {} but only {} normals — OOB entries will use defaults",
                    max, normals.len()
                );
            }
            if max >= uvs.len() {
                log::warn!(
                    "Index buffer contains index {} but only {} UVs — OOB entries will use defaults",
                    max, uvs.len()
                );
            }
        }

        unsafe {
            // 1. Device
            let t0 = Instant::now();
            let device = rtcNewDevice(std::ptr::null());
            if device.is_null() {
                return Err(EmbreeError::DeviceCreation);
            }
            let err = rtcGetDeviceError(device);
            if err != 0 {
                rtcReleaseDevice(device);
                return Err(EmbreeError::DeviceError(err));
            }
            let device_ms = t0.elapsed().as_secs_f64() * 1000.0;

            // 2. Prototype scene
            let prototype_scene = rtcNewScene(device);
            if prototype_scene.is_null() {
                rtcReleaseDevice(device);
                return Err(EmbreeError::SceneCreation);
            }

            // 3. Pad positions to 16-byte stride (4 floats per vertex)
            let t0 = Instant::now();
            let mut vertex_data = Vec::with_capacity(positions.len() * 4);
            for pos in positions {
                vertex_data.extend_from_slice(&[pos[0], pos[1], pos[2], 0.0]);
            }
            let index_data = indices.to_vec();
            let pad_ms = t0.elapsed().as_secs_f64() * 1000.0;

            // 4. Geometry + BVH build
            let t0 = Instant::now();
            // Subdivision data kept alive for Embree pointers
            let mut _face_data: Vec<u32> = Vec::new();
            let mut _subd_index_data: Vec<u32> = Vec::new();
            let mut _crease_index_data: Vec<u32> = Vec::new();
            let mut _crease_weight_data: Vec<f32> = Vec::new();

            let geom = if let Some(sd) = subd {
                // Subdivision geometry — Embree evaluates limit surface
                let geom = rtcNewGeometry(device, RTCGeometryType::Subdivision);
                if geom.is_null() {
                    rtcReleaseScene(prototype_scene);
                    rtcReleaseDevice(device);
                    return Err(EmbreeError::GeometryCreation);
                }

                // Vertex buffer (same as triangle path)
                rtcSetSharedGeometryBuffer(
                    geom,
                    RTCBufferType::Vertex as u32,
                    0,
                    RTCFormat::Float3 as u32,
                    vertex_data.as_ptr() as *const std::ffi::c_void,
                    0,
                    16,
                    positions.len(),
                );

                // Face buffer (number of vertices per face)
                _face_data = sd.face_vertex_counts.iter().map(|&c| c as u32).collect();
                rtcSetSharedGeometryBuffer(
                    geom,
                    RTCBufferType::Face as u32,
                    0,
                    RTCFormat::UInt as u32,
                    _face_data.as_ptr() as *const std::ffi::c_void,
                    0,
                    4,
                    _face_data.len(),
                );

                // Index buffer (polygon vertex indices, NOT triangulated)
                // Stored in _subd_index_data to keep alive for Embree pointer
                _subd_index_data = sd.polygon_indices.iter().map(|&i| i as u32).collect();
                rtcSetSharedGeometryBuffer(
                    geom,
                    RTCBufferType::Index as u32,
                    0,
                    RTCFormat::UInt as u32,
                    _subd_index_data.as_ptr() as *const std::ffi::c_void,
                    0,
                    4,
                    _subd_index_data.len(),
                );

                // Crease edges (optional)
                if !sd.crease_indices.is_empty() && !sd.crease_sharpnesses.is_empty() {
                    _crease_index_data = sd.crease_indices.iter().map(|&i| i as u32).collect();
                    _crease_weight_data = sd.crease_sharpnesses.to_vec();

                    rtcSetSharedGeometryBuffer(
                        geom,
                        RTCBufferType::EdgeCreaseIndex as u32,
                        0,
                        RTCFormat::UInt as u32,
                        _crease_index_data.as_ptr() as *const std::ffi::c_void,
                        0,
                        8, // pairs of u32
                        _crease_index_data.len() / 2,
                    );

                    rtcSetSharedGeometryBuffer(
                        geom,
                        RTCBufferType::EdgeCreaseWeight as u32,
                        0,
                        RTCFormat::Float as u32,
                        _crease_weight_data.as_ptr() as *const std::ffi::c_void,
                        0,
                        4,
                        _crease_weight_data.len(),
                    );
                }

                // Set Catmull-Clark mode with pin-corners boundary
                rtcSetGeometrySubdivisionMode(geom, 0, RTCSubdivisionMode::PinCorners);

                log::info!(
                    "Embree: subdivision geometry with {} faces, {} polygon indices, {} creases",
                    _face_data.len(),
                    sd.polygon_indices.len(),
                    _crease_weight_data.len()
                );

                geom
            } else {
                // Triangle geometry (default path)
                let geom = rtcNewGeometry(device, RTCGeometryType::Triangle);
                if geom.is_null() {
                    rtcReleaseScene(prototype_scene);
                    rtcReleaseDevice(device);
                    return Err(EmbreeError::GeometryCreation);
                }

                rtcSetSharedGeometryBuffer(
                    geom,
                    RTCBufferType::Vertex as u32,
                    0,
                    RTCFormat::Float3 as u32,
                    vertex_data.as_ptr() as *const std::ffi::c_void,
                    0,
                    16,
                    positions.len(),
                );

                let err = rtcGetDeviceError(device);
                if err != 0 {
                    rtcReleaseGeometry(geom);
                    rtcReleaseScene(prototype_scene);
                    rtcReleaseDevice(device);
                    return Err(EmbreeError::BufferSetup(format!(
                        "vertex buffer: error {}",
                        err
                    )));
                }

                rtcSetSharedGeometryBuffer(
                    geom,
                    RTCBufferType::Index as u32,
                    0,
                    RTCFormat::UInt3 as u32,
                    index_data.as_ptr() as *const std::ffi::c_void,
                    0,
                    12,
                    tri_count,
                );

                let err = rtcGetDeviceError(device);
                if err != 0 {
                    rtcReleaseGeometry(geom);
                    rtcReleaseScene(prototype_scene);
                    rtcReleaseDevice(device);
                    return Err(EmbreeError::BufferSetup(format!(
                        "index buffer: error {}",
                        err
                    )));
                }

                geom
            };

            rtcCommitGeometry(geom);
            rtcAttachGeometry(prototype_scene, geom);

            let err = rtcGetDeviceError(device);
            if err != 0 {
                let err_msg = match err {
                    1 => "RTC_ERROR_UNKNOWN",
                    2 => "RTC_ERROR_INVALID_ARGUMENT",
                    3 => "RTC_ERROR_INVALID_OPERATION",
                    4 => "RTC_ERROR_OUT_OF_MEMORY",
                    5 => "RTC_ERROR_UNSUPPORTED_CPU",
                    6 => "RTC_ERROR_CANCELLED",
                    _ => "UNKNOWN_ERROR",
                };
                rtcReleaseScene(prototype_scene);
                rtcReleaseDevice(device);
                return Err(EmbreeError::BufferSetup(format!(
                    "attach geometry: {} ({}), verts={}, tris={}",
                    err,
                    err_msg,
                    positions.len(),
                    tri_count
                )));
            }

            rtcReleaseGeometry(geom);
            rtcCommitScene(prototype_scene);
            let bvh_ms = t0.elapsed().as_secs_f64() * 1000.0;

            // 5. Instances
            let t0 = Instant::now();
            let scene = rtcNewScene(device);
            if scene.is_null() {
                rtcReleaseScene(prototype_scene);
                rtcReleaseDevice(device);
                return Err(EmbreeError::SceneCreation);
            }

            let transform_data: Vec<[f32; 16]> =
                transforms.iter().map(|t| t.to_cols_array()).collect();

            for transform_array in &transform_data {
                let inst_geom = rtcNewGeometry(device, RTCGeometryType::Instance);
                if inst_geom.is_null() {
                    log::warn!("Failed to create instance geometry");
                    continue;
                }
                rtcSetGeometryInstancedScene(inst_geom, prototype_scene);
                rtcSetGeometryTransform(
                    inst_geom,
                    0,
                    RTCFormat::Float4x4ColumnMajor as u32,
                    transform_array.as_ptr(),
                );
                rtcCommitGeometry(inst_geom);
                rtcAttachGeometry(scene, inst_geom);
                rtcReleaseGeometry(inst_geom);
            }

            rtcCommitScene(scene);
            let instance_ms = t0.elapsed().as_secs_f64() * 1000.0;

            // 6. Build hit data in parallel via rayon
            let t0 = Instant::now();

            // Store per-vertex UV/normal data directly (indexed lookup in hit()).
            // Saves ~6x memory vs expanding to per-triangle for high-sharing meshes.
            let per_vertex_uvs = uvs.to_vec();
            let per_vertex_normals = normals.to_vec();

            // Tangent data: 1 per triangle, computed from edges + UVs (must be per-triangle)
            let tangent_data: Vec<[f32; 3]> = (0..tri_count)
                .into_par_iter()
                .map(|tri| {
                    let i0 = indices[tri * 3] as usize;
                    let i1 = indices[tri * 3 + 1] as usize;
                    let i2 = indices[tri * 3 + 2] as usize;

                    let p0 = if i0 < positions.len() {
                        positions[i0]
                    } else {
                        [0.0; 3]
                    };
                    let p1 = if i1 < positions.len() {
                        positions[i1]
                    } else {
                        [0.0; 3]
                    };
                    let p2 = if i2 < positions.len() {
                        positions[i2]
                    } else {
                        [0.0; 3]
                    };

                    let uv0 = if i0 < uvs.len() { uvs[i0] } else { [0.0, 0.0] };
                    let uv1 = if i1 < uvs.len() { uvs[i1] } else { [1.0, 0.0] };
                    let uv2 = if i2 < uvs.len() { uvs[i2] } else { [0.0, 1.0] };

                    let edge1 = Vec3::new(p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]);
                    let edge2 = Vec3::new(p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]);
                    let duv1 = [uv1[0] - uv0[0], uv1[1] - uv0[1]];
                    let duv2 = [uv2[0] - uv0[0], uv2[1] - uv0[1]];

                    let det = duv1[0] * duv2[1] - duv2[0] * duv1[1];
                    if det.abs() > 1e-8 {
                        let r = 1.0 / det;
                        let t = (edge1 * duv2[1] - edge2 * duv1[1]) * r;
                        let len = t.length();
                        if len > 1e-8 {
                            (t / len).into()
                        } else {
                            [1.0, 0.0, 0.0]
                        }
                    } else {
                        [1.0, 0.0, 0.0]
                    }
                })
                .collect();
            let hitdata_ms = t0.elapsed().as_secs_f64() * 1000.0;

            let tri_mat_ids = if triangle_material_ids.is_empty() {
                vec![0u32; tri_count]
            } else {
                triangle_material_ids.to_vec()
            };

            // 7. Normal matrices (parallel)
            let t0 = Instant::now();
            let normal_matrices: Vec<Mat3> = transforms
                .par_iter()
                .map(|t| Mat3::from_mat4(*t).inverse().transpose())
                .collect();
            let normat_ms = t0.elapsed().as_secs_f64() * 1000.0;

            let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;
            log::info!(
                "EmbreeScene::from_indexed() timing: total={:.1}ms (device={:.1}, pad={:.1}, bvh={:.1}, instances={:.1}, hitdata={:.1}, normats={:.1}) | {} tris, {} instances, {} shared verts",
                total_ms, device_ms, pad_ms, bvh_ms, instance_ms, hitdata_ms, normat_ms,
                tri_count, transforms.len(), positions.len()
            );

            Ok(Self {
                device,
                scene,
                prototype_scene,
                materials,
                triangle_material_ids: tri_mat_ids,
                _vertex_data: vertex_data,
                _index_data: index_data,
                _transform_data: transform_data,
                _face_data,
                _subd_index_data,
                _crease_index_data,
                _crease_weight_data,
                uv_data: vec![],
                normal_data: vec![],
                tangent_data,
                per_vertex_uvs,
                per_vertex_normals,
                normal_matrices,
                instance_count: transforms.len(),
                triangle_count: tri_count,
            })
        }
    }

    /// Try to create Embree scene from indexed mesh data, returns None on error.
    #[allow(clippy::too_many_arguments)]
    pub fn try_from_indexed(
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        transforms: Vec<Mat4>,
        materials: Vec<Arc<OpenPbrSurface>>,
        triangle_material_ids: &[u32],
        subd: Option<&SubdivData<'_>>,
    ) -> Option<Self> {
        match Self::from_indexed(
            positions,
            normals,
            uvs,
            indices,
            transforms,
            materials,
            triangle_material_ids,
            subd,
        ) {
            Ok(scene) => Some(scene),
            Err(e) => {
                log::warn!("Embree scene creation failed: {}", e);
                None
            }
        }
    }

    /// Get instance count
    pub fn instance_count(&self) -> usize {
        self.instance_count
    }

    /// Get triangle count
    pub fn triangle_count(&self) -> usize {
        self.triangle_count
    }

    /// Update instance transforms without rebuilding geometry BVH.
    ///
    /// Uses `rtcSetGeometryTransform` + `rtcCommitScene` for a fast refit
    /// when only transforms changed (e.g. after Xform edits or animation).
    /// Returns `true` if the update was applied.
    pub fn update_transforms(&mut self, transforms: &[Mat4]) -> bool {
        if transforms.len() != self.instance_count {
            log::warn!(
                "update_transforms: count mismatch ({} vs {}), skipping",
                transforms.len(),
                self.instance_count
            );
            return false;
        }

        let new_data: Vec<[f32; 16]> = transforms.iter().map(|t| t.to_cols_array()).collect();

        unsafe {
            for (idx, transform_array) in new_data.iter().enumerate() {
                let geom = rtcGetGeometry(self.scene, idx as u32);
                if geom.is_null() {
                    continue;
                }
                rtcSetGeometryTransform(
                    geom,
                    0,
                    RTCFormat::Float4x4ColumnMajor as u32,
                    transform_array.as_ptr(),
                );
                rtcCommitGeometry(geom);
            }
            rtcCommitScene(self.scene);
        }

        self._transform_data = new_data;

        // Recompute normal matrices for correct shading normals
        self.normal_matrices = transforms
            .iter()
            .map(|t| Mat3::from_mat4(*t).inverse().transpose())
            .collect();

        true
    }
}

impl Hittable for EmbreeScene {
    fn hit<'a>(&'a self, ray: &Ray, ray_t: Interval, rec: &mut HitRecord<'a>) -> bool {
        unsafe {
            // 1. Convert to Embree ray-hit
            let mut rayhit = RTCRayHit::from_ray(ray, ray_t);

            // 2. Trace ray (Embree 4 API: scene, rayhit, args=NULL)
            rtcIntersect1(self.scene, &mut rayhit, std::ptr::null());

            // 3. Check if hit
            if rayhit.hit.geom_id == RTC_INVALID_GEOMETRY_ID {
                #[cfg(debug_assertions)]
                {
                    static MISS_COUNT: AtomicU32 = AtomicU32::new(0);
                    let count = MISS_COUNT.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
                    if count <= 5 {
                        log::debug!(
                            "Ray miss #{}: origin=({}, {}, {}), dir=({}, {}, {}), tfar={}",
                            count,
                            rayhit.ray.org_x,
                            rayhit.ray.org_y,
                            rayhit.ray.org_z,
                            rayhit.ray.dir_x,
                            rayhit.ray.dir_y,
                            rayhit.ray.dir_z,
                            rayhit.ray.tfar
                        );
                    }
                }
                return false;
            }

            #[cfg(debug_assertions)]
            {
                static HIT_COUNT: AtomicU32 = AtomicU32::new(0);
                let count = HIT_COUNT.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
                if count <= 5 {
                    log::info!(
                        "Ray hit #{}: t={}, geom_id={}, prim_id={}, normal=({}, {}, {})",
                        count,
                        rayhit.ray.tfar,
                        rayhit.hit.geom_id,
                        rayhit.hit.prim_id,
                        rayhit.hit.ng_x,
                        rayhit.hit.ng_y,
                        rayhit.hit.ng_z
                    );
                }
            }

            // 4. Fill HitRecord
            rec.t = rayhit.ray.tfar;
            rec.p = ray.at(rec.t);

            // Interpolate UVs and normals using barycentrics
            let prim_id = rayhit.hit.prim_id as usize;
            let bary_u = rayhit.hit.u;
            let bary_v = rayhit.hit.v;
            let bary_w = 1.0 - bary_u - bary_v;

            // Look up per-vertex UVs and normals — indexed path uses per-vertex
            // arrays with index buffer lookup, old path uses pre-expanded per-triangle arrays.
            debug_assert!(
                prim_id < self.tangent_data.len(),
                "prim_id OOB on tangent_data"
            );
            let (uv0, uv1, uv2, n0, n1, n2) = if !self.per_vertex_uvs.is_empty() {
                // Indexed path: look up via index buffer
                let idx_base = prim_id * 3;
                let i0 = self._index_data[idx_base] as usize;
                let i1 = self._index_data[idx_base + 1] as usize;
                let i2 = self._index_data[idx_base + 2] as usize;
                (
                    self.per_vertex_uvs[i0],
                    self.per_vertex_uvs[i1],
                    self.per_vertex_uvs[i2],
                    self.per_vertex_normals[i0],
                    self.per_vertex_normals[i1],
                    self.per_vertex_normals[i2],
                )
            } else {
                // Old per-triangle path
                let base = prim_id * 3;
                debug_assert!(base + 2 < self.uv_data.len(), "prim_id OOB on uv_data");
                debug_assert!(
                    base + 2 < self.normal_data.len(),
                    "prim_id OOB on normal_data"
                );
                (
                    self.uv_data[base],
                    self.uv_data[base + 1],
                    self.uv_data[base + 2],
                    self.normal_data[base],
                    self.normal_data[base + 1],
                    self.normal_data[base + 2],
                )
            };

            // Interpolate texture UVs: w*uv0 + u*uv1 + v*uv2
            rec.u = bary_w * uv0[0] + bary_u * uv1[0] + bary_v * uv2[0];
            rec.v = bary_w * uv0[1] + bary_u * uv1[1] + bary_v * uv2[1];
            let interp_normal = Vec3::new(
                bary_w * n0[0] + bary_u * n1[0] + bary_v * n2[0],
                bary_w * n0[1] + bary_u * n1[1] + bary_v * n2[1],
                bary_w * n0[2] + bary_u * n1[2] + bary_v * n2[2],
            );

            // Transform normal to world space using per-instance inverse-transpose
            let inst_id = rayhit.hit.inst_id[0] as usize;
            let normal = if inst_id < self.normal_matrices.len() {
                (self.normal_matrices[inst_id] * interp_normal).normalize()
            } else {
                interp_normal.normalize()
            };
            rec.normal = normal;

            // Per-triangle tangent (constant across triangle, no interpolation needed)
            let raw_tangent = Vec3::from_array(self.tangent_data[prim_id]);
            // Gram-Schmidt orthogonalize tangent against normal
            let gs = raw_tangent - normal * normal.dot(raw_tangent);
            let (tangent, bitangent) = if gs.length_squared() > 1e-8 {
                let t = gs.normalize();
                (t, normal.cross(t))
            } else {
                bif_math::build_orthonormal_basis(normal)
            };
            rec.tangent = tangent;
            rec.bitangent = bitangent;

            // Per-triangle material lookup
            // Belt-and-suspenders: validated in new(), but check in debug builds
            debug_assert!(
                !self.materials.is_empty(),
                "materials should never be empty"
            );
            let mat_id = self.triangle_material_ids[prim_id] as usize;
            let mat_id = mat_id.min(self.materials.len().saturating_sub(1));
            rec.material = &*self.materials[mat_id];

            // Set front face
            rec.set_face_normal(ray, normal);

            // Flip bitangent for back-face hits to keep TBN consistent
            if !rec.front_face {
                rec.bitangent = -rec.bitangent;
            }

            true
        }
    }

    fn bounding_box(&self) -> Aabb {
        unsafe {
            let mut bounds = RTCBounds::default();
            rtcGetSceneBounds(self.scene, &mut bounds);

            Aabb::from_points(
                Vec3::new(bounds.lower_x, bounds.lower_y, bounds.lower_z),
                Vec3::new(bounds.upper_x, bounds.upper_y, bounds.upper_z),
            )
        }
    }
}

impl Drop for EmbreeScene {
    fn drop(&mut self) {
        log::debug!(
            "Releasing Embree scene: {} instances, {} triangles",
            self.instance_count,
            self.triangle_count
        );
        unsafe {
            rtcReleaseScene(self.scene);
            rtcReleaseScene(self.prototype_scene); // Release prototype after top-level scene
            rtcReleaseDevice(self.device);
        }
    }
}

// ============================================================================
// Safety Notes
// ============================================================================

// SAFETY: EmbreeScene is Send + Sync because:
// - Embree's RTCDevice/RTCScene are thread-safe after rtcCommitScene
// - We store vertex_data to keep it alive (Embree holds pointers)
// - Drop releases Embree resources before Rust data
unsafe impl Send for EmbreeScene {}
unsafe impl Sync for EmbreeScene {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openpbr::OpenPbrSurface;

    fn default_material() -> Vec<Arc<OpenPbrSurface>> {
        vec![Arc::new(OpenPbrSurface::default())]
    }

    #[test]
    fn test_from_indexed_basic_triangle() {
        // Single triangle on XZ plane at y=0
        let positions: Vec<[f32; 3]> = vec![[-1.0, 0.0, -1.0], [1.0, 0.0, -1.0], [0.0, 0.0, 1.0]];
        let normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; 3];
        let uvs: Vec<[f32; 2]> = vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
        let indices: Vec<u32> = vec![0, 1, 2];

        let scene = EmbreeScene::from_indexed(
            &positions,
            &normals,
            &uvs,
            &indices,
            vec![Mat4::IDENTITY],
            default_material(),
            &[0],
            None,
        )
        .expect("should build from indexed");

        // Ray from above pointing down — should hit
        let ray = Ray::new(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, -1.0, 0.0), 0.0);
        let mut rec = HitRecord::default();
        assert!(scene.hit(&ray, Interval::new(0.001, f32::INFINITY), &mut rec));
        assert!((rec.p.y).abs() < 0.01, "hit y={}, expected ~0", rec.p.y);

        // Ray from above pointing up — should miss
        let ray_miss = Ray::new(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, 1.0, 0.0), 0.0);
        assert!(!scene.hit(&ray_miss, Interval::new(0.001, f32::INFINITY), &mut rec));
    }

    #[test]
    fn test_from_indexed_shared_vertices() {
        // Two triangles sharing 2 vertices (a quad split into 2 tris)
        let positions: Vec<[f32; 3]> = vec![
            [-1.0, 0.0, -1.0], // 0
            [1.0, 0.0, -1.0],  // 1
            [1.0, 0.0, 1.0],   // 2
            [-1.0, 0.0, 1.0],  // 3
        ];
        let normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; 4];
        let uvs: Vec<[f32; 2]> = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let indices: Vec<u32> = vec![0, 1, 2, 0, 2, 3]; // 2 triangles, shared verts

        let scene = EmbreeScene::from_indexed(
            &positions,
            &normals,
            &uvs,
            &indices,
            vec![Mat4::IDENTITY],
            default_material(),
            &[0, 0],
            None,
        )
        .expect("should build shared-vertex quad");

        // Hit center of quad
        let ray = Ray::new(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, -1.0, 0.0), 0.0);
        let mut rec = HitRecord::default();
        assert!(scene.hit(&ray, Interval::new(0.001, f32::INFINITY), &mut rec));

        // UV should be interpolated correctly (~0.5, 0.5 at center)
        assert!((rec.u - 0.5).abs() < 0.2, "u={}, expected ~0.5", rec.u);
    }

    #[test]
    fn test_from_indexed_instanced() {
        // Triangle with two instances at different positions
        let positions: Vec<[f32; 3]> = vec![[-0.5, 0.0, -0.5], [0.5, 0.0, -0.5], [0.0, 0.0, 0.5]];
        let normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; 3];
        let uvs: Vec<[f32; 2]> = vec![[0.0, 0.0]; 3];
        let indices: Vec<u32> = vec![0, 1, 2];
        let transforms = vec![
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)),
        ];

        let scene = EmbreeScene::from_indexed(
            &positions,
            &normals,
            &uvs,
            &indices,
            transforms,
            default_material(),
            &[0],
            None,
        )
        .expect("should build instanced scene");

        // Hit instance 0 at origin
        let ray0 = Ray::new(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, -1.0, 0.0), 0.0);
        let mut rec = HitRecord::default();
        assert!(scene.hit(&ray0, Interval::new(0.001, f32::INFINITY), &mut rec));
        assert!(
            rec.p.x.abs() < 1.0,
            "hit should be near origin, got x={}",
            rec.p.x
        );

        // Hit instance 1 at x=10
        let ray1 = Ray::new(Vec3::new(10.0, 5.0, 0.0), Vec3::new(0.0, -1.0, 0.0), 0.0);
        assert!(scene.hit(&ray1, Interval::new(0.001, f32::INFINITY), &mut rec));
        assert!(
            (rec.p.x - 10.0).abs() < 1.0,
            "hit should be near x=10, got x={}",
            rec.p.x
        );

        // Miss between instances
        let ray_miss = Ray::new(Vec3::new(5.0, 5.0, 0.0), Vec3::new(0.0, -1.0, 0.0), 0.0);
        assert!(!scene.hit(&ray_miss, Interval::new(0.001, f32::INFINITY), &mut rec));
    }

    #[test]
    fn test_from_indexed_no_materials_error() {
        let positions: Vec<[f32; 3]> = vec![[0.0; 3]; 3];
        let normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; 3];
        let uvs: Vec<[f32; 2]> = vec![[0.0; 2]; 3];
        let indices: Vec<u32> = vec![0, 1, 2];

        let result = EmbreeScene::from_indexed(
            &positions,
            &normals,
            &uvs,
            &indices,
            vec![Mat4::IDENTITY],
            vec![], // empty materials
            &[0],
            None,
        );
        assert!(result.is_err());
    }
}
