//! GPU types for viewport rendering.
//!
//! Contains uniform structs, vertex formats, and instance data for wgpu rendering.

use std::collections::HashMap;
use std::sync::Arc;

use bif_math::{Mat4, Vec4};

use crate::Camera;

/// Maximum number of textures in the viewport texture array.
/// 2048 supports production scenes (1000+ textures). Modern Vulkan/DX12 GPUs
/// handle 16K+ sampled textures per stage. Device creation requests this limit
/// and falls back if GPU doesn't support it.
pub const MAX_VIEWPORT_TEXTURES: usize = 2048;

/// No selection sentinel (0xFFFFFFFF means nothing is selected).
pub const NO_SELECTION: u32 = 0xFFFFFFFF;

/// Camera uniform data for GPU.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub camera_position: [f32; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    /// Selected instance ID for highlight tint (0xFFFFFFFF = no selection).
    pub selected_instance_id: u32,
    pub _pad_selection: [u32; 3],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            view: Mat4::IDENTITY.to_cols_array_2d(),
            camera_position: [0.0, 0.0, 5.0, 1.0],
            inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            selected_instance_id: NO_SELECTION,
            _pad_selection: [0; 3],
        }
    }

    pub fn update_view_proj(&mut self, camera: &Camera) {
        let vp = camera.view_projection_matrix();
        self.view_proj = vp.to_cols_array_2d();
        self.view = camera.view_matrix().to_cols_array_2d();
        self.camera_position = [camera.position.x, camera.position.y, camera.position.z, 1.0];
        self.inv_view_proj = vp.inverse().to_cols_array_2d();
    }
}

/// Environment IBL parameters uniform.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EnvironmentParamsUniform {
    pub intensity: f32,
    pub rotation: f32,
    pub has_environment: u32,
    pub max_mip: f32,
}

/// Maximum number of lights in the viewport.
/// 32 matches common DCC limits and allows for more complex scenes.
pub const MAX_VIEWPORT_LIGHTS: usize = 32;

/// Light type constants (matching shader).
pub const LIGHT_TYPE_DISTANT: u32 = 0;
pub const LIGHT_TYPE_POINT: u32 = 1;
pub const LIGHT_TYPE_RECT: u32 = 2;

/// Light data for GPU (packed for uniform buffer).
/// Layout: position_type.xyz = position, position_type.w = type
///         direction_radius.xyz = direction, direction_radius.w = radius
///         color_intensity.rgb = color, color_intensity.a = intensity
///         params.x = angle, params.y = width, params.z = height, params.w = unused
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightGpu {
    pub position_type: [f32; 4],    // xyz = position, w = type (as f32)
    pub direction_radius: [f32; 4], // xyz = direction, w = radius
    pub color_intensity: [f32; 4],  // rgb = color, a = intensity
    pub params: [f32; 4],           // angle, width, height, unused
}

impl LightGpu {
    /// Create a distant (directional) light.
    pub fn distant(direction: [f32; 3], color: [f32; 3], intensity: f32, angle: f32) -> Self {
        Self {
            position_type: [0.0, 0.0, 0.0, LIGHT_TYPE_DISTANT as f32],
            direction_radius: [direction[0], direction[1], direction[2], 0.0],
            color_intensity: [color[0], color[1], color[2], intensity],
            params: [angle, 0.0, 0.0, 0.0],
        }
    }

    /// Create a point (sphere) light.
    pub fn point(position: [f32; 3], color: [f32; 3], intensity: f32, radius: f32) -> Self {
        Self {
            position_type: [
                position[0],
                position[1],
                position[2],
                LIGHT_TYPE_POINT as f32,
            ],
            direction_radius: [0.0, 0.0, 0.0, radius],
            color_intensity: [color[0], color[1], color[2], intensity],
            params: [0.0, 0.0, 0.0, 0.0],
        }
    }

    /// Create an area (rect) light.
    pub fn rect(
        position: [f32; 3],
        direction: [f32; 3],
        color: [f32; 3],
        intensity: f32,
        width: f32,
        height: f32,
    ) -> Self {
        Self {
            position_type: [
                position[0],
                position[1],
                position[2],
                LIGHT_TYPE_RECT as f32,
            ],
            direction_radius: [direction[0], direction[1], direction[2], 0.0],
            color_intensity: [color[0], color[1], color[2], intensity],
            params: [0.0, width, height, 0.0],
        }
    }

