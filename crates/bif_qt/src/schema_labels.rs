//! USD schema → friendly label lookup.
//!
//! Tier 1 item #3. A single source of truth for the property inspector
//! (and any future surface) to translate raw USD attribute / prim type
//! names into names an artist would recognize. Unknown names fall
//! through to the raw string so obscure attrs stay debuggable.
//!
//! Growth policy: add entries conservatively — only when the artist-
//! facing name is clearer than USD's own. When in doubt, leave the
//! raw USD name so pipeline folks see what they expect.

/// Friendly attribute name. Returns the raw string when nothing matches.
///
/// Covers `xformOp:*`, `primvars:*`, geometry, visibility, material
/// binding — roughly the top of the UsdGeom + UsdShade surface that
/// artists hit daily.
pub fn friendly_attribute_name(raw: &str) -> &str {
    match raw {
        // Transform ops.
        "xformOp:translate" => "Position",
        "xformOp:rotateXYZ" => "Rotation (XYZ)",
        "xformOp:rotateXZY" => "Rotation (XZY)",
        "xformOp:rotateYXZ" => "Rotation (YXZ)",
        "xformOp:rotateYZX" => "Rotation (YZX)",
        "xformOp:rotateZXY" => "Rotation (ZXY)",
        "xformOp:rotateZYX" => "Rotation (ZYX)",
        "xformOp:rotateX" => "Rotation X",
        "xformOp:rotateY" => "Rotation Y",
        "xformOp:rotateZ" => "Rotation Z",
        "xformOp:scale" => "Scale",
        "xformOp:transform" => "Transform Matrix",
        "xformOp:orient" => "Orientation (Quat)",
        "xformOpOrder" => "Transform Op Order",

        // UsdGeomMesh / UsdGeomPointBased.
        "points" => "Vertex Positions",
        "faceVertexCounts" => "Face Vertex Counts",
        "faceVertexIndices" => "Face Vertex Indices",
        "normals" => "Normals",
        "velocities" => "Velocities",
        "accelerations" => "Accelerations",
        "subdivisionScheme" => "Subdivision Scheme",
        "interpolateBoundary" => "Interpolate Boundary",
        "faceVaryingLinearInterpolation" => "Face-Varying Linear Interp",
        "triangleSubdivisionRule" => "Triangle Subdivision Rule",
        "orientation" => "Winding Orientation",
        "doubleSided" => "Double Sided",
        "extent" => "Bounding Extent",
        "cornerIndices" => "Corner Indices",
        "cornerSharpnesses" => "Corner Sharpnesses",
        "creaseIndices" => "Crease Indices",
        "creaseLengths" => "Crease Lengths",
        "creaseSharpnesses" => "Crease Sharpnesses",

        // Common primvars.
        "primvars:st" => "UV Coordinates (st)",
        "primvars:st:indices" => "UV Indices",
        "primvars:displayColor" => "Display Color",
        "primvars:displayOpacity" => "Display Opacity",

        // UsdGeomImageable.
        "visibility" => "Visibility",
        "purpose" => "Purpose",
        "proxyPrim" => "Proxy Prim",

        // Model API / kind.
        "kind" => "Kind",
        "assetInfo" => "Asset Info",

        // Material binding.
        "material:binding" => "Material Binding",
        "material:binding:preview" => "Material (Preview)",
        "material:binding:full" => "Material (Full)",
        "material:binding:collection" => "Material (Collection)",

        // UsdGeomXformable activation.
        "xformOp:resetXformStack!" => "Reset Xform Stack",

        // UsdGeomPointInstancer.
        "protoIndices" => "Prototype Indices",
        "positions" => "Instance Positions",
        "orientations" => "Instance Orientations",
        "scales" => "Instance Scales",
        "ids" => "Instance IDs",
        "invisibleIds" => "Hidden Instance IDs",
        "prototypes" => "Prototype References",

        // UsdLux (lights).
        "inputs:intensity" => "Intensity",
        "inputs:exposure" => "Exposure",
        "inputs:color" => "Color",
        "inputs:colorTemperature" => "Color Temperature",
        "inputs:enableColorTemperature" => "Use Color Temperature",
        "inputs:diffuse" => "Diffuse Contribution",
        "inputs:specular" => "Specular Contribution",
        "inputs:normalize" => "Normalize Power",
        "inputs:texture:file" => "Texture File",
        "inputs:texture:format" => "Texture Format",
        "inputs:radius" => "Radius",
        "inputs:width" => "Width",
        "inputs:height" => "Height",
        "inputs:length" => "Length",
        "inputs:angle" => "Cone Angle",
        "inputs:shaping:cone:angle" => "Shaping Cone Angle",
        "inputs:shaping:cone:softness" => "Shaping Cone Softness",
        "inputs:shaping:focus" => "Shaping Focus",

        // UsdGeomCamera.
        "focalLength" => "Focal Length (mm)",
        "horizontalAperture" => "Horizontal Aperture",
        "verticalAperture" => "Vertical Aperture",
        "horizontalApertureOffset" => "H Aperture Offset",
        "verticalApertureOffset" => "V Aperture Offset",
        "focusDistance" => "Focus Distance",
        "fStop" => "f-stop",
        "clippingRange" => "Clipping Range",
        "clippingPlanes" => "Clipping Planes",
        "projection" => "Projection",
        "shutter:open" => "Shutter Open",
        "shutter:close" => "Shutter Close",

        _ => raw,
    }
}

