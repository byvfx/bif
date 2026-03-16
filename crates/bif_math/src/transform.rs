// Transform utilities for Mat4
//
// Extends glam::Mat4 with convenience methods for ray tracing transformations.
// Note: glam::Mat4 already provides transform_point3() and inverse()

use crate::Aabb;
use glam::{Mat4, Vec3};

/// Extension trait for Mat4 to provide additional transform utilities
pub trait Mat4Ext {
    /// Transform a vector in 3D space (applies rotation and scale, but NOT translation).
    /// Vectors have an implicit w=0 component.
    fn transform_vector3(&self, vector: Vec3) -> Vec3;

    /// Transform an axis-aligned bounding box using Arvo's method.
    /// Works directly with matrix columns instead of transforming 8 corners.
    fn transform_aabb(&self, aabb: &Aabb) -> Aabb;
}

impl Mat4Ext for Mat4 {
    fn transform_vector3(&self, vector: Vec3) -> Vec3 {
        // Transform as direction (w=0) - translation should not affect vectors
        self.col(0).truncate() * vector.x
            + self.col(1).truncate() * vector.y
            + self.col(2).truncate() * vector.z
    }

    fn transform_aabb(&self, aabb: &Aabb) -> Aabb {
        // Arvo's method: work with matrix columns directly (~2x faster than
        // transforming all 8 corners).
        let aabb_min = Vec3::new(aabb.x.min, aabb.y.min, aabb.z.min);
        let aabb_max = Vec3::new(aabb.x.max, aabb.y.max, aabb.z.max);

        // Start from translation column
        let mut new_min = self.col(3).truncate();
        let mut new_max = new_min;

        // Accumulate each axis contribution
        for i in 0..3 {
            let col = self.col(i).truncate();
            let a = col * aabb_min[i];
            let b = col * aabb_max[i];
            new_min += a.min(b);
            new_max += a.max(b);
        }

        Aabb::from_points(new_min, new_max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;

    #[test]
    fn test_transform_point3_identity() {
        let mat = Mat4::IDENTITY;
        let point = Vec3::new(1.0, 2.0, 3.0);
        let transformed = mat.transform_point3(point);

        assert_eq!(transformed, point);
    }

    #[test]
    fn test_transform_point3_translation() {
        let mat = Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0));
        let point = Vec3::new(1.0, 2.0, 3.0);
        let transformed = mat.transform_point3(point);

        assert_eq!(transformed, Vec3::new(11.0, 22.0, 33.0));
    }

    #[test]
    fn test_transform_vector3_no_translation() {
        let mat = Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0));
        let vector = Vec3::new(1.0, 0.0, 0.0);
        let transformed = mat.transform_vector3(vector);

        // Translation should NOT affect vectors (w=0)
        assert_eq!(transformed, vector);
    }

    #[test]
    fn test_transform_vector3_rotation() {
        use std::f32::consts::PI;

        // 90 degree rotation around Z axis
        let mat = Mat4::from_rotation_z(PI / 2.0);
        let vector = Vec3::new(1.0, 0.0, 0.0);
        let transformed = mat.transform_vector3(vector);

        // X vector should rotate to Y vector
        assert!((transformed.x - 0.0).abs() < 0.001);
        assert!((transformed.y - 1.0).abs() < 0.001);
        assert!((transformed.z - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_transform_aabb_identity() {
        let mat = Mat4::IDENTITY;
        let aabb = Aabb::from_points(Vec3::ZERO, Vec3::ONE);
        let transformed = mat.transform_aabb(&aabb);

        let orig_min = Vec3::new(aabb.x.min, aabb.y.min, aabb.z.min);
        let orig_max = Vec3::new(aabb.x.max, aabb.y.max, aabb.z.max);
        let trans_min = Vec3::new(transformed.x.min, transformed.y.min, transformed.z.min);
        let trans_max = Vec3::new(transformed.x.max, transformed.y.max, transformed.z.max);

        assert!((trans_min - orig_min).length() < 0.001);
        assert!((trans_max - orig_max).length() < 0.001);
    }

    #[test]
    fn test_transform_aabb_translation() {
        let mat = Mat4::from_translation(Vec3::new(5.0, 5.0, 5.0));
        let aabb = Aabb::from_points(Vec3::ZERO, Vec3::ONE);
        let transformed = mat.transform_aabb(&aabb);

        let trans_min = Vec3::new(transformed.x.min, transformed.y.min, transformed.z.min);
        let trans_max = Vec3::new(transformed.x.max, transformed.y.max, transformed.z.max);

        assert!((trans_min - Vec3::new(5.0, 5.0, 5.0)).length() < 0.001);
        assert!((trans_max - Vec3::new(6.0, 6.0, 6.0)).length() < 0.001);
    }

    #[test]
    fn test_mat4_inverse() {
        let translation = Vec3::new(10.0, 20.0, 30.0);
        let mat = Mat4::from_translation(translation);
        let inv = mat.inverse();

        let point = Vec3::new(1.0, 2.0, 3.0);
        let transformed = mat.transform_point3(point);
        let back = inv.transform_point3(transformed);

        // Should round-trip back to original
        assert!((back - point).length() < 0.001);
    }

    #[test]
    fn test_mat4_rotation_inverse() {
        use std::f32::consts::PI;

        let mat = Mat4::from_rotation_y(PI / 4.0); // 45 degrees
        let inv = mat.inverse();

        let point = Vec3::new(5.0, 3.0, 2.0);
        let transformed = mat.transform_point3(point);
        let back = inv.transform_point3(transformed);

        assert!((back - point).length() < 0.001);
    }
}
