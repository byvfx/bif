//! Scatter points on surfaces, grids, and spheres.
//!
//! Provides random, Poisson disk, grid, and sphere distribution of points,
//! producing `PointCloud` objects for point preview and future instancing.

use std::collections::HashMap;

use bif_math::{Mat4, Quat, Vec3};
use rand::prelude::*;

use crate::mesh::Mesh;
use crate::point_cloud::{DistributionMethod, PointAttributes, PointCloud};
use crate::scene::Transform;

/// Point generation source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointSource {
    /// Scatter on a mesh surface (requires input mesh).
    Surface,
    /// Regular 3D grid.
    Grid,
    /// Spherical distribution (surface or volume).
    Sphere,
}

/// Scatter distribution mode (surface source only).
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

impl ScatterConfig {
    /// Return scale range with min <= max guaranteed.
    ///
    /// Swaps if inverted; panics in debug if either value is non-finite.
    pub fn scale_range_normalized(&self) -> (f32, f32) {
        debug_assert!(
            self.scale_range.0.is_finite() && self.scale_range.1.is_finite(),
            "scale_range contains non-finite value: {:?}",
            self.scale_range
        );
        if self.scale_range.0 <= self.scale_range.1 {
            self.scale_range
        } else {
            (self.scale_range.1, self.scale_range.0)
        }
    }
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
/// `prototype_ids` maps proto_indices to scene prototype IDs.
pub fn scatter_on_surface(
    mesh: &Mesh,
    mesh_transform: &Mat4,
    mode: ScatterMode,
    config: &ScatterConfig,
    prototype_ids: Vec<usize>,
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
    let (s_lo, s_hi) = config.scale_range_normalized();

    for normal in &normals {
        let s = rng.gen_range(s_lo..=s_hi);
        scales.push(Vec3::splat(s));

        let orientation = if config.align_to_normal {
            let angle = if config.rotation_range > 0.0 {
                rng.gen_range(0.0..config.rotation_range)
            } else {
                0.0
            };
            orientation_from_normal(*normal, angle)
        } else {
            let angle = if config.rotation_range > 0.0 {
                rng.gen_range(0.0..config.rotation_range)
            } else {
                0.0
            };
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
        prototype_ids,
        transform: Transform::default(),
        distribution,
        invisible_ids: Vec::new(),
    }
}

/// Generate points on a regular 3D grid centered at origin.
///
/// Points per axis = `floor(size[axis] / spacing) + 1` (or 1 if axis size is 0).
/// Total point count is capped at `max_count`.
/// Per-point scales, orientations, and IDs are generated from `config`.
pub fn generate_grid_points(
    size: [f32; 3],
    spacing: f32,
    max_count: u32,
    config: &ScatterConfig,
) -> PointCloud {
    let spacing = spacing.max(0.001);

    let counts: [usize; 3] = [
        if size[0] <= 0.0 {
            1
        } else {
            (size[0] / spacing).floor() as usize + 1
        },
        if size[1] <= 0.0 {
            1
        } else {
            (size[1] / spacing).floor() as usize + 1
        },
        if size[2] <= 0.0 {
            1
        } else {
            (size[2] / spacing).floor() as usize + 1
        },
    ];

    let total = counts[0]
        .saturating_mul(counts[1])
        .saturating_mul(counts[2])
        .min(max_count as usize);
    let mut positions = Vec::with_capacity(total);

    let half = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];

    'outer: for iz in 0..counts[2] {
        for iy in 0..counts[1] {
            for ix in 0..counts[0] {
                if positions.len() >= total {
                    break 'outer;
                }
                let x = if counts[0] == 1 {
                    0.0
                } else {
                    ix as f32 * spacing - half[0]
                };
                let y = if counts[1] == 1 {
                    0.0
                } else {
                    iy as f32 * spacing - half[1]
                };
                let z = if counts[2] == 1 {
                    0.0
                } else {
                    iz as f32 * spacing - half[2]
                };
                positions.push(Vec3::new(x, y, z));
            }
        }
    }

    // Generate per-point attributes from config
    let count = positions.len();
    let mut rng = StdRng::seed_from_u64(config.seed);
    let mut scales = Vec::with_capacity(count);
    let mut orientations = Vec::with_capacity(count);
    let mut ids = Vec::with_capacity(count);
    let (s_lo, s_hi) = config.scale_range_normalized();

    for _ in 0..count {
        let s = rng.gen_range(s_lo..=s_hi);
        scales.push(Vec3::splat(s));

        let angle = if config.rotation_range > 0.0 {
            rng.gen_range(0.0..config.rotation_range)
        } else {
            0.0
        };
        orientations.push(Quat::from_rotation_y(angle));

        ids.push(rng.gen::<f32>());
    }

