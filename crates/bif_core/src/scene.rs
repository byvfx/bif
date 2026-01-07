//! Scene graph types for BIF.
//!
//! This module defines the core scene representation that maps closely
//! to USD concepts while remaining renderer-agnostic.

use std::sync::Arc;

use bif_math::{Aabb, Mat4, Quat, Vec3};

use crate::mesh::Mesh;

/// A material definition (placeholder for Milestone 10+).
#[derive(Clone, Debug, Default)]
pub struct Material {
    /// Material name
    pub name: String,

    /// Base color (RGB, 0-1)
    pub base_color: Vec3,
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

/// A complete scene containing prototypes and instances.
///
/// This corresponds to a `UsdStage` in USD terminology.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    /// Shared prototype definitions (meshes)
    pub prototypes: Vec<Arc<Prototype>>,

    /// Instances referencing prototypes
    pub instances: Vec<Instance>,

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
