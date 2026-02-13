//! Scatter points on mesh surfaces.
//!
//! Provides random and Poisson disk distribution of points on triangle
//! meshes, producing `PointCloud` objects for instancing.

use bif_math::{Mat4, Quat, Vec3};
use rand::prelude::*;

use crate::mesh::Mesh;
use crate::point_cloud::{DistributionMethod, PointAttributes, PointCloud};
use crate::scene::Transform;

/// Scatter distribution mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScatterMode {
    /// Uniform random scatter weighted by triangle area.
    Random,
    /// Poisson disk with minimum distance constraint.
    PoissonDisk,
}

/// Configuration for scatter operations.
#[derive(Clone, Debug)]
pub struct ScatterConfig {
    /// Target number of points to generate.
    pub count: usize,
    /// Minimum distance between points (Poisson disk mode).
    pub min_distance: f32,
    /// Random seed for deterministic results.
    pub seed: u64,
    /// Align instance Y-axis to surface normal.
    pub align_to_normal: bool,
    /// Scale range (min, max) — uniform random per point.
    pub scale_range: (f32, f32),
    /// Random rotation range around up axis (radians).
    pub rotation_range: f32,
}

impl Default for ScatterConfig {
    fn default() -> Self {
        Self {
            count: 1000,
            min_distance: 0.1,
            seed: 42,
            align_to_normal: false,
            scale_range: (0.8, 1.2),
            rotation_range: std::f32::consts::TAU,
        }
    }
}

/// Sample a random point on a triangle using barycentric coordinates.
///
/// Returns (position, normal) where normal is the face normal.
fn sample_triangle(v0: Vec3, v1: Vec3, v2: Vec3, rng: &mut StdRng) -> (Vec3, Vec3) {
    let mut u: f32 = rng.gen();
    let mut v: f32 = rng.gen();
    if u + v > 1.0 {
        u = 1.0 - u;
        v = 1.0 - v;
    }
    let w = 1.0 - u - v;

    let pos = v0 * w + v1 * u + v2 * v;
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let normal = edge1.cross(edge2).normalize_or_zero();

    (pos, normal)
}

/// Compute triangle areas and build a cumulative distribution function.
///
/// Returns (areas, cdf) where cdf[i] is the cumulative probability up to triangle i.
fn build_area_cdf(mesh: &Mesh) -> (Vec<f32>, Vec<f32>) {
    let tri_count = mesh.triangle_count();
    let mut areas = Vec::with_capacity(tri_count);

    for chunk in mesh.indices.chunks(3) {
        if chunk.len() < 3 {
            areas.push(0.0);
            continue;
        }
        let i0 = chunk[0] as usize;
        let i1 = chunk[1] as usize;
        let i2 = chunk[2] as usize;
        if i0 >= mesh.positions.len() || i1 >= mesh.positions.len() || i2 >= mesh.positions.len() {
            areas.push(0.0);
            continue;
        }

        let v0 = mesh.positions[i0];
        let v1 = mesh.positions[i1];
        let v2 = mesh.positions[i2];
        let area = (v1 - v0).cross(v2 - v0).length() * 0.5;
        areas.push(area);
    }

    let total: f32 = areas.iter().sum();
    let mut cdf = Vec::with_capacity(tri_count);
    let mut cumulative = 0.0;
    for &a in &areas {
        cumulative += a / total.max(f32::EPSILON);
        cdf.push(cumulative);
    }
    // Ensure last value is exactly 1.0
    if let Some(last) = cdf.last_mut() {
        *last = 1.0;
    }

    (areas, cdf)
}

/// Pick a random triangle index weighted by area using the CDF.
fn pick_triangle(cdf: &[f32], rng: &mut StdRng) -> usize {
    let r: f32 = rng.gen();
    match cdf.binary_search_by(|v| v.partial_cmp(&r).unwrap_or(std::cmp::Ordering::Equal)) {
        Ok(idx) => idx,
        Err(idx) => idx.min(cdf.len() - 1),
    }
}

