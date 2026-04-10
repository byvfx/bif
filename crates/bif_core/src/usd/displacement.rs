//! CPU vertex displacement for UsdPreviewSurface `displacement` inputs.
//!
//! Samples a heightmap texture per vertex and offsets `Mesh::positions` along
//! the vertex normal. Modifies meshes in place, so both the wgpu viewport and
//! the Embree Ivar renderer pick up displacement without renderer-specific code.
//!
//! Only scalar-along-normal displacement is supported in v1. Subdivision dicing
//! via Embree `rtcSetGeometryDisplacementFunction` is deferred to a later pass.
//!
//! # USD Convention
//!
//! UsdPreviewSurface treats displacement values as offsets around a 0.5 neutral
//! midpoint: `offset = (sample - 0.5) * scale`. A flat 0.5 grayscale heightmap
//! leaves positions unchanged; 1.0 pushes along +normal by `scale/2`, 0.0 pushes
//! along -normal by `scale/2`.
//!
//! # TODO (deferred)
//!
//! Store a side buffer `Mesh::displaced_positions: Option<Vec<Vec3>>` + keep
//! the original `positions` so hot texture swaps don't require a full scene
//! reload. For v1, modification is in place (original is lost).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;

use crate::mesh::Mesh;
use crate::scene::{Material, Scene};

