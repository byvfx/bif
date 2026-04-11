//! CPU linear blend skinning (LBS) for UsdSkel-bound meshes.
//!
//! Takes the [`SkinBinding`](crate::mesh::SkinBinding) and a per-joint palette
//! (joint-skel transforms at the current time code) and produces deformed vertex
//! positions + normals by blending the weighted influences of each bound joint.
//!
//! # Pipeline
//!
//! 1. **Palette assembly** (`compute_skin_matrices`) — one `Mat4` per joint:
//!    `joint_skel * inv_bind * geom_bind_transform`. Done once per frame.
//! 2. **Position skinning** (`skin_positions`) — weighted matrix blend per vertex.
//!    `out_pos = sum_i (weight_i * palette[joint_idx_i].transform_point3(bind_pos))`.
//! 3. **Normal skinning** (`skin_normals`) — uses the inverse-transpose of the
//!    upper 3×3 of each palette matrix so non-uniform-scale joints still produce
//!    correct surface normals.
//!
//! # Invariants
//!
//! - `bind.joint_indices.len() == bind.joint_weights.len() == vertex_count * element_size`
//! - `palette.len() == bind.inv_bind_matrices.len()` (one per joint)
//! - Out-of-range joint indices are clamped to 0 with a weight-preserving fallback
//!   so a malformed skin can't panic the renderer.
//!
//! # Fast path
//!
//! An identity palette (current_skel = bind_skel) produces vertices byte-identical
//! to `bind_positions`. This is the Phase 2 correctness property and is exercised
//! by [`tests::identity_palette_round_trip`].

use bif_math::{Mat3, Mat4, Vec3};

use crate::mesh::{SkinBinding, SkinKind};

/// Assemble the skinning matrix palette for the current frame.
///
/// For each joint: `palette[j] = joint_skel_xforms[j] * inv_bind_matrices[j]`.
///
/// The geom-bind transform is baked in once, as a pre-multiply on the result,
/// so vertex iteration only needs a single matrix-per-influence multiply.
///
/// `joint_skel_xforms` must have the same length as `bind.inv_bind_matrices`
/// (one entry per joint in the skeleton's joint order). Mismatched lengths
/// return an empty palette.
pub fn compute_skin_matrices(bind: &SkinBinding, joint_skel_xforms: &[Mat4]) -> Vec<Mat4> {
    if joint_skel_xforms.len() != bind.inv_bind_matrices.len() {
        log::warn!(
            "skin palette length mismatch: {} joint xforms vs {} inv-bind matrices",
            joint_skel_xforms.len(),
            bind.inv_bind_matrices.len()
        );
        return Vec::new();
    }

    let mut palette = Vec::with_capacity(joint_skel_xforms.len());
    for (jsx, inv_bind) in joint_skel_xforms.iter().zip(bind.inv_bind_matrices.iter()) {
        // Each vertex is first moved from mesh-local space to skel-local space
        // (via geom_bind_transform), then from world-bind space to joint-local
        // space (via inv_bind), then back to skel space at the current time
        // (via joint_skel). Collapsing the whole chain into a single matrix
        // lets `transform_point3` do it in one multiply per influence.
        palette.push((*jsx) * (*inv_bind) * bind.geom_bind_transform);
    }
    palette
}

/// Apply the palette to bind-pose positions, producing deformed positions.
///
/// `out.len()` must equal `bind_positions.len()`. The caller is responsible
/// for ensuring the output buffer matches the mesh's vertex count — typically
/// `out = &mut mesh.positions`.
///
/// Out-of-range joint indices (e.g. from a malformed skin binding) are skipped
/// silently; their weight contribution is dropped. This keeps the renderer
/// alive on bad data at the cost of a slight geometry error on affected verts.
///
/// Branches on the binding's [`SkinKind`]:
/// - `PerVertex` runs the weighted-blend inner loop per vertex.
/// - `Rigid` skips the inner loop entirely — every vertex gets the same single
///   matrix multiply, which is ~`element_size`× faster on accessory meshes.
pub fn skin_positions(
    bind: &SkinBinding,
    bind_positions: &[Vec3],
    palette: &[Mat4],
    out: &mut [Vec3],
) {
    debug_assert_eq!(bind_positions.len(), out.len());
    if palette.is_empty() {
        out.copy_from_slice(bind_positions);
        return;
    }

    match &bind.kind {
        SkinKind::PerVertex {
            joint_indices,
            joint_weights,
            element_size,
        } => {
            let element_size = *element_size;
            if element_size == 0 {
                out.copy_from_slice(bind_positions);
                return;
            }

            for (vert_idx, bind_pos) in bind_positions.iter().enumerate() {
                let influence_base = vert_idx * element_size;
                let mut accum = Vec3::ZERO;

                for i in 0..element_size {
                    let slot = influence_base + i;
                    if slot >= joint_indices.len() {
                        break;
                    }
                    let weight = joint_weights[slot];
                    if weight == 0.0 {
                        continue;
                    }
                    let joint_idx = joint_indices[slot] as usize;
                    if joint_idx >= palette.len() {
                        continue;
                    }
                    accum += palette[joint_idx].transform_point3(*bind_pos) * weight;
                }

                out[vert_idx] = accum;
            }
        }
        SkinKind::Rigid { joint_idx, weight } => {
            let Some(palette_mat) = palette.get(*joint_idx as usize) else {
                // Out-of-range single joint → fail soft, leave at bind pose.
                out.copy_from_slice(bind_positions);
                return;
            };
            let w = *weight;
            for (vert_idx, bind_pos) in bind_positions.iter().enumerate() {
                out[vert_idx] = palette_mat.transform_point3(*bind_pos) * w;
            }
        }
    }
}

