//! Frustum culling for GPU instancing optimization.
//!
//! Extracts frustum planes from a view-projection matrix and tests
//! axis-aligned bounding boxes for visibility.

use crate::{Aabb, Mat4, Vec3, Vec4};

/// A view frustum defined by 6 planes (left, right, bottom, top, near, far).
///
/// Each plane is stored as a Vec4 where (x, y, z) is the normal and w is the distance.
/// A point P is on the positive side of the plane if: dot(normal, P) + w >= 0
#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    planes: [Vec4; 6],
}

/// Plane indices for clarity
pub const PLANE_LEFT: usize = 0;
pub const PLANE_RIGHT: usize = 1;
pub const PLANE_BOTTOM: usize = 2;
pub const PLANE_TOP: usize = 3;
pub const PLANE_NEAR: usize = 4;
pub const PLANE_FAR: usize = 5;

impl Frustum {
    /// Extract frustum planes from a view-projection matrix.
    ///
    /// Uses the Gribb/Hartmann method for efficient plane extraction.
    /// Reference: "Fast Extraction of Viewing Frustum Planes from the World-View-Projection Matrix"
    pub fn from_view_projection(vp: Mat4) -> Self {
        // Get matrix rows
        let row0 = Vec4::new(vp.x_axis.x, vp.y_axis.x, vp.z_axis.x, vp.w_axis.x);
        let row1 = Vec4::new(vp.x_axis.y, vp.y_axis.y, vp.z_axis.y, vp.w_axis.y);
        let row2 = Vec4::new(vp.x_axis.z, vp.y_axis.z, vp.z_axis.z, vp.w_axis.z);
        let row3 = Vec4::new(vp.x_axis.w, vp.y_axis.w, vp.z_axis.w, vp.w_axis.w);

        let mut planes = [Vec4::ZERO; 6];

        // Left plane: row3 + row0
        planes[PLANE_LEFT] = row3 + row0;
        // Right plane: row3 - row0
        planes[PLANE_RIGHT] = row3 - row0;
        // Bottom plane: row3 + row1
        planes[PLANE_BOTTOM] = row3 + row1;
        // Top plane: row3 - row1
        planes[PLANE_TOP] = row3 - row1;
        // Near plane: row3 + row2
        planes[PLANE_NEAR] = row3 + row2;
        // Far plane: row3 - row2
        planes[PLANE_FAR] = row3 - row2;

        // Normalize all planes
        for plane in &mut planes {
            let length = Vec3::new(plane.x, plane.y, plane.z).length();
            if length > 0.0 {
                *plane /= length;
            }
        }

        Self { planes }
    }

    /// Test if an AABB intersects the frustum.
    ///
    /// Returns true if the AABB is at least partially inside the frustum.
    /// Uses the "p-vertex" optimization for fast rejection.
    pub fn intersects_aabb(&self, aabb: &Aabb) -> bool {
        let min = Vec3::new(aabb.x.min, aabb.y.min, aabb.z.min);
        let max = Vec3::new(aabb.x.max, aabb.y.max, aabb.z.max);

        for plane in &self.planes {
            let normal = Vec3::new(plane.x, plane.y, plane.z);

            // Find the "positive vertex" - the corner furthest in the direction of the normal
            let p_vertex = Vec3::new(
                if normal.x >= 0.0 { max.x } else { min.x },
                if normal.y >= 0.0 { max.y } else { min.y },
                if normal.z >= 0.0 { max.z } else { min.z },
            );

            // If the p-vertex is outside this plane, the AABB is completely outside
            if normal.dot(p_vertex) + plane.w < 0.0 {
                return false;
            }
        }

        true
    }

    /// Test if a point is inside the frustum.
    pub fn contains_point(&self, point: Vec3) -> bool {
        for plane in &self.planes {
            let normal = Vec3::new(plane.x, plane.y, plane.z);
            if normal.dot(point) + plane.w < 0.0 {
                return false;
            }
        }
        true
    }

    /// Get the distance from a point to the camera (approximated using near plane).
    /// Useful for LOD selection.
    pub fn distance_to_point(&self, point: Vec3) -> f32 {
        let near_plane = self.planes[PLANE_NEAR];
        let normal = Vec3::new(near_plane.x, near_plane.y, near_plane.z);
        (normal.dot(point) + near_plane.w).abs()
    }

    /// Get the distance from an AABB center to the near plane.
    pub fn distance_to_aabb(&self, aabb: &Aabb) -> f32 {
        let center = Vec3::new(
            (aabb.x.min + aabb.x.max) * 0.5,
            (aabb.y.min + aabb.y.max) * 0.5,
            (aabb.z.min + aabb.z.max) * 0.5,
        );
        self.distance_to_point(center)
    }
}

impl Default for Frustum {
    fn default() -> Self {
        // Default frustum that accepts everything
        Self {
            planes: [
                Vec4::new(1.0, 0.0, 0.0, f32::MAX),  // left
                Vec4::new(-1.0, 0.0, 0.0, f32::MAX), // right
                Vec4::new(0.0, 1.0, 0.0, f32::MAX),  // bottom
                Vec4::new(0.0, -1.0, 0.0, f32::MAX), // top
                Vec4::new(0.0, 0.0, 1.0, f32::MAX),  // near
                Vec4::new(0.0, 0.0, -1.0, f32::MAX), // far
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frustum_default_accepts_all() {
        let frustum = Frustum::default();
        let aabb = Aabb::from_points(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        assert!(frustum.intersects_aabb(&aabb));
    }

    #[test]
    fn test_frustum_from_perspective() {
        // Create a simple perspective projection looking down -Z
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        let vp = proj * view;

        let frustum = Frustum::from_view_projection(vp);

        // AABB at origin should be visible
        let visible_aabb = Aabb::from_points(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        assert!(frustum.intersects_aabb(&visible_aabb));

        // AABB far behind the camera should not be visible
        let behind_aabb = Aabb::from_points(Vec3::new(-1.0, -1.0, 20.0), Vec3::new(1.0, 1.0, 22.0));
        assert!(!frustum.intersects_aabb(&behind_aabb));

        // AABB way to the left should not be visible
        let left_aabb =
            Aabb::from_points(Vec3::new(-100.0, -1.0, -1.0), Vec3::new(-90.0, 1.0, 1.0));
        assert!(!frustum.intersects_aabb(&left_aabb));
    }

    #[test]
    fn test_frustum_contains_point() {
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        let vp = proj * view;

        let frustum = Frustum::from_view_projection(vp);

        // Origin should be visible
        assert!(frustum.contains_point(Vec3::ZERO));

        // Point behind camera should not be visible
        assert!(!frustum.contains_point(Vec3::new(0.0, 0.0, 20.0)));
    }

    #[test]
    fn test_frustum_distance() {
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        let vp = proj * view;

        let frustum = Frustum::from_view_projection(vp);

        // Distance to origin (camera at z=10, near=0.1)
        let dist = frustum.distance_to_point(Vec3::ZERO);
        // Should be approximately 10 - 0.1 = 9.9
        assert!(dist > 9.0 && dist < 11.0);
    }

    #[test]
    fn test_aabb_partially_in_frustum() {
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        let vp = proj * view;

        let frustum = Frustum::from_view_projection(vp);

        // Large AABB that straddles the frustum edge should still be visible
        let straddling_aabb =
            Aabb::from_points(Vec3::new(-50.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        assert!(frustum.intersects_aabb(&straddling_aabb));
    }
}
