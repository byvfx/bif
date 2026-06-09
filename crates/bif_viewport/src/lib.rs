use anyhow::Result;
use std::num::NonZeroU32;
use std::sync::Arc;
use wgpu::{util::DeviceExt, Device, Queue, Surface, SurfaceConfiguration};

use bif_math::{Camera, Mat4, Vec3};

// Typed event bus (replaces egui temp-data ad-hoc event passing)
pub mod app_event;

// New modular architecture
pub mod batch_render;
pub mod compute_ibl;
pub mod culling_manager;
pub mod curve_preview;
pub mod environment;
pub mod environment_manager;
pub mod frustum_culling;
pub mod gizmo;
pub mod gnomon;
pub mod gpu_types;
pub mod grid;
pub mod ivar_renderer;
pub mod ivar_state;
pub mod layer_stack_panel;
pub mod lights;
pub mod mesh_data;
pub mod multi_draw;
pub mod point_preview;
pub mod texture_loader;
mod transform_gizmo;

// Scene browser and property inspector modules
mod animation;
mod ivar_build;
mod node_dispatch;
pub mod node_graph;
pub mod persistence;
mod project_dispatch;
pub mod property_inspector;
mod render;
mod render_dispatch;
pub mod scene_browser;
mod scene_cmd;
mod scene_loader;
pub mod scene_manager;
mod scene_pipeline;
pub mod selection;
mod selection_dispatch;
pub mod skybox;
pub mod theme;
pub mod timeline;
mod types;

// Re-exports from new modules
pub use app_event::{AppEvent, EventBus};
pub use bif_math::OrthoPreset;
pub use culling_manager::CullingManager;
pub use environment::GpuEnvironment;
pub use environment_manager::EnvironmentManager;
pub use frustum_culling::{update_visible_instances, CullingResult};
pub use gnomon::GnomonRenderer;
pub use gpu_types::{
    CameraUniform, CullingScratch, EnvironmentParamsUniform, GnomonUniform, GnomonVertex,
    GpuTextureSet, InstanceData, LightGpu, LightsUniform, MaterialGpu, MaterialUniform,
    PrototypeGpuData, Vertex, MAX_VIEWPORT_LIGHTS, MAX_VIEWPORT_TEXTURES,
};
pub use grid::GridRenderer;
pub use ivar_renderer::{
    create_depth_texture, create_ivar_bind_group, create_ivar_pipeline, create_ivar_texture,
};
pub use ivar_state::{
    BatchRenderSettings, BatchRenderStatus, BuildStatus, CameraSnapshot, CameraSource, IvarMessage,
    IvarState, RenderMode,
};
pub use lights::LightsManager;
pub use mesh_data::MeshData;
pub use multi_draw::MultiDrawState;
pub use scene_cmd::SceneCmd;
pub use scene_manager::SceneManager;
pub use selection::SelectionManager;
pub use texture_loader::{
    collect_scene_texture_paths, create_default_gpu_textures, create_gpu_texture, MipmapGenerator,
    TextureLoadMessage, DEFAULT_MAX_VIEWPORT_TEXTURE_SIZE,
};
pub use timeline::TimelineState;
use transform_gizmo::TransformGizmoRenderer;
pub use types::*;

pub use node_graph::{render_node_graph, GraphNodeId, NodeGraphEvent, NodeGraphState, SceneNode};
pub use property_inspector::{render_property_inspector, PrimProperties, TransformEdit};
pub use scene_browser::{
    build_scene_graph_cache, CachedSceneGraph, CompositeProvider, EmptyPrimProvider,
    NodeFilteredProvider, PrimDataProvider, PrimDisplayInfo, ProceduralPrim, ProceduralPrimKind,
    SceneBrowserState, SceneBrowserViewMode,
};

/// Maximum instance count for dynamic instance buffer.
const MAX_INSTANCES: u32 = 100_000;

// Type definitions moved to types.rs — re-exported below via `pub use types::*`

/// GPU material state — uniforms, bind groups, material table, triangle materials.
pub(crate) struct GpuMaterialState {
    pub uniform: MaterialUniform,
    pub buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub table_buffer: wgpu::Buffer,
    pub table_len: u32,
    /// Per-triangle material IDs for primitive_index lookup (GeomSubsets).
    pub triangle_buffer: wgpu::Buffer,
    /// Whether we have per-triangle materials (vs instance materials).
    pub has_triangle_materials: bool,
}

/// GPU texture state — textures, sampler, bind group.
pub(crate) struct GpuTextureState {
    pub gpu_textures: GpuTextureSet,
    pub sampler: wgpu::Sampler,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct OutlineParamsUniform {
    pub color: [f32; 4],
    pub width_ndc: f32,
    pub _padding: [f32; 3],
}

/// GPU plumbing — surface, device, queue, config.
pub(crate) struct GpuContext {
    pub surface: Surface<'static>,
    pub device: Device,
    pub queue: Queue,
    pub config: SurfaceConfiguration,
}

/// Camera state — camera, uniform, GPU buffer, bind group, viewport source.
pub struct CameraState {
    pub camera: Camera,
    pub(crate) camera_uniform: CameraUniform,
    pub(crate) camera_buffer: wgpu::Buffer,
    pub(crate) camera_bind_group: wgpu::BindGroup,
    pub(crate) viewport_camera_source: CameraSource,
    pub(crate) camera_locked: bool,
    pub(crate) selected_usd_camera: Option<String>,
}

/// Ivar CPU path tracer integration — state, GPU resources, materials.
pub(crate) struct IvarContext {
    pub ivar_state: IvarState,
    pub ivar_texture: wgpu::Texture,
    pub ivar_texture_view: wgpu::TextureView,
    pub ivar_sampler: wgpu::Sampler,
    pub ivar_bind_group: wgpu::BindGroup,
    pub ivar_bind_group_layout: wgpu::BindGroupLayout,
    pub ivar_pipeline: wgpu::RenderPipeline,
    /// Cached OpenPBR materials (avoids re-loading textures on every Ivar build).
    pub ivar_materials: Option<Vec<Arc<bif_renderer::OpenPbrSurface>>>,
    /// Persistent texture cache — survives material rebuilds so unchanged
    /// textures reuse existing `Arc<Texture>` instead of reloading from disk.
    pub ivar_texture_cache: Option<bif_core::texture::TextureCache>,
}

/// Node graph evaluation state — graph, mappings, caches.
pub(crate) struct NodeGraphContext {
    pub node_graph_state: NodeGraphState,
    pub node_outputs: std::collections::HashMap<node_graph::GraphNodeId, node_graph::NodeOutputs>,
    pub next_cloud_id: usize,
    pub node_scatter_surface_map: std::collections::HashMap<node_graph::GraphNodeId, usize>,
    pub instancer_results:
        std::collections::BTreeMap<node_graph::GraphNodeId, Vec<bif_core::Instance>>,
    pub cached_scene_graph: scene_browser::CachedSceneGraph,
    pub scene_graph_dirty: bool,
    pub primitive_name_counters: std::collections::HashMap<String, usize>,
    pub materials_dirty: bool,
    pub xform_property_changed: Option<node_graph::GraphNodeId>,
    pub node_prim_counts: std::collections::HashMap<node_graph::GraphNodeId, usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UndoActionKind {
    Procedural,
    Usd,
}

/// Convert a synthetic `/BIF/{real_path}/{idx}` instance path back to the real path.
///
/// `resolve_prim_path` generates synthetic paths when an instance has no USD prim path
/// set; this undoes that transformation so selection events use real USD paths that
/// the tree browser can match.
fn denormalize_synthetic_path(prim_path: &str) -> String {
    if let Some(stripped) = prim_path.strip_prefix("/BIF/") {
        if let Some(last_slash) = stripped.rfind('/') {
            let suffix = &stripped[last_slash + 1..];
            if suffix.parse::<usize>().is_ok() {
                return stripped[..last_slash].to_string();
            }
        }
        return stripped.to_string();
    }
    prim_path.to_string()
}

fn normalize_display_path(path: &str) -> String {
    let denormalized = denormalize_synthetic_path(path);
    let trimmed = denormalized.trim().trim_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn shader_value_as_f32(value: &bif_core::usd::ShaderValue) -> Option<f32> {
    match value {
        bif_core::usd::ShaderValue::Float(v) => Some(*v),
        bif_core::usd::ShaderValue::Double(v) => Some(*v as f32),
        bif_core::usd::ShaderValue::Int(v) => Some(*v as f32),
        _ => None,
    }
}

fn shader_value_as_vec3(value: &bif_core::usd::ShaderValue) -> Option<Vec3> {
    match value {
        bif_core::usd::ShaderValue::Color3f(v) | bif_core::usd::ShaderValue::Vec3f(v) => {
            Some(Vec3::new(v[0], v[1], v[2]))
        }
        _ => None,
    }
}

fn apply_shader_value_to_material(
    material: &mut bif_core::Material,
    input_name: &str,
    value: &bif_core::usd::ShaderValue,
) {
    match input_name {
        "base_color" | "diffuseColor" => {
            if let Some(v) = shader_value_as_vec3(value) {
                material.base_color = v;
            }
        }
        "metallic" | "metalness" | "base_metalness" => {
            if let Some(v) = shader_value_as_f32(value) {
                material.base_metalness = v;
            }
        }
        "roughness" | "specular_roughness" | "base_diffuse_roughness" => {
            if let Some(v) = shader_value_as_f32(value) {
                material.specular_roughness = v;
            }
        }
        "specular_weight" => {
            if let Some(v) = shader_value_as_f32(value) {
                material.specular_weight = v;
            }
        }
        "specular_ior" => {
            if let Some(v) = shader_value_as_f32(value) {
                material.specular_ior = v;
            }
        }
        "geometry_opacity" | "opacity" => {
            if let Some(v) = shader_value_as_f32(value) {
                material.geometry_opacity = v;
            }
        }
        _ => {}
    }
}

fn apply_usd_material_to_material(
    material: &mut bif_core::Material,
    usd_material: &bif_core::usd::cpp_bridge::UsdMaterialData,
) {
    material.base_color = usd_material.base_color;
    material.base_metalness = usd_material.base_metalness;
    material.specular_roughness = usd_material.specular_roughness;
    material.specular_weight = usd_material.specular_weight;
    material.specular_ior = usd_material.specular_ior;
    material.emission_color = usd_material.emission_color;
    material.transmission_weight = usd_material.transmission_weight;
    material.geometry_opacity = usd_material.geometry_opacity;
    material.base_color_texture = usd_material.base_color_texture.as_deref().map(Arc::from);
    material.specular_roughness_texture = usd_material
        .specular_roughness_texture
        .as_deref()
        .map(Arc::from);
    material.base_metalness_texture = usd_material
        .base_metalness_texture
        .as_deref()
        .map(Arc::from);
    material.normal_texture = usd_material.normal_texture.as_deref().map(Arc::from);
    material.emission_texture = usd_material.emission_texture.as_deref().map(Arc::from);
    material.geometry_opacity_texture = usd_material
        .geometry_opacity_texture
        .as_deref()
        .map(Arc::from);
    material.displacement_texture = usd_material.displacement_texture.as_deref().map(Arc::from);
    material.displacement_scale = usd_material.displacement_scale;
}

/// Core renderer managing wgpu state
pub struct Renderer {
    /// Display scale factor (device-independent pixels per point). Used for
    /// egui overlay tessellation. Callers update via [`Renderer::resize`].
    pub(crate) scale_factor: f32,

    /// Optional hook invoked around native-dialog presentations to work
    /// around the Windows z-order issue where file/message dialogs can get
    /// stuck behind the main window. `bif_viewer` installs a closure that
    /// toggles its winit Window; `bif_qt` leaves this `None`.
    pub(crate) dialog_focus_hook: Option<Box<dyn Fn(bool)>>,

    pub(crate) gpu: GpuContext,
    pub size: (u32, u32),
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) wireframe_pipeline: wgpu::RenderPipeline,
    pub(crate) wireframe_cam_buffer: wgpu::Buffer,
    pub(crate) wireframe_cam_bind_group: wgpu::BindGroup,
    pub(crate) outline_params_buffer: wgpu::Buffer,
    pub(crate) outline_params_bind_group: wgpu::BindGroup,
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) index_buffer: wgpu::Buffer,
    pub(crate) num_indices: u32,
    pub(crate) instance_buffer: wgpu::Buffer,
    pub(crate) num_instances: u32,
    pub cam: CameraState,
    pub(crate) materials: GpuMaterialState,
    pub(crate) textures: GpuTextureState,
    pub(crate) mesh_bounds_min: Vec3,
    pub(crate) mesh_bounds_max: Vec3,
    pub(crate) depth_texture: wgpu::Texture,
    pub(crate) depth_view: wgpu::TextureView,

