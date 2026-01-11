//! Scene graph types for BIF.
//!
//! This module defines the core scene representation that maps closely
//! to USD concepts while remaining renderer-agnostic.

use std::sync::Arc;

use bif_math::{Aabb, Mat4, Quat, Vec3};

use crate::mesh::Mesh;

/// A PBR material definition based on UsdPreviewSurface.
///
/// Maps to the UsdPreviewSurface shader specification with support
/// for both constant values and texture paths.
#[derive(Clone, Debug)]
pub struct Material {
    /// Material name (from USD prim path)
    pub name: String,

    /// Diffuse/albedo color (RGB, 0-1)
    pub diffuse_color: Vec3,

    /// Metallic factor (0=dielectric, 1=metal)
    pub metallic: f32,

    /// Roughness factor (0=smooth, 1=rough)
    pub roughness: f32,

    /// Emissive color (RGB, for light-emitting surfaces)
    pub emissive_color: Vec3,

    /// Opacity (0=transparent, 1=opaque)
    pub opacity: f32,

    /// Specular factor (for non-metallic surfaces)
    pub specular: f32,

    /// Path to diffuse/albedo texture
    pub diffuse_texture: Option<String>,

    /// Path to roughness texture
    pub roughness_texture: Option<String>,

    /// Path to metallic texture
    pub metallic_texture: Option<String>,

    /// Path to normal map texture
    pub normal_texture: Option<String>,

    /// Path to emissive texture
    pub emissive_texture: Option<String>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: String::new(),
            diffuse_color: Vec3::new(0.5, 0.5, 0.5), // Grey default
            metallic: 0.0,
            roughness: 0.5,
            emissive_color: Vec3::ZERO,
            opacity: 1.0,
            specular: 0.5,
            diffuse_texture: None,
            roughness_texture: None,
            metallic_texture: None,
            normal_texture: None,
            emissive_texture: None,
        }
    }
}

impl Material {
    /// Create a new material with just a name and diffuse color.
    pub fn new(name: impl Into<String>, diffuse_color: Vec3) -> Self {
        Self {
            name: name.into(),
            diffuse_color,
            ..Default::default()
        }
    }

    /// Check if this material uses any textures.
    pub fn has_textures(&self) -> bool {
        self.diffuse_texture.is_some()
            || self.roughness_texture.is_some()
            || self.metallic_texture.is_some()
            || self.normal_texture.is_some()
            || self.emissive_texture.is_some()
    }

    /// Check if this material is emissive.
    pub fn is_emissive(&self) -> bool {
        self.emissive_color.length_squared() > 0.0 || self.emissive_texture.is_some()
    }
}

/// A prototype is a shared mesh + material that can be instanced.
///
/// This corresponds to a `UsdGeomMesh` in USD terminology.
#[derive(Clone, Debug)]
pub struct Prototype {
    /// Unique identifier within the scene
    pub id: usize,

    /// Prototype name (from USD prim path)
    pub name: String,

    /// Shared mesh geometry
    pub mesh: Arc<Mesh>,

    /// Material (optional, defaults to grey)
    pub material: Option<Arc<Material>>,

    /// Local bounding box (from mesh)
    pub bounds: Aabb,
}

impl Prototype {
    /// Create a new prototype from a mesh.
    pub fn new(id: usize, name: String, mesh: Arc<Mesh>) -> Self {
        let bounds = mesh.bounds;
        Self {
            id,
            name,
            mesh,
            material: None,
            bounds,
        }
    }

    /// Set the material for this prototype.
    pub fn with_material(mut self, material: Arc<Material>) -> Self {
        self.material = Some(material);
        self
    }
}

/// Transform components that can be composed into a matrix.
#[derive(Clone, Debug)]
pub struct Transform {
    /// Translation
    pub translation: Vec3,

    /// Rotation (as quaternion)
    pub rotation: Quat,