/// Errors returned by the displacement pass.
#[derive(Debug, Error)]
pub enum DisplacementError {
    /// Image loading failure (I/O, decode, unsupported format).
    #[error("image load failed: {0}")]
    Image(#[from] image::ImageError),
    /// Texture path could not be resolved to an absolute filesystem location.
    #[error("cannot resolve displacement texture path: {0:?}")]
    PathResolution(PathBuf),
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Resolve a USD asset path to an absolute filesystem path.
///
/// Absolute paths are returned as-is. Relative paths are joined against
/// `base_dir` if present (typically `Material::source_dir`).
fn resolve_path(tex_path: &str, base_dir: Option<&Path>) -> Option<PathBuf> {
    let p = Path::new(tex_path);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }
    base_dir.map(|base| base.join(p))
}

// ---------------------------------------------------------------------------
// Heightmap loading
// ---------------------------------------------------------------------------

/// Load a heightmap texture as grayscale f32 in row-major order (normally 0..=1).
///
/// Uses the `image` crate (with the `openexr` feature) for PNG / JPG / EXR.
/// Multi-channel images are converted to luminance; EXR float values are
/// preserved so HDR heightmaps work without quantization.
pub fn load_heightmap_sync(path: &Path) -> Result<(u32, u32, Vec<f32>), DisplacementError> {
    let img = image::open(path)?;
    let (w, h) = (img.width(), img.height());
    let luma = img.to_luma32f();
    Ok((w, h, luma.into_raw()))
}

// ---------------------------------------------------------------------------
// Sampling
// ---------------------------------------------------------------------------

/// Bilinear sample a single-channel grayscale image.
///
/// UVs wrap via `fract()` (matches USD's default `wrapS = repeat`). Handles
/// 1x1 and degenerate sizes by clamping indices. Returns 0.0 on an empty image.
pub fn bilinear_sample(data: &[f32], w: u32, h: u32, u: f32, v: f32) -> f32 {
    if w == 0 || h == 0 || data.is_empty() {
        return 0.0;
    }
    // Wrap into [0, 1) then scale to pixel space. `fract` handles negatives
    // (e.g. -0.25 -> 0.75) because f32::fract returns the signed fractional
    // part — we add 1.0 and fract again for safety.
    let wrap = |t: f32| {
        let f = t.fract();
        if f < 0.0 {
            f + 1.0
        } else {
            f
        }
    };
    let uf = wrap(u) * (w.saturating_sub(1) as f32);
    let vf = wrap(v) * (h.saturating_sub(1) as f32);
    let u0 = uf.floor() as u32;
    let v0 = vf.floor() as u32;
    let u1 = (u0 + 1).min(w - 1);
    let v1 = (v0 + 1).min(h - 1);
    let fu = uf - uf.floor();
    let fv = vf - vf.floor();
    let idx = |x: u32, y: u32| (y * w + x) as usize;
    let a = data[idx(u0, v0)];
    let b = data[idx(u1, v0)];
    let c = data[idx(u0, v1)];
    let d = data[idx(u1, v1)];
    let ab = a * (1.0 - fu) + b * fu;
    let cd = c * (1.0 - fu) + d * fu;
    ab * (1.0 - fv) + cd * fv
}

// ---------------------------------------------------------------------------
// Core displacement (pure, testable with in-memory heightmap)
// ---------------------------------------------------------------------------

/// Apply displacement to a mesh using a pre-loaded heightmap.
///
/// Returns `true` if the mesh was displaced, `false` if preconditions weren't
/// met (missing normals, missing UVs, attribute length mismatch). Non-fatal
/// skip cases log a `warn!`.
///
/// This is the pure function that can be unit-tested with synthetic heightmap
/// data — no file I/O. See [`apply_displacement`] for the file-loading wrapper.
pub fn apply_displacement_to_mesh(
    mesh: &mut Mesh,
    material: &Material,
    heightmap: &[f32],
    w: u32,
    h: u32,
) -> bool {
    let Some(normals) = &mesh.normals else {
        log::warn!("displacement: mesh missing normals, skipping");
        return false;
    };
    let Some(uvs) = &mesh.uvs else {
        log::warn!("displacement: mesh missing UVs, skipping");
        return false;
    };
    if mesh.positions.len() != normals.len() || mesh.positions.len() != uvs.len() {
        log::warn!(
            "displacement: vertex attribute length mismatch (pos={}, n={}, uv={}), skipping",
            mesh.positions.len(),
            normals.len(),
            uvs.len()
        );
        return false;
    }

    let scale = material.displacement_scale;
    // Clone the attribute borrows so we can mutate positions inside the loop.
    // Cheap — these are Vec<Vec3>/Vec<[f32;2]> and scene load is a one-shot path.
    let normals_copy = normals.clone();
    let uvs_copy = uvs.clone();
    for (i, pos) in mesh.positions.iter_mut().enumerate() {
        let uv = uvs_copy[i];
        let sample = bilinear_sample(heightmap, w, h, uv[0], uv[1]);
        // USD convention: 0.5 is neutral; displace around the midpoint.
        let offset = (sample - 0.5) * scale;
        *pos += normals_copy[i] * offset;
    }

    mesh.recompute_bounds();
    true
}

// ---------------------------------------------------------------------------
// File-loading wrapper (called from the loader)
// ---------------------------------------------------------------------------

/// Apply displacement to a single mesh by loading its material's displacement
/// texture from disk. Returns `Ok(true)` if the mesh was displaced, `Ok(false)`
/// if skipped (no texture, no UVs, no normals, or path not resolvable).
pub fn apply_displacement(mesh: &mut Mesh, material: &Material) -> Result<bool, DisplacementError> {
    let Some(tex_path) = &material.displacement_texture else {
        return Ok(false);
    };

    let base_dir = material.source_dir.as_deref();
    let Some(resolved) = resolve_path(tex_path.as_ref(), base_dir) else {
        log::warn!(
            "displacement: cannot resolve path '{}' (no base_dir)",
            tex_path
        );
        return Ok(false);
    };

    if !resolved.exists() {
        log::warn!(
            "displacement: texture not found at {:?}, skipping",
            resolved
        );
        return Ok(false);
    }

    let (w, h, data) = load_heightmap_sync(&resolved)?;
    log::info!(
        "displacement: loaded {}x{} heightmap from {:?} (scale={})",
        w,
        h,
        resolved,
        material.displacement_scale
    );
    Ok(apply_displacement_to_mesh(mesh, material, &data, w, h))
}

// ---------------------------------------------------------------------------
// Scene-level pass (walks prototypes, unwraps Arcs, applies displacement)
// ---------------------------------------------------------------------------

/// Walk every prototype in the scene and apply displacement to any mesh whose
/// bound material has a `displacement_texture`. Returns the number of meshes
/// successfully displaced.
///
/// Uses `Arc::make_mut` to get exclusive access to the Mesh — during the load
/// path, refcounts are 1, so this is effectively a pointer cast.
pub fn apply_displacement_to_scene(scene: &mut Scene) -> usize {
    let mut count = 0;
    for proto_arc in scene.prototypes.iter_mut() {
        let proto = Arc::make_mut(proto_arc);
        let Some(mat_arc) = proto.material.clone() else {
            continue;
        };
        let mesh = Arc::make_mut(&mut proto.mesh);
        match apply_displacement(mesh, &mat_arc) {
            Ok(true) => {
                log::info!(
                    "displacement: applied to prototype '{}' ({} verts)",
                    proto.name,
                    mesh.positions.len()
                );
                count += 1;
            }
            Ok(false) => {}
            Err(e) => log::warn!("displacement: failed for prototype '{}': {}", proto.name, e),
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Tests (no USD DLLs required)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bif_math::{Aabb, Vec3};

    // -- helpers -----------------------------------------------------------

    fn flat_plane_mesh() -> Mesh {
        // 2 vertices, flat along X axis, Y-up normals, full UV range.
        Mesh {
            positions: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)],
            normals: Some(vec![Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 1.0, 0.0)]),
            uvs: Some(vec![[0.0, 0.0], [1.0, 1.0]]),
            indices: vec![],
            face_material_ids: None,
            bounds: Aabb::EMPTY,
            subdivision_scheme: crate::usd::SubdivisionScheme::None,
            face_vertex_counts: None,
            polygon_indices: None,
            crease_indices: None,
            crease_lengths: None,
            crease_sharpnesses: None,
            vertices_orig: None,
            facevarying_uvs: None,
            facevarying_uv_indices: None,
            display_color: None,
        }
    }

