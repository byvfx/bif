//! Procedural geometry primitives (cube, sphere, camera wireframe).

use bif_math::Vec3;
use std::f32::consts::PI;

use crate::mesh::Mesh;

/// Kind of primitive geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveKind {
    Cube,
    Sphere,
    Camera,
}

/// Create a cube mesh centered at origin.
///
/// 24 vertices (unique normals per face), 12 triangles, with UVs.
pub fn create_cube(size: f32) -> Mesh {
    let h = size * 0.5;

    // 6 faces x 4 verts = 24 vertices
    #[rustfmt::skip]
    let positions = vec![
        // +X face
        Vec3::new( h, -h, -h), Vec3::new( h,  h, -h), Vec3::new( h,  h,  h), Vec3::new( h, -h,  h),
        // -X face
        Vec3::new(-h, -h,  h), Vec3::new(-h,  h,  h), Vec3::new(-h,  h, -h), Vec3::new(-h, -h, -h),
        // +Y face
        Vec3::new(-h,  h, -h), Vec3::new(-h,  h,  h), Vec3::new( h,  h,  h), Vec3::new( h,  h, -h),
        // -Y face
        Vec3::new(-h, -h,  h), Vec3::new(-h, -h, -h), Vec3::new( h, -h, -h), Vec3::new( h, -h,  h),
        // +Z face
        Vec3::new(-h, -h,  h), Vec3::new( h, -h,  h), Vec3::new( h,  h,  h), Vec3::new(-h,  h,  h),
        // -Z face
        Vec3::new( h, -h, -h), Vec3::new(-h, -h, -h), Vec3::new(-h,  h, -h), Vec3::new( h,  h, -h),
    ];

    #[rustfmt::skip]
    let normals = vec![
        // +X
        Vec3::X, Vec3::X, Vec3::X, Vec3::X,
        // -X
        Vec3::NEG_X, Vec3::NEG_X, Vec3::NEG_X, Vec3::NEG_X,
        // +Y
        Vec3::Y, Vec3::Y, Vec3::Y, Vec3::Y,
        // -Y
        Vec3::NEG_Y, Vec3::NEG_Y, Vec3::NEG_Y, Vec3::NEG_Y,
        // +Z
        Vec3::Z, Vec3::Z, Vec3::Z, Vec3::Z,
        // -Z
        Vec3::NEG_Z, Vec3::NEG_Z, Vec3::NEG_Z, Vec3::NEG_Z,
    ];

    #[rustfmt::skip]
    let uvs = vec![
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
    ];

    // Two triangles per face (CCW winding when viewed from outside)
    let mut indices = Vec::with_capacity(36);
    for face in 0..6u32 {
        let base = face * 4;
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    Mesh::new_with_uvs(positions, indices, Some(normals), Some(uvs))
}

/// Create a UV sphere mesh centered at origin.
///
/// `segments` controls both horizontal and vertical subdivisions.
pub fn create_sphere(radius: f32, segments: u32) -> Mesh {
    let rings = segments;
    let sectors = segments;

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();

    // Generate vertices
    for ring in 0..=rings {
        let phi = PI * ring as f32 / rings as f32; // 0..PI (top to bottom)
        let y = phi.cos() * radius;
        let ring_radius = phi.sin() * radius;

        for sector in 0..=sectors {
            let theta = 2.0 * PI * sector as f32 / sectors as f32;
            let x = ring_radius * theta.cos();
            let z = ring_radius * theta.sin();

            positions.push(Vec3::new(x, y, z));
            normals.push(Vec3::new(x, y, z).normalize_or_zero());
            uvs.push([sector as f32 / sectors as f32, ring as f32 / rings as f32]);
        }
    }

    // Generate indices (CCW winding) with triangle fans at poles
    let mut indices = Vec::new();
    let stride = sectors + 1;
    for ring in 0..rings {
        for sector in 0..sectors {
            let a = ring * stride + sector;
            let b = a + stride;
            let c = b + 1;
            let d = a + 1;

            if ring == 0 {
                // North pole: single triangle fan (CCW)
                indices.extend_from_slice(&[a, c, b]);
            } else if ring == rings - 1 {
                // South pole: single triangle fan (CCW)
                indices.extend_from_slice(&[a, d, b]);
            } else {
                // Normal quad: two triangles (CCW)
                indices.extend_from_slice(&[a, c, b, a, d, c]);
            }
        }
    }

    Mesh::new_with_uvs(positions, indices, Some(normals), Some(uvs))
}

/// Create a camera wireframe icon (pyramid frustum shape).
///
/// Small pyramid pointing down -Z with a rectangular base, like Maya/Houdini camera icons.
pub fn create_camera_wireframe() -> Mesh {
    let apex = Vec3::ZERO;
    let depth = 0.8_f32;
    let half_w = 0.4_f32;
    let half_h = 0.3_f32;

    // 4 base corners (in -Z direction from apex)
    let bl = Vec3::new(-half_w, -half_h, -depth);
    let br = Vec3::new(half_w, -half_h, -depth);
    let tr = Vec3::new(half_w, half_h, -depth);
    let tl = Vec3::new(-half_w, half_h, -depth);

    // Build as thin triangles for the wireframe edges
    // 4 side faces of pyramid + 2 triangles for base = 6 faces
    let positions = vec![
        // Side face: apex-bl-br
        apex, bl, br, // Side face: apex-br-tr
        apex, br, tr, // Side face: apex-tr-tl
        apex, tr, tl, // Side face: apex-tl-bl
        apex, tl, bl, // Base face 1: bl-tr-br
        bl, tr, br, // Base face 2: bl-tl-tr
        bl, tl, tr,
    ];

    let indices: Vec<u32> = (0..positions.len() as u32).collect();

    let mut mesh = Mesh::new(positions, indices, None);
    mesh.compute_normals();
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_cube() {
        let mesh = create_cube(1.0);
        assert_eq!(mesh.vertex_count(), 24);
        assert_eq!(mesh.triangle_count(), 12);
        assert!(mesh.has_normals());
        assert!(mesh.has_uvs());
    }

    #[test]
    fn test_create_sphere() {
        let mesh = create_sphere(1.0, 16);
        // (rings+1) * (sectors+1) = 17*17 = 289 verts
        assert_eq!(mesh.vertex_count(), 289);
        // Poles use fan (1 tri/sector), middle rings use quads (2 tri/sector)
        // = 2 * 16 + (16-2) * 16 * 2 = 32 + 448 = 480
        assert_eq!(mesh.triangle_count(), 480);
        assert!(mesh.has_normals());
        assert!(mesh.has_uvs());
    }

    #[test]
    fn test_create_camera_wireframe() {
        let mesh = create_camera_wireframe();
        // 6 faces x 3 verts = 18
        assert_eq!(mesh.vertex_count(), 18);
        assert_eq!(mesh.triangle_count(), 6);
        assert!(mesh.has_normals());
    }

    #[test]
    fn test_cube_bounds() {
        let mesh = create_cube(2.0);
        let bounds = &mesh.bounds;
        assert!((bounds.x.min - (-1.0)).abs() < 0.001);
        assert!((bounds.x.max - 1.0).abs() < 0.001);
        assert!((bounds.y.min - (-1.0)).abs() < 0.001);
        assert!((bounds.y.max - 1.0).abs() < 0.001);
    }
}