    PointCloud {
        id: 0,
        name: "Grid Points".into(),
        positions,
        attributes: PointAttributes {
            scales: Some(scales),
            orientations: Some(orientations),
            proto_indices: vec![0; count],
            ids: Some(ids),
        },
        prototype_ids: vec![],
        transform: Transform::default(),
        distribution: DistributionMethod::Grid { spacing },
        invisible_ids: Vec::new(),
    }
}

/// Generate points on or inside a sphere.
///
/// `on_surface = true` uses Fibonacci sphere for even coverage.
/// `on_surface = false` uses rejection sampling for uniform volume fill.
/// Capped at `max_count`. Per-point attributes generated from `config`.
pub fn generate_sphere_points(
    radius: f32,
    count: u32,
    on_surface: bool,
    max_count: u32,
    config: &ScatterConfig,
) -> PointCloud {
    let n = count.min(max_count) as usize;
    let mut positions = Vec::with_capacity(n);

    if on_surface {
        // Fibonacci sphere — deterministic, even distribution
        let golden_ratio = (1.0 + 5.0_f32.sqrt()) / 2.0;
        for i in 0..n {
            let theta = 2.0 * std::f32::consts::PI * i as f32 / golden_ratio;
            let phi = (1.0 - 2.0 * (i as f32 + 0.5) / n as f32).acos();
            let x = radius * phi.sin() * theta.cos();
            let y = radius * phi.sin() * theta.sin();
            let z = radius * phi.cos();
            positions.push(Vec3::new(x, y, z));
        }
    } else {
        // Rejection sampling for uniform volume
        let mut rng = StdRng::seed_from_u64(config.seed);
        let r_sq = radius * radius;
        while positions.len() < n {
            let x = rng.gen_range(-radius..=radius);
            let y = rng.gen_range(-radius..=radius);
            let z = rng.gen_range(-radius..=radius);
            if x * x + y * y + z * z <= r_sq {
                positions.push(Vec3::new(x, y, z));
            }
        }
    }

    // Generate per-point attributes (separate seed to keep positions deterministic)
    let pt_count = positions.len();
    let mut attr_rng = StdRng::seed_from_u64(config.seed.wrapping_add(1));
    let mut scales = Vec::with_capacity(pt_count);
    let mut orientations = Vec::with_capacity(pt_count);
    let mut ids = Vec::with_capacity(pt_count);
    let (s_lo, s_hi) = config.scale_range_normalized();

    for _ in 0..pt_count {
        let s = attr_rng.gen_range(s_lo..=s_hi);
        scales.push(Vec3::splat(s));

        let angle = if config.rotation_range > 0.0 {
            attr_rng.gen_range(0.0..config.rotation_range)
        } else {
            0.0
        };
        orientations.push(Quat::from_rotation_y(angle));

        ids.push(attr_rng.gen::<f32>());
    }

    PointCloud {
        id: 0,
        name: "Sphere Points".into(),
        positions,
        attributes: PointAttributes {
            scales: Some(scales),
            orientations: Some(orientations),
            proto_indices: vec![0; pt_count],
            ids: Some(ids),
        },
        prototype_ids: vec![],
        transform: Transform::default(),
        distribution: DistributionMethod::Sphere {
            seed: config.seed,
            on_surface,
        },
        invisible_ids: Vec::new(),
    }
}

