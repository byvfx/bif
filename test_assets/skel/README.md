# UsdSkel test fixtures

Committed fixtures for v0.13.5 UsdSkel import + CPU skinning.

## Files

- **`two_bone_arm.usda`** — 2-joint rig, 8-vertex box.
  - Joints: `Root` (identity), `Root/Bend` (+1 Y).
  - Bottom 4 verts weighted 100% to Root, top 4 verts 100% to Bend.
  - At bind pose: no deformation. Rotating Bend rotates the top half.
  - Used by `bif_core` unit tests and round-trip validation.

## Visual validation asset (not committed)

Pixar's HumanFemale UsdSkel example lives at:
`assets/UsdSkelExamples/HumanFemale/HumanFemale.walk.usd`

Relative from repo root. Gitignored (large, upstream sample). Used for manual
viewer validation of complex skinned playback.
