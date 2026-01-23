//! GPU types for viewport rendering.
//!
//! Contains uniform structs, vertex formats, and instance data for wgpu rendering.

use std::collections::HashMap;

use bif_math::{Mat4, Vec4};

use crate::Camera;

/// Maximum number of textures in the viewport texture array.
pub const MAX_VIEWPORT_TEXTURES: usize = 128;

/// Camera uniform data for GPU.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub camera_position: [f32; 4],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            view: Mat4::IDENTITY.to_cols_array_2d(),
            camera_position: [0.0, 0.0, 5.0, 1.0],
        }
    }

    pub fn update_view_proj(&mut self, camera: &Camera) {
        self.view_proj = camera.view_projection_matrix().to_cols_array_2d();
        self.view = camera.view_matrix().to_cols_array_2d();
        self.camera_position = [camera.position.x, camera.position.y, camera.position.z, 1.0];
    }
}

/// Environment IBL parameters uniform.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EnvironmentParamsUniform {
    pub intensity: f32,
    pub rotation: f32,
    pub has_environment: u32,
    pub show_background: u32,
}

impl EnvironmentParamsUniform {
    pub fn new() -> Self {
        Self {
            intensity: 1.0,
            rotation: 0.0,
            has_environment: 0,
            show_background: 0,
        }
    }
}

impl Default for EnvironmentParamsUniform {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self::new()
    }
}

/// Material uniform data for GPU (PBR properties).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform {
    pub diffuse_color: [f32; 4],      // RGB + padding
    pub metallic_roughness: [f32; 4], // metallic, roughness, specular, padding
}

impl MaterialUniform {
    pub fn new() -> Self {
        Self {
            diffuse_color: [0.5, 0.5, 0.5, 1.0],      // Grey default
            metallic_roughness: [0.0, 0.5, 0.5, 0.0], // dielectric, medium rough
        }
    }

    pub fn from_material(mat: &bif_core::Material) -> Self {
        Self {
            diffuse_color: [
                mat.diffuse_color.x,
                mat.diffuse_color.y,
                mat.diffuse_color.z,
                1.0,
            ],
            metallic_roughness: [mat.metallic, mat.roughness, mat.specular, 0.0],
        }
    }
}

impl Default for MaterialUniform {
    fn default() -> Self {
        Self::new()
    }
}

/// Material table entry for GPU sampling (per-material data).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialGpu {
    pub diffuse_color: [f32; 4],      // RGB + padding
    pub metallic_roughness: [f32; 4], // metallic, roughness, specular, padding
    pub texture_indices: [u32; 4],    // diffuse, roughness, metallic, emissive
    pub extra_indices: [u32; 4],      // normal, reserved, reserved, reserved
}

impl MaterialGpu {
    pub fn from_material(material: &bif_core::Material, textures: &GpuTextureSet) -> Self {
        let resolve_index = |path: &Option<String>| -> u32 {
            path.as_ref()
                .and_then(|p| textures.index_map.get(p).copied())
                .unwrap_or(0)
        };

        Self {
            diffuse_color: [
                material.diffuse_color.x,
                material.diffuse_color.y,
                material.diffuse_color.z,
                1.0,
            ],
            metallic_roughness: [
                material.metallic,
                material.roughness,
                material.specular,
                0.0,
            ],
            texture_indices: [
                resolve_index(&material.diffuse_texture),
                resolve_index(&material.roughness_texture),
                resolve_index(&material.metallic_texture),
                resolve_index(&material.emissive_texture),
            ],
            extra_indices: [resolve_index(&material.normal_texture), 0, 0, 0],
        }
    }
}

/// Gnomon uniform data for GPU (camera rotation only).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GnomonUniform {
    pub view_rotation: [[f32; 4]; 4],
}

impl GnomonUniform {
    pub fn new() -> Self {
        Self {
            view_rotation: Mat4::IDENTITY.to_cols_array_2d(),
        }
    }

    pub fn update_from_camera(&mut self, camera: &Camera) {
        // Extract rotation from view matrix (zero out translation)
        let view = camera.view_matrix();
        // The view matrix is [R | t], we want just R with no translation
        let rotation = Mat4::from_cols(
            view.col(0),
            view.col(1),
            view.col(2),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        );
        self.view_rotation = rotation.to_cols_array_2d();
    }
}