/// Repulsion-based point relaxation for more even spacing.
///
/// For each iteration, builds a spatial hash grid and applies repulsion
/// forces (linear falloff) between nearby points. If `surface_mesh` is
/// provided, points are projected back onto the nearest triangle after
/// each step.
pub fn repulsion_relax(
    positions: &mut [Vec3],
    iterations: u32,
    scale_radii: f32,
    max_relax_radius: f32,
    surface_mesh: Option<(&Mesh, &Mat4)>,
) {
    if iterations == 0 || positions.is_empty() {
        return;
    }

    // Warn about expensive surface projection (brute-force O(N*T*I))
    if let Some((mesh, _)) = surface_mesh {
        let cost = positions.len() as u64 * mesh.triangle_count() as u64 * iterations as u64;
        if cost > 10_000_000 {
            log::warn!(
                "repulsion_relax: surface projection will be slow ({} pts * {} tris * {} iters = {}M ops)",
                positions.len(),
                mesh.triangle_count(),
                iterations,
                cost / 1_000_000,
            );
        }
    }

    let radius = max_relax_radius * scale_radii;
    let radius_sq = radius * radius;

    for _ in 0..iterations {
        // Build spatial hash
        let cell_size = radius.max(0.01);
        let mut grid: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
        for (i, &p) in positions.iter().enumerate() {
            let cell = (
                (p.x / cell_size).floor() as i32,
                (p.y / cell_size).floor() as i32,
                (p.z / cell_size).floor() as i32,
            );
            grid.entry(cell).or_default().push(i);
        }

        // Compute displacement for each point
        let mut displacements = vec![Vec3::ZERO; positions.len()];
        for (i, &p) in positions.iter().enumerate() {
            let cell = (
                (p.x / cell_size).floor() as i32,
                (p.y / cell_size).floor() as i32,
                (p.z / cell_size).floor() as i32,
            );

            let mut repulsion = Vec3::ZERO;
            let mut neighbor_count = 0u32;

            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let neighbor_cell = (cell.0 + dx, cell.1 + dy, cell.2 + dz);
                        if let Some(indices) = grid.get(&neighbor_cell) {
                            for &j in indices {
                                if j == i {
                                    continue;
                                }
                                let diff = p - positions[j];
                                let dist_sq = diff.length_squared();
                                if dist_sq < radius_sq && dist_sq > 1e-10 {
                                    let dist = dist_sq.sqrt();
                                    // Repulsion strength falls off linearly
                                    let strength = 1.0 - dist / radius;
                                    repulsion += diff.normalize_or_zero() * strength;
                                    neighbor_count += 1;
                                }
                            }
                        }
                    }
                }
            }

            if neighbor_count > 0 {
                displacements[i] = repulsion / neighbor_count as f32 * radius * 0.5;
            }
        }

        // Apply displacements
        for (i, pos) in positions.iter_mut().enumerate() {
            *pos += displacements[i];
        }

        // Project back onto surface if constrained
        if let Some((mesh, transform)) = surface_mesh {
            for pos in positions.iter_mut() {
                if let Some(closest) = nearest_point_on_mesh(mesh, transform, *pos) {
                    *pos = closest;
                }
            }
        }
    }
}

/// Find the nearest point on a mesh surface to the given query point.
///
/// Brute-force scan over all triangles (suitable for small meshes;
/// BVH acceleration can be added later).
fn nearest_point_on_mesh(mesh: &Mesh, transform: &Mat4, query: Vec3) -> Option<Vec3> {
    if mesh.triangle_count() == 0 {
        return None;
    }

    let mut best_dist_sq = f32::INFINITY;
    let mut best_point = query;

    for tri_idx in 0..mesh.triangle_count() {
        if let Some((v0, v1, v2)) = get_triangle(mesh, tri_idx) {
            let wv0 = transform.transform_point3(v0);
            let wv1 = transform.transform_point3(v1);
            let wv2 = transform.transform_point3(v2);
            let closest = closest_point_on_triangle(query, wv0, wv1, wv2);
            let dist_sq = (closest - query).length_squared();
            if dist_sq < best_dist_sq {
                best_dist_sq = dist_sq;
                best_point = closest;
            }
        }
    }

    Some(best_point)
}