/// Get triangle vertices by index.
fn get_triangle(mesh: &Mesh, tri_idx: usize) -> Option<(Vec3, Vec3, Vec3)> {
    let base = tri_idx * 3;
    if base + 2 >= mesh.indices.len() {
        return None;
    }
    let i0 = mesh.indices[base] as usize;
    let i1 = mesh.indices[base + 1] as usize;
    let i2 = mesh.indices[base + 2] as usize;
    if i0 >= mesh.positions.len() || i1 >= mesh.positions.len() || i2 >= mesh.positions.len() {
        return None;
    }
    Some((mesh.positions[i0], mesh.positions[i1], mesh.positions[i2]))
}

/// Build an orientation quaternion aligning Y-up to the given normal,
/// with random rotation around the normal axis.
fn orientation_from_normal(normal: Vec3, angle: f32) -> Quat {
    // Build rotation from Y-up to normal
    let up = Vec3::Y;
    let base_rotation = if (normal - up).length_squared() < 1e-6 {
        Quat::IDENTITY
    } else if (normal + up).length_squared() < 1e-6 {
        Quat::from_rotation_x(std::f32::consts::PI)
    } else {
        Quat::from_rotation_arc(up, normal)
    };
    // Apply random twist around the normal
    let twist = Quat::from_axis_angle(normal, angle);
    twist * base_rotation
}

/// Scatter points randomly on a mesh surface, weighted by triangle area.
///
/// Points are transformed by `mesh_transform` to world space.
pub fn scatter_on_surface(
    mesh: &Mesh,
    mesh_transform: &Mat4,
    mode: ScatterMode,
    config: &ScatterConfig,
) -> PointCloud {
    let mut rng = StdRng::seed_from_u64(config.seed);

    let (positions, normals) = match mode {
        ScatterMode::Random => scatter_random(mesh, mesh_transform, config, &mut rng),
        ScatterMode::PoissonDisk => scatter_poisson(mesh, mesh_transform, config, &mut rng),
    };

    // Build per-point attributes
    let count = positions.len();
    let mut scales = Vec::with_capacity(count);
    let mut orientations = Vec::with_capacity(count);
    let mut ids = Vec::with_capacity(count);

    for normal in &normals {
        let s = rng.gen_range(config.scale_range.0..=config.scale_range.1);
        scales.push(Vec3::splat(s));

        let orientation = if config.align_to_normal {
            let angle = rng.gen_range(-config.rotation_range..=config.rotation_range);
            orientation_from_normal(*normal, angle)
        } else {
            let angle = rng.gen_range(-config.rotation_range..=config.rotation_range);
            Quat::from_rotation_y(angle)
        };
        orientations.push(orientation);

        ids.push(rng.gen::<f32>());
    }

    let distribution = match mode {
        ScatterMode::Random => DistributionMethod::RandomScatter {
            seed: config.seed,
            count: config.count,
        },
        ScatterMode::PoissonDisk => DistributionMethod::PoissonDisk {
            seed: config.seed,
            min_distance: config.min_distance,
        },
    };

    PointCloud {
        id: 0,
        name: "Scatter".into(),
        positions,
        attributes: PointAttributes {
            scales: Some(scales),
            orientations: Some(orientations),
            proto_indices: vec![0; count],
            ids: Some(ids),
        },
        prototype_ids: vec![],
        transform: Transform::default(),
        distribution,
    }
}

/// Random scatter: area-weighted triangle sampling.
fn scatter_random(
    mesh: &Mesh,
    mesh_transform: &Mat4,
    config: &ScatterConfig,
    rng: &mut StdRng,
) -> (Vec<Vec3>, Vec<Vec3>) {
    if mesh.triangle_count() == 0 {
        return (vec![], vec![]);
    }

    let (_areas, cdf) = build_area_cdf(mesh);
    let normal_mat = mesh_transform.inverse().transpose();

    let mut positions = Vec::with_capacity(config.count);
    let mut normals = Vec::with_capacity(config.count);

    for _ in 0..config.count {
        let tri_idx = pick_triangle(&cdf, rng);
        if let Some((v0, v1, v2)) = get_triangle(mesh, tri_idx) {
            let (local_pos, local_normal) = sample_triangle(v0, v1, v2, rng);
            let world_pos = mesh_transform.transform_point3(local_pos);
            let world_normal = normal_mat
                .transform_vector3(local_normal)
                .normalize_or_zero();
            positions.push(world_pos);
            normals.push(world_normal);
        }
    }

    (positions, normals)
}

