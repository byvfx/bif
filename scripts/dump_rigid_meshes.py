"""Dump rigid mesh transform data for debugging hair/nails offset."""
from pxr import Usd, UsdSkel, UsdGeom, Gf

stage = Usd.Stage.Open(r'G:\__projects\_programming\rust\bif\assets\UsdSkelExamples\HumanFemale\HumanFemale.walk.usd')
cache = UsdSkel.Cache()
for p in stage.Traverse():
    if p.IsA(UsdSkel.Root):
        cache.Populate(p, Usd.PrimDefaultPredicate)

xfc = UsdGeom.XformCache()

print("=== RIGID MESHES ===\n")
for prim in stage.Traverse():
    if prim.GetTypeName() != 'Mesh':
        continue
    sq = cache.GetSkinningQuery(prim)
    if not sq or not sq.IsRigidlyDeformed():
        continue

    path = str(prim.GetPath())
    ji, jw = sq.ComputeJointInfluences()
    gbt = sq.GetGeomBindTransform()
    mesh_world = xfc.GetLocalToWorldTransform(prim)

    # Find SkelRoot ancestor
    p = prim
    while p:
        if p.IsA(UsdSkel.Root):
            break
        p = p.GetParent()
    sr_world = xfc.GetLocalToWorldTransform(p) if p else Gf.Matrix4d(1)

    # Check if geomBindTransform is authored
    binding = UsdSkel.BindingAPI(prim)
    gbt_attr = binding.GetGeomBindTransformAttr() if binding else None
    gbt_authored = gbt_attr.HasAuthoredValue() if gbt_attr else False

    # Get first 3 vertex positions
    mesh_geom = UsdGeom.Mesh(prim)
    pts = mesh_geom.GetPointsAttr().Get()
    first3 = [(f'{v[0]:.2f}',f'{v[1]:.2f}',f'{v[2]:.2f}') for v in pts[:3]] if pts else []

    print(f'mesh={path}')
    print(f'  joint_idx={list(ji)[:2]}  rigid=True')
    print(f'  gbt_authored={gbt_authored}')
    print(f'  gbt_translate=({gbt[3][0]:.4f}, {gbt[3][1]:.4f}, {gbt[3][2]:.4f})')
    print(f'  mesh_world_t=({mesh_world[3][0]:.4f}, {mesh_world[3][1]:.4f}, {mesh_world[3][2]:.4f})')
    print(f'  skel_root_t=({sr_world[3][0]:.4f}, {sr_world[3][1]:.4f}, {sr_world[3][2]:.4f})')
    print(f'  delta_t=({mesh_world[3][0]-sr_world[3][0]:.4f}, {mesh_world[3][1]-sr_world[3][1]:.4f}, {mesh_world[3][2]-sr_world[3][2]:.4f})')
    print(f'  first3_verts={first3}')
    print()

print("\n=== FIRST PER-VERTEX MESH (for comparison) ===\n")
for prim in stage.Traverse():
    if prim.GetTypeName() != 'Mesh':
        continue
    sq = cache.GetSkinningQuery(prim)
    if not sq or sq.IsRigidlyDeformed():
        continue

    path = str(prim.GetPath())
    gbt = sq.GetGeomBindTransform()
    mesh_world = xfc.GetLocalToWorldTransform(prim)
    binding = UsdSkel.BindingAPI(prim)
    gbt_attr = binding.GetGeomBindTransformAttr() if binding else None
    gbt_authored = gbt_attr.HasAuthoredValue() if gbt_attr else False

    p = prim
    while p:
        if p.IsA(UsdSkel.Root):
            break
        p = p.GetParent()
    sr_world = xfc.GetLocalToWorldTransform(p) if p else Gf.Matrix4d(1)

    mesh_geom = UsdGeom.Mesh(prim)
    pts = mesh_geom.GetPointsAttr().Get()
    first3 = [(f'{v[0]:.2f}',f'{v[1]:.2f}',f'{v[2]:.2f}') for v in pts[:3]] if pts else []

    print(f'mesh={path}')
    print(f'  gbt_authored={gbt_authored}')
    print(f'  gbt_translate=({gbt[3][0]:.4f}, {gbt[3][1]:.4f}, {gbt[3][2]:.4f})')
    print(f'  mesh_world_t=({mesh_world[3][0]:.4f}, {mesh_world[3][1]:.4f}, {mesh_world[3][2]:.4f})')
    print(f'  skel_root_t=({sr_world[3][0]:.4f}, {sr_world[3][1]:.4f}, {sr_world[3][2]:.4f})')
    print(f'  delta_t=({mesh_world[3][0]-sr_world[3][0]:.4f}, {mesh_world[3][1]-sr_world[3][1]:.4f}, {mesh_world[3][2]-sr_world[3][2]:.4f})')
    print(f'  first3_verts={first3}')
    break  # just the first one
