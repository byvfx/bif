// ! Embree 4 integration for high-performance ray tracing with two-level BVH.
//!
//! Manual FFI bindings to Intel Embree 4 library, avoiding bindgen dependency.
//! Only includes the minimal API needed for instanced geometry rendering.

use bif_math::{Aabb, Interval, Mat4, Vec3};
use crate::{Ray, Material, hittable::{HitRecord, Hittable}};
use std::sync::Arc;

// ============================================================================
// Embree FFI Bindings
// ============================================================================

#[allow(non_camel_case_types)]
type RTCDevice = *mut std::ffi::c_void;

#[allow(non_camel_case_types)]
type RTCScene = *mut std::ffi::c_void;

#[allow(non_camel_case_types)]
type RTCGeometry = *mut std::ffi::c_void;

#[allow(non_camel_case_types)]
type RTCBuffer = *mut std::ffi::c_void;

// Embree geometry types (from rtcore_geometry.h)
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum RTCGeometryType {
    Triangle = 0,    // RTC_GEOMETRY_TYPE_TRIANGLE
    Instance = 121,  // RTC_GEOMETRY_TYPE_INSTANCE
}

// Embree buffer type
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum RTCBufferType {
    Index = 0,
    Vertex = 1,
    VertexAttribute = 2,
}

// Embree buffer format (from rtcore_common.h)
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum RTCFormat {
    Undefined = 0,
    UInt3 = 0x5003,    // RTC_FORMAT_UINT = 0x5001, +2 for UINT3
    Float3 = 0x9003,   // RTC_FORMAT_FLOAT = 0x9001, +2 for FLOAT3
    Float4x4ColumnMajor = 0x9244,  // For transforms
}

// Embree scene flags
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum RTCSceneFlags {
    None = 0,
    Robust = 1 << 0,
}

// Embree build quality
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum RTCBuildQuality {
    Low = 0,
    Medium = 1,
    High = 2,
}

// Ray structure matching Embree's RTCRay
#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
struct RTCRay {
    org_x: f32,
    org_y: f32,
    org_z: f32,
    tnear: f32,

    dir_x: f32,
    dir_y: f32,
    dir_z: f32,
    time: f32,

    tfar: f32,
    mask: u32,
    id: u32,
    flags: u32,
}

// Hit structure matching Embree's RTCHit
#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
struct RTCHit {
    ng_x: f32,
    ng_y: f32,
    ng_z: f32,

    u: f32,
    v: f32,

    prim_id: u32,
    geom_id: u32,
    inst_id: [u32; 1],
}

// Combined ray-hit structure for rtcIntersect1
#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
struct RTCRayHit {
    ray: RTCRay,
    hit: RTCHit,
}

// Bounds structure for rtcGetSceneBounds
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct RTCBounds {
    lower_x: f32,
    lower_y: f32,
    lower_z: f32,
    align0: f32,

    upper_x: f32,
    upper_y: f32,
    upper_z: f32,
    align1: f32,
}

// Invalid geometry ID constant
const RTC_INVALID_GEOMETRY_ID: u32 = 0xFFFFFFFF;

// Embree C API functions
#[link(name = "embree4")]
extern "C" {
    fn rtcNewDevice(config: *const std::ffi::c_char) -> RTCDevice;
    fn rtcReleaseDevice(device: RTCDevice);
    fn rtcGetDeviceError(device: RTCDevice) -> i32;

    fn rtcNewScene(device: RTCDevice) -> RTCScene;
    fn rtcReleaseScene(scene: RTCScene);
    fn rtcCommitScene(scene: RTCScene);
    fn rtcGetSceneBounds(scene: RTCScene, bounds: *mut RTCBounds);

    fn rtcNewGeometry(device: RTCDevice, geom_type: RTCGeometryType) -> RTCGeometry;
    fn rtcReleaseGeometry(geom: RTCGeometry);
    fn rtcCommitGeometry(geom: RTCGeometry);
    fn rtcAttachGeometry(scene: RTCScene, geom: RTCGeometry) -> u32;

    fn rtcSetSharedGeometryBuffer(
        geom: RTCGeometry,
        buffer_type: u32,
        slot: u32,
        format: u32,
        ptr: *const std::ffi::c_void,
        byte_offset: usize,
        byte_stride: usize,
        item_count: usize,
    );

    fn rtcSetGeometryInstancedScene(geom: RTCGeometry, scene: RTCScene);
    fn rtcSetGeometryTransform(
        geom: RTCGeometry,
        time_step: u32,
        format: u32,  // RTC_FORMAT_FLOAT4X4_COLUMN_MAJOR = 34
        xfm: *const f32,
    );

    fn rtcSetGeometryVertexAttributeCount(geom: RTCGeometry, vertex_attribute_count: u32);

    fn rtcIntersect1(
        scene: RTCScene,
        rayhit: *mut RTCRayHit,
        args: *const std::ffi::c_void,  // RTCIntersectArguments*, can be NULL
    );
}