    fn material_with_disp(scale: f32) -> Material {
        Material {
            displacement_texture: Some(Arc::from("dummy.exr")),
            displacement_scale: scale,
            ..Material::default()
        }
    }

    // -- bilinear_sample ---------------------------------------------------

    #[test]
    fn bilinear_sample_uniform_grid() {
        // 2x2 all-0.5 grid: any sample returns 0.5.
        let data = vec![0.5, 0.5, 0.5, 0.5];
        assert!((bilinear_sample(&data, 2, 2, 0.0, 0.0) - 0.5).abs() < 1e-6);
        assert!((bilinear_sample(&data, 2, 2, 0.5, 0.5) - 0.5).abs() < 1e-6);
        assert!((bilinear_sample(&data, 2, 2, 1.0, 1.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn bilinear_sample_corners() {
        // Row-major 2x2: [top-left, top-right, bottom-left, bottom-right]
        // = [0, 1, 0, 1] — vertical stripe, left dark, right bright.
        // Note: u=1.0 wraps to u=0.0 (USD default repeat), so we use 0.999
        // to test "near right edge" without triggering the wrap.
        let data = vec![0.0, 1.0, 0.0, 1.0];
        assert!((bilinear_sample(&data, 2, 2, 0.0, 0.0) - 0.0).abs() < 1e-6);
        assert!((bilinear_sample(&data, 2, 2, 0.999, 0.0) - 1.0).abs() < 1e-3);
        assert!((bilinear_sample(&data, 2, 2, 0.0, 0.999) - 0.0).abs() < 1e-6);
        assert!((bilinear_sample(&data, 2, 2, 0.999, 0.999) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn bilinear_sample_center_interpolates() {
        let data = vec![0.0, 1.0, 0.0, 1.0];
        // u = 0.5 should give mid-gray between left and right columns.
        let mid = bilinear_sample(&data, 2, 2, 0.5, 0.0);
        assert!((mid - 0.5).abs() < 1e-6, "expected 0.5, got {mid}");
    }

    #[test]
    fn bilinear_sample_wraps_uvs() {
        let data = vec![0.0, 1.0, 0.0, 1.0];
        // u = 1.5 should wrap to 0.5 -> mid value.
        let wrapped = bilinear_sample(&data, 2, 2, 1.5, 0.0);
        let straight = bilinear_sample(&data, 2, 2, 0.5, 0.0);
        assert!((wrapped - straight).abs() < 1e-6);
    }

    // -- apply_displacement_to_mesh ----------------------------------------

    #[test]
    fn displacement_neutral_heightmap_leaves_positions_unchanged() {
        let mut mesh = flat_plane_mesh();
        let original = mesh.positions.clone();
        let material = material_with_disp(1.0);
        let heightmap = vec![0.5; 4]; // 2x2 all neutral
        let applied = apply_displacement_to_mesh(&mut mesh, &material, &heightmap, 2, 2);
        assert!(applied);
        for (a, b) in mesh.positions.iter().zip(original.iter()) {
            assert!((a.x - b.x).abs() < 1e-6);
            assert!((a.y - b.y).abs() < 1e-6);
            assert!((a.z - b.z).abs() < 1e-6);
        }
    }

    #[test]
    fn displacement_max_heightmap_offsets_by_half_scale() {
        let mut mesh = flat_plane_mesh();
        let material = material_with_disp(2.0);
        let heightmap = vec![1.0; 4]; // all-max -> (1.0 - 0.5) * 2.0 = +1.0 along normal
        let applied = apply_displacement_to_mesh(&mut mesh, &material, &heightmap, 2, 2);
        assert!(applied);
        // Normals are +Y, so positions should move +1.0 on Y.
        assert!((mesh.positions[0].y - 1.0).abs() < 1e-5);
        assert!((mesh.positions[1].y - 1.0).abs() < 1e-5);
        // X and Z unchanged.
        assert!((mesh.positions[0].x - 0.0).abs() < 1e-6);
        assert!((mesh.positions[1].x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn displacement_min_heightmap_offsets_negative() {
        let mut mesh = flat_plane_mesh();
        let material = material_with_disp(2.0);
        let heightmap = vec![0.0; 4]; // all-min -> (0.0 - 0.5) * 2.0 = -1.0 along normal
        let applied = apply_displacement_to_mesh(&mut mesh, &material, &heightmap, 2, 2);
        assert!(applied);
        assert!((mesh.positions[0].y - (-1.0)).abs() < 1e-5);
        assert!((mesh.positions[1].y - (-1.0)).abs() < 1e-5);
    }

    #[test]
    fn displacement_skips_when_uvs_missing() {
        let mut mesh = flat_plane_mesh();
        mesh.uvs = None;
        let original = mesh.positions.clone();
        let material = material_with_disp(1.0);
        let heightmap = vec![1.0; 4];
        let applied = apply_displacement_to_mesh(&mut mesh, &material, &heightmap, 2, 2);
        assert!(!applied);
        assert_eq!(mesh.positions, original);
    }

    #[test]
    fn displacement_skips_when_normals_missing() {
        let mut mesh = flat_plane_mesh();
        mesh.normals = None;
        let original = mesh.positions.clone();
        let material = material_with_disp(1.0);
        let heightmap = vec![1.0; 4];
        let applied = apply_displacement_to_mesh(&mut mesh, &material, &heightmap, 2, 2);
        assert!(!applied);
        assert_eq!(mesh.positions, original);
    }

    #[test]
    fn displacement_recomputes_bounds_after_offset() {
        let mut mesh = flat_plane_mesh();
        mesh.recompute_bounds();
        let flat_max_y = mesh.bounds.y.max;
        let material = material_with_disp(2.0);
        let heightmap = vec![1.0; 4];
        apply_displacement_to_mesh(&mut mesh, &material, &heightmap, 2, 2);
        // Bounds should grow by +1.0 in Y after max displacement.
        assert!(mesh.bounds.y.max > flat_max_y);
        assert!((mesh.bounds.y.max - 1.0).abs() < 1e-4);
    }

    #[test]
    fn apply_displacement_skips_when_no_texture() {
        let mut mesh = flat_plane_mesh();
        let original = mesh.positions.clone();
        let material = Material::default(); // no displacement_texture
        let result = apply_displacement(&mut mesh, &material);
        assert!(matches!(result, Ok(false)));
        assert_eq!(mesh.positions, original);
    }

    #[test]
    fn resolve_path_absolute_passthrough() {
        let abs = if cfg!(windows) {
            "C:/tex/height.exr"
        } else {
            "/tex/height.exr"
        };
        let resolved = resolve_path(abs, Some(Path::new("/other/base")));
        assert_eq!(resolved, Some(PathBuf::from(abs)));
    }

    #[test]
    fn resolve_path_relative_joins_base() {
        let resolved = resolve_path("tex/height.exr", Some(Path::new("/base/dir")));
        assert_eq!(resolved, Some(PathBuf::from("/base/dir/tex/height.exr")));
    }

    #[test]
    fn resolve_path_relative_no_base_returns_none() {
        let resolved = resolve_path("tex/height.exr", None);
        assert_eq!(resolved, None);
    }
}
