use anyhow::Result;
use std::num::NonZeroU32;
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use wgpu::{util::DeviceExt, Device, Instance, Queue, Surface, SurfaceConfiguration};

use bif_math::{Camera, Mat4, Vec3};

// USD stage for scene browser
use bif_core::usd::UsdStage;

// New modular architecture
pub mod batch_render;
pub mod compute_ibl;
pub mod culling_manager;
pub mod environment;
pub mod environment_manager;
pub mod frustum_culling;
pub mod gizmo;
pub mod gnomon;
pub mod gpu_types;
pub mod grid;
pub mod ivar_renderer;
pub mod ivar_state;
pub mod lights;
pub mod mesh_data;
pub mod multi_draw;
pub mod point_preview;
pub mod texture_loader;

// Scene browser and property inspector modules
mod animation;
mod ivar_build;
pub mod node_graph;
pub mod property_inspector;
mod render;
pub mod scene_browser;
mod scene_loader;
pub mod skybox;
pub mod timeline;

// Re-exports from new modules
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
pub use texture_loader::{
    collect_scene_texture_paths, create_default_gpu_textures, create_gpu_texture,
    create_gpu_textures_for_scene,
};
pub use timeline::TimelineState;

pub use node_graph::{render_node_graph, NodeGraphEvent, NodeGraphState, SceneNode};
pub use property_inspector::{
    render_property_inspector, reset_transform_edit_cache, PrimProperties, TransformEdit,
};
pub use scene_browser::{
    CompositeProvider, EmptyPrimProvider, PrimDataProvider, PrimDisplayInfo, ProceduralPrim,
    SceneBrowserState,
};

use batch_render::BatchMessage;

/// Maximum instance count for dynamic instance buffer.
const MAX_INSTANCES: u32 = 100_000;

/// Status of an asynchronous USD load operation.
#[derive(Debug, Clone)]
pub enum UsdLoadStatus {
    /// No load in progress.
    Idle,
    /// Loading in progress with granular progress info.
    Loading(UsdLoadProgress),
    /// Load completed — scene + stage ready for GPU finalization.
    Ready,
    /// Load failed with error message.
    Error(String),
}

/// Granular progress stages for USD loading.
#[derive(Debug, Clone)]
pub enum UsdLoadProgress {
    /// Opening the USD stage via C++ bridge.
    OpeningStage,
    /// Extracting mesh geometry.
    ExtractingMeshes { current: usize, total: usize },
    /// Extracting materials.
    ExtractingMaterials { current: usize, total: usize },
    /// Extracting lights.
    ExtractingLights,
    /// Building BIF scene graph.
    BuildingScene,
    /// Finalizing (creating GPU buffers).
    Finalizing,
}

impl std::fmt::Display for UsdLoadProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpeningStage => write!(f, "Opening USD stage..."),
            Self::ExtractingMeshes { current, total } => {
                write!(f, "Extracting meshes ({current}/{total})...")
            }
            Self::ExtractingMaterials { current, total } => {
                write!(f, "Extracting materials ({current}/{total})...")
            }
            Self::ExtractingLights => write!(f, "Extracting lights..."),
            Self::BuildingScene => write!(f, "Building scene..."),
            Self::Finalizing => write!(f, "Creating GPU buffers..."),
        }
    }
}

impl std::fmt::Display for UsdLoadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, ""),
            Self::Loading(p) => write!(f, "{p}"),
            Self::Ready => write!(f, "Ready"),
            Self::Error(e) => write!(f, "Error: {e}"),
        }
    }
}

/// Message sent from the background USD load thread.
pub(crate) enum UsdLoadMessage {
    /// Progress update.
    Progress(UsdLoadProgress),
    /// Load completed with scene + stage.
    Complete {
        scene: Box<bif_core::Scene>,
        stage: bif_core::usd::UsdStage,
        path: std::path::PathBuf,
    },
    /// Load failed.
    Failed(String),
}