    // Gnomon resources
    pub(crate) gnomon: GnomonRenderer,

    // Ground grid
    pub(crate) grid: GridRenderer,

    // Selected-prim transform gizmo
    pub(crate) transform_gizmo: TransformGizmoRenderer,

    // UI state
    pub fps: f32,
    pub(crate) frame_count: u32,
    pub(crate) fps_update_timer: f32,

    // Stats - TODO: Track polygon count from source data for accuracy
    pub(crate) num_triangles: u64,

    // UI layout metrics (for viewport-safe overlays)
    pub(crate) ui_layout: UiLayout,

    // Ivar CPU path tracer integration
    pub(crate) ivar: IvarContext,

    // Scene data (geometry, instances, materials, USD stage, undo/redo)
    pub scene: SceneManager,
    pub(crate) last_action_stack: Vec<UndoActionKind>,
    pub(crate) redo_action_stack: Vec<UndoActionKind>,

    // Multi-draw state for per-prototype rendering
    pub(crate) multi_draw: MultiDrawState,

    // Culling and LOD state
    pub(crate) culling: CullingManager,

    /// Display settings (purpose toggle, LOD enable)
    pub display_settings: DisplaySettings,

    // Node graph evaluation state
    pub(crate) nodes: NodeGraphContext,

    // Typed event bus (replaces egui temp-data ad-hoc event passing)
    pub(crate) event_bus: EventBus,

    // Project persistence state (open file, dirty flag)
    pub project: persistence::ProjectState,
    /// Cached recent files list (avoids per-frame disk reads).
    pub(crate) recent_files: persistence::RecentFiles,

    // Unified selection state (prim path, properties, instance, scene browser, gizmo)
    pub selection: SelectionManager,

    /// Layer Stack panel UI state (v0.14.0). Data lives on
    /// `self.scene.layer_state`; this struct holds only the per-panel
    /// scroll/focus state. Phase F left it unread — the egui panel that
    /// consumed it is in-tree dead code pending Qt replacement.
    #[allow(dead_code)]
    pub(crate) layer_stack_panel: crate::layer_stack_panel::LayerStackPanel,

    // Timeline state for animation playback
    pub timeline_state: TimelineState,

    // Environment IBL and skybox state
    pub(crate) environment: EnvironmentManager,

    // Lights state (UsdLux)
    pub(crate) lights: LightsManager,

    // Viewport picking state
    /// Embree pick scene for click-to-select (rebuilt on scene load)
    pub(crate) pick_scene: Option<bif_renderer::EmbreePickScene>,

    // Point preview renderer for scatter visualization
    pub(crate) point_preview: point_preview::PointPreviewRenderer,
    /// Last viewport size used for point preview params (for change detection).
    pub(crate) point_preview_last_vp: (f32, f32),
    /// Whether point preview params need a GPU write next frame.
    pub(crate) point_preview_params_dirty: bool,

    // Curve/points preview renderer (UsdGeomBasisCurves + UsdGeomPoints)
    pub(crate) curve_preview: curve_preview::CurvePreviewRenderer,

    // Viewport display toggles
    pub show_grid: bool,
    /// Apply Z-to-Y axis correction when stage is Z-up.
    pub apply_axis_correction: bool,
    /// Apply metersPerUnit scaling to match viewport (assumed meters).
    pub apply_unit_scaling: bool,

    /// Async channel receivers and status for background operations.
    pub(crate) async_channels: AsyncChannels,
    /// GPU compute mipmap generator (shared across texture uploads).
    pub(crate) mipmap_generator: texture_loader::MipmapGenerator,
}

/// Required wgpu feature set for the renderer. Callers building their own
/// `wgpu::Device` (bif_viewer, bif_qt) should request at least these features.
pub const REQUIRED_FEATURES: wgpu::Features = wgpu::Features::TEXTURE_BINDING_ARRAY
    .union(wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING)
    .union(wgpu::Features::SHADER_PRIMITIVE_INDEX)
    .union(wgpu::Features::POLYGON_MODE_LINE);

/// Recommended wgpu limits for the renderer. Callers can clone or adjust but
/// must satisfy at least `max_sampled_textures_per_shader_stage = MAX_VIEWPORT_TEXTURES`
/// and `max_bind_groups >= 5`.
pub fn required_limits() -> wgpu::Limits {
    wgpu::Limits {
        max_sampled_textures_per_shader_stage: MAX_VIEWPORT_TEXTURES as u32,
        max_buffer_size: 1 << 30, // 1GB for large meshes
        max_bind_groups: 5,       // Groups 0-4 (camera, material, texture, env, lights)
        ..Default::default()
    }
}

impl Renderer {
    /// Create a node graph node from an external UI surface such as the Qt graph.
    ///
    /// `GraftBranches` is intentionally not bridged here while its behavior is
    /// being redesigned.
    pub fn node_graph_add_node(
        &mut self,
        type_name: &str,
        x: f32,
        y: f32,
    ) -> Option<node_graph::GraphNodeId> {
        let pos = egui::pos2(x, y);
        let node_id = match type_name {
            "UsdRead" => self.nodes.node_graph_state.add_usd_read(pos),
            "HdriEnvironment" => self.nodes.node_graph_state.add_hdri_environment(pos),
            "IvarRender" => self.nodes.node_graph_state.add_ivar_render(pos),
            "Cube" => self
                .nodes
                .node_graph_state
                .add_primitive(bif_core::PrimitiveKind::Cube, pos),
            "Sphere" => self
                .nodes
                .node_graph_state
                .add_primitive(bif_core::PrimitiveKind::Sphere, pos),
            "Camera" => self
                .nodes
                .node_graph_state
                .add_primitive(bif_core::PrimitiveKind::Camera, pos),
            "ScatterPoints" => self.nodes.node_graph_state.add_scatter_points(pos),
            "PointInstancer" => self.nodes.node_graph_state.add_point_instancer(pos),
            "Xform" => self.nodes.node_graph_state.add_xform(pos),
            "UsdPrim" => self.nodes.node_graph_state.add_usd_prim(pos),
            "UsdExport" => self.nodes.node_graph_state.add_usd_export(pos),
            "Cache" => self.nodes.node_graph_state.add_cache(pos),
            "GraftBranches" => return None,
            _ => return None,
        };

        let graph_id = node_graph::GraphNodeId::from(node_id);
        self.nodes.node_graph_state.selected_node = Some(graph_id);
        self.nodes.node_graph_state.dirty_nodes.insert(graph_id);
        self.nodes.scene_graph_dirty = true;
        self.project.mark_dirty();
        Some(graph_id)
    }

    pub fn node_graph_delete_node(&mut self, node_id: node_graph::GraphNodeId) -> bool {
        let snarl_id: egui_snarl::NodeId = node_id.into();
        let node_exists = self
            .nodes
            .node_graph_state
            .snarl
            .node_ids()
            .any(|(id, _)| id == snarl_id);
        if !node_exists {
            return false;
        }

        if self.nodes.node_graph_state.selected_node == Some(node_id) {
            self.nodes.node_graph_state.selected_node = None;
        }
        if self.nodes.node_graph_state.display_node == Some(node_id) {
            self.nodes.node_graph_state.display_node = None;
        }
        self.nodes.node_graph_state.dirty_nodes.remove(&node_id);
        self.nodes.node_graph_state.snarl.remove_node(snarl_id);
        self.handle_node_graph_event(node_graph::NodeGraphEvent::DeleteNode(node_id));
        self.nodes.scene_graph_dirty = true;
        self.project.mark_dirty();
        true
    }

    pub fn node_graph_select_node(&mut self, node_id: node_graph::GraphNodeId) -> Option<String> {
        let snarl_id: egui_snarl::NodeId = node_id.into();
        let node_exists = self
            .nodes
            .node_graph_state
            .snarl
            .node_ids()
            .any(|(id, _)| id == snarl_id);
        if !node_exists {
            return None;
        }

        self.handle_node_graph_event(node_graph::NodeGraphEvent::SelectNode(node_id));
        let prim_path = match &self.nodes.node_graph_state.snarl[snarl_id] {
            node_graph::SceneNode::Primitive { prim_path, .. }
            | node_graph::SceneNode::PointInstancer { prim_path, .. }
            | node_graph::SceneNode::UsdPrim { prim_path, .. } => Some(prim_path.clone()),
            node_graph::SceneNode::GraftBranches {
                destination_path, ..
            } => Some(destination_path.clone()),
            _ => None,
        };
        if let Some(path) = prim_path.as_deref() {
            self.handle_prim_selected(path.to_string());
        }
        prim_path
    }

