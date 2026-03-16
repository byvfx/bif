//! Orthonormal basis construction (Frisvad's method).

use glam::Vec3;

/// Build an orthonormal basis from a normal vector using Frisvad's method.
///
/// Returns `(tangent, bitangent)` such that `(tangent, bitangent, n)` form
/// a right-handed orthonormal frame.
pub fn build_orthonormal_basis(n: Vec3) -> (Vec3, Vec3) {
    let sign = if n.z >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (sign + n.z);
    let b = n.x * n.y * a;

    let tangent = Vec3::new(1.0 + sign * n.x * n.x * a, sign * b, -sign * n.x);
    let bitangent = Vec3::new(b, sign + n.y * n.y * a, -n.y);

    (tangent, bitangent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orthonormal_basis_up() {
        let n = Vec3::new(0.0, 1.0, 0.0);
        let (t, b) = build_orthonormal_basis(n);
        assert!(t.dot(n).abs() < 1e-5);
        assert!(b.dot(n).abs() < 1e-5);
        assert!(t.dot(b).abs() < 1e-5);
        assert!((t.length() - 1.0).abs() < 1e-5);
        assert!((b.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_orthonormal_basis_negative_z() {
        let n = Vec3::new(0.0, 0.0, -1.0);
        let (t, b) = build_orthonormal_basis(n);
        assert!(t.dot(n).abs() < 1e-5);
        assert!(b.dot(n).abs() < 1e-5);
        assert!(t.dot(b).abs() < 1e-5);
    }

    #[test]
    fn test_orthonormal_basis_right_handed() {
        // Verify that (tangent, bitangent, normal) form a right-handed system.
        // In a right-handed frame: tangent x bitangent == normal (approximately).
        let normals = [
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 1.0).normalize(),
            Vec3::new(-0.3, 0.7, 0.5).normalize(),
        ];

        for n in &normals {
            let (t, b) = build_orthonormal_basis(*n);
            let cross = t.cross(b);
            let diff = (cross - *n).length();
            assert!(
                diff < 1e-4,
                "basis for normal {:?} is not right-handed: t x b = {:?}, expected {:?}, diff = {}",
                n,
                cross,
                n,
                diff
            );
        }
    }
}