/// Core renderer managing wgpu state
pub struct Renderer {
    pub(crate) surface: Surface<'static>,
    pub(crate) device: Device,
    pub(crate) queue: Queue,
    pub(crate) config: SurfaceConfiguration,
    pub size: (u32, u32),
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) index_buffer: wgpu::Buffer,
    pub(crate) num_indices: u32,
    pub(crate) instance_buffer: wgpu::Buffer,
    pub(crate) num_instances: u32,
    pub camera: Camera,
    pub(crate) camera_uniform: CameraUniform,
    pub(crate) camera_buffer: wgpu::Buffer,
    pub(crate) camera_bind_group: wgpu::BindGroup,
    pub(crate) material_uniform: MaterialUniform,
    pub(crate) material_buffer: wgpu::Buffer,
    pub(crate) material_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) material_bind_group: wgpu::BindGroup,
    pub(crate) material_table_buffer: wgpu::Buffer,
    pub(crate) material_table_len: u32,
    /// Per-triangle material IDs for primitive_index lookup (GeomSubsets)
    pub(crate) triangle_material_buffer: wgpu::Buffer,
    /// Whether we have per-triangle materials (vs instance materials)
    pub(crate) has_triangle_materials: bool,
    pub(crate) gpu_textures: GpuTextureSet,
    pub(crate) texture_sampler: wgpu::Sampler,
    pub(crate) texture_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) texture_bind_group: wgpu::BindGroup,
    pub(crate) mesh_bounds_min: Vec3,
    pub(crate) mesh_bounds_max: Vec3,
    pub(crate) depth_texture: wgpu::Texture,
    pub(crate) depth_view: wgpu::TextureView,

    // Gnomon resources
    pub(crate) gnomon: GnomonRenderer,

    // Ground grid
    pub(crate) grid: GridRenderer,

    // egui state
    pub(crate) egui_ctx: egui::Context,
    pub(crate) egui_state: egui_winit::State,
    pub(crate) egui_renderer: egui_wgpu::Renderer,

    // UI state
    pub show_ui: bool,
    pub fps: f32,
    pub(crate) frame_count: u32,
    pub(crate) fps_update_timer: f32,

    // Stats - TODO: Track polygon count from source data for accuracy
    pub(crate) num_triangles: u64,

    // UI layout metrics (for viewport-safe overlays)
    pub(crate) ui_left_panel_width: f32,
    pub(crate) ui_right_panel_width: f32,
    pub(crate) ui_top_panel_height: f32,
    pub(crate) ui_bottom_panel_height: f32,

    // Ivar CPU path tracer state
    pub ivar_state: IvarState,
    pub(crate) ivar_texture: wgpu::Texture,
    pub(crate) ivar_texture_view: wgpu::TextureView,
    pub(crate) ivar_sampler: wgpu::Sampler,
    pub(crate) ivar_bind_group: wgpu::BindGroup,
    pub(crate) ivar_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) ivar_pipeline: wgpu::RenderPipeline,

    // Cached mesh data for Ivar scene building
    pub(crate) mesh_data: MeshData,

    // Instance transforms for Ivar (stored as Mat4 arrays)
    /// Base transforms (from scene load) - used for re-evaluation
    pub(crate) instance_transforms: Vec<Mat4>,
    /// Current transforms (after animation evaluation) - used for rendering
    pub(crate) current_transforms: Vec<Mat4>,
    pub(crate) instance_material_ids: Vec<u32>,
    /// Prototype ID for each instance (parallel to instance_transforms, for multi-draw rebuild)
    pub(crate) instance_prototype_ids: Vec<usize>,
    /// Prim path for each instance (parallel to instance_transforms, for USD export)
    pub(crate) instance_prim_paths: Vec<String>,

    // Animation data for viewport playback
    /// Animated transforms for instances (parallel to instances)
    pub(crate) instance_animations: Vec<Option<bif_core::AnimatedTransform>>,
    /// Last evaluated frame (for change detection)
    pub(crate) last_evaluated_frame: f64,
    /// Mesh indices that have vertex animation (deformation)
    pub(crate) vertex_animated_meshes: Vec<usize>,

    // Material for Ivar rendering (from loaded USD scene)
    pub(crate) scene_material: bif_core::Material,
    // All scene materials for multi-material Ivar rendering
    pub(crate) scene_materials: Vec<std::sync::Arc<bif_core::Material>>,
    // Base directory for resolving texture paths
    pub(crate) texture_base_dir: Option<std::path::PathBuf>,

    // Multi-draw state for per-prototype rendering
    pub(crate) multi_draw: MultiDrawState,

    // Culling and LOD state
    pub(crate) culling: CullingManager,

    // Scene browser state
    pub scene_browser_state: SceneBrowserState,

    // Currently selected prim path (synced with scene browser)
    pub selected_prim_path: Option<String>,

    // Properties for the selected prim (computed when selection changes)
    pub selected_prim_properties: Option<PrimProperties>,

    // USD stage for scene browser hierarchy (None if loaded via pure Rust parser)
    // Wrapped in Arc for sharing with batch render thread
    pub(crate) usd_stage: Option<Arc<UsdStage>>,

    /// Path to the currently loaded USD file (for sublayer export)
    pub(crate) loaded_usd_path: Option<String>,

    // Node graph state for scene assembly
    pub node_graph_state: NodeGraphState,

    // Timeline state for animation playback
    pub timeline_state: TimelineState,

    // Environment IBL and skybox state
    pub(crate) environment: EnvironmentManager,

    // Lights state (UsdLux)
    pub(crate) lights: LightsManager,

    // Batch render state
    pub(crate) batch_receiver: Option<mpsc::Receiver<BatchMessage>>,
    pub(crate) batch_cancel_flag: Option<Arc<AtomicBool>>,

    // Viewport camera selection state
    /// Active camera source for viewport (separate from batch render settings)
    pub(crate) viewport_camera_source: CameraSource,
    /// Lock camera controls when USD camera active
    pub(crate) camera_locked: bool,
    /// Selected USD camera path (for animation during playback)
    pub(crate) selected_usd_camera: Option<String>,

    // Viewport picking state
    /// Embree pick scene for click-to-select (rebuilt on scene load)
    pub(crate) pick_scene: Option<bif_renderer::EmbreePickScene>,
    /// Currently selected instance index (from viewport pick or scene browser)
    pub selected_instance_index: Option<usize>,

    // Undo/redo state
    /// Undo stack for reversible editing commands
    pub undo_stack: bif_core::UndoStack,
    /// Edit state with transform overrides
    pub edit_state: bif_core::EditState,

    // Scene cameras (from Camera primitives)
    pub(crate) scene_cameras: Vec<bif_core::SceneCamera>,

    // Translate gizmo state
    pub gizmo_state: gizmo::GizmoState,

    // Point preview renderer for scatter visualization
    pub(crate) point_preview: point_preview::PointPreviewRenderer,
    /// Last viewport size used for point preview params (for change detection).
    pub(crate) point_preview_last_vp: (f32, f32),
    /// Whether point preview params need a GPU write next frame.
    pub(crate) point_preview_params_dirty: bool,

    // Viewport display toggles
    pub show_grid: bool,
    /// Apply Z-to-Y axis correction when stage is Z-up.
    pub apply_axis_correction: bool,
    /// Apply metersPerUnit scaling to match viewport (assumed meters).
    pub apply_unit_scaling: bool,

    /// Persistent working scene that accumulates all primitives and USD objects.
    pub(crate) working_scene: bif_core::Scene,
    /// Counters for generating unique primitive names (e.g. "Cube", "Cube_2").
    pub(crate) primitive_name_counters: std::collections::HashMap<String, usize>,
    /// Mapping from node graph NodeId to working_scene prototype IDs.
    /// Single-proto nodes (Primitive) get a Vec of length 1; multi-proto (UsdRead) get multiple.
    pub(crate) node_proto_map: std::collections::HashMap<egui_snarl::NodeId, Vec<usize>>,
    /// True when material set changed (new USD load, node delete) — triggers texture rebuild.
    /// Transform-only changes (Xform drag, display toggle) skip the expensive texture path.
    pub(crate) materials_dirty: bool,
    /// Set by property inspector when Xform T/R/S is edited (consumed next frame).
    pub(crate) xform_property_changed: Option<egui_snarl::NodeId>,
    /// Mapping from scatter node NodeId to point cloud ID.
    pub(crate) node_cloud_map: std::collections::HashMap<egui_snarl::NodeId, usize>,
    /// Monotonically increasing counter for unique cloud IDs.
    pub(crate) next_cloud_id: usize,
    /// Scatter node → surface prototype ID (for cleanup on reconnect/delete).
    /// Rebuilt into a `HashSet` in `reload_working_scene()` to avoid ref-counting bugs.
    pub(crate) node_scatter_surface_map: std::collections::HashMap<egui_snarl::NodeId, usize>,
    /// Cached instancer expansion results: NodeId -> expanded instances.
    /// BTreeMap for deterministic iteration order (picking, culling, debug).
    pub(crate) instancer_results:
        std::collections::BTreeMap<egui_snarl::NodeId, Vec<bif_core::Instance>>,

    // Async USD loading state
    /// Receiver for messages from the background USD load thread.
    pub(crate) usd_load_receiver: Option<mpsc::Receiver<UsdLoadMessage>>,
    /// Current status of the async USD load (for UI display).
    pub usd_load_status: UsdLoadStatus,
}