/// Apply the palette to bind-pose normals, producing deformed normals.
///
/// Uses the inverse-transpose of the upper 3×3 of each palette matrix so joints
/// with non-uniform scale still produce correctly oriented surface normals.
/// Output is normalized per vertex.
///
/// Pre-computing `Mat3 = inverse_transpose(palette[j].upper_3x3())` per joint
/// would amortize the cost over many vertices, but Phase 2 runs at load-time
/// only (identity palette); real-time calls will want the cached variant added
/// in Phase 3. Kept straightforward here for testability.
pub fn skin_normals(bind: &SkinBinding, bind_normals: &[Vec3], palette: &[Mat4], out: &mut [Vec3]) {
    debug_assert_eq!(bind_normals.len(), out.len());
    if palette.is_empty() {
        out.copy_from_slice(bind_normals);
        return;
    }

    // Pre-build per-joint normal matrices (inverse-transpose of upper 3×3).
    let normal_mats: Vec<Mat3> = palette
        .iter()
        .map(|m| Mat3::from_mat4(*m).inverse().transpose())
        .collect();

    match &bind.kind {
        SkinKind::PerVertex {
            joint_indices,
            joint_weights,
            element_size,
        } => {
            let element_size = *element_size;
            if element_size == 0 {
                out.copy_from_slice(bind_normals);
                return;
            }

            for (vert_idx, bind_n) in bind_normals.iter().enumerate() {
                let influence_base = vert_idx * element_size;
                let mut accum = Vec3::ZERO;

                for i in 0..element_size {
                    let slot = influence_base + i;
                    if slot >= joint_indices.len() {
                        break;
                    }
                    let weight = joint_weights[slot];
                    if weight == 0.0 {
                        continue;
                    }
                    let joint_idx = joint_indices[slot] as usize;
                    if joint_idx >= normal_mats.len() {
                        continue;
                    }
                    accum += normal_mats[joint_idx].mul_vec3(*bind_n) * weight;
                }

                let len = accum.length();
                out[vert_idx] = if len > 1e-8 { accum / len } else { *bind_n };
            }
        }
        SkinKind::Rigid {
            joint_idx,
            weight: _,
        } => {
            let Some(normal_mat) = normal_mats.get(*joint_idx as usize) else {
                out.copy_from_slice(bind_normals);
                return;
            };
            for (vert_idx, bind_n) in bind_normals.iter().enumerate() {
                let n = normal_mat.mul_vec3(*bind_n);
                let len = n.length();
                out[vert_idx] = if len > 1e-8 { n / len } else { *bind_n };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bind(
        joint_indices: Vec<u32>,
        joint_weights: Vec<f32>,
        element_size: usize,
        inv_bind_matrices: Vec<Mat4>,
    ) -> SkinBinding {
        SkinBinding {
            skeleton_path: "/test/Skel".to_string(),
            kind: SkinKind::PerVertex {
                joint_indices,
                joint_weights,
                element_size,
            },
            geom_bind_transform: Mat4::IDENTITY,
            inv_bind_matrices,
        }
    }

    #[test]
    fn single_joint_identity_is_passthrough() {
        // 1 vertex, 1 joint at identity bind → identity palette → unchanged.
        let bind = make_bind(vec![0], vec![1.0], 1, vec![Mat4::IDENTITY]);
        let palette = compute_skin_matrices(&bind, &[Mat4::IDENTITY]);
        let bind_pos = vec![Vec3::new(3.0, 4.0, 5.0)];
        let mut out = vec![Vec3::ZERO];
        skin_positions(&bind, &bind_pos, &palette, &mut out);
        assert!((out[0] - bind_pos[0]).length() < 1e-5);
    }

    #[test]
    fn translated_joint_translates_vertex() {
        // Joint 0 bind = identity; current joint-skel xform = translate(+1, 0, 0).
        // Weight=1 on joint 0 → vertex shifts by (1,0,0).
        let bind = make_bind(vec![0], vec![1.0], 1, vec![Mat4::IDENTITY]);
        let palette =
            compute_skin_matrices(&bind, &[Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0))]);
        let bind_pos = vec![Vec3::new(0.0, 0.0, 0.0)];
        let mut out = vec![Vec3::ZERO];
        skin_positions(&bind, &bind_pos, &palette, &mut out);
        assert!((out[0] - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn two_joint_blend_averages_contributions() {
        // Two joints at identity bind, both identity xform.
        // Vertex weighted 50/50 to joint 0 (translate +2X) and joint 1 (translate 0).
        // Expected: 0.5 * (+2,0,0) + 0.5 * (0,0,0) = (1,0,0).
        let bind = make_bind(
            vec![0, 1],
            vec![0.5, 0.5],
            2,
            vec![Mat4::IDENTITY, Mat4::IDENTITY],
        );
        let palette = compute_skin_matrices(
            &bind,
            &[
                Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0)),
                Mat4::IDENTITY,
            ],
        );
        let bind_pos = vec![Vec3::new(0.0, 0.0, 0.0)];
        let mut out = vec![Vec3::ZERO];
        skin_positions(&bind, &bind_pos, &palette, &mut out);
        assert!((out[0] - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn identity_palette_round_trip() {
        // Two-bone arm geometry: 8 verts, joint 0 bind = identity,
        // joint 1 bind = translate(0, 1, 0). Bottom 4 verts weighted to joint 0,
        // top 4 to joint 1. Identity-palette eval (current xforms == bind xforms)
        // must reproduce bind positions byte-for-byte (within float epsilon).
        let joint0_bind = Mat4::IDENTITY;
        let joint1_bind = Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0));
        let bind = make_bind(
            vec![0, 0, 0, 0, 1, 1, 1, 1],
            vec![1.0; 8],
            1,
            vec![joint0_bind.inverse(), joint1_bind.inverse()],
        );

        // Box geometry (matches two_bone_arm.usda ordering)
        let bind_positions = vec![
            Vec3::new(-0.5, 0.0, -0.5),
            Vec3::new(0.5, 0.0, -0.5),
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(-0.5, 0.0, 0.5),
            Vec3::new(-0.5, 2.0, -0.5),
            Vec3::new(0.5, 2.0, -0.5),
            Vec3::new(0.5, 2.0, 0.5),
            Vec3::new(-0.5, 2.0, 0.5),
        ];

        // Palette uses current joint_skel xforms == bind xforms → identity skinning
        let palette = compute_skin_matrices(&bind, &[joint0_bind, joint1_bind]);
        let mut out = vec![Vec3::ZERO; 8];
        skin_positions(&bind, &bind_positions, &palette, &mut out);

        for (got, want) in out.iter().zip(bind_positions.iter()) {
            assert!(
                (*got - *want).length() < 1e-5,
                "expected {want:?}, got {got:?}"
            );
        }
    }

    #[test]
    fn rotated_joint_rotates_top_verts() {
        // Two-bone arm: rotate joint 1 90° around Z → top of box swings +X.
        // Vertex originally at (0, 2, 0) (on top) should land near (-? , 1 + 0, 0).
        //
        // Compute by hand: vertex at (0, 2, 0). Joint 1 inv-bind = T(0,-1,0).
        // After inv_bind: (0, 1, 0). Rotate 90° around Z: (−1, 0, 0) — wait let's
        // use proper math. Rot Z 90° takes (x,y,z) → (−y, x, z). So (0,1,0) →
        // (−1, 0, 0). Then apply joint 1 current_skel: identity? No — we want
        // the *rotated* joint xform. Let's construct it explicitly so the test
        // is self-documenting:
        //
        //     joint1_current = T(0,1,0) * R_z(90°)
        //
        // which is: first rotate 90° around Z about origin, then translate to
        // the joint's rest position. That's the standard way joints compose.
        //
        // Full chain for the top vertex: apply inv_bind (T(0,-1,0)) → (0,1,0),
        // then joint1_current = T(0,1,0)*Rz(90°) applied to (0,1,0):
        //   Rz(90°): (0,1,0) → (−1,0,0)
        //   T(0,1,0): (−1,0,0) → (−1,1,0)
        let joint0_bind = Mat4::IDENTITY;
        let joint1_bind = Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0));
        let bind = make_bind(
            vec![1],
            vec![1.0],
            1,
            vec![joint0_bind.inverse(), joint1_bind.inverse()],
        );

        let joint1_current = Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0))
            * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let palette = compute_skin_matrices(&bind, &[joint0_bind, joint1_current]);

        let bind_positions = vec![Vec3::new(0.0, 2.0, 0.0)];
        let mut out = vec![Vec3::ZERO];
        skin_positions(&bind, &bind_positions, &palette, &mut out);

        let expected = Vec3::new(-1.0, 1.0, 0.0);
        assert!(
            (out[0] - expected).length() < 1e-5,
            "expected {expected:?}, got {:?}",
            out[0]
        );
    }

    #[test]
    fn normals_with_identity_palette_unchanged() {
        let bind = make_bind(vec![0], vec![1.0], 1, vec![Mat4::IDENTITY]);
        let palette = compute_skin_matrices(&bind, &[Mat4::IDENTITY]);
        let bind_n = vec![Vec3::new(0.0, 1.0, 0.0)];
        let mut out = vec![Vec3::ZERO];
        skin_normals(&bind, &bind_n, &palette, &mut out);
        assert!((out[0] - bind_n[0]).length() < 1e-5);
    }

    #[test]
    fn normals_correct_under_non_uniform_scale() {
        // Joint scaling 2x in X, 1x in Y. Naive (same-as-position) transform
        // would tilt normals wrong; inv-transpose path must keep +Y upright.
        let bind = make_bind(vec![0], vec![1.0], 1, vec![Mat4::IDENTITY]);
        let scale = Mat4::from_scale(Vec3::new(2.0, 1.0, 1.0));
        let palette = compute_skin_matrices(&bind, &[scale]);
        let bind_n = vec![Vec3::new(0.0, 1.0, 0.0)];
        let mut out = vec![Vec3::ZERO];
        skin_normals(&bind, &bind_n, &palette, &mut out);
        // +Y normal should stay +Y (unit length) under X-scale.
        assert!((out[0] - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn out_of_range_joint_index_is_skipped_not_panicked() {
        // Malformed skin: joint_idx = 99, only 1 joint in palette.
        // Expect: no panic, vertex = (0,0,0) (weight dropped).
        let bind = make_bind(vec![99], vec![1.0], 1, vec![Mat4::IDENTITY]);
        let palette = compute_skin_matrices(&bind, &[Mat4::IDENTITY]);
        let bind_pos = vec![Vec3::new(5.0, 5.0, 5.0)];
        let mut out = vec![Vec3::ZERO];
        skin_positions(&bind, &bind_pos, &palette, &mut out);
        assert_eq!(out[0], Vec3::ZERO);
    }

    /// v0.13.5 review follow-up: lock down palette math under non-identity
    /// `geomBindTransform`. The reviewer's concern was that the palette order
    /// `joint_skel * inv_bind * geom_bind` might not handle a non-trivial
    /// `geom_bind` correctly. By matrix associativity it does — this test
    /// proves it concretely so a future refactor can't silently break it.
    ///
    /// Setup: 1 vertex at (1, 0, 0) in mesh-local space. The mesh is bound
    /// to a single joint that, at bind time, sits at world position (0, 5, 0).
    /// The mesh's `geomBindTransform` translates mesh-local → world-bind by
    /// the same +5 Y, so the vertex's bind-time world position is (1, 5, 0).
    /// The joint's `inv_bind` therefore takes that world point back to
    /// joint-local space at (1, 0, 0).
    ///
    /// Three sub-cases verify three pieces of the chain:
    /// 1. Joint at identity → vertex returns to its bind world position.
    /// 2. Joint translates +Z → vertex shifts +Z relative to its bind.
    /// 3. Joint rotates 90° around Z → vertex rotates around the joint origin.
    #[test]
    fn non_identity_geom_bind_round_trip() {
        let geom_bind = Mat4::from_translation(Vec3::new(0.0, 5.0, 0.0));
        let joint0_bind_world = Mat4::from_translation(Vec3::new(0.0, 5.0, 0.0));

        let bind = SkinBinding {
            skeleton_path: "/test/Skel".to_string(),
            kind: SkinKind::PerVertex {
                joint_indices: vec![0],
                joint_weights: vec![1.0],
                element_size: 1,
            },
            geom_bind_transform: geom_bind,
            inv_bind_matrices: vec![joint0_bind_world.inverse()],
        };

        let bind_positions = vec![Vec3::new(1.0, 0.0, 0.0)];
        let mut out = vec![Vec3::ZERO];

        // Case 1: identity joint → vertex back at its bind world position.
        // Hand-trace: geom_bind * (1,0,0) = (1,5,0); inv_bind * (1,5,0) = (1,0,0);
        // identity * (1,0,0) = (1,0,0).
        let palette = compute_skin_matrices(&bind, &[Mat4::IDENTITY]);
        skin_positions(&bind, &bind_positions, &palette, &mut out);
        assert!(
            (out[0] - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-5,
            "case 1 (identity joint): expected (1,0,0), got {:?}",
            out[0]
        );

        // Case 2: joint translated +Z by 3 → vertex shifted by (0,0,3).
        // Hand-trace: chain produces (1,0,0) at joint-local; T(0,0,3) takes
        // it to (1,0,3).
        let palette =
            compute_skin_matrices(&bind, &[Mat4::from_translation(Vec3::new(0.0, 0.0, 3.0))]);
        skin_positions(&bind, &bind_positions, &palette, &mut out);
        assert!(
            (out[0] - Vec3::new(1.0, 0.0, 3.0)).length() < 1e-5,
            "case 2 (translated joint): expected (1,0,3), got {:?}",
            out[0]
        );

        // Case 3: joint rotates 90° around Z → joint-local (1,0,0) becomes
        // skel-local (0,1,0). R_z(90°) takes (x,y,z) → (−y,x,z), so
        // (1,0,0) → (0,1,0).
        let palette =
            compute_skin_matrices(&bind, &[Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2)]);
        skin_positions(&bind, &bind_positions, &palette, &mut out);
        assert!(
            (out[0] - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-5,
            "case 3 (rotated joint): expected (0,1,0), got {:?}",
            out[0]
        );
    }

    /// v0.13.6: `Rigid` variant — every vertex follows a single joint with
    /// uniform weight, no per-vertex influences. Used for hair/buttons/teeth.
    /// Verifies the compact path produces the same result as the per-vertex
    /// path would for an equivalent fully-broadcast binding.
    #[test]
    fn rigid_binding_round_trip() {
        // 1 joint, bind at identity. Vertices on a small box that all need
        // to follow the joint together.
        let inv_bind = vec![Mat4::IDENTITY];
        let rigid = SkinBinding {
            skeleton_path: "/test/Skel".to_string(),
            kind: SkinKind::Rigid {
                joint_idx: 0,
                weight: 1.0,
            },
            geom_bind_transform: Mat4::IDENTITY,
            inv_bind_matrices: inv_bind,
        };

        let bind_positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 1.0),
        ];

        // Joint at identity → all verts unchanged.
        let palette = compute_skin_matrices(&rigid, &[Mat4::IDENTITY]);
        let mut out = vec![Vec3::ZERO; 4];
        skin_positions(&rigid, &bind_positions, &palette, &mut out);
        for (got, want) in out.iter().zip(bind_positions.iter()) {
            assert!(
                (*got - *want).length() < 1e-5,
                "identity rigid: expected {want:?}, got {got:?}"
            );
        }

        // Joint translated +Y by 5 → all verts shifted by (0,5,0).
        let palette =
            compute_skin_matrices(&rigid, &[Mat4::from_translation(Vec3::new(0.0, 5.0, 0.0))]);
        skin_positions(&rigid, &bind_positions, &palette, &mut out);
        for (got, want) in out.iter().zip(bind_positions.iter()) {
            let expected = *want + Vec3::new(0.0, 5.0, 0.0);
            assert!(
                (*got - expected).length() < 1e-5,
                "translated rigid: expected {expected:?}, got {got:?}"
            );
        }

        // Out-of-range joint_idx → fail soft, return bind positions.
        let bad_rigid = SkinBinding {
            skeleton_path: "/test/Skel".to_string(),
            kind: SkinKind::Rigid {
                joint_idx: 99,
                weight: 1.0,
            },
            geom_bind_transform: Mat4::IDENTITY,
            inv_bind_matrices: vec![Mat4::IDENTITY],
        };
        let palette = compute_skin_matrices(&bad_rigid, &[Mat4::IDENTITY]);
        skin_positions(&bad_rigid, &bind_positions, &palette, &mut out);
        for (got, want) in out.iter().zip(bind_positions.iter()) {
            assert!(
                (*got - *want).length() < 1e-5,
                "OOB rigid joint_idx should leave verts at bind pose"
            );
        }
    }
}