    /// Create a zeroed/empty light slot.
    pub fn empty() -> Self {
        Self {
            position_type: [0.0; 4],
            direction_radius: [0.0; 4],
            color_intensity: [0.0; 4],
            params: [0.0; 4],
        }
    }
}

/// Lights uniform buffer (array of lights + count).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightsUniform {
    pub lights: [LightGpu; MAX_VIEWPORT_LIGHTS],
    pub light_count: [u32; 4], // Use vec4 for alignment; only [0] is count
}

impl LightsUniform {
    pub fn new() -> Self {
        Self {
            lights: [LightGpu::empty(); MAX_VIEWPORT_LIGHTS],
            light_count: [0, 0, 0, 0],
        }
    }

    /// Create from scene lights (bif_core::Light).
    pub fn from_scene_lights(scene_lights: &[bif_core::Light]) -> Self {
        let mut uniform = Self::new();

        if scene_lights.len() > MAX_VIEWPORT_LIGHTS {
            log::warn!(
                "Scene has {} lights, viewport limited to {}",
                scene_lights.len(),
                MAX_VIEWPORT_LIGHTS
            );
        }

        let count = scene_lights.len().min(MAX_VIEWPORT_LIGHTS);
        uniform.light_count[0] = count as u32;

        for (i, light) in scene_lights.iter().take(MAX_VIEWPORT_LIGHTS).enumerate() {
            uniform.lights[i] = match light {
                bif_core::Light::Distant {
                    direction,
                    color,
                    intensity,
                    angle,
                } => LightGpu::distant(
                    [direction.x, direction.y, direction.z],
                    [color.x, color.y, color.z],
                    *intensity,
                    *angle,
                ),
                bif_core::Light::Point {
                    position,
                    color,
                    intensity,
                    radius,
                } => LightGpu::point(
                    [position.x, position.y, position.z],
                    [color.x, color.y, color.z],
                    *intensity,
                    *radius,
                ),
                bif_core::Light::Rect {
                    transform,
                    color,
                    intensity,
                    width,
                    height,
                } => {
                    // Extract position and direction from transform
                    let position = transform.w_axis.truncate();
                    let direction = -transform.z_axis.truncate().normalize();
                    LightGpu::rect(
                        [position.x, position.y, position.z],
                        [direction.x, direction.y, direction.z],
                        [color.x, color.y, color.z],
                        *intensity,
                        *width,
                        *height,
                    )
                }
                bif_core::Light::Dome { .. } => {
                    // DomeLights are handled via IBL, not as direct lights
                    LightGpu::empty()
                }
            };
        }

        uniform
    }
}

