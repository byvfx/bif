//! Scene graph types for BIF.
//!
//! This module defines the core scene representation that maps closely
//! to USD concepts while remaining renderer-agnostic.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use bif_math::{Aabb, Mat4, Quat, Vec3};

use crate::mesh::Mesh;
use crate::point_cloud::PointCloud;

/// USD purpose attribute (from UsdGeomImageable).
///
/// Controls viewport visibility filtering — DCCs let users toggle
/// which purposes display (proxy for fast nav, render for final look).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Purpose {
    /// Always visible regardless of mode.
    #[default]
    Default,
    /// Render-quality geometry.
    Render,
    /// Low-res proxy geometry (viewport preview).
    Proxy,
    /// Guide/helper geometry (usually hidden).
    Guide,
}

/// A PBR material definition using OpenPBR Surface naming.
///
/// Maps UsdPreviewSurface inputs to OpenPBR parameters on load.
/// Supports both constant values and texture paths.
#[derive(Clone, Debug)]
pub struct Material {
    /// Material name (from USD prim path)
    pub name: Arc<str>,

    // === Base ===
    /// Base color / albedo (RGB, 0-1)
    pub base_color: Vec3,

    /// Metalness factor (0=dielectric, 1=metal)
    pub base_metalness: f32,

    // === Specular ===
    /// Specular roughness (0=smooth, 1=rough)
    pub specular_roughness: f32,

    /// Specular weight (scales dielectric reflection)
    pub specular_weight: f32,

    /// Specular index of refraction (1.5 = glass/plastic)
    pub specular_ior: f32,

    // === Emission ===
    /// Emission color (RGB, for light-emitting surfaces)
    pub emission_color: Vec3,

    /// Emission luminance in nits
    pub emission_luminance: f32,

    // === Transmission ===
    /// Transmission weight (0=opaque, 1=fully transmissive glass)
    pub transmission_weight: f32,

    // === Geometry ===
    /// Opacity (0=transparent, 1=opaque)
    pub geometry_opacity: f32,

    // === Textures ===
    /// Path to base color texture
    pub base_color_texture: Option<Arc<str>>,

    /// Path to specular roughness texture
    pub specular_roughness_texture: Option<Arc<str>>,

    /// Path to base metalness texture
    pub base_metalness_texture: Option<Arc<str>>,

    /// Path to normal map texture
    pub normal_texture: Option<Arc<str>>,

    /// Path to emission texture
    pub emission_texture: Option<Arc<str>>,

    /// Path to geometry opacity texture
    pub geometry_opacity_texture: Option<Arc<str>>,

    /// Directory of the USD file this material was loaded from (for relative texture paths)
    pub source_dir: Option<PathBuf>,

    /// Double-sided flag (from UsdGeomMesh, applies to geometry using this material)
    pub double_sided: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: Arc::from(""),
            base_color: Vec3::new(0.8, 0.8, 0.8),
            base_metalness: 0.0,
            specular_roughness: 0.3,
            specular_weight: 1.0,
            specular_ior: 1.5,
            emission_color: Vec3::ZERO,
            emission_luminance: 0.0,
            transmission_weight: 0.0,
            geometry_opacity: 1.0,
            base_color_texture: None,
            specular_roughness_texture: None,
            base_metalness_texture: None,
            normal_texture: None,
            emission_texture: None,
            geometry_opacity_texture: None,
            source_dir: None,
            double_sided: false,
        }
    }
}

impl Material {
    /// Create a new material with just a name and base color.
    pub fn new(name: impl Into<Arc<str>>, base_color: Vec3) -> Self {
        Self {
            name: name.into(),
            base_color,
            ..Default::default()
        }
    }

    /// Check if this material uses any textures.
    #[must_use]
    pub fn has_textures(&self) -> bool {
        self.base_color_texture.is_some()
            || self.specular_roughness_texture.is_some()
            || self.base_metalness_texture.is_some()
            || self.normal_texture.is_some()
            || self.emission_texture.is_some()
            || self.geometry_opacity_texture.is_some()
    }

