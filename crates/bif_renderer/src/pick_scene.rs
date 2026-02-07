//! Stripped-down Embree scene for viewport picking.
//!
//! Only tracks vertices + instance transforms. No materials, UVs, or normals.
//! Uses two-level BVH: bottom = prototype triangles, top = instance transforms.
//! Returns the instance index on hit via `RTCHit.inst_id[0]`.

use bif_math::{Mat4, Vec3};
use thiserror::Error;

use crate::embree_ffi::*;

// ============================================================================
// Public types
// ============================================================================

/// Errors during pick scene creation.
#[derive(Debug, Error)]
pub enum PickError {
    #[error("Embree device creation failed")]
    DeviceCreation,
    #[error("Embree scene creation failed")]
    SceneCreation,
    #[error("Embree geometry creation failed")]
    GeometryCreation,
    #[error("No vertices provided")]
    NoVertices,
}

/// Result of a viewport pick operation.
#[derive(Debug, Clone)]
pub struct PickResult {
    /// Index of the hit instance (into the scene's instance array).
    pub instance_index: usize,
    /// Triangle index within the prototype mesh.
    pub triangle_index: usize,
    /// Ray parameter at hit point.
    pub t: f32,
    /// World-space hit point.
    pub hit_point: Vec3,
}

/// Stripped-down Embree scene for viewport picking.
///
/// Only stores vertices and instance transforms - no materials, UVs, or normals.
/// Each instance is attached with its scene-level instance index as the geometry ID,
/// so `RTCHit.geom_id` directly gives the instance index on hit.
pub struct EmbreePickScene {
    device: RTCDevice,
    scene: RTCScene,
    prototype_scene: RTCScene,
    // Must keep alive while Embree holds pointers
    _vertex_data: Vec<f32>,
    _index_data: Vec<u32>,
    _transform_data: Vec<[f32; 16]>,
    instance_count: usize,
}

impl EmbreePickScene {
    /// Build a pick scene from triangle vertices and instance transforms.
    ///
    /// # Arguments
    /// * `vertices` - Triangle vertices as `[Vec3; 3]` per triangle
    /// * `transforms` - Per-instance world transforms (same order as scene instances)
    pub fn new(vertices: &[[Vec3; 3]], transforms: &[Mat4]) -> Result<Self, PickError> {
        if vertices.is_empty() {
            return Err(PickError::NoVertices);
        }

        unsafe {
            // Device
            let device = rtcNewDevice(std::ptr::null());
            if device.is_null() {
                return Err(PickError::DeviceCreation);
            }
            if rtcGetDeviceError(device) != 0 {
                rtcReleaseDevice(device);
                return Err(PickError::DeviceCreation);
            }

            // Prototype scene (bottom-level BVH)
            let prototype_scene = rtcNewScene(device);
            if prototype_scene.is_null() {
                rtcReleaseDevice(device);
                return Err(PickError::SceneCreation);
            }

            // Flatten triangles into vertex + index arrays
            // 16-byte stride (4 floats per vertex) for Embree SIMD alignment safety
            let mut vertex_data = Vec::with_capacity(vertices.len() * 12);
            let mut index_data = Vec::with_capacity(vertices.len() * 3);
            for (tri_idx, tri) in vertices.iter().enumerate() {
                vertex_data.extend_from_slice(&[tri[0].x, tri[0].y, tri[0].z, 0.0]);
                vertex_data.extend_from_slice(&[tri[1].x, tri[1].y, tri[1].z, 0.0]);
                vertex_data.extend_from_slice(&[tri[2].x, tri[2].y, tri[2].z, 0.0]);
                let base = (tri_idx * 3) as u32;
                index_data.push(base);
                index_data.push(base + 1);
                index_data.push(base + 2);
            }

            // Triangle geometry
            let geom = rtcNewGeometry(device, RTCGeometryType::Triangle);
            if geom.is_null() {
                rtcReleaseScene(prototype_scene);
                rtcReleaseDevice(device);
                return Err(PickError::GeometryCreation);
            }

            rtcSetSharedGeometryBuffer(
                geom,
                RTCBufferType::Vertex as u32,
                0,
                RTCFormat::Float3 as u32,
                vertex_data.as_ptr() as *const std::ffi::c_void,
                0,
                16, // 4 floats per vertex for SIMD alignment
                vertex_data.len() / 4,
            );
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

            rtcCommitGeometry(geom);
            rtcAttachGeometryByID(prototype_scene, geom, 0);
            rtcReleaseGeometry(geom);
            rtcCommitScene(prototype_scene);

            // Top-level scene with instances
            let scene = rtcNewScene(device);
            if scene.is_null() {
                rtcReleaseScene(prototype_scene);
                rtcReleaseDevice(device);
                return Err(PickError::SceneCreation);
            }

            let transform_data: Vec<[f32; 16]> =
                transforms.iter().map(|t| t.to_cols_array()).collect();

            for (idx, xfm) in transform_data.iter().enumerate() {
                let inst_geom = rtcNewGeometry(device, RTCGeometryType::Instance);
                if inst_geom.is_null() {
                    continue;
                }
                rtcSetGeometryInstancedScene(inst_geom, prototype_scene);
                rtcSetGeometryTransform(
                    inst_geom,
                    0,
                    RTCFormat::Float4x4ColumnMajor as u32,
                    xfm.as_ptr(),
                );
                rtcCommitGeometry(inst_geom);
                // Attach with idx as geometry ID so geom_id == instance index
                rtcAttachGeometryByID(scene, inst_geom, idx as u32);
                rtcReleaseGeometry(inst_geom);
            }

            rtcCommitScene(scene);

            log::info!(
                "Pick scene built: {} triangles, {} instances",
                vertices.len(),
                transforms.len()
            );

            Ok(Self {
                device,
                scene,
                prototype_scene,
                _vertex_data: vertex_data,
                _index_data: index_data,
                _transform_data: transform_data,
                instance_count: transforms.len(),
            })
        }
    }