    /// Scale
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Transform {
    /// Create a new transform with only translation.
    pub fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            ..Default::default()
        }
    }

    /// Create a new transform from a 4x4 matrix.
    ///
    /// Decomposes the matrix into translation, rotation, and scale.
    pub fn from_matrix(matrix: Mat4) -> Self {
        let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// Convert to a 4x4 transformation matrix.
    ///
    /// Order: Scale -> Rotate -> Translate (SRT)
    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// An instance of a prototype with a transform.
///
/// This corresponds to a point in a `UsdGeomPointInstancer` or a
/// transformed `UsdGeomMesh` reference.
#[derive(Clone, Debug)]
pub struct Instance {
    /// Index of the prototype this instance references
    pub prototype_id: usize,

    /// Instance transform
    pub transform: Transform,
}

impl Instance {
    /// Create a new instance of a prototype.
    pub fn new(prototype_id: usize, transform: Transform) -> Self {
        Self {
            prototype_id,
            transform,
        }
    }

    /// Create an instance with just a translation.
    pub fn with_translation(prototype_id: usize, translation: Vec3) -> Self {
        Self::new(prototype_id, Transform::from_translation(translation))
    }

    /// Get the 4x4 model matrix for this instance.
    pub fn model_matrix(&self) -> Mat4 {
        self.transform.to_matrix()
    }
}

/// A complete scene containing prototypes, instances, and materials.
///
/// This corresponds to a `UsdStage` in USD terminology.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    /// Shared prototype definitions (meshes)
    pub prototypes: Vec<Arc<Prototype>>,

    /// Instances referencing prototypes
    pub instances: Vec<Instance>,

    /// Materials used in the scene
    pub materials: Vec<Arc<Material>>,

    /// Scene name (usually from filename)
    pub name: String,
}

impl Scene {
    /// Create an empty scene.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Add a prototype to the scene and return its ID.
    pub fn add_prototype(&mut self, mesh: Arc<Mesh>, name: String) -> usize {
        let id = self.prototypes.len();
        let prototype = Arc::new(Prototype::new(id, name, mesh));
        self.prototypes.push(prototype);
        id
    }

    /// Add an instance of a prototype.
    pub fn add_instance(&mut self, prototype_id: usize, transform: Transform) {
        self.instances.push(Instance::new(prototype_id, transform));
    }

    /// Add a material to the scene and return its ID.
    pub fn add_material(&mut self, material: Material) -> usize {
        let id = self.materials.len();
        self.materials.push(Arc::new(material));
        id
    }

    /// Get a material by ID.
    pub fn get_material(&self, id: usize) -> Option<&Arc<Material>> {
        self.materials.get(id)
    }

    /// Get material count.
    pub fn material_count(&self) -> usize {
        self.materials.len()
    }

    /// Get total triangle count across all instances.
    pub fn total_triangle_count(&self) -> usize {
        let mut count = 0;
        for instance in &self.instances {
            if let Some(proto) = self.prototypes.get(instance.prototype_id) {
                count += proto.mesh.triangle_count();
            }
        }
        count
    }

    /// Get total instance count.
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Get prototype count.
    pub fn prototype_count(&self) -> usize {
        self.prototypes.len()
    }

    /// Compute the world-space bounding box of all instances.
    pub fn world_bounds(&self) -> Aabb {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);

        for instance in &self.instances {
            if let Some(proto) = self.prototypes.get(instance.prototype_id) {
                let matrix = instance.model_matrix();

                // Transform all 8 corners of the prototype bounds
                let b = &proto.bounds;
                let corners = [
                    Vec3::new(b.x.min, b.y.min, b.z.min),
                    Vec3::new(b.x.max, b.y.min, b.z.min),
                    Vec3::new(b.x.min, b.y.max, b.z.min),
                    Vec3::new(b.x.max, b.y.max, b.z.min),
                    Vec3::new(b.x.min, b.y.min, b.z.max),
                    Vec3::new(b.x.max, b.y.min, b.z.max),
                    Vec3::new(b.x.min, b.y.max, b.z.max),
                    Vec3::new(b.x.max, b.y.max, b.z.max),
                ];

                for corner in corners {
                    let world_pos = matrix.transform_point3(corner);
                    min = min.min(world_pos);
                    max = max.max(world_pos);
                }
            }
        }

        if min.x.is_infinite() {
            Aabb::empty()
        } else {
            Aabb::from_points(min, max)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scene_creation() {
        let mut scene = Scene::new("test");

        let mesh = Arc::new(Mesh::new(
            vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            vec![0, 1, 2],
            None,
        ));

        let proto_id = scene.add_prototype(mesh, "triangle".to_string());
        assert_eq!(proto_id, 0);

        scene.add_instance(proto_id, Transform::default());
        scene.add_instance(
            proto_id,
            Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
        );

        assert_eq!(scene.prototype_count(), 1);
        assert_eq!(scene.instance_count(), 2);
        assert_eq!(scene.total_triangle_count(), 2);
    }

    #[test]
    fn test_transform_matrix_roundtrip() {
        let transform = Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
            scale: Vec3::new(2.0, 2.0, 2.0),
        };

        let matrix = transform.to_matrix();
        let recovered = Transform::from_matrix(matrix);

        assert!((recovered.translation - transform.translation).length() < 0.001);
        assert!((recovered.scale - transform.scale).length() < 0.001);
    }
}