impl Default for GnomonUniform {
    fn default() -> Self {
        Self::new()
    }
}

/// Gnomon vertex (position + color).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GnomonVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl GnomonVertex {
    pub const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GnomonVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }

    /// Create gnomon axis vertices (origin to X, Y, Z with colors).
    pub fn create_axes() -> Vec<Self> {
        vec![
            // X axis (red)
            GnomonVertex {
                position: [0.0, 0.0, 0.0],
                color: [1.0, 0.2, 0.2],
            },
            GnomonVertex {
                position: [1.0, 0.0, 0.0],
                color: [1.0, 0.2, 0.2],
            },
            // Y axis (green)
            GnomonVertex {
                position: [0.0, 0.0, 0.0],
                color: [0.2, 1.0, 0.2],
            },
            GnomonVertex {
                position: [0.0, 1.0, 0.0],
                color: [0.2, 1.0, 0.2],
            },
            // Z axis (blue)
            GnomonVertex {
                position: [0.0, 0.0, 0.0],
                color: [0.2, 0.5, 1.0],
            },
            GnomonVertex {
                position: [0.0, 0.0, 1.0],
                color: [0.2, 0.5, 1.0],
            },
        ]
    }
}

/// Vertex data for rendering.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
    pub material_id: u32,
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32x2, 4 => Uint32];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Instance data for GPU instancing.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceData {
    pub model_matrix: [[f32; 4]; 4],
    pub material_id: u32,
}

impl InstanceData {
    // Shifted to slots 5-9 to make room for vertex material_id at slot 4
    const ATTRIBS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
        8 => Float32x4,
        9 => Uint32
    ];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceData>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// GPU texture set with index mapping.
pub struct GpuTextureSet {
    pub textures: Vec<wgpu::Texture>,
    pub views: Vec<wgpu::TextureView>,
    pub index_map: HashMap<String, u32>,
}

/// Scratch buffers for frustum culling to avoid per-frame allocations.
pub struct CullingScratch {
    pub visible_with_distance: Vec<(f32, usize)>,
    pub near_instances: Vec<InstanceData>,
    pub far_instances: Vec<InstanceData>,
}

impl CullingScratch {
    pub fn new(max_instances: usize) -> Self {
        Self {
            visible_with_distance: Vec::with_capacity(max_instances),
            near_instances: Vec::with_capacity(max_instances),
            far_instances: Vec::with_capacity(max_instances),
        }
    }

    pub fn clear(&mut self) {
        self.visible_with_distance.clear();
        self.near_instances.clear();
        self.far_instances.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_culling_scratch_new_and_clear() {
        let mut scratch = CullingScratch::new(100);
        scratch.visible_with_distance.push((1.0, 0));
        scratch.near_instances.push(InstanceData {
            model_matrix: [[0.0; 4]; 4],
            material_id: 0,
        });
        scratch.clear();
        assert!(scratch.visible_with_distance.is_empty());
        assert!(scratch.near_instances.is_empty());
        assert!(scratch.far_instances.is_empty());
    }

    #[test]
    fn test_gnomon_vertex_create_axes() {
        let axes = GnomonVertex::create_axes();
        assert_eq!(axes.len(), 6); // 3 axes * 2 verts
    }

    #[test]
    fn test_material_uniform_default() {
        let mat = MaterialUniform::new();
        assert_eq!(mat.diffuse_color[0], 0.5);
        assert_eq!(mat.metallic_roughness[0], 0.0); // metallic
        assert_eq!(mat.metallic_roughness[1], 0.5); // roughness
    }

    #[test]
    fn test_camera_uniform_default() {
        let cam = CameraUniform::new();
        // Should be identity matrices
        assert_eq!(cam.view_proj[0][0], 1.0);
        assert_eq!(cam.view_proj[1][1], 1.0);
        assert_eq!(cam.view_proj[2][2], 1.0);
        assert_eq!(cam.view_proj[3][3], 1.0);
        // Camera position default
        assert_eq!(cam.camera_position[2], 5.0);
    }
}