/// Friendly prim type name. Returns the raw string when nothing matches.
pub fn friendly_prim_type(raw: &str) -> &str {
    match raw {
        "Xform" => "Transform",
        "Mesh" => "Mesh",
        "PointInstancer" => "Point Instancer",
        "Points" => "Points",
        "BasisCurves" => "Curves",
        "NurbsCurves" => "NURBS Curves",
        "NurbsPatch" => "NURBS Patch",
        "Capsule" => "Capsule",
        "Cone" => "Cone",
        "Cube" => "Cube",
        "Cylinder" => "Cylinder",
        "Sphere" => "Sphere",
        "Scope" => "Scope (Grouping)",
        "Camera" => "Camera",
        "SkelRoot" => "Skeleton Root",
        "Skeleton" => "Skeleton",
        "SkelAnimation" => "Skeleton Animation",
        "BlendShape" => "Blend Shape",
        "Material" => "Material",
        "Shader" => "Shader",
        "NodeGraph" => "Node Graph",
        "DomeLight" => "Dome Light",
        "DomeLight_1" => "Dome Light",
        "RectLight" => "Rect Light",
        "DistantLight" => "Distant Light",
        "DiskLight" => "Disk Light",
        "SphereLight" => "Sphere Light",
        "CylinderLight" => "Cylinder Light",
        "GeometryLight" => "Geometry Light",
        "PortalLight" => "Portal Light",
        "VolumeLight" => "Volume Light",
        "LightFilter" => "Light Filter",
        "Volume" => "Volume",
        "OpenVDBAsset" => "OpenVDB Volume",
        "GenericPrim" => "Generic Prim",
        "PhysicsScene" => "Physics Scene",
        "PhysicsCollisionGroup" => "Collision Group",
        "PhysicsRigidBodyAPI" => "Rigid Body",

        // BIF's own procedural prims.
        "Primitive" => "BIF Primitive",

        _ => raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_attribute_translates() {
        assert_eq!(friendly_attribute_name("xformOp:translate"), "Position");
        assert_eq!(
            friendly_attribute_name("primvars:st"),
            "UV Coordinates (st)"
        );
    }

    #[test]
    fn unknown_attribute_passes_through() {
        assert_eq!(
            friendly_attribute_name("something:obscure"),
            "something:obscure"
        );
    }

    #[test]
    fn known_prim_type_translates() {
        assert_eq!(friendly_prim_type("Xform"), "Transform");
        assert_eq!(friendly_prim_type("PointInstancer"), "Point Instancer");
    }

    #[test]
    fn unknown_prim_type_passes_through() {
        assert_eq!(friendly_prim_type("ExoticSchemaType"), "ExoticSchemaType");
    }
}