    /// Cast a ray and return the closest hit instance.
    ///
    /// # Arguments
    /// * `origin` - Ray origin in world space
    /// * `direction` - Normalized ray direction
    pub fn pick(&self, origin: Vec3, direction: Vec3) -> Option<PickResult> {
        unsafe {
            let mut rayhit = RTCRayHit {
                ray: RTCRay {
                    org_x: origin.x,
                    org_y: origin.y,
                    org_z: origin.z,
                    tnear: 0.001,
                    dir_x: direction.x,
                    dir_y: direction.y,
                    dir_z: direction.z,
                    time: 0.0,
                    tfar: f32::MAX,
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
            };

            rtcIntersect1(self.scene, &mut rayhit, std::ptr::null());

            if rayhit.hit.geom_id == RTC_INVALID_GEOMETRY_ID {
                return None;
            }

            // Two-level BVH: inst_id[0] holds the instance geometry ID from the
            // top-level scene. We attached each instance with its index as ID.
            let instance_index = rayhit.hit.inst_id[0] as usize;
            if instance_index >= self.instance_count {
                return None;
            }

            let t = rayhit.ray.tfar;
            let hit_point = origin + direction * t;

            Some(PickResult {
                instance_index,
                triangle_index: rayhit.hit.prim_id as usize,
                t,
                hit_point,
            })
        }
    }

    /// Update an instance transform without rebuilding the full BVH.
    ///
    /// O(1) per instance — much cheaper than a full rebuild.
    pub fn update_instance_transform(&self, instance_index: usize, transform: &Mat4) {
        if instance_index >= self.instance_count {
            return;
        }
        let cols = transform.to_cols_array();
        unsafe {
            let geom = rtcGetGeometry(self.scene, instance_index as u32);
            if !geom.is_null() {
                rtcSetGeometryTransform(
                    geom,
                    0,
                    RTCFormat::Float4x4ColumnMajor as u32,
                    cols.as_ptr(),
                );
                rtcCommitGeometry(geom);
            }
            rtcCommitScene(self.scene);
        }
    }

    /// Get the number of instances in this pick scene.
    pub fn instance_count(&self) -> usize {
        self.instance_count
    }
}

impl Drop for EmbreePickScene {
    fn drop(&mut self) {
        unsafe {
            rtcReleaseScene(self.scene);
            rtcReleaseScene(self.prototype_scene);
            rtcReleaseDevice(self.device);
        }
    }
}

// SAFETY: Same reasoning as EmbreeScene - Embree is thread-safe after commit.
unsafe impl Send for EmbreePickScene {}
unsafe impl Sync for EmbreePickScene {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_known_triangle() {
        // Triangle on XZ plane at y=0, centered at origin
        let vertices = vec![[
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, -1.0),
            Vec3::new(0.0, 0.0, 1.0),
        ]];
        let transforms = vec![Mat4::IDENTITY];

        let scene = EmbreePickScene::new(&vertices, &transforms).expect("pick scene should build");

        // Ray from above, pointing down - should hit
        let result = scene.pick(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, -1.0, 0.0));
        assert!(result.is_some(), "ray should hit triangle");
        let hit = result.unwrap();
        assert_eq!(hit.instance_index, 0);
        assert!((hit.hit_point.y).abs() < 0.01, "hit should be near y=0");

        // Ray from above, pointing up - should miss
        let miss = scene.pick(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        assert!(miss.is_none(), "ray should miss");
    }

    #[test]
    fn test_pick_multiple_instances() {
        let vertices = vec![[
            Vec3::new(-0.5, -0.5, 0.0),
            Vec3::new(0.5, -0.5, 0.0),
            Vec3::new(0.0, 0.5, 0.0),
        ]];
        // Instance 0 at origin, instance 1 at x=5
        let transforms = vec![
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0)),
        ];

        let scene = EmbreePickScene::new(&vertices, &transforms).expect("pick scene should build");

        // Hit instance 0
        let hit0 = scene.pick(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -1.0));
        assert!(hit0.is_some());
        assert_eq!(hit0.unwrap().instance_index, 0);

        // Hit instance 1
        let hit1 = scene.pick(Vec3::new(5.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -1.0));
        assert!(hit1.is_some());
        assert_eq!(hit1.unwrap().instance_index, 1);
    }
}