// ============================================================================
// Helper Functions
// ============================================================================

impl Default for RTCBounds {
    fn default() -> Self {
        Self {
            lower_x: 0.0,
            lower_y: 0.0,
            lower_z: 0.0,
            align0: 0.0,
            upper_x: 0.0,
            upper_y: 0.0,
            upper_z: 0.0,
            align1: 0.0,
        }
    }
}

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
/// let transforms = vec![Mat4::IDENTITY; 1000];
/// let material = Lambertian::new(Color::new(0.7, 0.7, 0.7));
///
/// let scene = EmbreeScene::new(&vertices, transforms, material);
/// // Now you can trace rays with 1000 instances efficiently!
/// ```
pub struct EmbreeScene<M: Material + Clone + 'static> {
    device: RTCDevice,
    scene: RTCScene,
    prototype_scene: RTCScene,  // Must stay alive while instances reference it!
    material: Arc<M>,

    // Keep vertex, index, and transform data alive (Embree holds pointers to this)
    _vertex_data: Vec<f32>,
    _index_data: Vec<u32>,
    _transform_data: Vec<[f32; 16]>,

    // For debugging/stats
    instance_count: usize,
    triangle_count: usize,
}

impl<M: Material + Clone + 'static> EmbreeScene<M> {
    /// Create Embree scene with instanced geometry.
    ///
    /// # Arguments
    /// * `vertices` - Triangle vertices as flat array of Vec3 triplets
    /// * `transforms` - Instance transforms (local-to-world matrices)
    /// * `material` - Shared material for all instances
    ///
    /// # Safety
    /// Requires Embree 3 library to be installed and linkable.
    pub fn new(
        vertices: &[[Vec3; 3]],
        transforms: Vec<Mat4>,
        material: M,
    ) -> Self {
        unsafe {
            // 1. Create Embree device
            let device = rtcNewDevice(std::ptr::null());
            if device.is_null() {
                panic!("Failed to create Embree device");
            }

            // Check for errors
            let err = rtcGetDeviceError(device);
            if err != 0 {
                panic!("Embree device error: {}", err);
            }

            // 2. Create scene for prototype mesh
            let prototype_scene = rtcNewScene(device);
            if prototype_scene.is_null() {
                rtcReleaseDevice(device);
                panic!("Failed to create Embree prototype scene");
            }

            // 3. Flatten triangles into separate vertex and index arrays
            // Embree requires indexed triangle meshes
            let mut vertex_data = Vec::with_capacity(vertices.len() * 9);
            let mut index_data = Vec::with_capacity(vertices.len() * 3);

            for (tri_idx, tri) in vertices.iter().enumerate() {
                // Add 3 vertices
                vertex_data.extend_from_slice(&[tri[0].x, tri[0].y, tri[0].z]);
                vertex_data.extend_from_slice(&[tri[1].x, tri[1].y, tri[1].z]);
                vertex_data.extend_from_slice(&[tri[2].x, tri[2].y, tri[2].z]);

                // Add indices (each triangle uses 3 consecutive vertices)
                let base_idx = (tri_idx * 3) as u32;
                index_data.push(base_idx);
                index_data.push(base_idx + 1);
                index_data.push(base_idx + 2);
            }

            // Debug first triangle
            if !vertices.is_empty() {
                log::debug!(
                    "First triangle: v0=({}, {}, {}), v1=({}, {}, {}), v2=({}, {}, {})",
                    vertices[0][0].x, vertices[0][0].y, vertices[0][0].z,
                    vertices[0][1].x, vertices[0][1].y, vertices[0][1].z,
                    vertices[0][2].x, vertices[0][2].y, vertices[0][2].z
                );
            }

            // 4. Create triangle mesh geometry
            let geom = rtcNewGeometry(device, RTCGeometryType::Triangle);
            if geom.is_null() {
                rtcReleaseScene(prototype_scene);
                rtcReleaseDevice(device);
                panic!("Failed to create Embree geometry");
            }

            log::info!("Setting up triangle geometry: {} triangles, {} vertices, {} indices",
                vertices.len(), vertex_data.len() / 3, index_data.len());

            // 5. Set vertex buffer
            rtcSetSharedGeometryBuffer(
                geom,
                RTCBufferType::Vertex as u32,
                0,  // slot
                RTCFormat::Float3 as u32,
                vertex_data.as_ptr() as *const std::ffi::c_void,
                0,  // byte offset
                12,  // stride: 3 * f32 = 12 bytes per vertex
                vertex_data.len() / 3,  // vertex count
            );

            let err = rtcGetDeviceError(device);
            if err != 0 {
                panic!("Embree error after setting vertex buffer: {}", err);
            }

            // 6. Set index buffer
            rtcSetSharedGeometryBuffer(
                geom,
                RTCBufferType::Index as u32,
                0,  // slot
                RTCFormat::UInt3 as u32,
                index_data.as_ptr() as *const std::ffi::c_void,
                0,  // byte offset
                12,  // stride: 3 * u32 = 12 bytes per triangle
                vertices.len(),  // triangle count
            );

            let err = rtcGetDeviceError(device);
            if err != 0 {
                panic!("Embree error after setting index buffer: {}", err);
            }

            rtcCommitGeometry(geom);
            let geom_id = rtcAttachGeometry(prototype_scene, geom);
            log::info!("Attached geometry to prototype scene: geom_id={}", geom_id);

            // Check for Embree errors
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
                panic!("Embree error after attaching geometry: {} ({})\nVertex count: {}, Triangle count: {}",
                    err, err_msg, vertices.len() * 3, vertices.len());
            }

            log::info!("Geometry attached successfully, checking commit...");

            rtcReleaseGeometry(geom);

            // 6. Commit prototype scene
            rtcCommitScene(prototype_scene);

            // Debug: Check prototype scene bounds
            let mut proto_bounds = RTCBounds::default();
            rtcGetSceneBounds(prototype_scene, &mut proto_bounds);
            log::debug!(
                "Prototype scene bounds: ({}, {}, {}) to ({}, {}, {})",
                proto_bounds.lower_x, proto_bounds.lower_y, proto_bounds.lower_z,
                proto_bounds.upper_x, proto_bounds.upper_y, proto_bounds.upper_z
            );

            // 7. Create top-level scene with instances
            let scene = rtcNewScene(device);
            if scene.is_null() {
                rtcReleaseScene(prototype_scene);
                rtcReleaseDevice(device);
                panic!("Failed to create Embree top-level scene");
            }

            // 8. Store transforms (Embree holds pointers, must keep alive)
            let transform_data: Vec<[f32; 16]> = transforms.iter()
                .map(|t| t.to_cols_array())
                .collect();

            // 9. Add instances
            for (idx, transform_array) in transform_data.iter().enumerate() {
                let inst_geom = rtcNewGeometry(device, RTCGeometryType::Instance);
                if inst_geom.is_null() {
                    log::warn!("Failed to create instance geometry");
                    continue;
                }

                // Set instanced scene
                rtcSetGeometryInstancedScene(inst_geom, prototype_scene);

                // Set transform (column-major Mat4)
                // From rtcore_common.h: RTC_FORMAT_FLOAT4X4_COLUMN_MAJOR = 0x9244
                rtcSetGeometryTransform(
                    inst_geom,
                    0,  // time step
                    RTCFormat::Float4x4ColumnMajor as u32,
                    transform_array.as_ptr(),
                );

                // Debug first transform
                if idx == 0 {
                    log::debug!(
                        "First transform:\n  [{}, {}, {}, {}]\n  [{}, {}, {}, {}]\n  [{}, {}, {}, {}]\n  [{}, {}, {}, {}]",
                        transform_array[0], transform_array[1], transform_array[2], transform_array[3],
                        transform_array[4], transform_array[5], transform_array[6], transform_array[7],
                        transform_array[8], transform_array[9], transform_array[10], transform_array[11],
                        transform_array[12], transform_array[13], transform_array[14], transform_array[15]
                    );
                }

                rtcCommitGeometry(inst_geom);
                rtcAttachGeometry(scene, inst_geom);
                rtcReleaseGeometry(inst_geom);
            }

            // 10. Commit top-level scene
            rtcCommitScene(scene);

            // NOTE: Do NOT release prototype_scene here!
            // Instances reference it and it must stay alive for the lifetime of EmbreeScene.

            // Get scene bounds for debugging
            let mut bounds = RTCBounds::default();
            rtcGetSceneBounds(scene, &mut bounds);

            log::info!(
                "Embree scene created: {} instances, {} triangles",
                transform_data.len(),
                vertices.len()
            );
            log::info!(
                "Scene bounds: ({}, {}, {}) to ({}, {}, {})",
                bounds.lower_x, bounds.lower_y, bounds.lower_z,
                bounds.upper_x, bounds.upper_y, bounds.upper_z
            );

            Self {
                device,
                scene,
                prototype_scene,  // Keep alive for instances
                material: Arc::new(material),
                _vertex_data: vertex_data,
                _index_data: index_data,
                _transform_data: transform_data,
                instance_count: transforms.len(),
                triangle_count: vertices.len(),
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
}

impl<M: Material + Clone + 'static> Hittable for EmbreeScene<M> {
    fn hit<'a>(&'a self, ray: &Ray, ray_t: Interval, rec: &mut HitRecord<'a>) -> bool {
        unsafe {
            // 1. Convert to Embree ray-hit
            let mut rayhit = RTCRayHit::from_ray(ray, ray_t);

            // 2. Trace ray (Embree 4 API: scene, rayhit, args=NULL)
            rtcIntersect1(self.scene, &mut rayhit, std::ptr::null());

            // 3. Check if hit
            if rayhit.hit.geom_id == RTC_INVALID_GEOMETRY_ID {
                // Debug first few misses
                static mut MISS_COUNT: u32 = 0;
                MISS_COUNT += 1;
                if MISS_COUNT <= 5 {
                    log::debug!(
                        "Ray miss #{}: origin=({}, {}, {}), dir=({}, {}, {}), tfar={}",
                        MISS_COUNT,
                        rayhit.ray.org_x, rayhit.ray.org_y, rayhit.ray.org_z,
                        rayhit.ray.dir_x, rayhit.ray.dir_y, rayhit.ray.dir_z,
                        rayhit.ray.tfar
                    );
                }
                return false;
            }

            // Debug first few hits
            static mut HIT_COUNT: u32 = 0;
            HIT_COUNT += 1;
            if HIT_COUNT <= 5 {
                log::info!(
                    "Ray hit #{}: t={}, geom_id={}, prim_id={}, normal=({}, {}, {})",
                    HIT_COUNT,
                    rayhit.ray.tfar,
                    rayhit.hit.geom_id,
                    rayhit.hit.prim_id,
                    rayhit.hit.ng_x, rayhit.hit.ng_y, rayhit.hit.ng_z
                );
            }

            // 4. Fill HitRecord
            rec.t = rayhit.ray.tfar;
            rec.p = ray.at(rec.t);

            // Embree returns geometric normal (not interpolated)
            let normal = Vec3::new(rayhit.hit.ng_x, rayhit.hit.ng_y, rayhit.hit.ng_z);
            rec.normal = normal.normalize();

            // UV coordinates from barycentric
            rec.u = rayhit.hit.u;
            rec.v = rayhit.hit.v;

            // Shared material
            rec.material = &*self.material;

            // Set front face
            rec.set_face_normal(ray, rec.normal);

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

impl<M: Material + Clone + 'static> Drop for EmbreeScene<M> {
    fn drop(&mut self) {
        unsafe {
            rtcReleaseScene(self.scene);
            rtcReleaseScene(self.prototype_scene);  // Release prototype after top-level scene
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
unsafe impl<M: Material + Clone + 'static> Send for EmbreeScene<M> {}
unsafe impl<M: Material + Clone + 'static> Sync for EmbreeScene<M> {}