impl Default for LightsUniform {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvironmentParamsUniform {
    pub fn new() -> Self {
        Self {
            intensity: 1.0,
            rotation: 0.0,
            has_environment: 0,
            max_mip: 4.0,
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
    pub base_color: [f32; 4],      // [r, g, b, metalness]
    pub specular_params: [f32; 4], // [roughness, ior, weight, pad]
}

impl MaterialUniform {
    pub fn new() -> Self {
        Self {
            base_color: [0.8, 0.8, 0.8, 0.0], // OpenPBR default, metalness=0
            specular_params: [0.3, 1.5, 1.0, 0.0], // roughness=0.3, ior=1.5, weight=1.0
        }
    }

    pub fn from_material(mat: &bif_core::Material) -> Self {
        Self {
            base_color: [
                mat.base_color.x,
                mat.base_color.y,
                mat.base_color.z,
                mat.base_metalness,
            ],
            specular_params: [
                mat.specular_roughness,
                mat.specular_ior,
                mat.specular_weight,
                0.0,
            ],
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
    pub base_color: [f32; 4],      // [r, g, b, metalness]
    pub specular_params: [f32; 4], // [roughness, ior, weight, pad]
    pub emission: [f32; 4],        // [r, g, b, luminance]
    pub extra_params: [f32; 4],    // [opacity, coat_weight, coat_roughness, pad]
    pub texture_indices: [u32; 4], // [base_color, roughness, metalness, emission]
    /// [0]=normal, [1]=udim_grid(cols<<16|rows), [2]=udim_offset(min_col<<16|min_row), [3]=reserved
    pub extra_indices: [u32; 4],
}

impl MaterialGpu {
    pub fn from_material(material: &bif_core::Material, textures: &GpuTextureSet) -> Self {
        let src_dir = material.source_dir.as_deref();
        let resolve_index = |path: &Option<Arc<str>>| -> u32 {
            path.as_ref()
                .and_then(|p| {
                    let normalized = p.replace('\\', "/");
                    if let Some(&idx) = textures.index_map.get(&normalized) {
                        return Some(idx);
                    }
                    if let Some(&idx) = textures.index_map.get(&**p) {
                        return Some(idx);
                    }
                    if let Some(dir) = src_dir {
                        let resolved = dir.join(&**p);
                        let resolved_str = resolved.to_string_lossy().replace('\\', "/");
                        if let Some(&idx) = textures.index_map.get(&resolved_str) {
                            return Some(idx);
                        }
                    }
                    None
                })
                .unwrap_or(0)
        };

        let base_color_idx = resolve_index(&material.base_color_texture);
        let rough_idx = resolve_index(&material.specular_roughness_texture);
        let metal_idx = resolve_index(&material.base_metalness_texture);
        let emissive_idx = resolve_index(&material.emission_texture);
        let normal_idx = resolve_index(&material.normal_texture);

        let udim_info = [
            base_color_idx,
            rough_idx,
            metal_idx,
            normal_idx,
            emissive_idx,
        ]
        .iter()
        .filter(|&&idx| idx != 0)
        .find_map(|&idx| textures.udim_grid.get(&idx))
        .copied();
        let (grid_packed, offset_packed) = udim_info
            .map(|g| ((g[0] << 16) | g[1], (g[2] << 16) | g[3]))
            .unwrap_or((0, 0));

        Self {
            base_color: [
                material.base_color.x,
                material.base_color.y,
                material.base_color.z,
                material.base_metalness,
            ],
            specular_params: [
                material.specular_roughness,
                material.specular_ior,
                material.specular_weight,
                0.0,
            ],
            emission: [
                material.emission_color.x,
                material.emission_color.y,
                material.emission_color.z,
                material.emission_luminance,
            ],
            extra_params: [material.geometry_opacity, 0.0, 0.0, 0.0],
            texture_indices: [base_color_idx, rough_idx, metal_idx, emissive_idx],
            extra_indices: [normal_idx, grid_packed, offset_packed, 0],
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
    /// Offset into the compact triangle material buffer for this instance's prototype.
    /// The shader adds this to `primitive_index` to index the correct prototype's
    /// per-triangle material IDs.
    pub tri_mat_offset: u32,
}

impl InstanceData {
    // Shifted to slots 5-10 to make room for vertex material_id at slot 4
    const ATTRIBS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
        8 => Float32x4,
        9 => Uint32,
        10 => Uint32
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
    /// UDIM atlas grid info per texture: tex_index → [cols, rows, min_col, min_row]
    pub udim_grid: HashMap<u32, [u32; 4]>,
}

/// Per-prototype GPU buffers for multi-draw rendering.
///
/// Instead of combining all meshes into a single buffer, each prototype
/// has its own vertex/index buffers. This enables:
/// - Per-instance transforms (no baked transforms)
/// - Simpler vertex animation (update one prototype's buffer)
/// - Multiple draw calls indexed by prototype
pub struct PrototypeGpuData {
    /// Vertex buffer for this prototype's mesh
    pub vertex_buffer: wgpu::Buffer,
    /// Index buffer for this prototype's mesh
    pub index_buffer: wgpu::Buffer,
    /// Number of indices to draw
    pub num_indices: u32,
    /// Number of vertices in the buffer
    pub num_vertices: u32,
    /// Prototype ID in the scene
    pub prototype_id: usize,
    /// USD mesh index (for vertex animation lookup)
    pub mesh_idx: usize,
    /// Per-triangle material IDs buffer (optional, for GeomSubsets)
    pub triangle_material_buffer: Option<wgpu::Buffer>,
    /// Offset into the compact triangle material buffer for this prototype.
    pub tri_mat_offset: u32,
    /// Number of triangles in this prototype (for compact buffer building).
    pub num_triangles: u32,
    /// Original vertices (CPU-side) for vertex animation updates.
    /// Stores normals/UVs so we can update just positions.
    pub vertices: Vec<Vertex>,
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
            tri_mat_offset: 0,
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
        assert_eq!(mat.base_color[0], 0.8); // OpenPBR default
        assert_eq!(mat.base_color[3], 0.0); // metalness
        assert_eq!(mat.specular_params[0], 0.3); // roughness
        assert_eq!(mat.specular_params[1], 1.5); // ior
        assert_eq!(mat.specular_params[2], 1.0); // weight
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
        // Selection default
        assert_eq!(cam.selected_instance_id, NO_SELECTION);
    }
}