/// Closest point on triangle to a query point (3D projection + clamping).
fn closest_point_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;

    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }

    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return a + ab * v;
    }

    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return a + ac * w;
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w;
    }

    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    a + ab * v + ac * w
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
        let cloud = scatter_on_surface(
            &plane,
            &Mat4::IDENTITY,
            ScatterMode::Random,
            &config,
            vec![0],
        );
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
        let cloud = scatter_on_surface(
            &plane,
            &Mat4::IDENTITY,
            ScatterMode::Random,
            &config,
            vec![0],
        );

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
        let cloud1 = scatter_on_surface(
            &plane,
            &Mat4::IDENTITY,
            ScatterMode::Random,
            &config,
            vec![0],
        );
        let cloud2 = scatter_on_surface(
            &plane,
            &Mat4::IDENTITY,
            ScatterMode::Random,
            &config,
            vec![0],
        );

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
        let cloud = scatter_on_surface(
            &plane,
            &Mat4::IDENTITY,
            ScatterMode::PoissonDisk,
            &config,
            vec![0],
        );

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
        let cloud = scatter_on_surface(
            &mesh,
            &Mat4::IDENTITY,
            ScatterMode::Random,
            &config,
            vec![0],
        );

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
        let cloud = scatter_on_surface(
            &plane,
            &Mat4::IDENTITY,
            ScatterMode::Random,
            &config,
            vec![0],
        );

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
        let cloud = scatter_on_surface(
            &mesh,
            &Mat4::IDENTITY,
            ScatterMode::Random,
            &config,
            vec![0],
        );
        assert!(cloud.positions.is_empty());
    }

    #[test]
    fn grid_points_count() {
        let cloud =
            generate_grid_points([10.0, 0.0, 10.0], 1.0, 1_000_000, &ScatterConfig::default());
        // 11 points per axis (0..10 at spacing 1), Y collapsed = 11 * 1 * 11 = 121
        assert_eq!(cloud.positions.len(), 121);
    }

    #[test]
    fn grid_points_3d() {
        let cloud =
            generate_grid_points([4.0, 4.0, 4.0], 1.0, 1_000_000, &ScatterConfig::default());
        // 5 per axis = 125
        assert_eq!(cloud.positions.len(), 125);
    }

    #[test]
    fn grid_points_flat() {
        let cloud =
            generate_grid_points([10.0, 0.0, 10.0], 1.0, 1_000_000, &ScatterConfig::default());
        // All y should be 0
        for pos in &cloud.positions {
            assert!(
                pos.y.abs() < 0.001,
                "Flat grid should have y=0, got {}",
                pos.y
            );
        }
    }

    #[test]
    fn grid_points_have_scales() {
        let config = ScatterConfig {
            scale_range: (0.5, 1.5),
            ..Default::default()
        };
        let cloud = generate_grid_points([4.0, 0.0, 4.0], 1.0, 1_000_000, &config);
        let scales = cloud
            .attributes
            .scales
            .as_ref()
            .expect("grid should produce scales");
        assert_eq!(scales.len(), cloud.positions.len());
        for s in scales {
            assert!(s.x >= 0.5 && s.x <= 1.5, "scale {} out of range", s.x);
            assert_eq!(s.x, s.y, "scale should be uniform");
            assert_eq!(s.x, s.z, "scale should be uniform");
        }
    }

    #[test]
    fn sphere_surface_points() {
        let radius = 5.0;
        let config = ScatterConfig {
            seed: 42,
            ..Default::default()
        };
        let cloud = generate_sphere_points(radius, 200, true, 1_000_000, &config);
        assert_eq!(cloud.positions.len(), 200);
        for pos in &cloud.positions {
            let dist = pos.length();
            assert!(
                (dist - radius).abs() < 0.01,
                "Surface point should be at radius {}, got {}",
                radius,
                dist
            );
        }
    }

    #[test]
    fn sphere_volume_points() {
        let radius = 5.0;
        let config = ScatterConfig {
            seed: 42,
            ..Default::default()
        };
        let cloud = generate_sphere_points(radius, 500, false, 1_000_000, &config);
        assert_eq!(cloud.positions.len(), 500);
        for pos in &cloud.positions {
            let dist = pos.length();
            assert!(
                dist <= radius + 0.01,
                "Volume point should be within radius {}, got {}",
                radius,
                dist
            );
        }
    }

    #[test]
    fn sphere_points_have_scales() {
        let config = ScatterConfig {
            seed: 42,
            scale_range: (0.3, 2.0),
            ..Default::default()
        };
        let cloud = generate_sphere_points(3.0, 100, true, 1_000_000, &config);
        let scales = cloud
            .attributes
            .scales
            .as_ref()
            .expect("sphere should produce scales");
        assert_eq!(scales.len(), cloud.positions.len());
        for s in scales {
            assert!(s.x >= 0.3 && s.x <= 2.0, "scale {} out of range", s.x);
        }
    }

    #[test]
    fn relax_increases_min_distance() {
        // Create clustered points
        let mut positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.1, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.1),
            Vec3::new(0.1, 0.0, 0.1),
        ];

        let min_before = min_pairwise_distance(&positions);
        repulsion_relax(&mut positions, 5, 1.0, 2.0, None);
        let min_after = min_pairwise_distance(&positions);

        assert!(
            min_after > min_before,
            "Relax should increase min distance: before={:.3}, after={:.3}",
            min_before,
            min_after
        );
    }

    #[test]
    fn relax_deterministic() {
        let base = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.1, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.1),
        ];

        let mut a = base.clone();
        let mut b = base;
        repulsion_relax(&mut a, 3, 1.0, 2.0, None);
        repulsion_relax(&mut b, 3, 1.0, 2.0, None);

        for (pa, pb) in a.iter().zip(b.iter()) {
            assert!(
                (*pa - *pb).length() < 1e-6,
                "Same input should produce same output"
            );
        }
    }

    #[test]
    fn max_point_limit_caps() {
        let cloud =
            generate_grid_points([100.0, 100.0, 100.0], 1.0, 1000, &ScatterConfig::default());
        assert!(
            cloud.positions.len() <= 1000,
            "Should cap at max_count=1000, got {}",
            cloud.positions.len()
        );
    }

    #[test]
    fn closest_point_vertex_a() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 0.0, 0.0);
        let c = Vec3::new(0.0, 1.0, 0.0);
        // Query nearest vertex A
        let result = closest_point_on_triangle(Vec3::new(-1.0, -1.0, 0.0), a, b, c);
        assert!((result - a).length() < 1e-5);
    }

    #[test]
    fn closest_point_vertex_b() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 0.0, 0.0);
        let c = Vec3::new(0.0, 1.0, 0.0);
        let result = closest_point_on_triangle(Vec3::new(2.0, -1.0, 0.0), a, b, c);
        assert!((result - b).length() < 1e-5);
    }

    #[test]
    fn closest_point_vertex_c() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 0.0, 0.0);
        let c = Vec3::new(0.0, 1.0, 0.0);
        let result = closest_point_on_triangle(Vec3::new(-1.0, 2.0, 0.0), a, b, c);
        assert!((result - c).length() < 1e-5);
    }

    #[test]
    fn closest_point_edge_ab() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 0.0, 0.0);
        let c = Vec3::new(0.0, 1.0, 0.0);
        // Below edge AB midpoint
        let result = closest_point_on_triangle(Vec3::new(0.5, -1.0, 0.0), a, b, c);
        assert!((result - Vec3::new(0.5, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn closest_point_interior() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 0.0, 0.0);
        let c = Vec3::new(0.0, 1.0, 0.0);
        // Point directly above the triangle interior
        let result = closest_point_on_triangle(Vec3::new(0.2, 0.2, 5.0), a, b, c);
        assert!((result - Vec3::new(0.2, 0.2, 0.0)).length() < 1e-5);
    }

    #[test]
    fn scale_range_normalized_swaps() {
        let cfg = |lo, hi| ScatterConfig {
            scale_range: (lo, hi),
            ..Default::default()
        };
        assert_eq!(cfg(5.0, 1.0).scale_range_normalized(), (1.0, 5.0));
        assert_eq!(cfg(1.0, 5.0).scale_range_normalized(), (1.0, 5.0));
        assert_eq!(cfg(3.0, 3.0).scale_range_normalized(), (3.0, 3.0));
    }

    #[test]
    fn scale_range_equal_no_panic() {
        let config = ScatterConfig {
            scale_range: (1.0, 1.0),
            ..Default::default()
        };
        let cloud = generate_grid_points([4.0, 0.0, 4.0], 1.0, 1_000_000, &config);
        for s in cloud.attributes.scales.as_ref().unwrap() {
            assert!((s.x - 1.0).abs() < 1e-6, "equal range should produce 1.0");
        }
    }

    #[test]
    fn scale_range_inverted_no_panic() {
        let config = ScatterConfig {
            scale_range: (2.0, 0.5),
            ..Default::default()
        };
        let cloud = generate_grid_points([4.0, 0.0, 4.0], 1.0, 1_000_000, &config);
        for s in cloud.attributes.scales.as_ref().unwrap() {
            assert!(
                s.x >= 0.5 && s.x <= 2.0,
                "scale {} out of swapped range",
                s.x
            );
        }
    }

    #[test]
    fn sphere_scale_range_inverted_no_panic() {
        let config = ScatterConfig {
            seed: 42,
            scale_range: (3.0, 1.0),
            ..Default::default()
        };
        let cloud = generate_sphere_points(5.0, 50, true, 1_000_000, &config);
        for s in cloud.attributes.scales.as_ref().unwrap() {
            assert!(
                s.x >= 1.0 && s.x <= 3.0,
                "scale {} out of swapped range",
                s.x
            );
        }
    }

    #[test]
    fn surface_scale_range_inverted_no_panic() {
        let plane = make_plane();
        let config = ScatterConfig {
            count: 50,
            seed: 42,
            scale_range: (1.5, 0.5),
            ..Default::default()
        };
        let cloud = scatter_on_surface(
            &plane,
            &Mat4::IDENTITY,
            ScatterMode::Random,
            &config,
            vec![0],
        );
        for s in cloud.attributes.scales.as_ref().unwrap() {
            assert!(
                s.x >= 0.5 && s.x <= 1.5,
                "scale {} out of swapped range",
                s.x
            );
        }
    }

    /// Helper: compute minimum pairwise distance in a point set.
    fn min_pairwise_distance(positions: &[Vec3]) -> f32 {
        let mut min_dist = f32::INFINITY;
        for i in 0..positions.len() {
            for j in (i + 1)..positions.len() {
                let dist = (positions[i] - positions[j]).length();
                if dist < min_dist {
                    min_dist = dist;
                }
            }
        }
        min_dist
    }
}