    /// Create a new renderer from caller-provided wgpu primitives.
    ///
    /// `bif_viewer` (winit) and `bif_qt` (Qt / raw HWND) each build their own
    /// `wgpu::Instance` + `Surface` from their native window handle, request
    /// an adapter/device/queue, and pass them here. This crate no longer
    /// depends on any specific windowing system — the caller owns that layer.
    ///
    /// `size` is the surface size in physical pixels. `scale_factor` is used
    /// for egui overlay tessellation; update both via [`Renderer::resize`].
    ///
    /// egui is **not** initialized by this call. Callers that want egui (i.e.
    /// `bif_viewer`) build `egui_winit::State` themselves from their winit
    /// window and install it via [`Renderer::attach_egui`].
    pub fn new(
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        size: (u32, u32),
        scale_factor: f32,
    ) -> Result<Self> {
        // Start with blank scene - no default mesh
        log::info!("Initializing blank scene (no default geometry)");

        // Create empty mesh data
        let mesh_data = MeshData::default();

        // Create camera at default position looking at origin
        let aspect = size.0 as f32 / size.1 as f32;
        let mut camera = Camera::new(
            Vec3::new(0.0, 10.0, 50.0), // Default position
            Vec3::new(0.0, 0.0, 0.0),   // Look at origin
            aspect,
        );

        // Set reasonable default near/far planes
        camera.near = 0.1;
        camera.far = 1000.0;

        log::info!(
            "Camera positioned at {:?}, looking at {:?}",
            camera.position,
            camera.target
        );
        log::info!("Camera near={:.2}, far={:.2}", camera.near, camera.far);

        // Create camera uniform buffer with correct initial values
        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&camera);

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group layout for camera
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // Create bind group for camera
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Create material uniform buffer
        let material_uniform = MaterialUniform::new();
        let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Material Buffer"),
            contents: bytemuck::cast_slice(&[material_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let gpu_textures = texture_loader::create_default_gpu_textures(&device, &queue);

        let material_table = vec![MaterialGpu::from_material(
            &bif_core::Material::default(),
            &gpu_textures,
        )];
        let material_table_len = material_table.len() as u32;
        let material_table_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Material Table Buffer"),
            contents: bytemuck::cast_slice(&material_table),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        // Create triangle material buffer (dummy for blank scene)
        let triangle_material_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Triangle Material Buffer"),
                contents: bytemuck::cast_slice(&[0xFFFFFFFFu32]), // Sentinel: use instance material
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
        let has_triangle_materials = false;

        // Create bind group layout for material (includes triangle material buffer)
        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Material Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // Create bind group for material
        let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Bind Group"),
            layout: &material_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: material_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: material_table_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: triangle_material_buffer.as_entire_binding(),
                },
            ],
        });

        let texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Viewport Texture Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear, // Trilinear filtering
            lod_min_clamp: 0.0,
            lod_max_clamp: 16.0,  // Allow full mip range
            anisotropy_clamp: 16, // Enable anisotropic filtering
            ..Default::default()
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: NonZeroU32::new(MAX_VIEWPORT_TEXTURES as u32),
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let texture_view_refs: Vec<&wgpu::TextureView> = gpu_textures.views.iter().collect();
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&texture_view_refs),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&texture_sampler),
                },
            ],
        });

        // Create environment manager (IBL + skybox)
        let environment =
            EnvironmentManager::new(&device, &queue, &camera_bind_group_layout, config.format);

        // Create lights manager (bind group 4)
        let lights = LightsManager::new(&device);

        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Basic Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/basic.wgsl").into()),
        });

        let outline_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Outline Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/outline.wgsl").into()),
        });

        // Create render pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[
                &camera_bind_group_layout,
                &material_bind_group_layout,
                &texture_bind_group_layout,
                environment.bind_group_layout(),
                lights.bind_group_layout(),
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc(), InstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // USD rightHanded = CCW front faces
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let outline_params_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Outline Params Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let outline_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Outline Pipeline Layout"),
                bind_group_layouts: &[&camera_bind_group_layout, &outline_params_bind_group_layout],
                push_constant_ranges: &[],
            });

        // Outline pipeline for selection highlight — normal-expanded back-face silhouette.
        // Uses outline.wgsl: expands vertices along normals in clip space, renders back
        // faces only so only the protruding rim (silhouette) passes depth test.
        let wireframe_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Selection Outline Pipeline"),
            layout: Some(&outline_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &outline_shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc(), InstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &outline_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Front), // Back faces only — rim protrudes beyond original
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                // LessEqual: expanded back faces behind the original fail (hidden inside mesh),
                // only the protruding rim passes, giving a clean silhouette outline.
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Wireframe camera buffer — same layout as camera_buffer, shading_mode=2 always set.
        // Needed because queue.write_buffer calls all execute before any render pass, so
        // we can't temporarily set shading_mode=2 on the shared camera buffer mid-frame.
        let wf_cam_uniform = CameraUniform {
            shading_mode: 2,
            ..CameraUniform::default()
        };
        let wireframe_cam_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Wireframe Camera Buffer"),
            contents: bytemuck::cast_slice(&[wf_cam_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let wireframe_cam_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Wireframe Camera Bind Group"),
            layout: &wireframe_pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wireframe_cam_buffer.as_entire_binding(),
            }],
        });
        let outline_params = OutlineParamsUniform {
            color: DisplaySettings::default().outline_color,
            width_ndc: DisplaySettings::default().outline_width,
            _padding: [0.0; 3],
        };
        let outline_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Outline Params Buffer"),
            contents: bytemuck::cast_slice(&[outline_params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let outline_params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Outline Params Bind Group"),
            layout: &wireframe_pipeline.get_bind_group_layout(1),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: outline_params_buffer.as_entire_binding(),
            }],
        });

        // Create empty vertex and index buffers (will be populated when USD loads)
        // Note: wgpu requires non-zero buffer sizes, so we use a dummy vertex/index
        let dummy_vertex = Vertex {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            color: [1.0, 1.0, 1.0],
            uv: [0.0, 0.0],
            material_id: 0xFFFFFFFF,
        };
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer (Empty)"),
            contents: bytemuck::cast_slice(&[dummy_vertex]),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer (Empty)"),
            contents: bytemuck::cast_slice(&[0u32]),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Create depth texture
        let (depth_texture, depth_view) = ivar_renderer::create_depth_texture(&device, size);

        // No instances by default - empty scene
        let dummy_instance = InstanceData {
            model_matrix: Mat4::IDENTITY.to_cols_array_2d(),
            material_id: 0,
            tri_mat_offset: 0,
        };

        // Preallocate instance buffer for up to MAX_INSTANCES (100K)
        // Uses COPY_DST for dynamic per-frame updates during frustum culling
        const MAX_INSTANCES: u32 = 100_000;
        let instance_buffer_size = (MAX_INSTANCES as usize) * std::mem::size_of::<InstanceData>();
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer (Dynamic)"),
            size: instance_buffer_size as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Write a single dummy instance so the buffer isn't empty
        queue.write_buffer(&instance_buffer, 0, bytemuck::cast_slice(&[dummy_instance]));

        log::info!(
            "Created dynamic instance buffer (capacity: {} instances)",
            MAX_INSTANCES
        );

        // Create culling manager
        let culling = CullingManager::new(&device, &camera, MAX_INSTANCES as usize);

        log::info!("Created culling manager");

        // Create gnomon renderer
        let gnomon = GnomonRenderer::new(&device, config.format);
        log::info!("Gnomon initialized");

        // Create ground grid renderer
        let grid = GridRenderer::new(&device, config.format, &camera_bind_group_layout);
        log::info!("Grid initialized");

        // Create selected-prim transform gizmo renderer
        let transform_gizmo =
            TransformGizmoRenderer::new(&device, config.format, &camera_bind_group_layout);
        log::info!("Transform gizmo initialized");

        // Create point preview renderer
        let point_preview = point_preview::PointPreviewRenderer::new(
            &device,
            config.format,
            &camera_bind_group_layout,
        );

        // Create curve/points preview renderer
        let curve_preview = curve_preview::CurvePreviewRenderer::new(
            &device,
            config.format,
            &camera_bind_group_layout,
        );
        log::info!("Point + curve preview initialized");

        // Calculate stats - empty scene has 0 triangles
        let num_triangles = 0;

        // Create Ivar resources for CPU path tracer display
        let (ivar_texture, ivar_texture_view) = ivar_renderer::create_ivar_texture(&device, size);

        let ivar_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Ivar Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let (ivar_pipeline, ivar_bind_group, ivar_bind_group_layout) =
            ivar_renderer::create_ivar_pipeline(
                &device,
                config.format,
                &ivar_texture_view,
                &ivar_sampler,
            );

        log::info!("Ivar resources initialized");

        let mipmap_generator = texture_loader::MipmapGenerator::new(&device);

        Ok(Self {
            scale_factor,
            dialog_focus_hook: None,
            gpu: GpuContext {
                surface,
                device,
                queue,
                config,
            },
            size,
            pipeline,
            wireframe_pipeline,
            wireframe_cam_buffer,
            wireframe_cam_bind_group,
            outline_params_buffer,
            outline_params_bind_group,
            vertex_buffer,
            index_buffer,
            num_indices: 0, // Empty scene - no indices
            instance_buffer,
            num_instances: 0, // Empty scene - no instances
            cam: CameraState {
                camera,
                camera_uniform,
                camera_buffer,
                camera_bind_group,
                viewport_camera_source: CameraSource::Viewport,
                camera_locked: false,
                selected_usd_camera: None,
            },
            materials: GpuMaterialState {
                uniform: material_uniform,
                buffer: material_buffer,
                bind_group_layout: material_bind_group_layout,
                bind_group: material_bind_group,
                table_buffer: material_table_buffer,
                table_len: material_table_len,
                triangle_buffer: triangle_material_buffer,
                has_triangle_materials,
            },
            textures: GpuTextureState {
                gpu_textures,
                sampler: texture_sampler,
                bind_group_layout: texture_bind_group_layout,
                bind_group: texture_bind_group,
            },
            mesh_bounds_min: mesh_data.bounds_min,
            mesh_bounds_max: mesh_data.bounds_max,
            depth_texture,
            depth_view,
            gnomon,
            grid,
            transform_gizmo,
            fps: 0.0,
            frame_count: 0,
            fps_update_timer: 0.0,
            num_triangles,
            ui_layout: UiLayout::default(),
            ivar: IvarContext {
                ivar_state: IvarState::default(),
                ivar_texture,
                ivar_texture_view,
                ivar_sampler,
                ivar_bind_group,
                ivar_bind_group_layout,
                ivar_pipeline,
                ivar_materials: None,
                ivar_texture_cache: None,
            },
            scene: SceneManager {
                mesh_data,
                ..SceneManager::new()
            },
            last_action_stack: Vec::new(),
            redo_action_stack: Vec::new(),
            multi_draw: MultiDrawState::new(),
            culling,
            selection: SelectionManager::new(),
            layer_stack_panel: crate::layer_stack_panel::LayerStackPanel::new(),
            nodes: NodeGraphContext {
                node_graph_state: NodeGraphState::new(),
                node_outputs: std::collections::HashMap::new(),
                next_cloud_id: 0,
                node_scatter_surface_map: std::collections::HashMap::new(),
                instancer_results: std::collections::BTreeMap::new(),
                cached_scene_graph: scene_browser::CachedSceneGraph::default(),
                scene_graph_dirty: true,
                primitive_name_counters: std::collections::HashMap::new(),
                materials_dirty: true,
                xform_property_changed: None,
                node_prim_counts: std::collections::HashMap::new(),
            },
            timeline_state: TimelineState::default(),
            environment,
            lights,
            pick_scene: None,
            point_preview,
            point_preview_last_vp: (0.0, 0.0),
            curve_preview,
            point_preview_params_dirty: true,
            show_grid: true,
            apply_axis_correction: false,
            apply_unit_scaling: false,
            event_bus: EventBus::default(),
            project: persistence::ProjectState::default(),
            recent_files: persistence::load_recent_files(),
            async_channels: AsyncChannels::default(),
            mipmap_generator,
            display_settings: DisplaySettings::default(),
        })
    }

    /// Check if camera controls are locked (USD camera active).
    pub fn is_camera_locked(&self) -> bool {
        self.cam.camera_locked
    }

    /// Whether the viewer needs another frame (animation, progressive render, gizmo drag).
    pub fn needs_redraw(&self) -> bool {
        self.timeline_state.is_playing
            || self.ivar.ivar_state.batch_status.is_rendering()
            || self.ivar.ivar_state.is_pass_in_flight()
            || self.ivar.ivar_state.needs_more_passes()
            || self.selection.gizmo_state.is_dragging
    }

    /// Access to the procedural-prim cache the scene browser uses. Lets
    /// `bif_qt` build a `CompositeProvider` (USD + procedural + synthetic
    /// `/BIF/`) without exposing the private `nodes` field. Mirrors how
    /// `scene.usd_stage` is the other half of the composite source.
    pub fn cached_scene_graph(&self) -> &scene_browser::CachedSceneGraph {
        &self.nodes.cached_scene_graph
    }

    /// Install a hook invoked around native-dialog presentations. The hook
    /// is called with `false` before the dialog opens and `true` after it
    /// closes — giving the caller a chance to hide/show its window to work
    /// around OS z-order issues (Windows).
    ///
    /// `bif_viewer` installs `Box::new(move |v| window.set_visible(v))`;
    /// `bif_qt` leaves this unset.
    pub fn set_dialog_focus_hook<F>(&mut self, hook: F)
    where
        F: Fn(bool) + 'static,
    {
        self.dialog_focus_hook = Some(Box::new(hook));
    }

    /// Current display scale factor (device-independent pixels per point).
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub(crate) fn effective_px(&self, logical_px: f32) -> f32 {
        logical_px * self.scale_factor.max(0.01)
    }

    /// Block until all submitted GPU work completes. Call before dropping
    /// the Renderer to avoid `OBJECT_DELETED_WHILE_STILL_IN_USE` errors
    /// on D3D12/Vulkan.
    ///
    /// If the device has already been lost (cumulative VRAM exhaustion on
    /// production scenes), `Device::poll` panics fatally via wgpu's
    /// `handle_error_fatal!`. Short-circuit + catch the panic so window
    /// close doesn't abort the process.
    pub fn wait_for_gpu(&self) {
        if !crate::texture_loader::gpu_is_healthy() {
            return;
        }
        let device = &self.gpu.device;
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            device.poll(wgpu::Maintain::Wait);
        }));
    }

    /// Reset the renderer to "no scene loaded" state. Drains GPU,
    /// replaces `SceneManager` with a fresh one, clears all node-graph
    /// caches (procedural prim cache, instancer results, prim counts,
    /// node↔proto / node↔cloud maps), and rebuilds the pick BVH.
    ///
    /// Called by `bif_qt` from both `close_stage` (Ctrl+W) and the
    /// implicit pre-load reset on second-open. Single source of truth
    /// for "evict the previous stage entirely" so no half-state (cached
    /// procedural prims, stale dirty flags) survives the swap.
    pub fn reset_scene_state(&mut self) {
        self.wait_for_gpu();
        self.scene = SceneManager::new();
        self.last_action_stack.clear();
        self.redo_action_stack.clear();
        self.nodes.cached_scene_graph = scene_browser::CachedSceneGraph::default();
        self.nodes.node_outputs.clear();
        self.nodes.instancer_results.clear();
        self.nodes.node_prim_counts.clear();
        self.nodes.scene_graph_dirty = true;
        self.nodes.materials_dirty = true;
        self.nodes.primitive_name_counters.clear();
        // Stale selection (prim path, instance index, gizmo, tree
        // expansion) from the prior stage would resolve to wrong rows
        // on the fresh scene — clear before rebuilding pick BVH.
        self.selection.clear();
        self.rebuild_pick_scene();
    }

    /// Drive selection from a USD prim path (typically a tree-view click).
    ///
    /// Public wrapper over `handle_prim_selected` so UI layers (bif_qt,
    /// bif_viewer) can sync viewport gizmo + outline highlight when the
    /// scene browser's selection changes. Resolves the prim path back to
    /// an instance index when possible (synthetic `/BIF/` paths handled).
    pub fn select_prim_by_path(&mut self, prim_path: &str) {
        self.handle_prim_selected(prim_path.to_string());
    }

    /// Handle window resize. `scale_factor` is the caller's current display
    /// scale (device-independent pixels per point); pass whatever your
    /// windowing layer reports (`winit::Window::scale_factor()` in
    /// `bif_viewer`; `QScreen::devicePixelRatio()` in `bif_qt`).
    pub fn resize(&mut self, new_size: (u32, u32), scale_factor: f32) {
        if new_size.0 > 0 && new_size.1 > 0 {
            self.size = new_size;
            self.scale_factor = scale_factor;
            self.gpu.config.width = new_size.0;
            self.gpu.config.height = new_size.1;
            self.gpu
                .surface
                .configure(&self.gpu.device, &self.gpu.config);

            // Recreate depth texture with new size
            let (depth_texture, depth_view) =
                ivar_renderer::create_depth_texture(&self.gpu.device, new_size);
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;

            // Recreate Ivar texture with new size
            let (ivar_texture, ivar_texture_view) =
                ivar_renderer::create_ivar_texture(&self.gpu.device, new_size);
            self.ivar.ivar_texture = ivar_texture;
            self.ivar.ivar_texture_view = ivar_texture_view;

            // Recreate Ivar bind group with new texture view (reuse existing layout)
            self.ivar.ivar_bind_group = ivar_renderer::create_ivar_bind_group(
                &self.gpu.device,
                &self.ivar.ivar_bind_group_layout,
                &self.ivar.ivar_texture_view,
                &self.ivar.ivar_sampler,
            );

            // Full Ivar state reset on resize.
            // image_buffer = None because target dims are unknown until next
            // frame's viewport_rect(). One frame of black during resize is
            // acceptable (fundamentally different from orbit).
            self.ivar.ivar_state.reset_on_resize();

            // Update camera aspect ratio from viewport (excludes UI panels)
            let (_, _, vp_w, vp_h) = self.viewport_rect();
            let aspect = vp_w / vp_h;
            self.cam.camera.set_aspect(aspect);
            self.update_camera();
        }
    }

    /// Returns the viewport rect (x, y, w, h) in pixels after subtracting all UI panels.
    pub fn viewport_rect(&self) -> (f32, f32, f32, f32) {
        let x = self.ui_layout.left_panel_width;
        let y = self.ui_layout.top_panel_height;
        let w = (self.size.0 as f32
            - self.ui_layout.left_panel_width
            - self.ui_layout.right_panel_width)
            .max(1.0);
        let h = (self.size.1 as f32
            - self.ui_layout.top_panel_height
            - self.ui_layout.bottom_panel_height)
            .max(1.0);
        (x, y, w, h)
    }

    /// Returns bounds-safe u32 scissor rect (x, y, w, h) for `set_scissor_rect`.
    fn viewport_scissor(&self) -> (u32, u32, u32, u32) {
        let (vp_x, vp_y, vp_w, vp_h) = self.viewport_rect();
        let sx = (vp_x.round() as u32).min(self.size.0.saturating_sub(1));
        let sy = (vp_y.round() as u32).min(self.size.1.saturating_sub(1));
        let sw = (vp_w.round() as u32).min(self.size.0 - sx);
        let sh = (vp_h.round() as u32).min(self.size.1 - sy);
        (sx, sy, sw, sh)
    }

    /// Update camera uniform buffer (call after modifying camera)
    pub fn update_camera(&mut self) {
        self.cam.camera_uniform.update_view_proj(&self.cam.camera);
        // Sync selection + shading mode into uniform
        self.cam.camera_uniform.selected_instance_id = self.selection.gpu_highlight_id();
        self.cam.camera_uniform.shading_mode = self.display_settings.shading_mode.as_u32();
        self.gpu.queue.write_buffer(
            &self.cam.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.cam.camera_uniform]),
        );

        // Keep wireframe camera buffer in sync (same view/proj, shading_mode=2 always)
        let mut wf_uniform = self.cam.camera_uniform;
        wf_uniform.shading_mode = 2;
        self.gpu.queue.write_buffer(
            &self.wireframe_cam_buffer,
            0,
            bytemuck::cast_slice(&[wf_uniform]),
        );
        self.write_outline_params();

        // Update gnomon uniform with camera rotation
        self.gnomon
            .update_from_camera(&self.gpu.queue, &self.cam.camera);
    }

    fn write_outline_params(&mut self) {
        let outline = OutlineParamsUniform {
            color: self.display_settings.outline_color,
            width_ndc: self.display_settings.outline_width * self.scale_factor.max(0.01),
            _padding: [0.0; 3],
        };
        self.gpu.queue.write_buffer(
            &self.outline_params_buffer,
            0,
            bytemuck::cast_slice(&[outline]),
        );
    }

    /// Update environment parameters without regenerating maps.
    pub fn update_environment_params(
        &mut self,
        intensity: f32,
        rotation: f32,
        show_background: bool,
    ) {
        self.environment
            .update_params(&self.gpu.queue, intensity, rotation, show_background);
        // Sync to Ivar state for live CPU path tracer updates
        self.ivar.ivar_state.hdri_rotation = rotation;
        self.ivar.ivar_state.hdri_intensity = intensity;
        self.ivar.ivar_state.hdri_show_background = show_background;
    }

    /// Update lights uniform buffer from scene lights.
    pub fn update_lights(&mut self, lights: &[bif_core::Light]) {
        self.lights.update(&self.gpu.queue, lights);
        if !lights.is_empty() {
            log::info!("Updated {} lights in viewport", lights.len());
        }
    }

    /// Perform frustum culling and LOD selection, updating visible instance buffers.
    ///
    /// This method:
    /// 1. Filters instances by camera frustum visibility
    /// 2. Sorts visible instances by distance (near to far)
    /// 3. Fills polygon budget with full mesh, rest become box LOD
    /// 4. Uploads only visible instances to GPU each frame
    ///
    /// Uses `lod_max_polys` as the polygon budget - nearest instances get full
    /// mesh until budget is exhausted, then remaining use box proxy.
    pub fn update_visible_instances(&mut self) {
        let lod_enabled = self.display_settings.lod_enabled;
        let purpose_mode = self.display_settings.purpose_mode;
        self.culling.update_visible_instances(
            &self.gpu.queue,
            &self.instance_buffer,
            &self.cam.camera,
            &self.scene.instances.current,
            &self.scene.instances.material_ids,
            lod_enabled,
            &self.scene.instances.purposes,
            purpose_mode,
        );
    }

    /// Frame the camera on the loaded mesh
    pub fn frame_mesh(&mut self) {
        let mesh_center = (self.mesh_bounds_min + self.mesh_bounds_max) * 0.5;
        let mesh_size = (self.mesh_bounds_max - self.mesh_bounds_min).length();
        let camera_distance = mesh_size * 1.5;

        // Position camera looking at mesh center from current yaw/pitch
        self.cam.camera.target = mesh_center;
        self.cam.camera.distance = camera_distance;
        self.cam.camera.update_position_from_angles();

        self.update_camera();
        log::info!(
            "Framed mesh at center {:?}, distance {:.2}",
            mesh_center,
            camera_distance
        );
    }

    /// Sync viewport camera to a USD camera at the current timeline frame
    pub fn sync_viewport_to_usd_camera(&mut self, camera_path: &str) {
        let Some(ref stage_mtx) = self.scene.usd_stage else {
            log::warn!("No USD stage loaded");
            return;
        };

        let time = self.timeline_state.current_frame;

        // Query stage data under lock, then drop guard before mutating self
        let (xform_result, props_result) = {
            let stage = stage_mtx.lock().expect("UsdStage mutex poisoned");
            (
                stage.get_camera_xform_at_time(camera_path, time),
                stage.get_camera_properties(camera_path, time),
            )
        };

        match xform_result {
            Ok(xform) => {
                // C++ bridge flat-copies USD row-major data; from_cols_array()
                // implicitly transposes to glam column-vector convention:
                //   col(0) = X axis, col(1) = Y (up), col(2) = Z, col(3) = translation
                let position = xform.col(3).truncate();
                let forward = -xform.col(2).truncate().normalize(); // camera looks -Z
                let up = xform.col(1).truncate().normalize();
                let target = position + forward * 10.0;

                log::info!(
                    "USD camera '{}' at frame {}: pos={:?}, target={:?}, up={:?}",
                    camera_path,
                    time,
                    position,
                    target,
                    up
                );

                self.cam.camera.position = position;
                self.cam.camera.target = target;
                self.cam.camera.up = up;
                self.cam.camera.distance = 10.0;

                // Sync FOV/near/far from USD camera properties
                if let Ok(props) = props_result {
                    self.cam.camera.fov_y = props.fov_y();
                    self.cam.camera.near = props.clip_near.max(0.001);
                    self.cam.camera.far = props.clip_far.max(props.clip_near + 1.0);
                }

                // Yaw/pitch must match Camera::new / update_position_from_angles convention
                let dir = (position - target).normalize();
                self.cam.camera.yaw = dir.z.atan2(dir.x);
                self.cam.camera.pitch = dir.y.asin();

                self.update_camera();
            }
            Err(e) => {
                log::error!("Failed to get USD camera transform: {:?}", e);
            }
        }
    }

    /// Look through a USD camera at the current timeline frame.
    pub fn apply_usd_camera(&mut self, camera_path: &str) {
        self.cam.viewport_camera_source = CameraSource::UsdCamera(camera_path.to_string());
        self.cam.selected_usd_camera = Some(camera_path.to_string());
        self.cam.camera_locked = true;
        self.sync_viewport_to_usd_camera(camera_path);
    }

    /// Snap viewport to an orthographic preset view.
    pub fn apply_ortho_view(&mut self, preset: bif_math::OrthoPreset) {
        let dir = preset.direction();
        let up = preset.up();
        let (_, _, vp_w, vp_h) = self.viewport_rect();
        self.cam.camera.set_aspect(vp_w / vp_h);
        let dist = (self.cam.camera.target - self.cam.camera.position)
            .length()
            .max(5.0);
        self.cam.camera.position = self.cam.camera.target + dir * dist;
        self.cam.camera.up = up;
        let normalized = (self.cam.camera.position - self.cam.camera.target).normalize();
        self.cam.camera.yaw = normalized.z.atan2(normalized.x);
        self.cam.camera.pitch = normalized.y.asin();
        self.cam.camera.distance = dist;
        self.cam.camera.projection = bif_math::ProjectionMode::Orthographic {
            ortho_size: dist * 0.5,
        };
        self.cam.viewport_camera_source = CameraSource::OrthoView(preset);
        self.cam.selected_usd_camera = None;
        self.cam.camera_locked = false;
        self.update_camera();
    }

    /// Reset to free perspective orbit (clears any USD/ortho camera lock).
    pub fn apply_free_fly(&mut self) {
        self.cam.camera.projection = bif_math::ProjectionMode::Perspective;
        self.cam.camera_locked = false;
        self.cam.viewport_camera_source = CameraSource::Viewport;
        self.cam.selected_usd_camera = None;
        self.update_camera();
    }

    /// AOV channel currently displayed by the Ivar preview overlay.
    pub fn preview_aov(&self) -> ivar_state::AovChannel {
        self.ivar.ivar_state.preview_aov
    }

    /// Change the AOV channel shown by the Ivar preview overlay. Picked up
    /// on the next 16ms viewport tick via `upload_ivar_pixels`.
    pub fn set_preview_aov(&mut self, aov: ivar_state::AovChannel) {
        self.ivar.ivar_state.preview_aov = aov;
    }

    /// Active render mode (Vulkan rasterizer vs Ivar CPU path tracer overlay).
    pub fn render_mode(&self) -> ivar_state::RenderMode {
        self.ivar.ivar_state.mode
    }

    /// Switch the render mode. Vulkan flips back to the wgpu rasterizer
    /// immediately; Ivar shows whatever the path tracer has accumulated so far
    /// (use `trigger_ivar_render` to start a fresh pass).
    pub fn set_render_mode(&mut self, mode: ivar_state::RenderMode) {
        self.ivar.ivar_state.mode = mode;
    }

    /// Whether the Ivar path tracer's blue sky gradient background is enabled.
    /// When off, escaped rays return solid black (or the configured background)
    /// instead of the white→blue gradient at `bif_renderer::sky_gradient`.
    pub fn sky_gradient_enabled(&self) -> bool {
        self.ivar.ivar_state.use_sky_gradient
    }

    /// Toggle the Ivar path tracer's blue sky gradient background. Takes effect
    /// on the next bucket dispatched by `trigger_ivar_render`. Set
    /// `IvarState::use_sky_gradient`, which is read into `RenderConfig` at
    /// pass start.
    pub fn set_sky_gradient_enabled(&mut self, enabled: bool) {
        self.ivar.ivar_state.use_sky_gradient = enabled;
    }

    /// Sync viewport camera to a scene camera (from Camera primitive).
    ///
    /// Reads the instance transform and applies the camera's FOV.
    pub fn sync_viewport_to_scene_camera(&mut self, cam_idx: usize) {
        let cam = match self.scene.scene_cameras.get(cam_idx) {
            Some(c) => c.clone(),
            None => {
                log::warn!("Scene camera index {} out of range", cam_idx);
                return;
            }
        };

        let inst_idx = cam.instance_index;
        if inst_idx >= self.scene.instances.current.len() {
            log::warn!("Scene camera instance {} out of range", inst_idx);
            return;
        }

        let mat = self.scene.instances.current[inst_idx];
        let transform = bif_core::Transform::from_matrix(mat);

        // Camera looks down -Z in its local space
        let forward = transform.rotation * -Vec3::Z;
        let up = transform.rotation * Vec3::Y;

        self.cam.camera.position = transform.translation;
        self.cam.camera.target = transform.translation + forward * 10.0;
        self.cam.camera.up = up;
        self.cam.camera.distance = 10.0;
        self.cam.camera.fov_y = cam.fov_y;

        // Recalculate yaw/pitch
        let dir = (self.cam.camera.position - self.cam.camera.target).normalize();
        self.cam.camera.yaw = dir.x.atan2(dir.z);
        self.cam.camera.pitch = (-dir.y).asin();

        self.update_camera();

        log::info!(
            "Synced to scene camera '{}' (instance {})",
            cam.name,
            inst_idx
        );
    }

    /// Rebuild the Embree pick scene from current mesh + instance data.
    ///
    /// Call after scene load or when geometry changes. Uses the first prototype's
    /// triangles (single-draw) or combined mesh triangles.
    pub fn rebuild_pick_scene(&mut self) {
        if self.scene.mesh_data.indices.is_empty() || self.scene.instances.current.is_empty() {
            self.pick_scene = None;
            return;
        }

        // Skip pick scene for huge meshes — Embree BVH allocation would OOM
        const MAX_PICK_TRIS: usize = 50_000_000;
        let tri_count = self.scene.mesh_data.indices.len() / 3;
        if tri_count > MAX_PICK_TRIS {
            log::warn!(
                "Pick scene skipped: {} M tris exceeds {} M limit (click-to-select disabled)",
                tri_count / 1_000_000,
                MAX_PICK_TRIS / 1_000_000
            );
            self.pick_scene = None;
            return;
        }

        // Use indexed path — pass shared positions + indices directly
        let positions = self.scene.mesh_data.extract_positions();

        match bif_renderer::EmbreePickScene::from_indexed(
            &positions,
            &self.scene.mesh_data.indices,
            &self.scene.instances.current,
        ) {
            Ok(scene) => {
                log::info!(
                    "Pick scene rebuilt (indexed): {} tris, {} shared verts, {} instances",
                    self.scene.mesh_data.indices.len() / 3,
                    positions.len(),
                    self.scene.instances.current.len()
                );
                self.pick_scene = Some(scene);
            }
            Err(e) => {
                log::warn!("Failed to build pick scene: {}", e);
                self.pick_scene = None;
            }
        }
    }

    /// Pick an instance at the given screen coordinates.
    ///
    /// Converts screen coords to a world-space ray via inverse view-projection,
    /// then casts into the Embree pick scene.
    ///
    /// Handle a viewport click: pick instance, update selection, emit PrimSelected for tree sync.
    ///
    /// Combines pick + selection state update + event emission so the tree and property
    /// inspector stay in sync with the viewport. Sets selected_instance_index immediately
    /// (outline appears same frame); PrimSelected is processed next frame for full sync.
    pub fn select_at_screen(&mut self, screen_x: f32, screen_y: f32) {
        // Ignore clicks on UI panels — only act when cursor is inside the 3D viewport.
        let (vp_x, vp_y, vp_w, vp_h) = self.viewport_rect();
        if screen_x < vp_x || screen_x > vp_x + vp_w || screen_y < vp_y || screen_y > vp_y + vp_h {
            return;
        }

        let picked = self.pick_instance_at(screen_x, screen_y);
        self.selection.selected_instance_index = picked;
        self.selection.gizmo_state.reset();
        if let Some(idx) = picked {
            if let Some(prim_path) = self.scene.instances.prim_paths.get(idx) {
                // Normalize synthetic /BIF/{usd_path}/{idx} back to the real USD path
                // so the tree (which shows USD paths) can highlight the matching row.
                let display_path = denormalize_synthetic_path(prim_path);
                self.event_bus
                    .emit(app_event::AppEvent::PrimSelected(display_path));
            }
        } else {
            // Click on empty space — deselect everything
            self.selection.selected_prim_path = None;
            self.selection.selected_prim_properties = None;
            self.selection.scene_browser_state.clear_selection();
        }
    }

    /// Returns the instance index if hit, or None if click hit empty space.
    pub fn pick_instance_at(&self, screen_x: f32, screen_y: f32) -> Option<usize> {
        let pick_scene = self.pick_scene.as_ref()?;

        // Convert screen coords to viewport-relative
        let (vp_x, vp_y, vp_w, vp_h) = self.viewport_rect();
        let vp_rel_x = screen_x - vp_x;
        let vp_rel_y = screen_y - vp_y;

        // Check if click is within viewport
        if vp_rel_x < 0.0 || vp_rel_x > vp_w || vp_rel_y < 0.0 || vp_rel_y > vp_h {
            return None;
        }

        // Convert to NDC (wgpu: Y-down screen → Y-up NDC, depth [0,1])
        let ndc_x = (vp_rel_x / vp_w) * 2.0 - 1.0;
        let ndc_y = 1.0 - (vp_rel_y / vp_h) * 2.0;

        // Unproject near and far points via inverse view-projection
        let inv_vp = self.cam.camera.view_projection_matrix().inverse();

        let near_clip = bif_math::Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let far_clip = bif_math::Vec4::new(ndc_x, ndc_y, 1.0, 1.0);

        let near_world = inv_vp * near_clip;
        let far_world = inv_vp * far_clip;

        // Perspective divide
        if near_world.w.abs() < 1e-8 || far_world.w.abs() < 1e-8 {
            return None;
        }
        let near_pos = Vec3::new(
            near_world.x / near_world.w,
            near_world.y / near_world.w,
            near_world.z / near_world.w,
        );
        let far_pos = Vec3::new(
            far_world.x / far_world.w,
            far_world.y / far_world.w,
            far_world.z / far_world.w,
        );

        let direction = (far_pos - near_pos).normalize();

        pick_scene.pick(near_pos, direction).and_then(|result| {
            // Post-hit visibility filter: the pick scene (Embree BVH) is
            // only rebuilt on full reload_working_scene(). After a visibility
            // toggle, hidden geometry still exists in the BVH at the old
            // instance index. Check against the pre-filtered prim_paths
            // snapshot to skip hits on hidden prims.
            if result.instance_index < self.scene.instances.all_prim_paths.len() {
                let path = &self.scene.instances.all_prim_paths[result.instance_index];
                if !path.is_empty() && self.scene.hidden_prim_paths.contains(path.as_str()) {
                    return None;
                }
            }
            log::info!(
                "Picked instance {} (tri={}, t={:.2}, pos=({:.2},{:.2},{:.2}))",
                result.instance_index,
                result.triangle_index,
                result.t,
                result.hit_point.x,
                result.hit_point.y,
                result.hit_point.z
            );
            Some(result.instance_index)
        })
    }

    /// Apply a transform override from edit_state to the GPU instance buffer.
    ///
    /// Reads the override for `idx` and updates `current_transforms` + GPU.
    pub fn apply_transform_override(&mut self, idx: usize) {
        if let Some(transform) = self.scene.edit_state.transform_overrides.get(&idx) {
            let mat = transform.to_matrix();
            if idx < self.scene.instances.current.len() {
                self.scene.instances.current[idx] = mat;
                self.culling.mark_dirty();
                self.update_visible_instances();
                // Keep Embree pick scene in sync
                if let Some(pick_scene) = &self.pick_scene {
                    pick_scene.update_instance_transform(idx, &mat);
                }
            }
        }
    }

    /// Apply all transform overrides from edit_state to GPU.
    pub fn apply_all_transform_overrides(&mut self) {
        let overrides: Vec<(usize, Mat4)> = self
            .scene
            .edit_state
            .transform_overrides
            .iter()
            .filter(|(idx, _)| **idx < self.scene.instances.current.len())
            .map(|(idx, t)| (*idx, t.to_matrix()))
            .collect();

        for (idx, mat) in &overrides {
            self.scene.instances.current[*idx] = *mat;
        }

        if !overrides.is_empty() {
            self.culling.mark_dirty();
            self.update_visible_instances();
            // Keep Embree pick scene in sync
            if let Some(pick_scene) = &self.pick_scene {
                for (idx, mat) in &overrides {
                    pick_scene.update_instance_transform(*idx, mat);
                }
            }
        }
    }

    /// Push a transform command onto the undo stack and apply it.
    pub fn push_transform_command(
        &mut self,
        instance_index: usize,
        old_transform: bif_core::Transform,
        new_transform: bif_core::Transform,
    ) {
        let cmd = bif_core::TransformCommand {
            instance_index,
            old_transform,
            new_transform,
        };
        self.scene
            .undo_stack
            .push(Box::new(cmd), &mut self.scene.edit_state);
        self.last_action_stack.push(UndoActionKind::Procedural);
        self.redo_action_stack.clear();
        self.apply_transform_override(instance_index);
        self.project.mark_dirty();

        // Invalidate Ivar scene so transform change is reflected
        if self.ivar.ivar_state.mode == ivar_state::RenderMode::Ivar {
            self.invalidate_ivar_scene();
        }
    }

    pub(crate) fn instance_to_opinion_key(
        &self,
        idx: usize,
        slot: bif_core::usd::AttrSlot,
    ) -> Option<bif_core::usd::OpinionKey> {
        let prim_path = self.scene.instances.prim_paths.get(idx)?;
        if prim_path.is_empty() {
            return None;
        }
        Some(bif_core::usd::OpinionKey::new(
            denormalize_synthetic_path(prim_path),
            slot,
        ))
    }

    pub(crate) fn apply_usd_edit(
        &mut self,
        op: bif_core::usd::EditOperation,
    ) -> anyhow::Result<String> {
        let op_for_viewport = op.clone();
        let stage_arc = self
            .scene
            .usd_stage
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no USD stage loaded"))?;
        let mut layer_state = self
            .scene
            .layer_state
            .take()
            .ok_or_else(|| anyhow::anyhow!("no layer state loaded"))?;
        let result = {
            let stage = stage_arc
                .lock()
                .map_err(|e| anyhow::anyhow!("UsdStage mutex poisoned: {e}"))?;
            layer_state.apply_edit_operation(&stage, op)
        };
        self.scene.layer_state = Some(layer_state);
        let desc = result.map_err(|e| anyhow::anyhow!("{e}"))?;
        self.last_action_stack.push(UndoActionKind::Usd);
        self.redo_action_stack.clear();
        self.project.mark_dirty();
        self.reload_after_usd_edit("USD edit", Some(&op_for_viewport));
        Ok(desc)
    }

    pub(crate) fn reload_after_usd_edit(
        &mut self,
        action: &str,
        op: Option<&bif_core::usd::EditOperation>,
    ) {
        use bif_core::usd::EditOperation;
        match op {
            // Visibility toggle — only instance arrays change, geometry/materials/textures
            // stay the same. Use the lightweight instance-level reload to avoid the
            // multi-second stall from full reload_working_scene (texture reload from disk).
            Some(EditOperation::Visibility { .. }) => {
                self.refresh_usd_visibility_state();
                if let Err(e) = self.reload_instance_visibility() {
                    log::warn!("instance visibility reload after {action} failed: {e}");
                }
            }
            // Transform commit — the local fast path in handle_transform_edit already
            // wrote the new matrix to GPU buffers. reload_working_scene() here would be
            // dead work immediately overwritten by the local write. Just refresh
            // visibility state for correctness (future-proofing).
            Some(EditOperation::Transform { .. }) => {
                self.refresh_usd_visibility_state();
            }
            // MaterialParamOverride — material uniform values changed, need material
            // table rebuild, but textures are unchanged. Keep reload_working_scene()
            // (rebuilds material table) but skip materials_dirty (avoids texture reload).
            Some(EditOperation::MaterialParamOverride { key, after, .. }) => {
                if let bif_core::usd::AttrSlot::ShaderInput { name, .. } = &key.attr {
                    self.update_working_material_param(&key.prim_path, name, after);
                }
                self.refresh_usd_visibility_state();
                self.sync_working_materials_from_stage();
                if let Err(e) = self.reload_working_scene() {
                    log::warn!("scene reload after {action} failed: {e}");
                }
            }
            // Default: full rebuild — covers SetShaderId, ReplaceLayerContents,
            // VariantSelect, MaterialAssign, and any future variants that may
            // change geometry, shader structure, or material bindings.
            _ => {
                self.refresh_usd_visibility_state();
                self.sync_working_materials_from_stage();
                self.nodes.materials_dirty = true;
                if let Err(e) = self.reload_working_scene() {
                    log::warn!("scene reload after {action} failed: {e}");
                }
            }
        }
    }

    fn refresh_usd_visibility_state(&mut self) {
        self.scene.hidden_prim_paths.clear();
        let Some(stage_arc) = self.scene.usd_stage.clone() else {
            return;
        };
        let Ok(stage) = stage_arc.lock() else {
            return;
        };
        for inst in self.scene.working_scene.instances() {
            let path = normalize_display_path(inst.prim_path.as_ref());
            if path == "/" {
                continue;
            }
            if stage
                .get_prim_info_by_path(&path)
                .ok()
                .is_some_and(|info| !info.visible)
            {
                self.scene.hidden_prim_paths.insert(path);
            }
        }
    }

    fn sync_working_materials_from_stage(&mut self) {
        let Some(stage_arc) = self.scene.usd_stage.clone() else {
            return;
        };
        let materials = {
            let Ok(stage) = stage_arc.lock() else {
                return;
            };
            stage.materials().unwrap_or_default()
        };
        if materials.is_empty() {
            return;
        }

        let by_path: std::collections::HashMap<&str, &bif_core::usd::cpp_bridge::UsdMaterialData> =
            materials
                .iter()
                .map(|material| (material.path.as_str(), material))
                .collect();

        for material in &mut self.scene.working_scene.materials {
            if let Some(usd_material) = by_path.get(material.name.as_ref()) {
                apply_usd_material_to_material(std::sync::Arc::make_mut(material), usd_material);
            }
        }
        for proto in &mut self.scene.working_scene.prototypes {
            if let Some(material) = std::sync::Arc::make_mut(proto).material.as_mut() {
                if let Some(usd_material) = by_path.get(material.name.as_ref()) {
                    apply_usd_material_to_material(
                        std::sync::Arc::make_mut(material),
                        usd_material,
                    );
                }
            }
        }
    }

    fn update_working_material_param(
        &mut self,
        shader_path: &str,
        input_name: &str,
        value: &bif_core::usd::ShaderValue,
    ) {
        let material_path = shader_path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or(shader_path);
        for material in &mut self.scene.working_scene.materials {
            if material.name.as_ref() == material_path {
                apply_shader_value_to_material(
                    std::sync::Arc::make_mut(material),
                    input_name,
                    value,
                );
            }
        }
        for proto in &mut self.scene.working_scene.prototypes {
            if let Some(material) = std::sync::Arc::make_mut(proto).material.as_mut() {
                if material.name.as_ref() == material_path {
                    apply_shader_value_to_material(
                        std::sync::Arc::make_mut(material),
                        input_name,
                        value,
                    );
                }
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.last_action_stack.is_empty()
            || self.scene.undo_stack.can_undo()
            || self
                .scene
                .layer_state
                .as_ref()
                .is_some_and(bif_core::SceneLayerState::has_usd_undo)
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_action_stack.is_empty()
            || self.scene.undo_stack.can_redo()
            || self
                .scene
                .layer_state
                .as_ref()
                .is_some_and(bif_core::SceneLayerState::has_usd_redo)
    }

    /// Undo the last command. Returns description if successful.
    pub fn undo(&mut self) -> Option<String> {
        let kind = self.last_action_stack.pop().or_else(|| {
            if self.scene.undo_stack.can_undo() {
                Some(UndoActionKind::Procedural)
            } else if self
                .scene
                .layer_state
                .as_ref()
                .is_some_and(bif_core::SceneLayerState::has_usd_undo)
            {
                Some(UndoActionKind::Usd)
            } else {
                None
            }
        })?;
        let desc = match kind {
            UndoActionKind::Procedural => {
                let desc = self
                    .scene
                    .undo_stack
                    .undo(&mut self.scene.edit_state)?
                    .to_string();
                self.apply_all_transform_overrides();
                desc
            }
            UndoActionKind::Usd => {
                let stage_arc = self.scene.usd_stage.clone()?;
                let mut layer_state = self.scene.layer_state.take()?;
                let result = {
                    let stage = stage_arc.lock().ok()?;
                    layer_state.undo_usd_edit(&stage)
                };
                self.scene.layer_state = Some(layer_state);
                let desc = result.ok().flatten()?;
                self.reload_after_usd_edit("USD undo", None);
                desc
            }
        };
        self.redo_action_stack.push(kind);
        self.project.mark_dirty();
        if self.ivar.ivar_state.mode == ivar_state::RenderMode::Ivar {
            self.invalidate_ivar_scene();
        }
        Some(desc)
    }

    /// Redo the next command. Returns description if successful.
    pub fn redo(&mut self) -> Option<String> {
        let kind = self.redo_action_stack.pop().or_else(|| {
            if self.scene.undo_stack.can_redo() {
                Some(UndoActionKind::Procedural)
            } else if self
                .scene
                .layer_state
                .as_ref()
                .is_some_and(bif_core::SceneLayerState::has_usd_redo)
            {
                Some(UndoActionKind::Usd)
            } else {
                None
            }
        })?;
        let desc = match kind {
            UndoActionKind::Procedural => {
                let desc = self
                    .scene
                    .undo_stack
                    .redo(&mut self.scene.edit_state)?
                    .to_string();
                self.apply_all_transform_overrides();
                desc
            }
            UndoActionKind::Usd => {
                let stage_arc = self.scene.usd_stage.clone()?;
                let mut layer_state = self.scene.layer_state.take()?;
                let result = {
                    let stage = stage_arc.lock().ok()?;
                    layer_state.redo_usd_edit(&stage)
                };
                self.scene.layer_state = Some(layer_state);
                let desc = result.ok().flatten()?;
                self.reload_after_usd_edit("USD redo", None);
                desc
            }
        };
        self.last_action_stack.push(kind);
        self.project.mark_dirty();
        if self.ivar.ivar_state.mode == ivar_state::RenderMode::Ivar {
            self.invalidate_ivar_scene();
        }
        Some(desc)
    }

    /// Get the current transform for an instance, preferring edit overrides.
    pub fn get_instance_transform(&self, idx: usize) -> Option<bif_core::Transform> {
        if let Some(t) = self.scene.edit_state.transform_overrides.get(&idx) {
            return Some(*t);
        }
        if idx < self.scene.instances.current.len() {
            return Some(bif_core::Transform::from_matrix(
                self.scene.instances.current[idx],
            ));
        }
        None
    }

    /// Set a live transform override and update GPU (without undo).
    pub fn set_live_transform(&mut self, idx: usize, transform: bif_core::Transform) {
        let mat = transform.to_matrix();
        if idx < self.scene.instances.current.len() {
            self.scene.instances.current[idx] = mat;
            self.scene
                .edit_state
                .transform_overrides
                .insert(idx, transform);
            self.culling.mark_dirty();
            self.update_visible_instances();
            // Keep Embree pick scene in sync
            if let Some(pick_scene) = &self.pick_scene {
                pick_scene.update_instance_transform(idx, &mat);
            }
        }
    }

    /// Set a keyframe for the given instance at the current timeline frame.
    ///
    /// Inserts (or updates) a keyframe in the instance's animation data
    /// using the current transform from edit_state.
    pub fn set_keyframe(&mut self, instance_index: usize) {
        let frame = self.timeline_state.current_frame;
        let transform = match self.get_instance_transform(instance_index) {
            Some(t) => t,
            None => return,
        };

        // Get existing keyframes (from edit overrides or scene animations)
        let old_keyframes = self
            .scene
            .edit_state
            .keyframe_overrides
            .get(&instance_index)
            .and_then(|a| a.keyframes.clone())
            .or_else(|| {
                self.scene
                    .instance_animations
                    .get(instance_index)
                    .and_then(|opt| opt.as_ref())
                    .and_then(|a| a.keyframes.clone())
            });

        // Build new keyframes list with the new keyframe inserted/replaced
        let mut new_keyframes = old_keyframes.clone().unwrap_or_default();
        let keyframe = bif_core::TransformKeyframe {
            time: frame,
            transform,
        };

        // Replace existing keyframe at same time, or insert sorted
        if let Some(pos) = new_keyframes
            .iter()
            .position(|k| (k.time - frame).abs() < 0.001)
        {
            new_keyframes[pos] = keyframe;
        } else {
            let insert_pos = new_keyframes.partition_point(|k| k.time < frame);
            new_keyframes.insert(insert_pos, keyframe);
        }

        // Push undo command
        let cmd = bif_core::KeyframeCommand {
            instance_index,
            old_keyframes,
            new_keyframes: Some(new_keyframes.clone()),
        };
        self.scene
            .undo_stack
            .push(Box::new(cmd), &mut self.scene.edit_state);
        self.project.mark_dirty();

        // Also update the live animation data for playback
        let anim = self
            .scene
            .edit_state
            .keyframe_overrides
            .entry(instance_index)
            .or_insert_with(|| {
                self.scene
                    .instance_animations
                    .get(instance_index)
                    .and_then(|opt| opt.clone())
                    .unwrap_or_else(|| {
                        bif_core::AnimatedTransform::static_only(bif_core::Transform::default())
                    })
            });
        anim.keyframes = Some(new_keyframes);

        // Sync to instance_animations for playback
        if instance_index < self.scene.instance_animations.len() {
            self.scene.instance_animations[instance_index] = Some(anim.clone());
        }

        // Ensure timeline has a range if it didn't before
        if !self.timeline_state.has_range() {
            self.timeline_state.start_frame = 0.0;
            self.timeline_state.end_frame = 100.0;
            self.timeline_state.fps = 24.0;
        }

        // Update keyframe markers
        self.update_keyframe_times();

        log::info!(
            "Set keyframe at frame {:.0} for instance {}",
            frame,
            instance_index
        );
    }

    /// Update timeline keyframe_times from the selected instance's animation.
    pub fn update_keyframe_times(&mut self) {
        let times = self
            .selection
            .selected_instance_index
            .and_then(|idx| {
                self.scene
                    .edit_state
                    .keyframe_overrides
                    .get(&idx)
                    .or_else(|| {
                        self.scene
                            .instance_animations
                            .get(idx)
                            .and_then(|opt| opt.as_ref())
                    })
            })
            .and_then(|anim| anim.keyframes.as_ref())
            .map(|kfs| kfs.iter().map(|k| k.time).collect::<Vec<_>>())
            .unwrap_or_default();

        self.timeline_state.keyframe_times = times;
    }

    /// Export transform overrides, keyframes, and point clouds as a USD layer.
    pub fn export_edit_layer(&self, output_path: &str) -> anyhow::Result<()> {
        let config = bif_core::ExportConfig {
            output_path: output_path.to_string(),
            source_usd_path: self.scene.loaded_usd_path.clone(),
            as_sublayer: self.scene.loaded_usd_path.is_some(),
            export_root: "/BIF".to_string(),
            authored_prims: Vec::new(),
            graft_prefix: None,
            stage_metadata: self.scene.working_scene.stage_metadata.clone(),
            hidden_prim_paths: Vec::new(),
        };

        let result = bif_core::usd::export::export_scene(
            &self.scene.working_scene,
            &self.scene.edit_state,
            &self.scene.instances.prim_paths,
            &config,
        )
        .map_err(|e| anyhow::anyhow!("Export failed: {}", e))?;

        log::info!("Exported: {}", result);
        Ok(())
    }

    /// Export with a full config (used by UsdExport node).
    pub fn export_with_config(
        &self,
        config: &bif_core::ExportConfig,
    ) -> anyhow::Result<bif_core::ExportResult> {
        bif_core::usd::export::export_scene(
            &self.scene.working_scene,
            &self.scene.edit_state,
            &self.scene.instances.prim_paths,
            config,
        )
        .map_err(|e| anyhow::anyhow!("Export failed: {}", e))
    }

    /// Update FPS counter (call each frame with delta_time)
    pub fn update_fps(&mut self, delta_time: f32) {
        self.frame_count += 1;
        self.fps_update_timer += delta_time;

        // Update FPS every 0.5 seconds
        if self.fps_update_timer >= 0.5 {
            self.fps = self.frame_count as f32 / self.fps_update_timer;
            self.frame_count = 0;
            self.fps_update_timer = 0.0;
        }
    }

    // -----------------------------------------------------------------------
    // Project persistence (M30)
    // -----------------------------------------------------------------------

    /// Extract current renderer state into a ProjectFile for saving.
    pub fn extract_project(&self) -> persistence::ProjectFile {
        persistence::ProjectFile {
            version: persistence::FORMAT_VERSION,
            graph: self.nodes.node_graph_state.snarl.clone(),
            display_node: self.nodes.node_graph_state.display_node,
            camera: persistence::CameraData::from(&self.cam.camera),
            render_settings: self.ivar.ivar_state.batch_settings.clone(),
            display_settings: self.display_settings.clone(),
            eval_mode: self.nodes.node_graph_state.eval_mode,
        }
    }

    /// Apply a loaded ProjectFile onto this renderer, restoring all state.
    pub fn apply_project(&mut self, project: persistence::ProjectFile) {
        // Restore node graph
        self.nodes.node_graph_state.snarl = project.graph;
        self.nodes.node_graph_state.display_node = project.display_node;
        self.nodes.node_graph_state.selected_node = None;
        self.nodes.node_graph_state.eval_mode = project.eval_mode;
        self.nodes.node_graph_state.dirty_nodes.clear();

        // Restore camera (preserve aspect from current window)
        project.camera.apply_to(&mut self.cam.camera);

        // Restore settings
        self.ivar.ivar_state.batch_settings = project.render_settings;
        self.display_settings = project.display_settings;

        // Clear runtime caches — will be rebuilt on next evaluation
        self.nodes.node_outputs.clear();
        self.nodes.instancer_results.clear();
        self.nodes.scene_graph_dirty = true;
        self.nodes.materials_dirty = true;
        self.nodes.primitive_name_counters.clear();

        // Trigger reload for all UsdRead nodes with a file_path
        let node_ids: Vec<_> = self
            .nodes
            .node_graph_state
            .snarl
            .node_ids()
            .map(|(id, _)| id)
            .collect();
        let mut events = Vec::new();
        for nid in &node_ids {
            match &self.nodes.node_graph_state.snarl[*nid] {
                node_graph::SceneNode::UsdRead { file_path, .. } if !file_path.is_empty() => {
                    events.push(node_graph::NodeGraphEvent::LoadUsdFile {
                        path: file_path.clone(),
                        node_id: node_graph::GraphNodeId::from(*nid),
                    });
                }
                node_graph::SceneNode::HdriEnvironment {
                    file_path,
                    rotation,
                    intensity,
                    show_background,
                    ..
                } if !file_path.is_empty() => {
                    events.push(node_graph::NodeGraphEvent::LoadHdri {
                        path: file_path.clone(),
                        rotation: *rotation,
                        intensity: *intensity,
                        show_background: *show_background,
                    });
                }
                // Mark compute nodes dirty so they rebuild after USD loads
                node_graph::SceneNode::Primitive { .. }
                | node_graph::SceneNode::ScatterPoints { .. }
                | node_graph::SceneNode::PointInstancer { .. } => {
                    self.nodes
                        .node_graph_state
                        .dirty_nodes
                        .insert(node_graph::GraphNodeId::from(*nid));
                }
                _ => {}
            }
        }
        if !events.is_empty() {
            self.event_bus.emit(app_event::AppEvent::NodeGraph(events));
        }

        log::info!("Project applied ({} nodes)", node_ids.len());
    }

    /// Reset to empty project state.
    pub fn reset_project(&mut self) {
        // Clear node graph
        self.nodes.node_graph_state.snarl = egui_snarl::Snarl::new();
        self.nodes.node_graph_state.display_node = None;
        self.nodes.node_graph_state.selected_node = None;
        self.nodes.node_graph_state.dirty_nodes.clear();
        self.nodes.node_outputs.clear();
        self.nodes.instancer_results.clear();
        self.nodes.scene_graph_dirty = true;
        self.nodes.materials_dirty = true;
        self.nodes.primitive_name_counters.clear();

        // Clear scene data (geometry, instances, USD stage)
        self.scene.working_scene = bif_core::Scene::default();
        self.scene.instances = SceneInstances::default();
        self.scene.usd_stage = None;
        self.scene.loaded_usd_path = None;

        // Clear selection
        self.selection.selected_prim_path = None;
        self.selection.selected_prim_properties = None;
        self.selection.selected_instance_index = None;
        self.selection.scene_browser_state = scene_browser::SceneBrowserState::new();

        // Project state
        self.project.file_path = None;
        self.project.dirty = false;
        log::info!("New project");
    }

    /// Open a project from a file path.
    pub fn open_project(&mut self, path: &std::path::Path) {
        match persistence::load_project(path) {
            Ok(project) => {
                self.apply_project(project);
                self.project.file_path = Some(path.to_path_buf());
                self.project.mark_clean();

                // Update recent files (cached + disk)
                self.recent_files.add(path);
                persistence::save_recent_files(&self.recent_files);
            }
            Err(e) => {
                log::error!("Failed to open project: {}", e);
                self.with_dialog_focus(|| {
                    rfd::MessageDialog::new()
                        .set_title("Open Failed")
                        .set_description(format!("Could not open project:\n{}", e))
                        .set_buttons(rfd::MessageButtons::Ok)
                        .show()
                });
            }
        }
    }

    /// Save project to a specific path.
    pub fn save_project_to(&mut self, path: &std::path::Path) {
        let project = self.extract_project();
        match persistence::save_project(&project, path) {
            Ok(()) => {
                self.project.file_path = Some(path.to_path_buf());
                self.project.mark_clean();

                // Update recent files (cached + disk)
                self.recent_files.add(path);
                persistence::save_recent_files(&self.recent_files);
            }
            Err(e) => {
                log::error!("Failed to save project: {}", e);
                self.with_dialog_focus(|| {
                    rfd::MessageDialog::new()
                        .set_title("Save Failed")
                        .set_description(format!("Could not save project:\n{}", e))
                        .set_buttons(rfd::MessageButtons::Ok)
                        .show()
                });
            }
        }
    }

    /// Show Save As dialog and save.
    pub fn save_project_as(&mut self) {
        let path = self.with_dialog_focus(|| {
            rfd::FileDialog::new()
                .add_filter("BIF Project (ASCII)", &["bifa"])
                .add_filter("BIF Project (Binary)", &["bif"])
                .set_file_name("untitled.bifa")
                .save_file()
        });
        if let Some(path) = path {
            self.save_project_to(&path);
        }
    }

    /// Run a closure that shows a native dialog, hiding the main window
    /// so the dialog isn't stuck behind it (Windows z-order workaround).
    ///
    /// Delegates visibility toggling to the caller-installed
    /// [`dialog_focus_hook`](Self::set_dialog_focus_hook). When no hook is
    /// installed (headless / Qt consumers) the closure simply runs with no
    /// visibility change.
    fn with_dialog_focus<T>(&self, f: impl FnOnce() -> T) -> T {
        if let Some(hook) = &self.dialog_focus_hook {
            hook(false);
            let result = f();
            hook(true);
            result
        } else {
            f()
        }
    }

    /// Show "Save changes?" dialog if dirty. Returns action to take.
    pub fn prompt_unsaved_changes(&self, action: &str) -> persistence::SavePromptResult {
        if !self.project.dirty {
            return persistence::SavePromptResult::Discard;
        }
        let result = self.with_dialog_focus(|| {
            rfd::MessageDialog::new()
                .set_title(action)
                .set_description("You have unsaved changes. Save before continuing?")
                .set_buttons(rfd::MessageButtons::YesNoCancel)
                .show()
        });
        match result {
            rfd::MessageDialogResult::Yes => persistence::SavePromptResult::Save,
            rfd::MessageDialogResult::No => persistence::SavePromptResult::Discard,
            _ => persistence::SavePromptResult::Cancel,
        }
    }

    /// Save-then-proceed helper for unsaved changes prompts.
    /// Returns true if OK to proceed with the destructive action.
    pub fn save_if_needed_then_proceed(&mut self, action: &str) -> bool {
        match self.prompt_unsaved_changes(action) {
            persistence::SavePromptResult::Save => {
                if let Some(path) = self.project.file_path.clone() {
                    self.save_project_to(&path);
                } else {
                    self.save_project_as();
                }
                true
            }
            persistence::SavePromptResult::Discard => true,
            persistence::SavePromptResult::Cancel => false,
        }
    }
}