/// Poisson disk scatter: oversample then reject points too close together.
fn scatter_poisson(
    mesh: &Mesh,
    mesh_transform: &Mat4,
    config: &ScatterConfig,
    rng: &mut StdRng,
) -> (Vec<Vec3>, Vec<Vec3>) {
    if mesh.triangle_count() == 0 || config.min_distance <= 0.0 {
        return scatter_random(mesh, mesh_transform, config, rng);
    }

    // Oversample at 4x
    let oversample = config.count * 4;
    let oversample_config = ScatterConfig {
        count: oversample,
        ..*config
    };
    let (candidates, candidate_normals) =
        scatter_random(mesh, mesh_transform, &oversample_config, rng);

    // Spatial hash grid for fast neighbor lookup
    let cell_size = config.min_distance;
    let min_dist_sq = config.min_distance * config.min_distance;

    let mut grid: std::collections::HashMap<(i32, i32, i32), Vec<usize>> =
        std::collections::HashMap::new();
    let mut accepted_positions = Vec::with_capacity(config.count);
    let mut accepted_normals = Vec::with_capacity(config.count);

    for (idx, &pos) in candidates.iter().enumerate() {
        if accepted_positions.len() >= config.count {
            break;
        }

        let cell = (
            (pos.x / cell_size).floor() as i32,
            (pos.y / cell_size).floor() as i32,
            (pos.z / cell_size).floor() as i32,
        );

        // Check 3x3x3 neighborhood
        let mut too_close = false;
        'outer: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let neighbor = (cell.0 + dx, cell.1 + dy, cell.2 + dz);
                    if let Some(indices) = grid.get(&neighbor) {
                        for &existing_idx in indices {
                            let diff: Vec3 = accepted_positions[existing_idx] - pos;
                            let dist_sq = diff.length_squared();
                            if dist_sq < min_dist_sq {
                                too_close = true;
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }

        if !too_close {
            let accepted_idx = accepted_positions.len();
            accepted_positions.push(pos);
            accepted_normals.push(candidate_normals[idx]);
            grid.entry(cell).or_default().push(accepted_idx);
        }
    }

    (accepted_positions, accepted_normals)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a flat plane mesh (2 triangles, 10x10 in XZ).
    fn make_plane() -> Mesh {
        let positions = vec![
            Vec3::new(-5.0, 0.0, -5.0),
            Vec3::new(5.0, 0.0, -5.0),
            Vec3::new(5.0, 0.0, 5.0),
            Vec3::new(-5.0, 0.0, 5.0),
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        Mesh::new(positions, indices, None)
    }

    /// Create a tilted plane (45 deg around X axis).
    fn make_tilted_plane() -> Mesh {
        let angle = std::f32::consts::FRAC_PI_4;
        let c = angle.cos();
        let s = angle.sin();
        let positions = vec![
            Vec3::new(-5.0, -5.0 * s, -5.0 * c),
            Vec3::new(5.0, -5.0 * s, -5.0 * c),
            Vec3::new(5.0, 5.0 * s, 5.0 * c),
            Vec3::new(-5.0, 5.0 * s, 5.0 * c),
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        Mesh::new(positions, indices, None)
    }

    #[test]
    fn random_scatter_count() {
        let plane = make_plane();
        let config = ScatterConfig {
            count: 500,
            seed: 123,
            ..Default::default()
        };
        let cloud = scatter_on_surface(&plane, &Mat4::IDENTITY, ScatterMode::Random, &config);
        assert_eq!(cloud.positions.len(), 500);
    }

    #[test]
    fn random_scatter_on_plane() {
        let plane = make_plane();
        let config = ScatterConfig {
            count: 200,
            seed: 42,
            ..Default::default()
        };
        let cloud = scatter_on_surface(&plane, &Mat4::IDENTITY, ScatterMode::Random, &config);

        // All points should have y ~= 0 (on the XZ plane)
        for pos in &cloud.positions {
            assert!(
                pos.y.abs() < 0.01,
                "Point y={} should be ~0 on flat plane",
                pos.y
            );
        }
    }

    #[test]
    fn random_scatter_deterministic() {
        let plane = make_plane();
        let config = ScatterConfig {
            count: 100,
            seed: 999,
            ..Default::default()
        };
        let cloud1 = scatter_on_surface(&plane, &Mat4::IDENTITY, ScatterMode::Random, &config);
        let cloud2 = scatter_on_surface(&plane, &Mat4::IDENTITY, ScatterMode::Random, &config);

        assert_eq!(cloud1.positions.len(), cloud2.positions.len());
        for (a, b) in cloud1.positions.iter().zip(cloud2.positions.iter()) {
            assert!(
                (*a - *b).length() < 1e-6,
                "Same seed should produce identical results"
            );
        }
    }

    #[test]
    fn poisson_min_distance() {
        let plane = make_plane();
        let min_dist = 1.0;
        let config = ScatterConfig {
            count: 50,
            min_distance: min_dist,
            seed: 42,
            ..Default::default()
        };
        let cloud = scatter_on_surface(&plane, &Mat4::IDENTITY, ScatterMode::PoissonDisk, &config);

        // Check all pairwise distances >= min_distance
        for i in 0..cloud.positions.len() {
            for j in (i + 1)..cloud.positions.len() {
                let dist = (cloud.positions[i] - cloud.positions[j]).length();
                assert!(
                    dist >= min_dist - 0.01,
                    "Points {} and {} too close: {:.3} < {:.3}",
                    i,
                    j,
                    dist,
                    min_dist
                );
            }
        }
    }

    #[test]
    fn scatter_area_weighted() {
        // Create mesh with 1 tiny triangle and 1 huge triangle
        let positions = vec![
            // Tiny triangle (0.01 area)
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.1, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.1),
            // Huge triangle (50 area)
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 10.0),
        ];
        let indices = vec![0, 1, 2, 3, 4, 5];
        let mesh = Mesh::new(positions, indices, None);

        let config = ScatterConfig {
            count: 1000,
            seed: 42,
            ..Default::default()
        };
        let cloud = scatter_on_surface(&mesh, &Mat4::IDENTITY, ScatterMode::Random, &config);

        // Count points in tiny triangle region (x < 0.15, z < 0.15)
        let tiny_count = cloud
            .positions
            .iter()
            .filter(|p| p.x < 0.15 && p.z < 0.15 && p.x >= 0.0 && p.z >= 0.0)
            .count();

        // Tiny triangle has ~0.005 area vs ~50 total = ~0.01% of samples
        // With 1000 samples, expect < 5 points in tiny triangle
        assert!(
            tiny_count < 50,
            "Tiny triangle got {} points (expected very few)",
            tiny_count
        );
    }

    #[test]
    fn scatter_normal_alignment() {
        let plane = make_tilted_plane();
        let config = ScatterConfig {
            count: 50,
            seed: 42,
            align_to_normal: true,
            rotation_range: 0.0, // No random twist so alignment is pure
            ..Default::default()
        };
        let cloud = scatter_on_surface(&plane, &Mat4::IDENTITY, ScatterMode::Random, &config);

        // Check that orientations are not all identity (they should align to the tilted normal)
        let all_identity = cloud
            .attributes
            .orientations
            .as_ref()
            .unwrap()
            .iter()
            .all(|q| (*q - Quat::IDENTITY).length_squared() < 0.01);
        assert!(
            !all_identity,
            "Tilted surface should produce non-identity orientations"
        );
    }

    #[test]
    fn scatter_empty_mesh() {
        let mesh = Mesh::new(vec![], vec![], None);
        let config = ScatterConfig {
            count: 100,
            seed: 42,
            ..Default::default()
        };
        let cloud = scatter_on_surface(&mesh, &Mat4::IDENTITY, ScatterMode::Random, &config);
        assert!(cloud.positions.is_empty());
    }
}