    /// Check if this material is emissive.
    #[must_use]
    pub fn is_emissive(&self) -> bool {
        self.emission_color.length_squared() > 0.0 || self.emission_texture.is_some()
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
    pub name: Arc<str>,

    /// Shared mesh geometry
    pub mesh: Arc<Mesh>,

    /// Material (optional, defaults to grey)
    pub material: Option<Arc<Material>>,

    /// Local bounding box (from mesh)
    pub bounds: Aabb,
}

impl Prototype {
    /// Create a new prototype from a mesh.
    pub fn new(id: usize, name: Arc<str>, mesh: Arc<Mesh>) -> Self {
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

/// Timeline information from the USD stage.
#[derive(Clone, Debug)]
pub struct TimelineInfo {
    /// Start frame
    pub start_frame: f64,
    /// End frame
    pub end_frame: f64,
    /// Frames per second
    pub fps: f64,
}

impl Default for TimelineInfo {
    fn default() -> Self {
        Self {
            start_frame: 0.0,
            end_frame: 0.0,
            fps: 24.0,
        }
    }
}

/// A transform keyframe at a specific time.
#[derive(Clone, Debug)]
pub struct TransformKeyframe {
    /// Time code for this keyframe
    pub time: f64,
    /// Transform at this time
    pub transform: Transform,
}

/// An animated transform with optional keyframes.
#[derive(Clone, Debug)]
pub struct AnimatedTransform {
    /// Static transform (used when no keyframes or for base evaluation)
    pub static_transform: Transform,
    /// Optional keyframes for animation
    pub keyframes: Option<Vec<TransformKeyframe>>,
}

impl AnimatedTransform {
    /// Create a static (non-animated) transform.
    pub fn static_only(transform: Transform) -> Self {
        Self {
            static_transform: transform,
            keyframes: None,
        }
    }

    /// Create an animated transform with keyframes.
    pub fn with_keyframes(static_transform: Transform, keyframes: Vec<TransformKeyframe>) -> Self {
        Self {
            static_transform,
            keyframes: if keyframes.is_empty() {
                None
            } else {
                Some(keyframes)
            },
        }
    }

    /// Check if this transform has animation.
    #[must_use]
    pub fn is_animated(&self) -> bool {
        self.keyframes.as_ref().is_some_and(|k| !k.is_empty())
    }

    /// Evaluate the transform at a given time.
    ///
    /// Returns the interpolated transform between keyframes, or the static
    /// transform if there are no keyframes.
    #[must_use]
    pub fn evaluate(&self, time: f64) -> Transform {
        let keyframes = match &self.keyframes {
            Some(kf) if !kf.is_empty() => kf,
            _ => return self.static_transform.clone(),
        };

        // Handle edge cases
        if keyframes.len() == 1 {
            return keyframes[0].transform.clone();
        }

        let first = &keyframes[0];
        let last = &keyframes[keyframes.len() - 1];

        // Before first keyframe
        if time <= first.time {
            return first.transform.clone();
        }

        // After last keyframe
        if time >= last.time {
            return last.transform.clone();
        }

        // Find surrounding keyframes
        for i in 0..keyframes.len() - 1 {
            let kf0 = &keyframes[i];
            let kf1 = &keyframes[i + 1];

            if time >= kf0.time && time <= kf1.time {
                // Compute interpolation factor
                let t = if (kf1.time - kf0.time).abs() < 1e-10 {
                    0.0
                } else {
                    ((time - kf0.time) / (kf1.time - kf0.time)) as f32
                };

                return Transform::lerp(&kf0.transform, &kf1.transform, t);
            }
        }

        // Fallback
        self.static_transform.clone()
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
    #[must_use]
    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /// Linearly interpolate between two transforms.
    ///
    /// Translation and scale use linear interpolation.
    /// Rotation uses spherical linear interpolation (slerp).
    #[must_use]
    pub fn lerp(a: &Transform, b: &Transform, t: f32) -> Transform {
        Transform {
            translation: a.translation.lerp(b.translation, t),
            rotation: a.rotation.slerp(b.rotation, t),
            scale: a.scale.lerp(b.scale, t),
        }
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

    /// USD prim path for export (e.g., `/World/mesh_0` for standalone meshes,
    /// or empty for BIF-created instances which use `/BIF/` convention).
    pub prim_path: Arc<str>,

    /// USD purpose (Default, Render, Proxy, Guide).
    pub purpose: Purpose,
}

impl Instance {
    /// Create a new instance of a prototype.
    pub fn new(prototype_id: usize, transform: Transform) -> Self {
        Self {
            prototype_id,
            transform,
            prim_path: Arc::from(""),
            purpose: Purpose::Default,
        }
    }

    /// Create a new instance with an explicit USD prim path.
    pub fn with_prim_path(prototype_id: usize, transform: Transform, prim_path: Arc<str>) -> Self {
        Self {
            prototype_id,
            transform,
            prim_path,
            purpose: Purpose::Default,
        }
    }

    /// Create an instance with just a translation.
    pub fn with_translation(prototype_id: usize, translation: Vec3) -> Self {
        Self::new(prototype_id, Transform::from_translation(translation))
    }

    /// Get the 4x4 model matrix for this instance.
    #[must_use]
    pub fn model_matrix(&self) -> Mat4 {
        self.transform.to_matrix()
    }
}

/// A light in the scene (from UsdLux).
#[derive(Clone, Debug)]
pub enum Light {
    /// Directional/distant light (like sun).
    Distant {
        /// World-space direction the light points
        direction: Vec3,
        /// Light color (RGB, 0-1)
        color: Vec3,
        /// Light intensity
        intensity: f32,
        /// Angular diameter in degrees (for soft shadows)
        angle: f32,
    },
    /// Point/sphere light.
    Point {
        /// World-space position
        position: Vec3,
        /// Light color (RGB, 0-1)
        color: Vec3,
        /// Light intensity
        intensity: f32,
        /// Light radius (for soft shadows / area)
        radius: f32,
    },
    /// Area/rect light.
    Rect {
        /// World transform (position + orientation)
        transform: Mat4,
        /// Light color (RGB, 0-1)
        color: Vec3,
        /// Light intensity
        intensity: f32,
        /// Width of the light
        width: f32,
        /// Height of the light
        height: f32,
    },
    /// Environment/dome light.
    Dome {
        /// Rotation around Y axis in radians
        rotation: f32,
        /// Light intensity multiplier
        intensity: f32,
        /// Path to HDRI texture (if any)
        texture_path: Option<Arc<str>>,
    },
}

/// A curves primitive (from UsdGeomBasisCurves).
#[derive(Clone, Debug)]
pub struct CurvesPrim {
    /// USD prim path
    pub path: String,
    /// Control points
    pub points: Vec<Vec3>,
    /// Per-vertex or per-curve widths
    pub widths: Option<Vec<f32>>,
    /// Number of vertices per curve
    pub curve_vertex_counts: Vec<i32>,
    /// Curve type (linear, cubic)
    pub curve_type: crate::usd::cpp_bridge::CurveType,
    /// Basis (bezier, bspline, catmull-rom)
    pub basis: crate::usd::cpp_bridge::CurveBasis,
    /// Wrap mode (nonperiodic, periodic)
    pub wrap: crate::usd::cpp_bridge::CurveWrap,
    /// World transform
    pub transform: Mat4,
    /// USD purpose (C++ bridge doesn't expose this yet; defaults to Default).
    pub purpose: Purpose,
}

/// A points primitive (from UsdGeomPoints).
#[derive(Clone, Debug)]
pub struct PointsPrim {
    /// USD prim path
    pub path: String,
    /// World-space positions
    pub positions: Vec<Vec3>,
    /// Per-point widths
    pub widths: Option<Vec<f32>>,
    /// Per-point normals
    pub normals: Option<Vec<Vec3>>,
    /// Per-point IDs
    pub ids: Option<Vec<i64>>,
    /// World transform
    pub transform: Mat4,
    /// USD purpose (C++ bridge doesn't expose this yet; defaults to Default).
    pub purpose: Purpose,
}

/// A camera defined in the scene graph (from a Camera primitive).
#[derive(Clone, Debug)]
pub struct SceneCamera {
    /// Display name (e.g. "Camera")
    pub name: String,
    /// Instance index in the scene's instance list
    pub instance_index: usize,
    /// Vertical field of view in radians
    pub fov_y: f32,
    /// Near clip plane
    pub near: f32,
    /// Far clip plane
    pub far: f32,
}

/// A complete scene containing prototypes, instances, and materials.
///
/// This corresponds to a `UsdStage` in USD terminology.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    /// Shared prototype definitions (meshes)
    pub prototypes: Vec<Arc<Prototype>>,

    /// Instances referencing prototypes.
    /// Private - use accessor methods to maintain invariant with instance_animations.
    instances: Vec<Instance>,

    /// Animated transforms for instances (parallel to instances vec).
    /// Private - use accessor methods to maintain invariant with instances.
    instance_animations: Vec<Option<AnimatedTransform>>,

    /// Materials used in the scene
    pub materials: Vec<Arc<Material>>,

    /// Lights in the scene (from UsdLux)
    pub lights: Vec<Light>,

    /// Scene name (usually from filename)
    pub name: String,

    /// Timeline info (optional, only if stage has authored time range)
    pub timeline: Option<TimelineInfo>,

    /// Stage metadata (metersPerUnit, upAxis) — only set for USD-loaded scenes.
    pub stage_metadata: Option<crate::usd::cpp_bridge::UsdStageMetadata>,

    /// Scene cameras (from Camera primitives)
    pub cameras: Vec<SceneCamera>,

    /// Point clouds that expand to instances.
    pub point_clouds: Vec<PointCloud>,

    /// Curves primitives (from UsdGeomBasisCurves)
    pub curves: Vec<CurvesPrim>,

    /// Points primitives (from UsdGeomPoints)
    pub points_prims: Vec<PointsPrim>,
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
    pub fn add_prototype(&mut self, mesh: Arc<Mesh>, name: impl Into<Arc<str>>) -> usize {
        let id = self.prototypes.len();
        let prototype = Arc::new(Prototype::new(id, name.into(), mesh));
        self.prototypes.push(prototype);
        id
    }

    /// Add an existing prototype, re-assigning its ID to fit this scene.
    ///
    /// Unlike `add_prototype`, this preserves the material binding and all
    /// other fields from the source prototype.
    pub fn add_prototype_full(&mut self, proto: &Arc<Prototype>) -> usize {
        let id = self.prototypes.len();
        let mut p = (**proto).clone();
        p.id = id;
        self.prototypes.push(Arc::new(p));
        id
    }

    /// Add an instance of a prototype.
    pub fn add_instance(&mut self, prototype_id: usize, transform: Transform) {
        self.instances.push(Instance::new(prototype_id, transform));
        self.instance_animations.push(None);
    }

    /// Add an instance with an explicit USD prim path.
    pub fn add_instance_with_path(
        &mut self,
        prototype_id: usize,
        transform: Transform,
        prim_path: impl Into<Arc<str>>,
    ) {
        self.instances.push(Instance::with_prim_path(
            prototype_id,
            transform,
            prim_path.into(),
        ));
        self.instance_animations.push(None);
    }

    /// Add an instance with animation data.
    pub fn add_animated_instance(
        &mut self,
        prototype_id: usize,
        transform: Transform,
        animation: AnimatedTransform,
    ) {
        self.instances.push(Instance::new(prototype_id, transform));
        self.instance_animations.push(Some(animation));
    }

    /// Add an instance with animation data and an explicit USD prim path.
    pub fn add_animated_instance_with_path(
        &mut self,
        prototype_id: usize,
        transform: Transform,
        animation: AnimatedTransform,
        prim_path: impl Into<Arc<str>>,
    ) {
        self.instances.push(Instance::with_prim_path(
            prototype_id,
            transform,
            prim_path.into(),
        ));
        self.instance_animations.push(Some(animation));
    }

    /// Set the purpose of the most recently added instance.
    pub fn set_last_instance_purpose(&mut self, purpose: Purpose) {
        if let Some(inst) = self.instances.last_mut() {
            inst.purpose = purpose;
        }
    }

    /// Check if the scene has any animation.
    pub fn has_animation(&self) -> bool {
        self.timeline.is_some()
            && self
                .instance_animations
                .iter()
                .any(|opt| opt.as_ref().is_some_and(|a| a.is_animated()))
    }

    /// Iterate over instances with their optional animations.
    ///
    /// Returns an iterator of (instance, optional_animation) pairs.
    pub fn instances_with_animations(
        &self,
    ) -> impl Iterator<Item = (&Instance, Option<&AnimatedTransform>)> {
        debug_assert_eq!(
            self.instances.len(),
            self.instance_animations.len(),
            "instances and instance_animations must have same length"
        );
        self.instances
            .iter()
            .zip(self.instance_animations.iter())
            .map(|(inst, anim)| (inst, anim.as_ref()))
    }

    /// Get an instance and its optional animation by index.
    pub fn get_instance(&self, idx: usize) -> Option<(&Instance, Option<&AnimatedTransform>)> {
        debug_assert_eq!(
            self.instances.len(),
            self.instance_animations.len(),
            "instances and instance_animations must have same length"
        );
        let inst = self.instances.get(idx)?;
        let anim = self.instance_animations.get(idx)?.as_ref();
        Some((inst, anim))
    }

    /// Get read-only access to the instances slice.
    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    /// Get read-only access to the instance animations slice.
    pub fn instance_animations(&self) -> &[Option<AnimatedTransform>] {
        &self.instance_animations
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
    #[must_use]
    pub fn material_count(&self) -> usize {
        self.materials.len()
    }

    /// Add a point cloud and return its ID.
    pub fn add_point_cloud(&mut self, cloud: PointCloud) -> usize {
        let id = self.point_clouds.len();
        self.point_clouds.push(cloud);
        id
    }

    /// Expand all point clouds into instances, appending to existing instances.
    pub fn expand_point_clouds(&mut self) {
        let all_instances: Vec<Instance> = self
            .point_clouds
            .iter()
            .flat_map(|cloud| cloud.expand())
            .collect();
        for inst in all_instances {
            self.add_instance(inst.prototype_id, inst.transform);
        }
    }

    /// Remove a point cloud by its `.id` field. Returns true if found.
    pub fn remove_point_cloud(&mut self, cloud_id: usize) -> bool {
        let Some(idx) = self.point_clouds.iter().position(|c| c.id == cloud_id) else {
            return false;
        };
        self.point_clouds.remove(idx);
        true
    }

    /// Get total triangle count across all instances.
    #[must_use]
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
    #[must_use]
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Get prototype count.
    #[must_use]
    pub fn prototype_count(&self) -> usize {
        self.prototypes.len()
    }

    /// Remove a prototype and all instances that reference it.
    ///
    /// Re-indexes remaining instances' prototype_ids to account for
    /// the shift. Returns `true` if the prototype existed.
    pub fn remove_prototype(&mut self, proto_id: usize) -> bool {
        if proto_id >= self.prototypes.len() {
            return false;
        }

        self.prototypes.remove(proto_id);

        // Collect indices of instances to remove (those referencing this proto)
        let mut removed_indices: Vec<usize> = Vec::new();
        let mut i = self.instances.len();
        while i > 0 {
            i -= 1;
            if self.instances[i].prototype_id == proto_id {
                self.instances.remove(i);
                self.instance_animations.remove(i);
                removed_indices.push(i);
            } else if self.instances[i].prototype_id > proto_id {
                self.instances[i].prototype_id -= 1;
            }
        }

        // Re-index prototype IDs in remaining prototypes
        for (new_id, proto) in self.prototypes.iter_mut().enumerate() {
            if proto.id != new_id {
                let mut p = (**proto).clone();
                p.id = new_id;
                *proto = Arc::new(p);
            }
        }

        // Remove cameras that pointed at deleted instances
        self.cameras
            .retain(|cam| !removed_indices.contains(&cam.instance_index));
        // Re-index camera instance indices to account for removed instances
        // TODO: O(cameras * removed_indices) — consider sorted removed_indices
        //       with binary search if this becomes a bottleneck on large scenes.
        for cam in &mut self.cameras {
            let shift = removed_indices
                .iter()
                .filter(|&&idx| idx < cam.instance_index)
                .count();
            cam.instance_index -= shift;
        }

        true
    }

    /// Remove a single instance by index.
    pub fn remove_instance(&mut self, instance_index: usize) -> bool {
        if instance_index >= self.instances.len() {
            return false;
        }
        self.instances.remove(instance_index);
        self.instance_animations.remove(instance_index);

        // Remove cameras that pointed at the deleted instance, then fix indices
        self.cameras
            .retain(|cam| cam.instance_index != instance_index);
        for cam in &mut self.cameras {
            if cam.instance_index > instance_index {
                cam.instance_index -= 1;
            }
        }

        true
    }

    /// Remove materials not referenced by any prototype and remap face_material_ids.
    pub fn compact_materials(&mut self) {
        // 1. Collect all referenced material indices
        let mut used: HashSet<u32> = HashSet::new();
        for proto in &self.prototypes {
            if let Some(ref mat) = proto.material {
                if let Some(idx) = self.materials.iter().position(|m| Arc::ptr_eq(m, mat)) {
                    used.insert(idx as u32);
                }
            }
            if let Some(ref ids) = proto.mesh.face_material_ids {
                used.extend(ids.iter().copied());
            }
        }

        // Skip if nothing to compact
        if used.len() == self.materials.len() {
            return;
        }

        // 2. Build remap: old_idx → new_idx
        let mut remap: HashMap<u32, u32> = HashMap::new();
        let mut new_materials = Vec::new();
        for (old_idx, mat) in self.materials.iter().enumerate() {
            if used.contains(&(old_idx as u32)) {
                remap.insert(old_idx as u32, new_materials.len() as u32);
                new_materials.push(mat.clone());
            }
        }

        let removed = self.materials.len() - new_materials.len();

        // 3. Remap face_material_ids on all remaining prototypes
        for proto in &mut self.prototypes {
            let proto_mut = Arc::make_mut(proto);
            if let Some(ref mut ids) = Arc::make_mut(&mut proto_mut.mesh).face_material_ids {
                for id in ids.iter_mut() {
                    if let Some(&new_id) = remap.get(id) {
                        *id = new_id;
                    }
                }
            }
        }

        // 4. Replace materials vec
        self.materials = new_materials;
        log::info!(
            "Compacted materials: removed {} orphans, {} remain",
            removed,
            self.materials.len()
        );
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
            Aabb::EMPTY
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
    fn test_scene_add_point_cloud() {
        use crate::point_cloud::{DistributionMethod, PointAttributes, PointCloud};

        let mut scene = Scene::new("test");

        let mesh = Arc::new(Mesh::new(
            vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            vec![0, 1, 2],
            None,
        ));
        let proto_id = scene.add_prototype(mesh, "tri".to_string());

        let cloud = PointCloud {
            id: 0,
            name: "cloud".into(),
            positions: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0)],
            attributes: PointAttributes {
                proto_indices: vec![0, 0],
                ..Default::default()
            },
            prototype_ids: vec![proto_id],
            transform: Transform::default(),
            distribution: DistributionMethod::Manual,
            invisible_ids: Vec::new(),
        };

        let cloud_id = scene.add_point_cloud(cloud);
        assert_eq!(cloud_id, 0);
        assert_eq!(scene.point_clouds.len(), 1);
        assert_eq!(scene.point_clouds[0].point_count(), 2);

        // Expand and check instances appear
        scene.expand_point_clouds();
        assert_eq!(scene.instance_count(), 2);
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

    #[test]
    fn test_animated_transform_static() {
        let transform = Transform::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let animated = AnimatedTransform::static_only(transform.clone());

        assert!(!animated.is_animated());
        let result = animated.evaluate(0.0);
        assert!((result.translation - transform.translation).length() < 0.001);
    }

    #[test]
    fn test_animated_transform_interpolation() {
        let t0 = Transform::from_translation(Vec3::new(0.0, 0.0, 0.0));
        let t1 = Transform::from_translation(Vec3::new(10.0, 0.0, 0.0));

        let keyframes = vec![
            TransformKeyframe {
                time: 0.0,
                transform: t0.clone(),
            },
            TransformKeyframe {
                time: 10.0,
                transform: t1.clone(),
            },
        ];

        let animated = AnimatedTransform::with_keyframes(t0, keyframes);

        assert!(animated.is_animated());

        // At keyframe
        let at_0 = animated.evaluate(0.0);
        assert!((at_0.translation.x - 0.0).abs() < 0.001);

        let at_10 = animated.evaluate(10.0);
        assert!((at_10.translation.x - 10.0).abs() < 0.001);

        // Midpoint
        let at_5 = animated.evaluate(5.0);
        assert!((at_5.translation.x - 5.0).abs() < 0.001);

        // Before first keyframe
        let before = animated.evaluate(-1.0);
        assert!((before.translation.x - 0.0).abs() < 0.001);

        // After last keyframe
        let after = animated.evaluate(15.0);
        assert!((after.translation.x - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_transform_lerp() {
        let a = Transform::from_translation(Vec3::new(0.0, 0.0, 0.0));
        let b = Transform::from_translation(Vec3::new(10.0, 20.0, 30.0));

        let mid = Transform::lerp(&a, &b, 0.5);
        assert!((mid.translation - Vec3::new(5.0, 10.0, 15.0)).length() < 0.001);
    }

    #[test]
    fn test_compact_materials() {
        let mut scene = Scene::new("test");

        // Add 3 materials: mat0, mat1, mat2
        let mat0 = Arc::new(Material::new("mat0", Vec3::new(1.0, 0.0, 0.0)));
        let mat1 = Arc::new(Material::new("mat1", Vec3::new(0.0, 1.0, 0.0)));
        let mat2 = Arc::new(Material::new("mat2", Vec3::new(0.0, 0.0, 1.0)));
        scene.materials.push(mat0);
        scene.materials.push(mat1.clone());
        scene.materials.push(mat2);

        // Proto A references mat1 via material binding + face_material_ids=[1,1,2]
        let mesh_a = Arc::new(Mesh::new_with_materials(
            vec![
                Vec3::ZERO,
                Vec3::X,
                Vec3::Y,
                Vec3::Z,
                Vec3::ONE,
                Vec3::NEG_ONE,
            ],
            vec![0, 1, 2, 3, 4, 5],
            None,
            None,
            Some(vec![1, 1, 2]),
        ));
        let proto_a = Arc::new(Prototype::new(0, "proto_a".into(), mesh_a).with_material(mat1));
        scene.prototypes.push(proto_a);

        // mat0 is now orphaned (no prototype references it)
        scene.compact_materials();

        // mat0 removed → 2 materials remain (old mat1→new 0, old mat2→new 1)
        assert_eq!(scene.materials.len(), 2);
        assert_eq!(&*scene.materials[0].name, "mat1");
        assert_eq!(&*scene.materials[1].name, "mat2");

        // face_material_ids remapped: [1,1,2] → [0,0,1]
        let ids = scene.prototypes[0].mesh.face_material_ids.as_ref().unwrap();
        assert_eq!(ids, &[0, 0, 1]);
    }
}