impl Renderer {
    /// Create a new renderer for the given window
    pub async fn new(window: std::sync::Arc<winit::window::Window>) -> Result<Self> {
        let size = window.inner_size();

        // Create wgpu instance
        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        // Create surface
        let surface = instance.create_surface(window.clone())?;

        // Request adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("Failed to find suitable GPU adapter"))?;

        // Request device and queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("BIF Device"),
                    required_features: wgpu::Features::TEXTURE_BINDING_ARRAY
                        | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
                        | wgpu::Features::SHADER_PRIMITIVE_INDEX,
                    required_limits: wgpu::Limits {
                        max_sampled_textures_per_shader_stage: MAX_VIEWPORT_TEXTURES as u32,
                        max_buffer_size: 1 << 30, // 1GB for large meshes
                        max_bind_groups: 5,       // Groups 0-4 (camera, material, texture, env, lights)
                        ..Default::default()
                    },
                    memory_hints: Default::default(),
                },
                None,
            )
            .await?;

        // Configure surface
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo, // VSync for proper frame pacing // VSync
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        // Start with blank scene - no default mesh
        log::info!("Initializing blank scene (no default geometry)");

        // Create empty mesh data
        let mesh_data = MeshData {
            vertices: vec![],
            indices: vec![],
            bounds_min: Vec3::new(0.0, 0.0, 0.0),
            bounds_max: Vec3::new(0.0, 0.0, 0.0),
            triangle_material_ids: None,
            mesh_ranges: None,
        };

        // Create camera at default position looking at origin
        let aspect = size.width as f32 / size.height as f32;
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
                front_face: wgpu::FrontFace::Cw, // USD uses CW winding
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
        let (depth_texture, depth_view) =
            ivar_renderer::create_depth_texture(&device, (size.width, size.height));

        // No instances by default - empty scene
        let dummy_instance = InstanceData {
            model_matrix: Mat4::IDENTITY.to_cols_array_2d(),
            material_id: 0,
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

        // Initialize egui
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None, // max_texture_side (use default)
        );

        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            None, // No depth testing for egui
            1,
            false, // allow_srgb_render_target
        );

        log::info!("egui initialized");

        // Create gnomon renderer
        let gnomon = GnomonRenderer::new(&device, config.format);
        log::info!("Gnomon initialized");

        // Create ground grid renderer
        let grid = GridRenderer::new(&device, config.format, &camera_bind_group_layout);
        log::info!("Grid initialized");

        // Create point preview renderer
        let point_preview = point_preview::PointPreviewRenderer::new(
            &device,
            config.format,
            &camera_bind_group_layout,
        );
        log::info!("Point preview initialized");

        // Calculate stats - empty scene has 0 triangles
        let num_triangles = 0;

        // Create Ivar resources for CPU path tracer display
        let (ivar_texture, ivar_texture_view) =
            ivar_renderer::create_ivar_texture(&device, (size.width, size.height));

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

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size: (size.width, size.height),
            pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: 0, // Empty scene - no indices
            instance_buffer,
            num_instances: 0, // Empty scene - no instances
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            material_uniform,
            material_buffer,
            material_bind_group_layout,
            material_bind_group,
            material_table_buffer,
            material_table_len,
            triangle_material_buffer,
            has_triangle_materials,
            gpu_textures,
            texture_sampler,
            texture_bind_group_layout,
            texture_bind_group,
            mesh_bounds_min: mesh_data.bounds_min,
            mesh_bounds_max: mesh_data.bounds_max,
            depth_texture,
            depth_view,
            gnomon,
            grid,
            egui_ctx,
            egui_state,
            egui_renderer,
            show_ui: true,
            fps: 0.0,
            frame_count: 0,
            fps_update_timer: 0.0,
            num_triangles,
            ui_left_panel_width: 0.0,
            ui_right_panel_width: 0.0,
            ui_top_panel_height: 0.0,
            ui_bottom_panel_height: 0.0,
            ivar_state: IvarState::default(),
            ivar_texture,
            ivar_texture_view,
            ivar_sampler,
            ivar_bind_group,
            ivar_bind_group_layout,
            ivar_pipeline,
            mesh_data,
            instance_transforms: vec![], // Empty scene - no instances
            current_transforms: vec![],
            instance_material_ids: vec![],
            instance_prototype_ids: vec![],
            instance_prim_paths: vec![],
            instance_animations: vec![],
            last_evaluated_frame: 0.0,
            vertex_animated_meshes: vec![],
            scene_material: bif_core::Material::default(),
            scene_materials: vec![],
            texture_base_dir: None,
            multi_draw: MultiDrawState::new(),
            culling,
            scene_browser_state: SceneBrowserState::new(),
            selected_prim_path: None,
            selected_prim_properties: None,
            usd_stage: None,
            loaded_usd_path: None,
            node_graph_state: NodeGraphState::new(),
            timeline_state: TimelineState::default(),
            environment,
            lights,
            batch_receiver: None,
            batch_cancel_flag: None,
            viewport_camera_source: CameraSource::Viewport,
            camera_locked: false,
            selected_usd_camera: None,
            pick_scene: None,
            selected_instance_index: None,
            undo_stack: bif_core::UndoStack::new(),
            edit_state: bif_core::EditState::default(),
            scene_cameras: vec![],
            gizmo_state: gizmo::GizmoState::new(),
            point_preview,
            point_preview_last_vp: (0.0, 0.0),
            point_preview_params_dirty: true,
            show_grid: true,
            apply_axis_correction: false,
            apply_unit_scaling: false,
            working_scene: bif_core::Scene::new("Working"),
            primitive_name_counters: std::collections::HashMap::new(),
            node_proto_map: std::collections::HashMap::new(),
            materials_dirty: true,
            xform_property_changed: None,
            node_cloud_map: std::collections::HashMap::new(),
            next_cloud_id: 0,
            node_scatter_surface_map: std::collections::HashMap::new(),
            instancer_results: std::collections::BTreeMap::new(),
            usd_load_receiver: None,
            usd_load_status: UsdLoadStatus::Idle,
        })
    }

    /// Check if camera controls are locked (USD camera active).
    pub fn is_camera_locked(&self) -> bool {
        self.camera_locked
    }

    /// Handle window resize
    pub fn resize(&mut self, new_size: (u32, u32)) {
        if new_size.0 > 0 && new_size.1 > 0 {
            self.size = new_size;
            self.config.width = new_size.0;
            self.config.height = new_size.1;
            self.surface.configure(&self.device, &self.config);

            // Recreate depth texture with new size
            let (depth_texture, depth_view) =
                ivar_renderer::create_depth_texture(&self.device, new_size);
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;

            // Recreate Ivar texture with new size
            let (ivar_texture, ivar_texture_view) =
                ivar_renderer::create_ivar_texture(&self.device, new_size);
            self.ivar_texture = ivar_texture;
            self.ivar_texture_view = ivar_texture_view;

            // Recreate Ivar bind group with new texture view (reuse existing layout)
            self.ivar_bind_group = ivar_renderer::create_ivar_bind_group(
                &self.device,
                &self.ivar_bind_group_layout,
                &self.ivar_texture_view,
                &self.ivar_sampler,
            );

            // Full Ivar state reset on resize.
            // image_buffer = None because target dims are unknown until next
            // frame's viewport_rect(). One frame of black during resize is
            // acceptable (fundamentally different from orbit).
            self.ivar_state.cancel_flag.store(true, Ordering::Relaxed);
            self.ivar_state.cancel_flag = Arc::new(AtomicBool::new(false));
            self.ivar_state.receiver = None;
            self.ivar_state.image_buffer = None;
            self.ivar_state.render_complete = false;
            self.ivar_state.accumulated_samples = 0;
            self.ivar_state.buckets_completed = 0;
            self.ivar_state.current_scale = 1;
            self.ivar_state.last_interaction_time = None;
            self.ivar_state.last_camera_snapshot = None;
            self.ivar_state.render_start_time = None;
            self.ivar_state.final_render_secs = None;

            // Update camera aspect ratio from viewport (excludes UI panels)
            let (_, _, vp_w, vp_h) = self.viewport_rect();
            let aspect = vp_w / vp_h;
            self.camera.set_aspect(aspect);
            self.update_camera();
        }
    }

    /// Returns the viewport rect (x, y, w, h) in pixels after subtracting all UI panels.
    pub fn viewport_rect(&self) -> (f32, f32, f32, f32) {
        let x = self.ui_left_panel_width;
        let y = self.ui_top_panel_height;
        let w =
            (self.size.0 as f32 - self.ui_left_panel_width - self.ui_right_panel_width).max(1.0);
        let h =
            (self.size.1 as f32 - self.ui_top_panel_height - self.ui_bottom_panel_height).max(1.0);
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
        self.camera_uniform.update_view_proj(&self.camera);
        // Sync selection state into uniform
        self.camera_uniform.selected_instance_id = self
            .selected_instance_index
            .map(|i| i as u32)
            .unwrap_or(gpu_types::NO_SELECTION);
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );

        // Update gnomon uniform with camera rotation
        self.gnomon.update_from_camera(&self.queue, &self.camera);
    }

    /// Update environment parameters without regenerating maps.
    pub fn update_environment_params(
        &mut self,
        intensity: f32,
        rotation: f32,
        show_background: bool,
    ) {
        self.environment
            .update_params(&self.queue, intensity, rotation, show_background);
        // Sync to Ivar state for live CPU path tracer updates
        self.ivar_state.hdri_rotation = rotation;
        self.ivar_state.hdri_intensity = intensity;
    }

    /// Update lights uniform buffer from scene lights.
    pub fn update_lights(&mut self, lights: &[bif_core::Light]) {
        self.lights.update(&self.queue, lights);
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
        self.culling.update_visible_instances(
            &self.queue,
            &self.instance_buffer,
            &self.camera,
            &self.current_transforms,
            &self.instance_material_ids,
        );
    }

    /// Frame the camera on the loaded mesh
    pub fn frame_mesh(&mut self) {
        let mesh_center = (self.mesh_bounds_min + self.mesh_bounds_max) * 0.5;
        let mesh_size = (self.mesh_bounds_max - self.mesh_bounds_min).length();
        let camera_distance = mesh_size * 1.5;

        // Position camera looking at mesh center from current yaw/pitch
        self.camera.target = mesh_center;
        self.camera.distance = camera_distance;
        self.camera.update_position_from_angles();

        self.update_camera();
        log::info!(
            "Framed mesh at center {:?}, distance {:.2}",
            mesh_center,
            camera_distance
        );
    }

    /// Sync viewport camera to a USD camera at the current timeline frame
    pub fn sync_viewport_to_usd_camera(&mut self, camera_path: &str) {
        let Some(ref stage) = self.usd_stage else {
            log::warn!("No USD stage loaded");
            return;
        };

        let time = self.timeline_state.current_frame;

        match stage.get_camera_xform_at_time(camera_path, time) {
            Ok(xform) => {
                // USD stores translation in row 3, not column 3
                let position = xform.row(3).truncate();

                // Extract forward direction (negative Z in camera space)
                // USD row-major: row 2 is the Z axis
                let forward =
                    -Vec3::new(xform.row(0).z, xform.row(1).z, xform.row(2).z).normalize();

                // Target is position + forward * reasonable distance
                let target = position + forward * 10.0;

                // Extract up vector (Y axis) from rows
                let up = Vec3::new(xform.row(0).y, xform.row(1).y, xform.row(2).y).normalize();

                log::info!(
                    "USD camera '{}' at frame {}: pos={:?}, target={:?}, up={:?}",
                    camera_path,
                    time,
                    position,
                    target,
                    up
                );

                // Update viewport camera
                self.camera.position = position;
                self.camera.target = target;
                self.camera.up = up;
                self.camera.distance = 10.0;

                // Recalculate yaw/pitch from the new orientation
                let dir = (position - target).normalize();
                self.camera.yaw = dir.x.atan2(dir.z);
                self.camera.pitch = (-dir.y).asin();

                self.update_camera();
            }
            Err(e) => {
                log::error!("Failed to get USD camera transform: {:?}", e);
            }
        }
    }

    /// Sync viewport camera to a scene camera (from Camera primitive).
    ///
    /// Reads the instance transform and applies the camera's FOV.
    pub fn sync_viewport_to_scene_camera(&mut self, cam_idx: usize) {
        let cam = match self.scene_cameras.get(cam_idx) {
            Some(c) => c.clone(),
            None => {
                log::warn!("Scene camera index {} out of range", cam_idx);
                return;
            }
        };

        let inst_idx = cam.instance_index;
        if inst_idx >= self.current_transforms.len() {
            log::warn!("Scene camera instance {} out of range", inst_idx);
            return;
        }

        let mat = self.current_transforms[inst_idx];
        let transform = bif_core::Transform::from_matrix(mat);

        // Camera looks down -Z in its local space
        let forward = transform.rotation * -Vec3::Z;
        let up = transform.rotation * Vec3::Y;

        self.camera.position = transform.translation;
        self.camera.target = transform.translation + forward * 10.0;
        self.camera.up = up;
        self.camera.distance = 10.0;
        self.camera.fov_y = cam.fov_y;

        // Recalculate yaw/pitch
        let dir = (self.camera.position - self.camera.target).normalize();
        self.camera.yaw = dir.x.atan2(dir.z);
        self.camera.pitch = (-dir.y).asin();

        self.update_camera();

        log::info!(
            "Synced to scene camera '{}' (instance {})",
            cam.name,
            inst_idx
        );
    }

    /// Handle egui window event - returns true if event was consumed by egui
    pub fn handle_egui_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        let response = self.egui_state.on_window_event(window, event);
        response.consumed
    }

    /// Rebuild the Embree pick scene from current mesh + instance data.
    ///
    /// Call after scene load or when geometry changes. Uses the first prototype's
    /// triangles (single-draw) or combined mesh triangles.
    pub fn rebuild_pick_scene(&mut self) {
        if self.mesh_data.indices.is_empty() || self.current_transforms.is_empty() {
            self.pick_scene = None;
            return;
        }

        // Extract triangle vertices from mesh data
        let indices = &self.mesh_data.indices;
        let verts = &self.mesh_data.vertices;
        let tri_count = indices.len() / 3;
        let mut triangles = Vec::with_capacity(tri_count);

        for tri in 0..tri_count {
            let i0 = indices[tri * 3] as usize;
            let i1 = indices[tri * 3 + 1] as usize;
            let i2 = indices[tri * 3 + 2] as usize;
            if i0 < verts.len() && i1 < verts.len() && i2 < verts.len() {
                triangles.push([
                    Vec3::from_array(verts[i0].position),
                    Vec3::from_array(verts[i1].position),
                    Vec3::from_array(verts[i2].position),
                ]);
            }
        }

        match bif_renderer::EmbreePickScene::new(&triangles, &self.current_transforms) {
            Ok(scene) => {
                log::info!(
                    "Pick scene rebuilt: {} tris, {} instances",
                    triangles.len(),
                    self.current_transforms.len()
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
        let inv_vp = self.camera.view_projection_matrix().inverse();

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

        pick_scene.pick(near_pos, direction).map(|result| {
            log::info!(
                "Picked instance {} (tri={}, t={:.2}, pos=({:.2},{:.2},{:.2}))",
                result.instance_index,
                result.triangle_index,
                result.t,
                result.hit_point.x,
                result.hit_point.y,
                result.hit_point.z
            );
            result.instance_index
        })
    }

    /// Apply a transform override from edit_state to the GPU instance buffer.
    ///
    /// Reads the override for `idx` and updates `current_transforms` + GPU.
    pub fn apply_transform_override(&mut self, idx: usize) {
        if let Some(transform) = self.edit_state.transform_overrides.get(&idx) {
            let mat = transform.to_matrix();
            if idx < self.current_transforms.len() {
                self.current_transforms[idx] = mat;
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
            .edit_state
            .transform_overrides
            .iter()
            .filter(|(idx, _)| **idx < self.current_transforms.len())
            .map(|(idx, t)| (*idx, t.to_matrix()))
            .collect();

        for (idx, mat) in &overrides {
            self.current_transforms[*idx] = *mat;
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
        self.undo_stack.push(Box::new(cmd), &mut self.edit_state);
        self.apply_transform_override(instance_index);

        // Invalidate Ivar scene so transform change is reflected
        if self.ivar_state.mode == ivar_state::RenderMode::Ivar {
            self.invalidate_ivar_scene();
        }
    }

    /// Undo the last command. Returns description if successful.
    pub fn undo(&mut self) -> Option<String> {
        let desc = self.undo_stack.undo(&mut self.edit_state)?.to_string();
        self.apply_all_transform_overrides();
        if self.ivar_state.mode == ivar_state::RenderMode::Ivar {
            self.invalidate_ivar_scene();
        }
        Some(desc)
    }

    /// Redo the next command. Returns description if successful.
    pub fn redo(&mut self) -> Option<String> {
        let desc = self.undo_stack.redo(&mut self.edit_state)?.to_string();
        self.apply_all_transform_overrides();
        if self.ivar_state.mode == ivar_state::RenderMode::Ivar {
            self.invalidate_ivar_scene();
        }
        Some(desc)
    }

    /// Get the current transform for an instance, preferring edit overrides.
    pub fn get_instance_transform(&self, idx: usize) -> Option<bif_core::Transform> {
        if let Some(t) = self.edit_state.transform_overrides.get(&idx) {
            return Some(t.clone());
        }
        if idx < self.current_transforms.len() {
            return Some(bif_core::Transform::from_matrix(
                self.current_transforms[idx],
            ));
        }
        None
    }

    /// Set a live transform override and update GPU (without undo).
    pub fn set_live_transform(&mut self, idx: usize, transform: bif_core::Transform) {
        let mat = transform.to_matrix();
        if idx < self.current_transforms.len() {
            self.current_transforms[idx] = mat;
            self.edit_state.transform_overrides.insert(idx, transform);
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
            .edit_state
            .keyframe_overrides
            .get(&instance_index)
            .and_then(|a| a.keyframes.clone())
            .or_else(|| {
                self.instance_animations
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
        self.undo_stack.push(Box::new(cmd), &mut self.edit_state);

        // Also update the live animation data for playback
        let anim = self
            .edit_state
            .keyframe_overrides
            .entry(instance_index)
            .or_insert_with(|| {
                self.instance_animations
                    .get(instance_index)
                    .and_then(|opt| opt.clone())
                    .unwrap_or_else(|| {
                        bif_core::AnimatedTransform::static_only(bif_core::Transform::default())
                    })
            });
        anim.keyframes = Some(new_keyframes);

        // Sync to instance_animations for playback
        if instance_index < self.instance_animations.len() {
            self.instance_animations[instance_index] = Some(anim.clone());
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
            .selected_instance_index
            .and_then(|idx| {
                self.edit_state.keyframe_overrides.get(&idx).or_else(|| {
                    self.instance_animations
                        .get(idx)
                        .and_then(|opt| opt.as_ref())
                })
            })
            .and_then(|anim| anim.keyframes.as_ref())
            .map(|kfs| kfs.iter().map(|k| k.time).collect::<Vec<_>>())
            .unwrap_or_default();

        self.timeline_state.keyframe_times = times;
    }

    /// Reset cached transform edit values in the egui data store.
    pub fn reset_transform_edit_cache(&self) {
        reset_transform_edit_cache(&self.egui_ctx);
    }

    /// Export transform overrides, keyframes, and point clouds as a USD layer.
    pub fn export_edit_layer(&self, output_path: &str) -> anyhow::Result<()> {
        let config = bif_core::ExportConfig {
            output_path: output_path.to_string(),
            source_usd_path: self.loaded_usd_path.clone(),
            as_sublayer: self.loaded_usd_path.is_some(),
            export_root: "/BIF".to_string(),
            authored_prims: Vec::new(),
            graft_prefix: None,
        };

        let result = bif_core::usd::export::export_scene(
            &self.working_scene,
            &self.edit_state,
            &self.instance_prim_paths,
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
            &self.working_scene,
            &self.edit_state,
            &self.instance_prim_paths,
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
}
