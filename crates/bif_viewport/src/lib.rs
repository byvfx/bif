use anyhow::Result;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;

use wgpu::{util::DeviceExt, Device, Instance, Queue, Surface, SurfaceConfiguration};

use bif_math::{Aabb, Camera, Frustum, Mat4, Mat4Ext, Vec3};

// USD stage for scene browser
use bif_core::usd::UsdStage;

// Re-export bif_renderer types for Ivar integration
use bif_renderer::{
    render_bucket_with_aovs, BvhNode, Color, DisneyBSDF, EmbreeScene, Hittable, LightList,
    RenderConfig,
};

// New modular architecture
pub mod batch_render;
pub mod compute_ibl;
pub mod environment;
pub mod frustum_culling;
pub mod gnomon;
pub mod gpu_types;
pub mod ivar_renderer;
pub mod ivar_state;
pub mod lights;
pub mod mesh_data;
pub mod texture_loader;

// Scene browser and property inspector modules
pub mod node_graph;
pub mod property_inspector;
pub mod scene_browser;
pub mod skybox;

// Re-exports from new modules
pub use environment::GpuEnvironment;
pub use frustum_culling::{update_visible_instances, CullingResult};
pub use gpu_types::{
    CameraUniform, CullingScratch, EnvironmentParamsUniform, GnomonUniform, GnomonVertex,
    GpuTextureSet, InstanceData, LightGpu, LightsUniform, MaterialGpu, MaterialUniform,
    PrototypeGpuData, Vertex, MAX_VIEWPORT_LIGHTS, MAX_VIEWPORT_TEXTURES,
};
pub use gnomon::GnomonRenderer;
pub use ivar_renderer::{create_depth_texture, create_ivar_pipeline, create_ivar_texture};
pub use lights::LightsManager;
pub use ivar_state::{
    BatchRenderSettings, BatchRenderStatus, BuildStatus, CameraSnapshot, CameraSource, IvarMessage,
    IvarState, RenderMode,
};
pub use mesh_data::MeshData;
pub use texture_loader::{
    collect_scene_texture_paths, create_default_gpu_textures, create_gpu_texture,
    create_gpu_textures_for_scene,
};

pub use node_graph::{render_node_graph, NodeGraphEvent, NodeGraphState, SceneNode};
pub use property_inspector::{render_property_inspector, PrimProperties};
pub use scene_browser::{EmptyPrimProvider, PrimDataProvider, PrimDisplayInfo, SceneBrowserState};

use batch_render::{BatchMessage, BatchSceneData, SceneBuilderData, TriangleData};

/// Timeline state for animation playback.
#[derive(Clone, Debug)]
pub struct TimelineState {
    /// Whether animation is playing
    pub is_playing: bool,
    /// Current frame
    pub current_frame: f64,
    /// Start frame from USD stage
    pub start_frame: f64,
    /// End frame from USD stage
    pub end_frame: f64,
    /// Frames per second
    pub fps: f64,
    /// Loop playback when reaching end
    pub loop_playback: bool,
    /// Use USD camera instead of viewport camera
    pub use_usd_camera: bool,
    /// Snap to integer frames (no sub-frame interpolation)
    pub snap_to_frames: bool,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            is_playing: false,
            current_frame: 0.0,
            start_frame: 0.0,
            end_frame: 0.0,
            fps: 24.0,
            loop_playback: true,
            use_usd_camera: false,
            snap_to_frames: true, // Default to integer frames for predictable playback
        }
    }
}

impl TimelineState {
    /// Check if the timeline has a valid frame range.
    pub fn has_range(&self) -> bool {
        self.end_frame > self.start_frame
    }

    /// Advance the timeline by delta time.
    pub fn advance(&mut self, delta_time: f32) {
        if !self.is_playing || !self.has_range() {
            return;
        }

        let frame_delta = delta_time as f64 * self.fps;
        self.current_frame += frame_delta;

        if self.current_frame > self.end_frame {
            if self.loop_playback {
                self.current_frame = self.start_frame;
            } else {
                self.current_frame = self.end_frame;
                self.is_playing = false;
            }
        }
    }

    /// Go to start frame.
    pub fn go_to_start(&mut self) {
        self.current_frame = self.start_frame;
    }

    /// Go to end frame.
    pub fn go_to_end(&mut self) {
        self.current_frame = self.end_frame;
    }

    /// Set timeline from scene info.
    pub fn set_from_scene(&mut self, start: f64, end: f64, fps: f64) {
        self.start_frame = start;
        self.end_frame = end;
        self.fps = fps;
        self.current_frame = start;
    }

    /// Get the effective frame for animation evaluation.
    /// If snap_to_frames is true, returns the nearest integer frame.
    pub fn effective_frame(&self) -> f64 {
        if self.snap_to_frames {
            self.current_frame.round()
        } else {
            self.current_frame
        }
    }
}

/// Result from background IBL generation thread.
enum IblResult {
    Success {
        /// HDR pixels for GPU compute IBL (viewport)
        hdr_pixels: Vec<[f32; 3]>,
        hdr_width: u32,
        hdr_height: u32,
        /// Ivar CPU path tracer environment
        ivar_env: Arc<bif_renderer::HdriEnvironment>,
        path: String,
        rotation_rad: f32,
        intensity: f32,
        show_background: bool,
    },
    Error {
        path: String,
        message: String,
    },
}

/// Core renderer managing wgpu state
pub struct Renderer {
    pub surface: Surface<'static>,
    pub device: Device,
    pub queue: Queue,
    pub config: SurfaceConfiguration,
    pub size: (u32, u32),
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    instance_buffer: wgpu::Buffer,
    num_instances: u32,
    pub camera: Camera,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    material_uniform: MaterialUniform,
    material_buffer: wgpu::Buffer,
    material_bind_group_layout: wgpu::BindGroupLayout,
    material_bind_group: wgpu::BindGroup,
    material_table_buffer: wgpu::Buffer,
    material_table_len: u32,
    /// Per-triangle material IDs for primitive_index lookup (GeomSubsets)
    triangle_material_buffer: wgpu::Buffer,
    /// Whether we have per-triangle materials (vs instance materials)
    has_triangle_materials: bool,
    gpu_textures: GpuTextureSet,
    texture_sampler: wgpu::Sampler,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group: wgpu::BindGroup,
    mesh_bounds_min: Vec3,
    mesh_bounds_max: Vec3,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,

    // Gnomon resources
    gnomon: GnomonRenderer,

    // egui state
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,

    // UI state
    pub show_ui: bool,
    pub fps: f32,
    frame_count: u32,
    fps_update_timer: f32,

    // Stats - TODO: Track polygon count from source data for accuracy
    num_triangles: u64,

    // UI layout metrics (for viewport-safe overlays)
    ui_left_panel_width: f32,
    ui_right_panel_width: f32,
    ui_bottom_panel_height: f32,

    // Ivar CPU path tracer state
    pub ivar_state: IvarState,
    ivar_texture: wgpu::Texture,
    ivar_texture_view: wgpu::TextureView,
    ivar_sampler: wgpu::Sampler,
    ivar_bind_group: wgpu::BindGroup,
    ivar_pipeline: wgpu::RenderPipeline,

    // Cached mesh data for Ivar scene building
    mesh_data: MeshData,

    // Instance transforms for Ivar (stored as Mat4 arrays)
    /// Base transforms (from scene load) - used for re-evaluation
    instance_transforms: Vec<Mat4>,
    /// Current transforms (after animation evaluation) - used for rendering
    current_transforms: Vec<Mat4>,
    instance_material_ids: Vec<u32>,
    /// Prototype ID for each instance (parallel to instance_transforms, for multi-draw rebuild)
    instance_prototype_ids: Vec<usize>,

    // Animation data for viewport playback
    /// Animated transforms for instances (parallel to instances)
    instance_animations: Vec<Option<bif_core::AnimatedTransform>>,
    /// Last evaluated frame (for change detection)
    last_evaluated_frame: f64,
    /// Mesh indices that have vertex animation (deformation)
    vertex_animated_meshes: Vec<usize>,

    // Material for Ivar rendering (from loaded USD scene)
    scene_material: bif_core::Material,
    // All scene materials for multi-material Ivar rendering
    scene_materials: Vec<std::sync::Arc<bif_core::Material>>,
    // Base directory for resolving texture paths
    texture_base_dir: Option<std::path::PathBuf>,

    // Multi-draw state for per-prototype rendering
    /// Per-prototype GPU buffers (vertex/index) for multi-draw rendering
    prototype_gpu_data: Vec<PrototypeGpuData>,
    /// Instances grouped by prototype ID for multi-draw
    instance_groups: HashMap<usize, Vec<InstanceData>>,
    /// Whether using multi-draw mode (multiple prototypes) vs single buffer
    use_multi_draw: bool,

    // Frustum culling for GPU instancing optimization
    /// Maximum instances the buffer can hold (preallocated)
    #[allow(dead_code)]
    max_instances: u32,
    /// Precomputed world-space AABBs for each instance (for frustum culling)
    instance_aabbs: Vec<Aabb>,
    /// Local-space AABB of the prototype mesh
    prototype_aabb: Aabb,
    /// Number of visible instances after frustum culling (updated per frame)
    visible_instance_count: u32,
    /// LOD distance threshold - instances beyond this use box proxy (legacy, kept for fallback)
    #[allow(dead_code)]
    lod_distance_threshold: f32,
    /// Maximum polygon budget before LOD kicks in (user-adjustable)
    pub lod_max_polys: u32,
    /// Triangles per instance (for polygon budget calculation)
    triangles_per_instance: u32,

    // Box LOD proxy for distant instances
    /// Box proxy vertex buffer (generated from prototype AABB)
    lod_box_vertex_buffer: wgpu::Buffer,
    /// Box proxy index buffer
    lod_box_index_buffer: wgpu::Buffer,
    /// Number of indices in box proxy mesh (36 = 12 triangles)
    lod_box_num_indices: u32,
    /// Number of instances rendered as box proxies (stored after near instances in instance_buffer)
    lod_box_instance_count: u32,
    /// Pre-allocated scratch buffers for frustum culling (avoids per-frame allocations)
    culling_scratch: CullingScratch,
    /// Cached frustum (recomputed only when camera changes)
    cached_frustum: Frustum,
    /// Camera snapshot for frustum cache invalidation
    frustum_camera_snapshot: CameraSnapshot,

    // Scene browser state
    pub scene_browser_state: SceneBrowserState,

    // Currently selected prim path (synced with scene browser)
    pub selected_prim_path: Option<String>,

    // Properties for the selected prim (computed when selection changes)
    pub selected_prim_properties: Option<PrimProperties>,

    // USD stage for scene browser hierarchy (None if loaded via pure Rust parser)
    // Wrapped in Arc for sharing with batch render thread
    usd_stage: Option<Arc<UsdStage>>,

    // Node graph state for scene assembly
    pub node_graph_state: NodeGraphState,

    // Timeline state for animation playback
    pub timeline_state: TimelineState,

    // Environment IBL state
    pub gpu_environment: GpuEnvironment,
    /// Whether to render the skybox background
    pub show_background: bool,
    // Skybox rendering
    skybox_pipeline: wgpu::RenderPipeline,
    skybox_bind_group: wgpu::BindGroup,

    // Lights state (UsdLux)
    lights: LightsManager,

    // Async IBL generation
    ibl_receiver: Option<mpsc::Receiver<IblResult>>,
    // Async .tx conversion result
    tx_conversion_receiver: Option<mpsc::Receiver<String>>,
    // GPU compute IBL pipelines
    compute_ibl: compute_ibl::ComputeIbl,

    // Batch render state
    batch_receiver: Option<mpsc::Receiver<BatchMessage>>,
    batch_cancel_flag: Option<Arc<AtomicBool>>,

    // Viewport camera selection state
    /// Active camera source for viewport (separate from batch render settings)
    viewport_camera_source: CameraSource,
    /// Lock camera controls when USD camera active
    camera_locked: bool,
    /// Selected USD camera path (for animation during playback)
    selected_usd_camera: Option<String>,
}

impl Renderer {
    /// Upload Ivar image buffer to GPU texture, using selected AOV channel.
    fn upload_ivar_pixels(&self) {
        let Some(ref image) = self.ivar_state.image_buffer else {
            return;
        };
        let width = image.width;
        let height = image.height;

        // Generate RGBA bytes based on selected AOV channel
        let rgba = match self.ivar_state.preview_aov {
            ivar_state::AovChannel::Beauty => image.to_rgba(),
            ivar_state::AovChannel::Alpha => {
                // Alpha as grayscale
                if let Some(ref alpha) = self.ivar_state.alpha_buffer {
                    alpha
                        .iter()
                        .flat_map(|&a| {
                            let v = (a.clamp(0.0, 1.0) * 255.0) as u8;
                            [v, v, v, 255]
                        })
                        .collect()
                } else {
                    image.to_rgba()
                }
            }
            ivar_state::AovChannel::Depth => {
                // Depth normalized to [0, 1] as grayscale
                if let Some(ref depth) = self.ivar_state.depth_buffer {
                    let depth_near = self.ivar_state.batch_settings.aov_settings.depth_near;
                    let depth_far = self.ivar_state.batch_settings.aov_settings.depth_far;
                    let range = depth_far - depth_near;
                    depth
                        .iter()
                        .flat_map(|&d| {
                            let normalized = if d >= f32::INFINITY || range <= 0.0 {
                                0.0
                            } else {
                                ((d - depth_near) / range).clamp(0.0, 1.0)
                            };
                            let v = (normalized * 255.0) as u8;
                            [v, v, v, 255]
                        })
                        .collect()
                } else {
                    image.to_rgba()
                }
            }
            ivar_state::AovChannel::Normal => {
                // Normal mapped from [-1,1] to [0,1] as RGB
                if let Some(ref normal) = self.ivar_state.normal_buffer {
                    normal
                        .iter()
                        .flat_map(|n| {
                            // Map [-1, 1] to [0, 255]
                            let r = ((n[0] * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
                            let g = ((n[1] * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
                            let b = ((n[2] * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
                            [r, g, b, 255]
                        })
                        .collect()
                } else {
                    image.to_rgba()
                }
            }
        };

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.ivar_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

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
            present_mode: wgpu::PresentMode::Mailbox, // VSync
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

        // Create environment IBL resources (fallback black cubemaps)
        let gpu_environment = GpuEnvironment::new_default(&device, &queue);

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
                &gpu_environment.bind_group_layout,
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

        // Preallocate instance buffer for up to MAX_INSTANCES (10K)
        // Uses COPY_DST for dynamic per-frame updates during frustum culling
        const MAX_INSTANCES: u32 = 10_000;
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

        // Create LOD box proxy buffers (for distant instances)
        // Start with a unit cube, will be regenerated when mesh loads
        let unit_aabb = Aabb::from_points(Vec3::ZERO, Vec3::ONE);
        let lod_box_mesh = MeshData::from_aabb(&unit_aabb);

        let lod_box_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Box Vertex Buffer"),
            contents: bytemuck::cast_slice(&lod_box_mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let lod_box_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Box Index Buffer"),
            contents: bytemuck::cast_slice(&lod_box_mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        log::info!("Created LOD box proxy buffers");

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
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let (ivar_pipeline, ivar_bind_group, _) = ivar_renderer::create_ivar_pipeline(
            &device,
            config.format,
            &ivar_texture_view,
            &ivar_sampler,
        );

        log::info!("Ivar resources initialized");

        // Skybox pipeline
        let skybox_bind_group_layout = skybox::create_skybox_bind_group_layout(&device);
        let skybox_pipeline = skybox::create_skybox_pipeline(
            &device,
            &camera_bind_group_layout,
            &skybox_bind_group_layout,
            config.format,
        );
        let skybox_bind_group = skybox::create_skybox_bind_group(
            &device,
            &skybox_bind_group_layout,
            &gpu_environment.prefiltered_view,
            &gpu_environment.sampler,
            &gpu_environment.params_buffer,
        );

        let compute_ibl = compute_ibl::ComputeIbl::new(&device);

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
            ui_bottom_panel_height: 0.0,
            ivar_state: IvarState::default(),
            ivar_texture,
            ivar_texture_view,
            ivar_sampler,
            ivar_bind_group,
            ivar_pipeline,
            mesh_data,
            instance_transforms: vec![], // Empty scene - no instances
            current_transforms: vec![],
            instance_material_ids: vec![],
            instance_prototype_ids: vec![],
            instance_animations: vec![],
            last_evaluated_frame: 0.0,
            vertex_animated_meshes: vec![],
            scene_material: bif_core::Material::default(),
            scene_materials: vec![],
            texture_base_dir: None,
            prototype_gpu_data: vec![],
            instance_groups: HashMap::new(),
            use_multi_draw: false,
            max_instances: MAX_INSTANCES,
            instance_aabbs: vec![],
            prototype_aabb: Aabb::empty(),
            visible_instance_count: 0,
            lod_distance_threshold: 100.0, // Default LOD threshold
            lod_max_polys: 5_000_000,      // 5M poly budget default
            triangles_per_instance: 0,     // Empty scene
            lod_box_vertex_buffer,
            lod_box_index_buffer,
            lod_box_num_indices: lod_box_mesh.indices.len() as u32,
            lod_box_instance_count: 0,
            culling_scratch: CullingScratch::new(MAX_INSTANCES as usize),
            cached_frustum: Frustum::from_view_projection(
                camera.projection_matrix() * camera.view_matrix(),
            ),
            frustum_camera_snapshot: CameraSnapshot::from_camera(&camera),
            scene_browser_state: SceneBrowserState::new(),
            selected_prim_path: None,
            selected_prim_properties: None,
            usd_stage: None,
            node_graph_state: NodeGraphState::new(),
            timeline_state: TimelineState::default(),
            gpu_environment,
            show_background: true,
            skybox_pipeline,
            skybox_bind_group,
            lights,
            ibl_receiver: None,
            tx_conversion_receiver: None,
            compute_ibl,
            batch_receiver: None,
            batch_cancel_flag: None,
            viewport_camera_source: CameraSource::Viewport,
            camera_locked: false,
            selected_usd_camera: None,
        })
    }

    /// Create a new renderer for the given window, loading a USD scene
    pub async fn new_with_scene(
        window: std::sync::Arc<winit::window::Window>,
        scene: &bif_core::Scene,
    ) -> Result<Self> {
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
            present_mode: wgpu::PresentMode::Mailbox,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        // Convert first prototype to MeshData (for now, use first prototype)
        if scene.prototypes.is_empty() {
            anyhow::bail!("Scene has no prototypes");
        }

        let proto = &scene.prototypes[0];
        let mesh_data = MeshData::from_core_mesh(&proto.mesh);

        // Get material from prototype (or use default)
        let scene_material = proto
            .material
            .as_ref()
            .map(|m| (**m).clone())
            .unwrap_or_default();
        log::info!(
            "Material: {} (metallic={:.2}, roughness={:.2})",
            scene_material.name,
            scene_material.metallic,
            scene_material.roughness
        );

        log::info!(
            "Loaded {} vertices, {} indices from USD scene",
            mesh_data.vertices.len(),
            mesh_data.indices.len()
        );

        log::info!(
            "Mesh bounds: min={:?}, max={:?}",
            mesh_data.bounds_min,
            mesh_data.bounds_max
        );
        log::info!(
            "Scene has {} prototypes, {} instances",
            scene.prototype_count(),
            scene.instance_count()
        );

        // Calculate proper camera distance to frame the scene
        // TODO: Add frame_scene() method that can be called to re-frame based on world_bounds
        let world_bounds = scene.world_bounds();
        log::info!(
            "World bounds: min=({:.1}, {:.1}, {:.1}), max=({:.1}, {:.1}, {:.1})",
            world_bounds.x.min,
            world_bounds.y.min,
            world_bounds.z.min,
            world_bounds.x.max,
            world_bounds.y.max,
            world_bounds.z.max
        );
        let mesh_center = Vec3::new(
            (world_bounds.x.min + world_bounds.x.max) * 0.5,
            (world_bounds.y.min + world_bounds.y.max) * 0.5,
            (world_bounds.z.min + world_bounds.z.max) * 0.5,
        );
        let world_extent = Vec3::new(
            world_bounds.x.max - world_bounds.x.min,
            world_bounds.y.max - world_bounds.y.min,
            world_bounds.z.max - world_bounds.z.min,
        );
        let mesh_size = world_extent.length();
        let camera_distance = mesh_size * 1.5;

        // Create camera positioned to view the scene
        let aspect = size.width as f32 / size.height as f32;
        let camera = Camera::new(
            mesh_center + Vec3::new(0.0, 0.0, camera_distance),
            mesh_center,
            aspect,
        );

        let mut camera = camera;
        camera.near = camera_distance * 0.01;
        camera.far = camera_distance * 20.0;

        log::info!(
            "Camera positioned at {:?}, looking at {:?}",
            camera.position,
            camera.target
        );

        // Create camera uniform buffer
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

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Create material uniform buffer (use scene material)
        let material_uniform = MaterialUniform::from_material(&scene_material);
        let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Material Buffer"),
            contents: bytemuck::cast_slice(&[material_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let gpu_textures =
            texture_loader::create_gpu_textures_for_scene(&device, &queue, scene, None);

        let material_table = if scene.materials.is_empty() {
            vec![MaterialGpu::from_material(
                &bif_core::Material::default(),
                &gpu_textures,
            )]
        } else {
            scene
                .materials
                .iter()
                .map(|mat| MaterialGpu::from_material(mat.as_ref(), &gpu_textures))
                .collect()
        };
        let material_table_len = material_table.len() as u32;
        let material_table_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Material Table Buffer"),
            contents: bytemuck::cast_slice(&material_table),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        // Create triangle material buffer from mesh data
        let (triangle_material_buffer, has_triangle_materials) =
            if let Some(ref tri_mats) = mesh_data.triangle_material_ids {
                log::info!(
                    "Creating triangle material buffer with {} entries",
                    tri_mats.len()
                );
                (
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(tri_mats),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    }),
                    true,
                )
            } else {
                (
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(&[0xFFFFFFFFu32]),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    }),
                    false,
                )
            };

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

        // Create environment IBL resources (fallback black cubemaps)
        let gpu_environment = GpuEnvironment::new_default(&device, &queue);

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
                &gpu_environment.bind_group_layout,
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

        // Create vertex and index buffers
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&mesh_data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&mesh_data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Create depth texture
        let (depth_texture, depth_view) =
            ivar_renderer::create_depth_texture(&device, (size.width, size.height));

        let material_index_by_name: HashMap<String, u32> = scene
            .materials
            .iter()
            .enumerate()
            .map(|(idx, mat)| (mat.name.clone(), idx as u32))
            .collect();

        // Generate instances from scene - collect both GPU data and transforms for Ivar
        let mut instance_transforms: Vec<Mat4> = Vec::with_capacity(scene.instance_count());
        let mut instance_material_ids: Vec<u32> = Vec::with_capacity(scene.instance_count());
        let mut instance_prototype_ids: Vec<usize> = Vec::with_capacity(scene.instance_count());
        let instances: Vec<InstanceData> = scene
            .instances()
            .iter()
            .enumerate()
            .map(|(i, inst)| {
                let model_matrix = inst.model_matrix();
                instance_transforms.push(model_matrix);
                instance_prototype_ids.push(inst.prototype_id);
                let material_id = scene
                    .prototypes
                    .get(inst.prototype_id)
                    .and_then(|proto| proto.material.as_ref())
                    .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                    .unwrap_or(0);
                instance_material_ids.push(material_id);
                // Debug: log first few instance transforms
                if i < 5 || i == scene.instance_count() - 1 {
                    let translation = model_matrix.w_axis.truncate();
                    log::info!("Instance {}: translation = {:?}", i, translation);
                }
                InstanceData {
                    model_matrix: model_matrix.to_cols_array_2d(),
                    material_id,
                }
            })
            .collect();

        // Preallocate dynamic instance buffer for frustum culling
        const MAX_INSTANCES: u32 = 10_000;

        // Warn if instance count exceeds buffer capacity
        if instances.len() > MAX_INSTANCES as usize {
            log::warn!(
                "Instance count {} exceeds buffer capacity {}. Some instances will be truncated.",
                instances.len(),
                MAX_INSTANCES
            );
        }

        // Compute prototype AABB and per-instance world-space AABBs for frustum culling
        let prototype_aabb = Aabb::from_points(mesh_data.bounds_min, mesh_data.bounds_max);
        let instance_aabbs: Vec<Aabb> = instance_transforms
            .iter()
            .map(|transform| transform.transform_aabb(&prototype_aabb))
            .collect();
        let instance_buffer_size = (MAX_INSTANCES as usize) * std::mem::size_of::<InstanceData>();
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer (Dynamic)"),
            size: instance_buffer_size as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Write initial instances
        queue.write_buffer(&instance_buffer, 0, bytemuck::cast_slice(&instances));

        log::info!(
            "Created {} instances from USD scene (buffer capacity: {})",
            instances.len(),
            MAX_INSTANCES
        );

        // Create LOD box proxy buffers (for distant instances)
        let lod_box_mesh = MeshData::from_aabb(&prototype_aabb);

        let lod_box_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Box Vertex Buffer"),
            contents: bytemuck::cast_slice(&lod_box_mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let lod_box_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Box Index Buffer"),
            contents: bytemuck::cast_slice(&lod_box_mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        log::info!(
            "Created LOD box proxy buffers (prototype AABB: {:?} to {:?})",
            prototype_aabb.min_point(),
            prototype_aabb.max_point()
        );

        // Initialize egui
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        let egui_renderer = egui_wgpu::Renderer::new(&device, config.format, None, 1, false);

        log::info!("Renderer initialized with USD scene");

        // Create gnomon renderer
        let gnomon = GnomonRenderer::new(&device, config.format);
        log::info!("Gnomon initialized");

        // Calculate stats - TODO: Track polygon count from source mesh for accuracy
        let num_triangles = mesh_data.indices.len() as u64 / 3;

        // Create Ivar resources for CPU path tracer display
        let (ivar_texture, ivar_texture_view) =
            ivar_renderer::create_ivar_texture(&device, (size.width, size.height));

        let ivar_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Ivar Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let (ivar_pipeline, ivar_bind_group, _) = ivar_renderer::create_ivar_pipeline(
            &device,
            config.format,
            &ivar_texture_view,
            &ivar_sampler,
        );

        log::info!("Ivar resources initialized");

        // Skybox pipeline
        let skybox_bind_group_layout = skybox::create_skybox_bind_group_layout(&device);
        let skybox_pipeline = skybox::create_skybox_pipeline(
            &device,
            &camera_bind_group_layout,
            &skybox_bind_group_layout,
            config.format,
        );
        let skybox_bind_group = skybox::create_skybox_bind_group(
            &device,
            &skybox_bind_group_layout,
            &gpu_environment.prefiltered_view,
            &gpu_environment.sampler,
            &gpu_environment.params_buffer,
        );

        let compute_ibl = compute_ibl::ComputeIbl::new(&device);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size: (size.width, size.height),
            pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: mesh_data.indices.len() as u32,
            instance_buffer,
            num_instances: instances.len() as u32,
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
            ui_bottom_panel_height: 0.0,
            ivar_state: IvarState::default(),
            ivar_texture,
            ivar_texture_view,
            ivar_sampler,
            ivar_bind_group,
            ivar_pipeline,
            mesh_data,
            current_transforms: instance_transforms.clone(),
            instance_transforms,
            instance_material_ids,
            instance_prototype_ids,
            instance_animations: scene.instance_animations().to_vec(),
            last_evaluated_frame: 0.0,
            vertex_animated_meshes: vec![], // Detected during reload_scene with stage
            scene_material,
            scene_materials: scene.materials.clone(),
            texture_base_dir: None,
            prototype_gpu_data: vec![],
            instance_groups: HashMap::new(),
            use_multi_draw: false,
            max_instances: MAX_INSTANCES,
            instance_aabbs,
            prototype_aabb,
            visible_instance_count: instances.len() as u32,
            lod_distance_threshold: 100.0,
            lod_max_polys: 5_000_000, // 5M poly budget default
            triangles_per_instance: num_triangles as u32,
            lod_box_vertex_buffer,
            lod_box_index_buffer,
            lod_box_num_indices: lod_box_mesh.indices.len() as u32,
            lod_box_instance_count: 0,
            culling_scratch: CullingScratch::new(MAX_INSTANCES as usize),
            cached_frustum: Frustum::from_view_projection(
                camera.projection_matrix() * camera.view_matrix(),
            ),
            frustum_camera_snapshot: CameraSnapshot::from_camera(&camera),
            scene_browser_state: SceneBrowserState::new(),
            selected_prim_path: None,
            selected_prim_properties: None,
            usd_stage: None,
            node_graph_state: NodeGraphState::new(),
            timeline_state: TimelineState::default(),
            gpu_environment,
            show_background: true,
            skybox_pipeline,
            skybox_bind_group,
            lights,
            ibl_receiver: None,
            tx_conversion_receiver: None,
            compute_ibl,
            batch_receiver: None,
            batch_cancel_flag: None,
            viewport_camera_source: CameraSource::Viewport,
            camera_locked: false,
            selected_usd_camera: None,
        })
    }

    /// Create a new renderer with scene AND USD stage for scene browser
    pub async fn new_with_scene_and_stage(
        window: std::sync::Arc<winit::window::Window>,
        scene: &bif_core::Scene,
        stage: UsdStage,
    ) -> Result<Self> {
        let mut renderer = Self::new_with_scene(window, scene).await?;
        renderer.usd_stage = Some(Arc::new(stage));
        Ok(renderer)
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

            // Recreate Ivar bind group with new texture view
            let (_, ivar_bind_group, _) = ivar_renderer::create_ivar_pipeline(
                &self.device,
                self.config.format,
                &self.ivar_texture_view,
                &self.ivar_sampler,
            );
            self.ivar_bind_group = ivar_bind_group;

            // Reset Ivar render state on resize
            self.ivar_state.cancel_flag.store(true, Ordering::Relaxed);
            self.ivar_state.image_buffer = None;
            self.ivar_state.render_complete = false;

            // Update camera aspect ratio
            let aspect = new_size.0 as f32 / new_size.1 as f32;
            self.camera.set_aspect(aspect);
            self.update_camera();
        }
    }

    /// Update camera uniform buffer (call after modifying camera)
    pub fn update_camera(&mut self) {
        self.camera_uniform.update_view_proj(&self.camera);
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
        self.gpu_environment.params.intensity = intensity;
        self.gpu_environment.params.rotation = rotation;
        self.show_background = show_background;
        self.gpu_environment.update_params(&self.queue);
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
        if self.instance_aabbs.is_empty() {
            self.visible_instance_count = self.num_instances;
            self.lod_box_instance_count = 0;
            return;
        }

        // Clear scratch buffers (reuse pre-allocated capacity)
        self.culling_scratch.clear();

        // Update cached frustum only when camera changes
        let current_snapshot = CameraSnapshot::from_camera(&self.camera);
        if current_snapshot.has_changed(&self.frustum_camera_snapshot) {
            let vp = self.camera.projection_matrix() * self.camera.view_matrix();
            self.cached_frustum = Frustum::from_view_projection(vp);
            self.frustum_camera_snapshot = current_snapshot;
        }

        let camera_pos = self.camera.position;

        // Collect visible instances with their distances
        for (idx, aabb) in self.instance_aabbs.iter().enumerate() {
            // Frustum culling first
            if !self.cached_frustum.intersects_aabb(aabb) {
                continue;
            }

            // Calculate distance for sorting
            let instance_center = aabb.center();
            let distance_sq = (instance_center - camera_pos).length_squared();
            self.culling_scratch
                .visible_with_distance
                .push((distance_sq, idx));
        }

        // Calculate how many instances fit in polygon budget
        let tris_per_instance = self.triangles_per_instance as u64;
        let max_polys = self.lod_max_polys as u64;
        let budget_count = if tris_per_instance > 0 {
            (max_polys / tris_per_instance) as usize
        } else {
            self.culling_scratch.visible_with_distance.len()
        };

        let visible_count = self.culling_scratch.visible_with_distance.len();

        // Partition: O(n) instead of O(n log n) full sort
        // After this, indices 0..budget_count are the nearest (unordered among themselves)
        if budget_count > 0 && budget_count < visible_count {
            self.culling_scratch
                .visible_with_distance
                .select_nth_unstable_by(budget_count, |a, b| {
                    a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                });
        }

        // Split into near (full mesh) and far (box proxy)
        let split_point = budget_count.min(visible_count);

        for &(_distance_sq, idx) in &self.culling_scratch.visible_with_distance[..split_point] {
            let transform = &self.current_transforms[idx];
            let material_id = self.instance_material_ids.get(idx).copied().unwrap_or(0);
            self.culling_scratch.near_instances.push(InstanceData {
                model_matrix: transform.to_cols_array_2d(),
                material_id,
            });
        }

        for &(_distance_sq, idx) in &self.culling_scratch.visible_with_distance[split_point..] {
            let transform = &self.current_transforms[idx];
            let material_id = self.instance_material_ids.get(idx).copied().unwrap_or(0);
            self.culling_scratch.far_instances.push(InstanceData {
                model_matrix: transform.to_cols_array_2d(),
                material_id,
            });
        }

        // Update GPU buffer: [near_instances... | far_instances...] in single contiguous write
        let near_count = self.culling_scratch.near_instances.len();
        let far_count = self.culling_scratch.far_instances.len();

        if near_count > 0 || far_count > 0 {
            // Write near instances at offset 0
            if near_count > 0 {
                self.queue.write_buffer(
                    &self.instance_buffer,
                    0,
                    bytemuck::cast_slice(&self.culling_scratch.near_instances),
                );
            }
            // Write far instances immediately after near instances
            if far_count > 0 {
                let far_offset = (near_count * std::mem::size_of::<InstanceData>()) as u64;
                self.queue.write_buffer(
                    &self.instance_buffer,
                    far_offset,
                    bytemuck::cast_slice(&self.culling_scratch.far_instances),
                );
            }
        }

        self.visible_instance_count = self.culling_scratch.near_instances.len() as u32;
        self.lod_box_instance_count = self.culling_scratch.far_instances.len() as u32;

        log::trace!(
            "LOD split: {} near (full mesh), {} far (box LOD), {}/{} total visible",
            self.visible_instance_count,
            self.lod_box_instance_count,
            self.visible_instance_count + self.lod_box_instance_count,
            self.num_instances
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

    /// Load a USD scene file and update the viewport
    ///
    /// This method reloads the viewport with a new USD file:
    /// 1. Loads the USD file via the C++ bridge
    /// 2. Converts geometry to GPU-ready buffers
    /// 3. Updates the scene browser with the new hierarchy
    /// 4. Invalidates the Ivar cache for re-rendering
    pub fn load_usd_scene<P: AsRef<std::path::Path>>(&mut self, path: P) -> Result<()> {
        use bif_core::usd::load_usd_with_stage;
        use std::sync::atomic::Ordering;
        use std::time::Instant;

        let path = path.as_ref();
        log::info!("Loading USD scene: {:?}", path);
        let viewport_load_start = Instant::now();

        // Check if file exists
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {:?}", path));
        }

        // Load USD file via C++ bridge (handles usda, usdc, usd)
        let (scene, stage) = load_usd_with_stage(path).map_err(|e| {
            log::error!("USD bridge error: {:?}", e);
            log::error!("Hint: Ensure USD environment is set up. Run: . .\\setup_usd_env.ps1");
            anyhow::anyhow!("Failed to load USD: {}", e)
        })?;

        if scene.prototypes.is_empty() {
            return Err(anyhow::anyhow!("Scene has no geometry"));
        }

        // Multi-draw architecture: create per-prototype GPU buffers
        // This allows proper instancing without baking transforms into vertices
        let use_multi_draw = scene.prototypes.len() > 1;

        if use_multi_draw {
            log::info!(
                "Scene has {} prototypes - using multi-draw with per-prototype buffers",
                scene.prototypes.len()
            );
        }

        // Create per-prototype GPU data
        let gpu_start = Instant::now();
        let prototype_gpu_data: Vec<PrototypeGpuData> = scene
            .prototypes
            .iter()
            .enumerate()
            .map(|(proto_id, proto)| {
                let mesh_data = MeshData::from_core_mesh(&proto.mesh);

                let vertex_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&format!("Prototype {} Vertex Buffer", proto_id)),
                            contents: bytemuck::cast_slice(&mesh_data.vertices),
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        });

                let index_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&format!("Prototype {} Index Buffer", proto_id)),
                            contents: bytemuck::cast_slice(&mesh_data.indices),
                            usage: wgpu::BufferUsages::INDEX,
                        });

                // Per-triangle material buffer if present
                let triangle_material_buffer =
                    mesh_data.triangle_material_ids.as_ref().map(|tri_mats| {
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some(&format!(
                                    "Prototype {} Triangle Material Buffer",
                                    proto_id
                                )),
                                contents: bytemuck::cast_slice(tri_mats),
                                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                            })
                    });

                log::debug!(
                    "Prototype {}: {} vertices, {} indices",
                    proto_id,
                    mesh_data.vertices.len(),
                    mesh_data.indices.len()
                );

                PrototypeGpuData {
                    vertex_buffer,
                    index_buffer,
                    num_indices: mesh_data.indices.len() as u32,
                    num_vertices: mesh_data.vertices.len() as u32,
                    prototype_id: proto_id,
                    mesh_idx: proto_id,
                    triangle_material_buffer,
                    vertices: mesh_data.vertices.clone(),
                }
            })
            .collect();

        // For backwards compatibility, also create a combined mesh_data for single-draw fallback
        // and for Ivar rendering (which expects a single mesh)
        let mesh_data = if scene.prototypes.len() == 1 {
            MeshData::from_core_mesh(&scene.prototypes[0].mesh)
        } else if !scene.instances().is_empty() {
            // Instanced scene: combine prototypes with instance transforms
            let mut meshes_with_transforms: Vec<(&bif_core::Mesh, Mat4, usize)> = Vec::new();
            for (mesh_idx, inst) in scene.instances().iter().enumerate() {
                if let Some(proto) = scene.prototypes.get(inst.prototype_id) {
                    meshes_with_transforms.push((&proto.mesh, inst.model_matrix(), mesh_idx));
                }
            }
            MeshData::combine_with_transforms(&meshes_with_transforms)
        } else {
            // Direct meshes (no instancers): combine prototypes with identity transforms
            let meshes_with_transforms: Vec<(&bif_core::Mesh, Mat4, usize)> = scene
                .prototypes
                .iter()
                .enumerate()
                .map(|(idx, proto)| (proto.mesh.as_ref(), Mat4::IDENTITY, idx))
                .collect();
            MeshData::combine_with_transforms(&meshes_with_transforms)
        };

        // Get material from first prototype (or use default)
        let scene_material = scene
            .prototypes
            .first()
            .and_then(|p| p.material.as_ref())
            .map(|m| (**m).clone())
            .unwrap_or_default();
        log::info!(
            "Material: {} (metallic={:.2}, roughness={:.2})",
            scene_material.name,
            scene_material.metallic,
            scene_material.roughness
        );

        log::info!(
            "Loaded {} vertices, {} indices from USD scene",
            mesh_data.vertices.len(),
            mesh_data.indices.len()
        );
        let gpu_time = gpu_start.elapsed();

        // Refresh texture resources and material table for the new scene
        let texture_start = Instant::now();
        let base_dir = path.parent();
        self.gpu_textures = texture_loader::create_gpu_textures_for_scene(
            &self.device,
            &self.queue,
            &scene,
            base_dir,
        );
        let texture_time = texture_start.elapsed();
        let texture_count = self.gpu_textures.textures.len();

        let material_table = if scene.materials.is_empty() {
            vec![MaterialGpu::from_material(
                &bif_core::Material::default(),
                &self.gpu_textures,
            )]
        } else {
            scene
                .materials
                .iter()
                .map(|mat| MaterialGpu::from_material(mat.as_ref(), &self.gpu_textures))
                .collect()
        };
        self.material_table_len = material_table.len() as u32;
        self.material_table_buffer =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Material Table Buffer"),
                    contents: bytemuck::cast_slice(&material_table),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });

        // Create triangle material buffer from mesh data
        if let Some(ref tri_mats) = mesh_data.triangle_material_ids {
            log::info!(
                "Creating triangle material buffer with {} entries",
                tri_mats.len()
            );
            self.triangle_material_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(tri_mats),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
            self.has_triangle_materials = true;
        } else {
            self.triangle_material_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(&[0xFFFFFFFFu32]),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
            self.has_triangle_materials = false;
        }

        self.material_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Bind Group"),
            layout: &self.material_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.material_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.material_table_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.triangle_material_buffer.as_entire_binding(),
                },
            ],
        });

        let texture_view_refs: Vec<&wgpu::TextureView> = self.gpu_textures.views.iter().collect();
        self.texture_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&texture_view_refs),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.texture_sampler),
                },
            ],
        });

        // Create new vertex buffer (COPY_DST needed for vertex animation updates)
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(&mesh_data.vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        // Create new index buffer
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(&mesh_data.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        let material_index_by_name: HashMap<String, u32> = scene
            .materials
            .iter()
            .enumerate()
            .map(|(idx, mat)| (mat.name.clone(), idx as u32))
            .collect();

        // Generate instances from scene
        // Multi-draw (wgpu viewport) uses per-instance transforms
        // For Ivar (ray tracing): if multi-prototype, transforms are baked into combined mesh_data
        // so build_ivar_scene* will use identity transform when use_multi_draw is true
        let mut instance_transforms = Vec::with_capacity(scene.instance_count());
        let mut instance_material_ids = Vec::with_capacity(scene.instance_count());
        let mut instance_prototype_ids = Vec::with_capacity(scene.instance_count());
        let instances: Vec<InstanceData> = scene
            .instances()
            .iter()
            .map(|inst| {
                let model_matrix = inst.model_matrix();
                instance_transforms.push(model_matrix);
                instance_prototype_ids.push(inst.prototype_id);
                let material_id = scene
                    .prototypes
                    .get(inst.prototype_id)
                    .and_then(|proto| proto.material.as_ref())
                    .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                    .unwrap_or(0);
                instance_material_ids.push(material_id);
                InstanceData {
                    model_matrix: model_matrix.to_cols_array_2d(),
                    material_id,
                }
            })
            .collect();

        // Warn if instance count exceeds buffer capacity
        if instances.len() > self.max_instances as usize {
            log::warn!(
                "Instance count {} exceeds buffer capacity {}. Some instances will be truncated.",
                instances.len(),
                self.max_instances
            );
        }

        // Compute prototype AABB and per-instance world-space AABBs for frustum culling
        let prototype_aabb = Aabb::from_points(mesh_data.bounds_min, mesh_data.bounds_max);
        let instance_aabbs: Vec<Aabb> = instance_transforms
            .iter()
            .map(|transform| transform.transform_aabb(&prototype_aabb))
            .collect();

        // Write instances to dynamic buffer (reuse existing preallocated buffer)
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));

        log::info!("Created {} instances from USD scene", instances.len());

        self.instance_material_ids = instance_material_ids;
        self.instance_prototype_ids = instance_prototype_ids;

        // Store animation data for viewport playback
        self.instance_animations = scene.instance_animations().to_vec();
        self.last_evaluated_frame = 0.0;

        let animated_count = self
            .instance_animations
            .iter()
            .filter(|opt| opt.as_ref().is_some_and(|anim| anim.is_animated()))
            .count();
        if animated_count > 0 {
            log::info!(
                "{} of {} instances have animation data",
                animated_count,
                instances.len()
            );
        }

        // Detect meshes with vertex animation (deformation)
        self.vertex_animated_meshes.clear();
        let mesh_count = stage.mesh_count().unwrap_or(0);
        for mesh_idx in 0..mesh_count {
            if let Ok(times) = stage.get_mesh_vertex_animation_times(mesh_idx) {
                if !times.is_empty() {
                    log::info!(
                        "Mesh {} has vertex animation ({} time samples)",
                        mesh_idx,
                        times.len()
                    );
                    self.vertex_animated_meshes.push(mesh_idx);
                }
            }
        }

        // Calculate world bounds for camera framing
        let world_bounds = scene.world_bounds();
        let mesh_center = Vec3::new(
            (world_bounds.x.min + world_bounds.x.max) * 0.5,
            (world_bounds.y.min + world_bounds.y.max) * 0.5,
            (world_bounds.z.min + world_bounds.z.max) * 0.5,
        );
        let world_extent = Vec3::new(
            world_bounds.x.max - world_bounds.x.min,
            world_bounds.y.max - world_bounds.y.min,
            world_bounds.z.max - world_bounds.z.min,
        );
        let mesh_size = world_extent.length();
        let camera_distance = mesh_size * 1.5;

        // Update renderer state
        self.vertex_buffer = vertex_buffer;
        self.index_buffer = index_buffer;
        self.num_indices = mesh_data.indices.len() as u32;
        // Note: instance_buffer is reused (dynamic), don't reassign
        self.num_instances = instances.len() as u32;
        self.visible_instance_count = instances.len() as u32;
        self.mesh_bounds_min = mesh_data.bounds_min;
        self.mesh_bounds_max = mesh_data.bounds_max;
        self.mesh_data = mesh_data;
        self.current_transforms = instance_transforms.clone();
        self.instance_transforms = instance_transforms;
        self.scene_material = scene_material.clone();
        self.scene_materials = scene.materials.clone();
        self.texture_base_dir = path.parent().map(|p| p.to_path_buf());

        // Store multi-draw state
        self.prototype_gpu_data = prototype_gpu_data;
        self.use_multi_draw = use_multi_draw;

        // Group instances by prototype for multi-draw rendering
        let mut instance_groups: HashMap<usize, Vec<InstanceData>> = HashMap::new();
        for inst in scene.instances() {
            let material_id = scene
                .prototypes
                .get(inst.prototype_id)
                .and_then(|proto| proto.material.as_ref())
                .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                .unwrap_or(0);

            instance_groups
                .entry(inst.prototype_id)
                .or_default()
                .push(InstanceData {
                    model_matrix: inst.model_matrix().to_cols_array_2d(),
                    material_id,
                });
        }
        self.instance_groups = instance_groups;

        if use_multi_draw {
            log::info!(
                "Multi-draw: {} prototypes, {} total instances across groups",
                self.prototype_gpu_data.len(),
                self.instance_groups
                    .values()
                    .map(|v| v.len())
                    .sum::<usize>()
            );
        }

        // Update material uniform buffer for viewport PBR
        self.material_uniform = MaterialUniform::from_material(&scene_material);
        self.queue.write_buffer(
            &self.material_buffer,
            0,
            bytemuck::cast_slice(&[self.material_uniform]),
        );

        self.instance_aabbs = instance_aabbs;
        self.prototype_aabb = prototype_aabb;
        self.triangles_per_instance = self.num_indices / 3;
        self.num_triangles = self.triangles_per_instance as u64 * self.num_instances as u64;
        self.lod_box_instance_count = 0;

        // Regenerate LOD box mesh for new prototype AABB
        let lod_box_mesh = MeshData::from_aabb(&prototype_aabb);
        self.lod_box_vertex_buffer =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("LOD Box Vertex Buffer"),
                    contents: bytemuck::cast_slice(&lod_box_mesh.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
        self.lod_box_index_buffer =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("LOD Box Index Buffer"),
                    contents: bytemuck::cast_slice(&lod_box_mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
        self.lod_box_num_indices = lod_box_mesh.indices.len() as u32;
        log::info!(
            "Regenerated LOD box mesh for prototype AABB: {:?} to {:?}",
            prototype_aabb.min_point(),
            prototype_aabb.max_point()
        );

        // Update USD stage for scene browser (wrapped in Arc for batch render sharing)
        let stage = Arc::new(stage);

        // Log available cameras for batch render
        match stage.camera_paths() {
            Ok(paths) if !paths.is_empty() => {
                log::info!("Found {} USD camera(s): {:?}", paths.len(), paths);
            }
            Ok(_) => {
                log::info!("No USD cameras found in scene");
            }
            Err(e) => {
                log::warn!("Failed to query cameras: {:?}", e);
            }
        }

        self.usd_stage = Some(stage);

        // Reset scene browser selection
        self.selected_prim_path = None;
        self.selected_prim_properties = None;

        // Update camera to frame the scene
        self.camera.target = mesh_center;
        self.camera.distance = camera_distance;
        self.camera.near = camera_distance * 0.01;
        self.camera.far = camera_distance * 20.0;
        self.camera.update_position_from_angles();
        self.update_camera();

        // Invalidate Ivar scene cache
        self.ivar_state.world = None;
        self.ivar_state.build_status = BuildStatus::NotStarted;
        self.ivar_state.cancel_flag.store(true, Ordering::Relaxed);
        self.ivar_state.render_complete = false;

        // Initialize timeline from scene data
        if let Some(ref timeline) = scene.timeline {
            self.timeline_state.set_from_scene(
                timeline.start_frame,
                timeline.end_frame,
                timeline.fps,
            );
            log::info!(
                "Timeline initialized: frames {:.0}-{:.0} @ {:.0} fps",
                timeline.start_frame,
                timeline.end_frame,
                timeline.fps
            );
        } else if !self.vertex_animated_meshes.is_empty() {
            // No scene timeline, but we have vertex animation - detect time range from vertex animation
            let mut min_time = f64::MAX;
            let mut max_time = f64::MIN;

            if let Some(ref usd_stage) = self.usd_stage {
                for &mesh_idx in &self.vertex_animated_meshes {
                    if let Ok(times) = usd_stage.get_mesh_vertex_animation_times(mesh_idx) {
                        for &t in &times {
                            min_time = min_time.min(t);
                            max_time = max_time.max(t);
                        }
                    }
                }
            }

            if min_time < max_time {
                // Use USD default 24 fps if not specified
                let fps = 24.0;
                self.timeline_state.set_from_scene(min_time, max_time, fps);
                log::info!(
                    "Timeline initialized from vertex animation: frames {:.0}-{:.0} @ {:.0} fps",
                    min_time,
                    max_time,
                    fps
                );
            } else {
                self.timeline_state = TimelineState::default();
            }
        } else {
            self.timeline_state = TimelineState::default();
        }

        log::info!(
            "USD scene loaded successfully: {} triangles x {} instances",
            self.num_indices / 3,
            self.num_instances
        );

        // Update lights from scene
        self.update_lights(&scene.lights);

        // Log viewport timing breakdown
        let total_viewport_time = viewport_load_start.elapsed();
        log::info!("Viewport Setup:");
        log::info!("  GPU buffers: {:>7.1}ms", gpu_time.as_secs_f64() * 1000.0);
        log::info!(
            "  Textures:    {:>7.1}ms ({} textures)",
            texture_time.as_secs_f64() * 1000.0,
            texture_count
        );
        log::info!(
            "  Total:       {:>7.1}ms",
            total_viewport_time.as_secs_f64() * 1000.0
        );

        Ok(())
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

    /// Update timeline animation (call each frame with delta_time).
    ///
    /// Advances the timeline if playing and updates instance transforms.
    pub fn update_animation(&mut self, delta_time: f32) {
        // Advance timeline if playing
        self.timeline_state.advance(delta_time);

        // Check if frame changed enough to warrant re-evaluation
        // Use larger tolerance (0.5 frame) to avoid excessive updates from rapid redraws
        let current_frame = self.timeline_state.current_frame;
        let frame_tolerance = 0.5;
        let frame_diff = (current_frame - self.last_evaluated_frame).abs();
        if frame_diff < frame_tolerance {
            return; // Not enough change yet
        }

        // Get effective frame (snapped to integer if enabled)
        let eval_frame = self.timeline_state.effective_frame();

        // Sync USD camera if selected (animates camera during playback)
        // Do this BEFORE checking for mesh animations - camera can animate alone
        if let Some(camera_path) = self.selected_usd_camera.clone() {
            self.sync_viewport_to_usd_camera(&camera_path);
        }

        // Check if we have any mesh animations (transform or vertex)
        let animated_count = self
            .instance_animations
            .iter()
            .filter(|opt| opt.as_ref().is_some_and(|anim| anim.is_animated()))
            .count();
        let has_transform_animations = animated_count > 0;
        let has_vertex_animations = !self.vertex_animated_meshes.is_empty();
        let has_mesh_animations = has_transform_animations || has_vertex_animations;

        if !has_mesh_animations {
            self.last_evaluated_frame = current_frame;
            return;
        }

        // Evaluate transforms and update GPU buffer
        log::debug!("Evaluating animation at frame {:.1}", eval_frame);
        if has_transform_animations {
            self.evaluate_animation_frame(eval_frame);
        }

        // Update vertex buffer for meshes with vertex animation
        if has_vertex_animations {
            self.update_vertex_animation(eval_frame);
            // Only invalidate Ivar cache when in Ivar mode (avoid overhead during viewport playback)
            if self.ivar_state.mode == RenderMode::Ivar {
                self.invalidate_ivar_scene();
            }
        }

        self.last_evaluated_frame = current_frame;
    }

    /// Evaluate all animated transforms at the given frame and update GPU buffer.
    fn evaluate_animation_frame(&mut self, frame: f64) {
        // Build updated instances
        let mut instances: Vec<InstanceData> = Vec::with_capacity(self.instance_transforms.len());
        let mut updated_transforms: Vec<Mat4> = Vec::with_capacity(self.instance_transforms.len());

        for (i, (base_transform, anim)) in self
            .instance_transforms
            .iter()
            .zip(self.instance_animations.iter())
            .enumerate()
        {
            let model_matrix = if let Some(anim) = anim {
                // Evaluate animated transform
                let evaluated = anim.evaluate(frame);
                let mat = evaluated.to_matrix();
                // Debug: show first animated instance's transform
                if i == 0 && frame as i32 % 12 == 0 {
                    log::debug!(
                        "Instance {} at frame {}: pos=({:.2}, {:.2}, {:.2})",
                        i,
                        frame,
                        evaluated.translation.x,
                        evaluated.translation.y,
                        evaluated.translation.z
                    );
                }
                mat
            } else {
                // Use static transform
                *base_transform
            };

            updated_transforms.push(model_matrix);

            let material_id = self.instance_material_ids.get(i).copied().unwrap_or(0);
            instances.push(InstanceData {
                model_matrix: model_matrix.to_cols_array_2d(),
                material_id,
            });
        }

        // Store evaluated transforms for use by update_visible_instances
        // base transforms stay in instance_transforms for re-evaluation
        self.current_transforms = updated_transforms;

        // Recompute instance AABBs for frustum culling
        let prototype_aabb = self.prototype_aabb;
        self.instance_aabbs = self
            .current_transforms
            .iter()
            .map(|t| t.transform_aabb(&prototype_aabb))
            .collect();

        // Invalidate frustum cache
        self.frustum_camera_snapshot = CameraSnapshot::default();

        // Rebuild instance_groups with animated transforms for multi-draw rendering
        if self.use_multi_draw {
            self.instance_groups.clear();
            for (i, model_matrix) in self.current_transforms.iter().enumerate() {
                let prototype_id = self.instance_prototype_ids.get(i).copied().unwrap_or(0);
                let material_id = self.instance_material_ids.get(i).copied().unwrap_or(0);

                self.instance_groups
                    .entry(prototype_id)
                    .or_default()
                    .push(InstanceData {
                        model_matrix: model_matrix.to_cols_array_2d(),
                        material_id,
                    });
            }
        }
    }

    /// Update vertex buffer for meshes with vertex animation (deformation).
    fn update_vertex_animation(&mut self, frame: f64) {
        let stage = match &self.usd_stage {
            Some(s) => s,
            None => return,
        };

        // Multi-draw mode: update per-prototype vertex buffers directly
        // This must come BEFORE mesh_ranges check because multi-draw renders from
        // prototype_gpu_data buffers, not the combined self.vertex_buffer
        if self.use_multi_draw {
            log::debug!(
                "update_vertex_animation: use_multi_draw=true, vertex_animated_meshes={:?}",
                self.vertex_animated_meshes
            );
            for &mesh_idx in &self.vertex_animated_meshes {
                log::debug!(
                    "Looking for prototype with mesh_idx={}, available: {:?}",
                    mesh_idx,
                    self.prototype_gpu_data
                        .iter()
                        .map(|p| p.mesh_idx)
                        .collect::<Vec<_>>()
                );
                // Find the prototype GPU data for this mesh
                let proto_idx = match self
                    .prototype_gpu_data
                    .iter()
                    .position(|p| p.mesh_idx == mesh_idx)
                {
                    Some(idx) => idx,
                    None => {
                        log::warn!(
                            "No prototype found for vertex-animated mesh_idx={}",
                            mesh_idx
                        );
                        continue;
                    }
                };

                let positions = match stage.get_mesh_vertices_at_time(mesh_idx, frame) {
                    Ok(p) => p,
                    Err(e) => {
                        log::warn!(
                            "Failed to get vertices at time {} for mesh {}: {:?}",
                            frame,
                            mesh_idx,
                            e
                        );
                        continue;
                    }
                };

                let vertex_count = positions.len() / 3;
                let proto_data = &mut self.prototype_gpu_data[proto_idx];

                log::debug!(
                    "Got {} vertices for mesh {} at frame {}, proto has {} vertices",
                    vertex_count,
                    mesh_idx,
                    frame,
                    proto_data.num_vertices
                );

                if vertex_count != proto_data.num_vertices as usize {
                    log::warn!(
                        "Vertex count mismatch for mesh {}: USD {} vs proto {}",
                        mesh_idx,
                        vertex_count,
                        proto_data.num_vertices
                    );
                    continue;
                }

                // Update positions while preserving original normals/UVs
                for (i, vertex) in proto_data.vertices.iter_mut().enumerate() {
                    vertex.position =
                        [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
                }

                log::debug!(
                    "Writing {} vertices to prototype {} buffer",
                    vertex_count,
                    proto_idx
                );
                self.queue.write_buffer(
                    &proto_data.vertex_buffer,
                    0,
                    bytemuck::cast_slice(&proto_data.vertices),
                );
            }
            return;
        }

        // Multi-mesh combined buffer: use mesh_ranges to update correct vertex range
        // (only used when NOT in multi-draw mode)
        if let Some(ref ranges) = self.mesh_data.mesh_ranges {
            let mut updated_any = false;

            for &mesh_idx in &self.vertex_animated_meshes {
                let range = match ranges.iter().find(|r| r.usd_mesh_index == mesh_idx) {
                    Some(r) => r,
                    None => continue,
                };

                let positions = match stage.get_mesh_vertices_at_time(mesh_idx, frame) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let vertex_count = positions.len() / 3;
                if vertex_count != range.vertex_count as usize {
                    log::warn!(
                        "Vertex count mismatch for mesh {}: USD {} vs range {}",
                        mesh_idx,
                        vertex_count,
                        range.vertex_count
                    );
                    continue;
                }

                // Update only this mesh's range
                let start = range.vertex_offset as usize;
                for (i, vertex) in self.mesh_data.vertices[start..start + vertex_count]
                    .iter_mut()
                    .enumerate()
                {
                    vertex.position =
                        [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
                }
                updated_any = true;
            }

            if updated_any {
                self.queue.write_buffer(
                    &self.vertex_buffer,
                    0,
                    bytemuck::cast_slice(&self.mesh_data.vertices),
                );
            }
            return;
        }

        // Single-mesh fallback (original logic)
        if self.vertex_animated_meshes.len() != 1 {
            return;
        }

        let mesh_idx = self.vertex_animated_meshes[0];
        if let Ok(positions) = stage.get_mesh_vertices_at_time(mesh_idx, frame) {
            let vertex_count = positions.len() / 3;
            if vertex_count == 0 || vertex_count != self.mesh_data.vertices.len() {
                return;
            }

            for (i, vertex) in self.mesh_data.vertices.iter_mut().enumerate() {
                vertex.position = [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
            }

            self.queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&self.mesh_data.vertices),
            );
        }
    }

    /// Build Ivar scene from viewport mesh data using instancing (async in background thread).
    ///
    /// NEW: Uses InstancedGeometry to build ONE BVH for the prototype mesh
    /// instead of duplicating 28M triangles. This reduces build time from
    /// ~4 seconds to ~40ms (100x faster) and memory from ~5GB to ~50MB.
    ///
    /// ASYNC: Runs on background thread to keep UI responsive during build.
    fn build_ivar_scene(&mut self) {
        // Check if already building or complete
        match self.ivar_state.build_status {
            BuildStatus::Building => {
                // Already building in background, skip
                return;
            }
            BuildStatus::Complete => {
                // Already built, skip
                return;
            }
            _ => {}
        }

        log::info!(
            "Starting background Ivar scene build: {} instances, {} tris/instance",
            self.instance_transforms.len(),
            self.mesh_data.indices.len() / 3
        );

        // Mark as building
        self.ivar_state.build_status = BuildStatus::Building;

        // Build triangles on main thread (handles animation via USD queries)
        let current_time = if self.vertex_animated_meshes.is_empty() {
            None
        } else {
            Some(self.timeline_state.current_frame)
        };
        let (triangle_vertices, triangle_uvs, triangle_normals) =
            self.build_triangles_at_time(current_time);

        // When using multi-draw (combined mesh), transforms are already baked into vertices
        // Use single identity transform to avoid double-transforming
        let transforms = if self.use_multi_draw {
            log::info!(
                "Multi-prototype: using identity transform for Ivar (transforms baked into combined mesh)"
            );
            vec![Mat4::IDENTITY]
        } else {
            self.instance_transforms.clone()
        };
        let scene_materials = self.scene_materials.clone();
        let fallback_material = self.scene_material.clone();
        let texture_base_dir = self.texture_base_dir.clone();
        let tri_mat_ids: Vec<u32> = self
            .mesh_data
            .triangle_material_ids
            .as_ref()
            .cloned()
            .unwrap_or_default();

        // Create channel for build completion
        let (tx, rx) = mpsc::channel();
        self.ivar_state.build_receiver = Some(rx);

        let tri_count = triangle_vertices.len();
        let instance_count = transforms.len();

        // Spawn background thread to build scene
        std::thread::spawn(move || {
            let start_time = Instant::now();

            log::info!(
                "Background thread: Building Embree scene ({} triangles, {} instances)...",
                tri_count,
                instance_count
            );

            // Load all materials with textures
            let mut texture_cache = match texture_base_dir {
                Some(dir) => bif_core::texture::TextureCache::with_base_dir(dir),
                None => bif_core::texture::TextureCache::new(),
            };
            let materials: Vec<Arc<DisneyBSDF>> = if scene_materials.is_empty() {
                // Single fallback material
                vec![Arc::new(DisneyBSDF::from_material_with_textures(
                    &fallback_material,
                    &mut texture_cache,
                ))]
            } else {
                scene_materials
                    .iter()
                    .map(|mat| {
                        Arc::new(DisneyBSDF::from_material_with_textures(
                            mat.as_ref(),
                            &mut texture_cache,
                        ))
                    })
                    .collect()
            };
            log::info!(
                "Loaded {} materials for Ivar (textures cached: {})",
                materials.len(),
                texture_cache.len()
            );

            // Try to create Embree scene first, fall back to CPU BVH if unavailable
            let world = if let Some(embree_scene) = EmbreeScene::try_new(
                &triangle_vertices,
                &triangle_uvs,
                &triangle_normals,
                transforms,
                materials,
                &tri_mat_ids,
            ) {
                log::info!("Using Embree for hardware-accelerated ray tracing");
                // Wrap Embree scene in a BVH node (BVH contains just 1 object)
                let objects: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(embree_scene)];
                Arc::new(BvhNode::new(objects))
            } else {
                log::warn!("Embree not available - using CPU BVH (slower performance)");
                log::info!("To enable Embree acceleration, ensure embree4.dll is in PATH");
                // Fall back to CPU BVH - create instances manually
                // TODO: Implement CPU-based instancing fallback
                // For now, just create empty BVH
                let objects: Vec<Box<dyn Hittable + Send + Sync>> = vec![];
                Arc::new(BvhNode::new(objects))
            };

            let elapsed = start_time.elapsed();
            log::info!(
                "Background thread: Ivar scene built in {:.2}ms",
                elapsed.as_secs_f64() * 1000.0
            );

            // Send completed scene to main thread
            let _ = tx.send(world);
        });
    }

    /// Extract triangles from mesh data, optionally querying USD for animated vertices at a time.
    ///
    /// For static geometry (no stage or no animation), uses cached mesh_data vertices.
    /// For animated geometry with a stage and time, queries USD for interpolated positions.
    fn build_triangles_at_time(&self, time: Option<f64>) -> TriangleData {
        let tri_count = self.mesh_data.indices.len() / 3;
        let mut triangle_vertices = Vec::with_capacity(tri_count);
        let mut triangle_uvs: Vec<[[f32; 2]; 3]> = Vec::with_capacity(tri_count);
        let mut triangle_normals: Vec<[[f32; 3]; 3]> = Vec::with_capacity(tri_count);

        // Get vertices - update positions from USD if animated, otherwise use static
        let vertices: &[gpu_types::Vertex] = &self.mesh_data.vertices;
        let updated_vertices: Option<Vec<gpu_types::Vertex>> = time.and_then(|t| {
            if self.vertex_animated_meshes.is_empty() {
                return None;
            }

            let stage = self.usd_stage.as_ref()?;

            // Multi-mesh scene: use mesh_ranges to update each animated mesh's vertices
            if let Some(ref ranges) = self.mesh_data.mesh_ranges {
                let mut updated = self.mesh_data.vertices.clone();

                for &mesh_idx in &self.vertex_animated_meshes {
                    let range = match ranges.iter().find(|r| r.usd_mesh_index == mesh_idx) {
                        Some(r) => r,
                        None => continue,
                    };

                    let positions = match stage.get_mesh_vertices_at_time(mesh_idx, t) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    let vertex_count = positions.len() / 3;
                    if vertex_count != range.vertex_count as usize {
                        log::warn!(
                            "Vertex count mismatch for mesh {}: USD {} vs range {}",
                            mesh_idx,
                            vertex_count,
                            range.vertex_count
                        );
                        continue;
                    }

                    // Update only this mesh's vertex range
                    let start = range.vertex_offset as usize;
                    for (i, v) in updated[start..start + vertex_count].iter_mut().enumerate() {
                        v.position = [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
                    }
                }
                return Some(updated);
            }

            // Single-mesh fallback
            if self.vertex_animated_meshes.len() != 1 {
                return None;
            }

            let mesh_idx = self.vertex_animated_meshes[0];
            let positions = stage.get_mesh_vertices_at_time(mesh_idx, t).ok()?;

            let vertex_count = positions.len() / 3;
            if vertex_count != self.mesh_data.vertices.len() {
                log::warn!(
                    "Vertex count mismatch: USD {} vs mesh {} - using static",
                    vertex_count,
                    self.mesh_data.vertices.len()
                );
                return None;
            }

            // Clone and update positions
            let mut updated = self.mesh_data.vertices.clone();
            for (i, v) in updated.iter_mut().enumerate() {
                v.position = [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
            }
            Some(updated)
        });

        let vertices = updated_vertices.as_deref().unwrap_or(vertices);

        for i in (0..self.mesh_data.indices.len()).step_by(3) {
            let i0 = self.mesh_data.indices[i] as usize;
            let i1 = self.mesh_data.indices[i + 1] as usize;
            let i2 = self.mesh_data.indices[i + 2] as usize;

            triangle_vertices.push([
                Vec3::from_array(vertices[i0].position),
                Vec3::from_array(vertices[i1].position),
                Vec3::from_array(vertices[i2].position),
            ]);

            triangle_uvs.push([vertices[i0].uv, vertices[i1].uv, vertices[i2].uv]);

            triangle_normals.push([
                vertices[i0].normal,
                vertices[i1].normal,
                vertices[i2].normal,
            ]);
        }

        (triangle_vertices, triangle_uvs, triangle_normals)
    }

    /// Build Ivar scene synchronously (blocking). Used for batch render.
    fn build_ivar_scene_sync(&self) -> Arc<BvhNode> {
        self.build_ivar_scene_at_time(None)
    }

    /// Build Ivar scene at a specific time. Used for animated batch render.
    fn build_ivar_scene_at_time(&self, time: Option<f64>) -> Arc<BvhNode> {
        let start_time = Instant::now();

        log::info!(
            "Building Ivar scene (sync): {} triangles, {} instances, time={:?}",
            self.mesh_data.indices.len() / 3,
            self.instance_transforms.len(),
            time
        );

        // Extract triangle vertices, UVs, and normals
        let (triangle_vertices, triangle_uvs, triangle_normals) =
            self.build_triangles_at_time(time);

        // Load materials with textures
        let mut texture_cache = match &self.texture_base_dir {
            Some(dir) => bif_core::texture::TextureCache::with_base_dir(dir.clone()),
            None => bif_core::texture::TextureCache::new(),
        };

        let materials: Vec<Arc<DisneyBSDF>> = if self.scene_materials.is_empty() {
            vec![Arc::new(DisneyBSDF::from_material_with_textures(
                &self.scene_material,
                &mut texture_cache,
            ))]
        } else {
            self.scene_materials
                .iter()
                .map(|mat| {
                    Arc::new(DisneyBSDF::from_material_with_textures(
                        mat.as_ref(),
                        &mut texture_cache,
                    ))
                })
                .collect()
        };

        let tri_mat_ids: Vec<u32> = self
            .mesh_data
            .triangle_material_ids
            .as_ref()
            .cloned()
            .unwrap_or_default();

        // When using multi-draw (combined mesh), transforms are already baked into vertices
        // Use single identity transform to avoid double-transforming
        let ivar_transforms = if self.use_multi_draw {
            vec![Mat4::IDENTITY]
        } else {
            self.instance_transforms.clone()
        };

        // Create Embree scene or fallback
        let world = if let Some(embree_scene) = EmbreeScene::try_new(
            &triangle_vertices,
            &triangle_uvs,
            &triangle_normals,
            ivar_transforms,
            materials,
            &tri_mat_ids,
        ) {
            log::info!("Using Embree for batch render");
            let objects: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(embree_scene)];
            Arc::new(BvhNode::new(objects))
        } else {
            log::warn!("Embree not available for batch render");
            let objects: Vec<Box<dyn Hittable + Send + Sync>> = vec![];
            Arc::new(BvhNode::new(objects))
        };

        let elapsed = start_time.elapsed();
        log::info!("Scene built in {:.2}ms", elapsed.as_secs_f64() * 1000.0);

        world
    }

    /// Invalidate cached Ivar scene (call when geometry changes or user requests rebuild).
    ///
    /// This will:
    /// 1. Clear the cached BVH
    /// 2. Reset build status to NotStarted
    /// 3. Cancel any active render
    ///
    /// Next time user switches to Ivar mode, scene will rebuild from scratch.
    pub fn invalidate_ivar_scene(&mut self) {
        log::info!("Invalidating Ivar scene cache");

        // Clear cached scene
        self.ivar_state.world = None;
        self.ivar_state.build_status = BuildStatus::NotStarted;
        self.ivar_state.build_receiver = None;

        // Cancel any active render
        self.ivar_state.cancel_flag.store(true, Ordering::Relaxed);
        self.ivar_state.cancel_flag = Arc::new(AtomicBool::new(false));

        // Clear render state
        self.ivar_state.render_complete = false;
        self.ivar_state.buckets_completed = 0;
        self.ivar_state.image_buffer = None;

        log::info!("Ivar scene cache cleared - will rebuild on next render");
    }

    /// Poll for scene build completion (call each frame).
    ///
    /// Checks if background scene build is complete, and if so:
    /// 1. Stores the completed scene
    /// 2. Marks build as complete
    /// 3. Starts the render
    fn poll_scene_build(&mut self) {
        // Only poll if we're currently building
        if self.ivar_state.build_status != BuildStatus::Building {
            return;
        }

        let Some(ref receiver) = self.ivar_state.build_receiver else {
            return;
        };

        // Non-blocking check for completion
        if let Ok(world) = receiver.try_recv() {
            log::info!("Scene build completed, received on main thread");

            // Store completed scene
            self.ivar_state.world = Some(world);
            self.ivar_state.build_status = BuildStatus::Complete;

            // Clear receiver
            self.ivar_state.build_receiver = None;

            // Now start the actual render
            log::info!("Starting Ivar render with built scene");
            self.start_ivar_render();
        }
    }

    /// Create Ivar camera from viewport camera
    fn create_ivar_camera(&self) -> bif_renderer::Camera {
        let mut camera = bif_renderer::Camera::new()
            .with_resolution(self.size.0, self.size.1)
            .with_position(self.camera.position, self.camera.target, Vec3::Y)
            .with_lens(
                self.camera.fov_y.to_degrees(),
                0.0, // No DOF for preview
                (self.camera.target - self.camera.position).length(),
            )
            .with_quality(self.ivar_state.samples_per_pixel, self.ivar_state.max_depth);

        camera.initialize();
        camera
    }

    /// Collect unique texture paths from current scene materials.
    #[cfg(feature = "oiio")]
    fn collect_material_texture_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for mat in &self.scene_materials {
            let candidates = [
                mat.diffuse_texture.as_deref(),
                mat.roughness_texture.as_deref(),
                mat.metallic_texture.as_deref(),
                mat.normal_texture.as_deref(),
                mat.emissive_texture.as_deref(),
            ];
            for path in candidates.into_iter().flatten() {
                if seen.insert(path.to_string()) {
                    paths.push(path.to_string());
                }
            }
        }
        paths
    }

    /// Start Ivar background render
    fn start_ivar_render(&mut self) {
        // Build scene if needed
        self.build_ivar_scene();

        let Some(world) = self.ivar_state.world.clone() else {
            log::error!("Cannot start Ivar render: no scene");
            return;
        };

        // Reset render state
        self.ivar_state.reset_render(self.size.0, self.size.1);

        // Create Ivar camera
        let ivar_camera = self.create_ivar_camera();

        // Create channel for bucket results
        let (tx, rx) = mpsc::channel();
        self.ivar_state.receiver = Some(rx);

        // Clone values needed for background thread
        let buckets = self.ivar_state.buckets.clone();
        let cancel_flag = self.ivar_state.cancel_flag.clone();
        let config = RenderConfig {
            samples_per_pixel: self.ivar_state.samples_per_pixel,
            max_depth: self.ivar_state.max_depth,
            background: Color::new(0.1, 0.1, 0.1),
            use_sky_gradient: true,
            environment: self.ivar_state.environment.clone(),
            lights: Arc::new(LightList::from(self.lights.scene_lights.as_slice())),
        };

        let start_time = Instant::now();

        log::info!(
            "Starting Ivar render: {}x{} @ {} SPP, {} buckets",
            self.size.0,
            self.size.1,
            self.ivar_state.samples_per_pixel,
            buckets.len()
        );

        // Spawn background render thread
        std::thread::spawn(move || {
            use rayon::prelude::*;

            // Process buckets in parallel
            buckets.par_iter().for_each(|bucket| {
                // Check for cancellation
                if cancel_flag.load(Ordering::Relaxed) {
                    return;
                }

                // Render bucket with AOVs
                let result = render_bucket_with_aovs(bucket, &ivar_camera, world.as_ref(), &config);

                let _ = tx.send(IvarMessage::BucketComplete(result));
            });

            // Check if cancelled
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(IvarMessage::Cancelled);
            } else {
                let elapsed = start_time.elapsed().as_secs_f32();
                let _ = tx.send(IvarMessage::RenderComplete {
                    elapsed_secs: elapsed,
                });
            }
        });
    }

    /// Poll for Ivar bucket completion messages
    fn poll_ivar_messages(&mut self) {
        let Some(ref receiver) = self.ivar_state.receiver else {
            return;
        };

        // Process all available messages (non-blocking)
        while let Ok(msg) = receiver.try_recv() {
            match msg {
                IvarMessage::BucketComplete(result) => {
                    let image_width = self
                        .ivar_state
                        .image_buffer
                        .as_ref()
                        .map_or(0, |img| img.width);

                    // Copy pixels to image buffer and AOV buffers
                    for local_y in 0..result.bucket.height {
                        for local_x in 0..result.bucket.width {
                            let global_x = result.bucket.x + local_x;
                            let global_y = result.bucket.y + local_y;
                            let pixel_idx = (local_y * result.bucket.width + local_x) as usize;
                            let global_idx = (global_y * image_width + global_x) as usize;

                            // Copy beauty
                            if let Some(ref mut image) = self.ivar_state.image_buffer {
                                if pixel_idx < result.pixels.len() {
                                    image.set(global_x, global_y, result.pixels[pixel_idx]);
                                }
                            }

                            // Copy AOVs
                            if let Some(ref mut alpha) = self.ivar_state.alpha_buffer {
                                if pixel_idx < result.alphas.len() && global_idx < alpha.len() {
                                    alpha[global_idx] = result.alphas[pixel_idx];
                                }
                            }
                            if let Some(ref mut depth) = self.ivar_state.depth_buffer {
                                if pixel_idx < result.depths.len() && global_idx < depth.len() {
                                    depth[global_idx] = result.depths[pixel_idx];
                                }
                            }
                            if let Some(ref mut normal) = self.ivar_state.normal_buffer {
                                if pixel_idx < result.normals.len() && global_idx < normal.len() {
                                    normal[global_idx] = result.normals[pixel_idx];
                                }
                            }
                        }
                    }
                    self.ivar_state.buckets_completed += 1;
                }
                IvarMessage::RenderComplete { elapsed_secs } => {
                    self.ivar_state.render_complete = true;
                    self.node_graph_state.mark_ivar_render_complete();
                    log::info!("Ivar render complete in {:.2}s", elapsed_secs);
                }
                IvarMessage::Cancelled => {
                    log::info!("Ivar render cancelled");
                }
            }
        }
    }

    /// Start a batch render to disk
    fn start_batch_render(&mut self) {
        // Build scene synchronously if not already built
        if self.ivar_state.world.is_none() {
            log::info!("Building scene for batch render...");
            self.ivar_state.batch_status = BatchRenderStatus::Rendering {
                current_frame: 0,
                total_frames: self.ivar_state.batch_settings.frame_count(),
                frame_progress: 0.0,
            };

            // Build synchronously (same logic as async build_ivar_scene)
            let world = self.build_ivar_scene_sync();
            self.ivar_state.world = Some(world.clone());
            self.ivar_state.build_status = BuildStatus::Complete;
        }

        let Some(world) = self.ivar_state.world.clone() else {
            log::error!("Cannot start batch render: no scene");
            self.ivar_state.batch_status = BatchRenderStatus::Failed("No scene loaded".to_string());
            return;
        };

        // Check for animated geometry
        let has_animated_geometry = !self.vertex_animated_meshes.is_empty();

        // Create scene builder for animated geometry
        let scene_builder: Option<batch_render::SceneBuilderFn> = if has_animated_geometry {
            let builder_data = SceneBuilderData {
                vertices: self.mesh_data.vertices.clone(),
                indices: self.mesh_data.indices.clone(),
                triangle_material_ids: self.mesh_data.triangle_material_ids.clone(),
                scene_materials: self.scene_materials.clone(),
                scene_material: self.scene_material.clone(),
                texture_base_dir: self.texture_base_dir.clone(),
                instance_transforms: self.instance_transforms.clone(),
                use_multi_draw: self.use_multi_draw,
                vertex_animated_meshes: self.vertex_animated_meshes.clone(),
                stage: self.usd_stage.clone(),
                mesh_ranges: self.mesh_data.mesh_ranges.clone(),
            };
            Some(Box::new(move |time: f64| {
                builder_data.build_scene_at_time(time)
            }))
        } else {
            None
        };

        // Create scene data for batch render
        let scene_data = BatchSceneData {
            world,
            environment: self.ivar_state.environment.clone(),
            stage: self.usd_stage.clone(),
            viewport_camera: self.camera,
            has_animated_geometry,
            scene_builder,
            lights: Arc::new(LightList::from(self.lights.scene_lights.as_slice())),
        };

        // Clone settings and compute auto depth bounds if enabled
        let mut settings = self.ivar_state.batch_settings.clone();
        if settings.aov_settings.auto_depth_bounds && settings.aov_settings.include_depth {
            // Compute scene diagonal length for depth far
            let scene_size = (self.mesh_bounds_max - self.mesh_bounds_min).length();
            if scene_size > 0.0 {
                settings.aov_settings.depth_far = scene_size * 2.0;
                settings.aov_settings.depth_near = 0.01;
                log::info!(
                    "Auto depth bounds: near={}, far={} (scene_size={})",
                    settings.aov_settings.depth_near,
                    settings.aov_settings.depth_far,
                    scene_size
                );
            }
        }

        log::info!(
            "Starting batch render: frames {}-{} step {}, {}x{} @ {} SPP",
            settings.start_frame,
            settings.end_frame,
            settings.frame_step,
            settings.resolution_x,
            settings.resolution_y,
            settings.samples_per_pixel
        );

        // Start the batch render
        let (rx, cancel_flag) = batch_render::start_batch_render(settings, scene_data);
        self.batch_receiver = Some(rx);
        self.batch_cancel_flag = Some(cancel_flag);
        self.ivar_state.batch_status = BatchRenderStatus::Rendering {
            current_frame: 1,
            total_frames: self.ivar_state.batch_settings.frame_count(),
            frame_progress: 0.0,
        };
    }

    /// Render a frame with the given clear color
    pub fn render(
        &mut self,
        clear_color: wgpu::Color,
        window: &winit::window::Window,
    ) -> Result<()> {
        // Poll for completed async IBL generation
        let ibl_result = self.ibl_receiver.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = ibl_result {
            match result {
                IblResult::Success {
                    hdr_pixels,
                    hdr_width,
                    hdr_height,
                    ivar_env,
                    path,
                    rotation_rad,
                    intensity,
                    show_background,
                } => {
                    // GPU compute IBL for viewport
                    let output = self.compute_ibl.generate(
                        &self.device,
                        &self.queue,
                        hdr_width,
                        hdr_height,
                        &hdr_pixels,
                    );
                    let mip_count = compute_ibl::PREFILTER_MIP_COUNT;
                    self.gpu_environment.params.max_mip = (mip_count - 1).max(1) as f32;
                    self.gpu_environment.load_from_compute(
                        &self.device,
                        &self.queue,
                        output,
                        mip_count,
                    );
                    self.update_environment_params(intensity, rotation_rad, show_background);
                    // Rebuild skybox bind group
                    let skybox_bgl = skybox::create_skybox_bind_group_layout(&self.device);
                    self.skybox_bind_group = skybox::create_skybox_bind_group(
                        &self.device,
                        &skybox_bgl,
                        &self.gpu_environment.prefiltered_view,
                        &self.gpu_environment.sampler,
                        &self.gpu_environment.params_buffer,
                    );
                    // Set Ivar CPU environment
                    self.ivar_state.environment = Some(ivar_env);
                    self.node_graph_state.mark_hdri_loaded(&path);
                    log::info!("HDRI loaded (GPU compute): {}", path);
                }
                IblResult::Error { path, message } => {
                    log::error!("Failed to load HDRI: {}", message);
                    self.node_graph_state.mark_hdri_error(&path, message);
                }
            }
            self.ibl_receiver = None;
        }

        // Poll for completed .tx conversion
        let tx_result = self
            .tx_conversion_receiver
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        if let Some(status) = tx_result {
            self.node_graph_state.mark_tx_conversion_complete(status);
            self.tx_conversion_receiver = None;
        }

        // Poll for batch render messages
        let mut clear_batch_state = false;
        if let Some(ref rx) = self.batch_receiver {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    BatchMessage::Progress {
                        current_frame,
                        total_frames,
                        frame_progress,
                    } => {
                        self.ivar_state.batch_status = BatchRenderStatus::Rendering {
                            current_frame,
                            total_frames,
                            frame_progress,
                        };
                    }
                    BatchMessage::FrameComplete {
                        frame,
                        elapsed_secs,
                    } => {
                        log::info!("Batch frame {} complete in {:.1}s", frame, elapsed_secs);
                    }
                    BatchMessage::Complete { total_elapsed_secs } => {
                        log::info!("Batch render complete in {:.1}s", total_elapsed_secs);
                        self.ivar_state.batch_status =
                            BatchRenderStatus::Complete { total_elapsed_secs };
                        clear_batch_state = true;
                    }
                    BatchMessage::Cancelled => {
                        log::info!("Batch render cancelled");
                        self.ivar_state.batch_status = BatchRenderStatus::Cancelled;
                        clear_batch_state = true;
                    }
                    BatchMessage::Error(e) => {
                        log::error!("Batch render error: {}", e);
                        self.ivar_state.batch_status = BatchRenderStatus::Failed(e);
                        clear_batch_state = true;
                    }
                }
            }
        }
        if clear_batch_state {
            self.batch_receiver = None;
            self.batch_cancel_flag = None;
        }

        // Update frustum culling before rendering (in Vulkan mode)
        if self.ivar_state.mode == RenderMode::Vulkan {
            self.update_visible_instances();
        }

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Prepare egui UI
        let raw_input = self.egui_state.take_egui_input(window);

        // Build UI - need to split borrow to avoid closure borrowing entire self
        let show_ui = self.show_ui;
        let fps = self.fps;
        let camera = &self.camera;
        let num_instances = self.num_instances;
        let visible_instances = self.visible_instance_count;
        let lod_box_instances = self.lod_box_instance_count;
        let triangles_per_instance = self.triangles_per_instance;
        let mesh_bounds_min = self.mesh_bounds_min;
        let mesh_bounds_max = self.mesh_bounds_max;
        let size = self.size;
        let mut gnomon_size = self.gnomon.size;
        let mut lod_max_polys = self.lod_max_polys;
        let mut left_panel_width = self.ui_left_panel_width;
        let mut right_panel_width = self.ui_right_panel_width;
        let mut bottom_panel_height = self.ui_bottom_panel_height;

        // Ivar state for UI
        let mut render_mode = self.ivar_state.mode;
        let ivar_progress = self.ivar_state.progress();
        let ivar_buckets_completed = self.ivar_state.buckets_completed;
        let ivar_total_buckets = self.ivar_state.buckets.len();
        let ivar_elapsed = self.ivar_state.elapsed_secs();
        let ivar_render_complete = self.ivar_state.render_complete;
        let ivar_spp = self.ivar_state.samples_per_pixel;

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            if !show_ui {
                left_panel_width = 0.0;
                right_panel_width = 0.0;
                bottom_panel_height = 0.0;
                return;
            }

            let stats_panel = egui::SidePanel::left("stats_panel")
                .default_width(300.0)
                .show(ctx, |ui| {
                    ui.heading("BIF Viewer");
                    ui.separator();

                    // Render Mode Dropdown (Houdini-style)
                    ui.horizontal(|ui| {
                        ui.label("Renderer:");
                        egui::ComboBox::from_id_salt("render_mode")
                            .selected_text(render_mode.display_name())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut render_mode, RenderMode::Vulkan, "Vulkan");
                                ui.selectable_value(&mut render_mode, RenderMode::Ivar, "Ivar");
                            });
                    });

                    // Show Ivar stats when in Ivar mode
                    if render_mode == RenderMode::Ivar {
                        ui.separator();
                        ui.label("Ivar Path Tracer");

                        // Show build status or render progress
                        match self.ivar_state.build_status {
                            BuildStatus::NotStarted => {
                                ui.label("Preparing scene...");
                            }
                            BuildStatus::Building => {
                                // Show spinner while building
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Building scene geometry...");
                                });
                                ui.label(format!(
                                    "{} instances, {} tris/instance",
                                    self.instance_transforms.len(),
                                    self.mesh_data.indices.len() / 3
                                ));
                            }
                            BuildStatus::Failed => {
                                ui.colored_label(egui::Color32::RED, "⚠ Scene build failed");
                            }
                            BuildStatus::Complete => {
                                // Scene is built, show render progress

                                // Progress bar
                                let progress_bar = egui::ProgressBar::new(ivar_progress / 100.0)
                                    .text(format!("{:.1}%", ivar_progress));
                                ui.add(progress_bar);

                                // Stats
                                ui.label(format!(
                                    "Buckets: {} / {}",
                                    ivar_buckets_completed, ivar_total_buckets
                                ));
                                ui.label(format!("SPP: {}", ivar_spp));
                                ui.label(format!("Time: {:.1}s", ivar_elapsed));

                                if ivar_render_complete {
                                    ui.colored_label(egui::Color32::GREEN, "✓ Render Complete");
                                } else if ivar_buckets_completed > 0 {
                                    ui.colored_label(egui::Color32::YELLOW, "⟳ Rendering...");
                                }
                            }
                        }

                        // AOV preview dropdown
                        ui.horizontal(|ui| {
                            ui.label("Preview:");
                            egui::ComboBox::from_id_salt("aov_preview")
                                .selected_text(self.ivar_state.preview_aov.display_name())
                                .show_ui(ui, |ui| {
                                    for channel in ivar_state::AovChannel::all() {
                                        ui.selectable_value(
                                            &mut self.ivar_state.preview_aov,
                                            *channel,
                                            channel.display_name(),
                                        );
                                    }
                                });
                        });

                        // Rebuild Scene button
                        ui.separator();
                        // Note: Can't call self.invalidate_ivar_scene() here due to borrow rules
                        // Using ctx.data_mut() to store the request
                        if ui.button("Rebuild Scene").clicked() {
                            ctx.data_mut(|d| {
                                d.insert_temp(egui::Id::new("rebuild_scene_requested"), true)
                            });
                        }
                        ui.label("↻ Rebuild if geometry changes");

                        // TODO: Add progressive multi-pass rendering (1 SPP preview → full SPP)
                    }

                    ui.separator();

                    // FPS Counter
                    ui.label(format!("FPS: {:.1}", fps));
                    ui.separator();

                    // Scene Stats
                    ui.collapsing("Scene Stats", |ui| {
                        ui.label(format!("Instances: {} total", num_instances));
                        let total_visible = visible_instances + lod_box_instances;
                        ui.label(format!(
                            "Visible: {} ({:.0}%)",
                            total_visible,
                            if num_instances > 0 {
                                (total_visible as f32 / num_instances as f32) * 100.0
                            } else {
                                0.0
                            }
                        ));
                        ui.label(format!("  Full mesh: {}", visible_instances));
                        ui.label(format!("  Box LOD: {}", lod_box_instances));
                        // Triangle count: full mesh triangles + 12 triangles per LOD box
                        let full_mesh_tris = triangles_per_instance * visible_instances;
                        let box_tris = 12 * lod_box_instances;
                        ui.label(format!(
                            "Triangles: {} ({}+{})",
                            full_mesh_tris + box_tris,
                            full_mesh_tris,
                            box_tris
                        ));
                        ui.label(format!("Tris/Instance: {}", triangles_per_instance));

                        ui.separator();
                        ui.label("LOD Budget Control:");
                        // Slider for max polys (in millions for readability)
                        let max_millions = (lod_max_polys as f32 / 1_000_000.0).max(0.1);
                        let mut millions = max_millions;
                        ui.add(
                            egui::Slider::new(&mut millions, 0.1..=100.0)
                                .logarithmic(true)
                                .text("Max M tris")
                                .suffix("M"),
                        );
                        if (millions - max_millions).abs() > 0.001 {
                            lod_max_polys = (millions * 1_000_000.0) as u32;
                        }
                        let budget_used =
                            (full_mesh_tris as f32 / lod_max_polys as f32 * 100.0).min(100.0);
                        ui.label(format!("Budget: {:.0}% used", budget_used));
                    });

                    ui.separator();

                    // Camera Stats
                    ui.collapsing("Camera", |ui| {
                        ui.label(format!(
                            "Position: ({:.2}, {:.2}, {:.2})",
                            camera.position.x, camera.position.y, camera.position.z
                        ));
                        ui.label(format!(
                            "Target: ({:.2}, {:.2}, {:.2})",
                            camera.target.x, camera.target.y, camera.target.z
                        ));
                        ui.label(format!("Distance: {:.2}", camera.distance));
                        ui.label(format!("Yaw: {:.2}°", camera.yaw.to_degrees()));
                        ui.label(format!("Pitch: {:.2}°", camera.pitch.to_degrees()));
                        ui.label(format!("FOV: {:.2}°", camera.fov_y.to_degrees()));
                        ui.label(format!("Near: {:.2}", camera.near));
                        ui.label(format!("Far: {:.2}", camera.far));

                        ui.label("Press F to frame mesh");
                    });

                    ui.separator();

                    // Mesh Info
                    ui.collapsing("Mesh Bounds", |ui| {
                        let mesh_center = (mesh_bounds_min + mesh_bounds_max) * 0.5;
                        let mesh_size = (mesh_bounds_max - mesh_bounds_min).length();

                        ui.label(format!(
                            "Bounds Min: ({:.2}, {:.2}, {:.2})",
                            mesh_bounds_min.x, mesh_bounds_min.y, mesh_bounds_min.z
                        ));
                        ui.label(format!(
                            "Bounds Max: ({:.2}, {:.2}, {:.2})",
                            mesh_bounds_max.x, mesh_bounds_max.y, mesh_bounds_max.z
                        ));
                        ui.label(format!(
                            "Center: ({:.2}, {:.2}, {:.2})",
                            mesh_center.x, mesh_center.y, mesh_center.z
                        ));
                        ui.label(format!("Size: {:.2}", mesh_size));
                    });

                    ui.separator();

                    // Viewport Info
                    ui.collapsing("Viewport", |ui| {
                        ui.label(format!("Resolution: {}x{}", size.0, size.1));
                        ui.label(format!("Aspect: {:.3}", size.0 as f32 / size.1 as f32));
                        ui.add(egui::Slider::new(&mut gnomon_size, 40..=120).text("Gnomon Size"));
                    });

                    ui.separator();

                    // Controls Help
                    ui.collapsing("Controls", |ui| {
                        ui.label("🖱️ Left Mouse: Tumble (orbit)");
                        ui.label("🖱️ Middle Mouse: Track (pan)");
                        ui.label("🖱️ Scroll Wheel: Dolly (zoom)");
                        ui.label("⌨️ W/A/S/D: Move forward/left/back/right");
                        ui.label("⌨️ Q/E: Move down/up");
                        ui.label("⌨️ F: Frame mesh");
                    });

                    ui.separator();

                    // Render to Disk
                    ui.collapsing("Render to Disk", |ui| {
                        let settings = &mut self.ivar_state.batch_settings;

                        // Camera source
                        ui.horizontal(|ui| {
                            ui.label("Camera:");
                            egui::ComboBox::from_id_salt("batch_camera")
                                .selected_text(settings.camera_source.display_name())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut settings.camera_source,
                                        ivar_state::CameraSource::Viewport,
                                        "Viewport",
                                    );
                                    // List USD cameras if stage available
                                    if let Some(ref stage) = self.usd_stage {
                                        if let Ok(paths) = stage.camera_paths() {
                                            for path in paths {
                                                let is_selected = matches!(
                                                    &settings.camera_source,
                                                    ivar_state::CameraSource::UsdCamera(p) if p == &path
                                                );
                                                if ui
                                                    .selectable_label(is_selected, &path)
                                                    .clicked()
                                                {
                                                    settings.camera_source =
                                                        ivar_state::CameraSource::UsdCamera(path.clone());
                                                }
                                            }
                                        }
                                    }
                                });
                        });

                        // Frame range
                        ui.horizontal(|ui| {
                            ui.label("Frames:");
                            ui.add(
                                egui::DragValue::new(&mut settings.start_frame)
                                    .speed(1.0)
                                    .prefix(""),
                            );
                            ui.label("-");
                            ui.add(
                                egui::DragValue::new(&mut settings.end_frame)
                                    .speed(1.0)
                                    .prefix(""),
                            );
                        });

                        // Use timeline range button
                        if ui.button("Use Timeline Range").clicked()
                            && self.timeline_state.has_range()
                        {
                            settings.start_frame = self.timeline_state.start_frame as i32;
                            settings.end_frame = self.timeline_state.end_frame as i32;
                        }

                        ui.horizontal(|ui| {
                            ui.label("Step:");
                            ui.add(egui::DragValue::new(&mut settings.frame_step).speed(1.0).range(1..=100));
                        });

                        // Resolution
                        ui.horizontal(|ui| {
                            ui.label("Resolution:");
                            ui.add(
                                egui::DragValue::new(&mut settings.resolution_x)
                                    .speed(10.0)
                                    .range(64..=8192),
                            );
                            ui.label("x");
                            ui.add(
                                egui::DragValue::new(&mut settings.resolution_y)
                                    .speed(10.0)
                                    .range(64..=8192),
                            );
                        });

                        // Quality
                        ui.horizontal(|ui| {
                            ui.label("SPP:");
                            ui.add(
                                egui::DragValue::new(&mut settings.samples_per_pixel)
                                    .speed(1.0)
                                    .range(1..=1024),
                            );
                        });

                        ui.horizontal(|ui| {
                            ui.label("Max Depth:");
                            ui.add(
                                egui::DragValue::new(&mut settings.max_depth)
                                    .speed(1.0)
                                    .range(1..=32),
                            );
                        });

                        // Compression
                        ui.horizontal(|ui| {
                            ui.label("Compression:");
                            egui::ComboBox::from_id_salt("batch_compression")
                                .selected_text(settings.compression.display_name())
                                .show_ui(ui, |ui| {
                                    for comp in bif_renderer::ExrCompression::all() {
                                        ui.selectable_value(
                                            &mut settings.compression,
                                            *comp,
                                            comp.display_name(),
                                        );
                                    }
                                });
                        });

                        // Per-AOV checkboxes
                        ui.collapsing("AOVs", |ui| {
                            let aov = &mut settings.aov_settings;
                            ui.checkbox(&mut aov.include_alpha, "Alpha (A)");
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut aov.include_depth, "Depth (Z)");
                                if aov.include_depth {
                                    ui.add(egui::DragValue::new(&mut aov.depth_near)
                                        .speed(0.1)
                                        .range(0.001..=aov.depth_far)
                                        .prefix("Near: "));
                                    ui.add(egui::DragValue::new(&mut aov.depth_far)
                                        .speed(10.0)
                                        .range(aov.depth_near..=100000.0)
                                        .prefix("Far: "));
                                }
                            });
                            if aov.include_depth {
                                ui.checkbox(&mut aov.auto_depth_bounds, "Auto bounds from scene");
                            }
                            ui.checkbox(&mut aov.include_normal, "Normal (N)");
                        });

                        // Output path
                        ui.horizontal(|ui| {
                            ui.label("Output:");
                            ui.text_edit_singleline(&mut settings.output_pattern);
                        });

                        ui.horizontal(|ui| {
                            ui.label("Dir:");
                            ui.text_edit_singleline(&mut settings.output_directory);
                        });

                        // Sync viewport to USD camera button
                        if let ivar_state::CameraSource::UsdCamera(ref cam_path) = settings.camera_source {
                            if ui.button("Sync Viewport to Camera").clicked() {
                                ctx.data_mut(|d| {
                                    d.insert_temp(egui::Id::new("sync_viewport_to_usd_camera"), cam_path.clone())
                                });
                            }
                        }

                        ui.separator();

                        // Render button and status on same line
                        let status = &self.ivar_state.batch_status;
                        let is_rendering = matches!(status, ivar_state::BatchRenderStatus::Rendering { .. });
                        let can_render = !settings.output_directory.is_empty() && !is_rendering;

                        ui.horizontal(|ui| {
                            if ui.add_enabled(can_render, egui::Button::new("Render")).clicked() {
                                ctx.data_mut(|d| {
                                    d.insert_temp(egui::Id::new("start_batch_render"), true)
                                });
                            }

                            // Show status next to button
                            match status {
                                ivar_state::BatchRenderStatus::Idle => {
                                    if settings.output_directory.is_empty() {
                                        ui.label("Set output directory");
                                    }
                                }
                                ivar_state::BatchRenderStatus::Complete { total_elapsed_secs } => {
                                    ui.colored_label(
                                        egui::Color32::GREEN,
                                        format!("Done ({:.1}s)", total_elapsed_secs),
                                    );
                                }
                                ivar_state::BatchRenderStatus::Cancelled => {
                                    ui.colored_label(egui::Color32::YELLOW, "Cancelled");
                                }
                                ivar_state::BatchRenderStatus::Failed(msg) => {
                                    ui.colored_label(egui::Color32::RED, format!("Failed: {}", msg));
                                }
                                _ => {}
                            }
                        });

                        // Progress bar and cancel button when rendering
                        if let ivar_state::BatchRenderStatus::Rendering {
                            current_frame,
                            total_frames,
                            frame_progress,
                        } = status
                        {
                            let overall = ((*current_frame - 1) as f32 + frame_progress)
                                / *total_frames as f32;
                            ui.add(
                                egui::ProgressBar::new(overall)
                                    .text(format!("Frame {}/{}", current_frame, total_frames)),
                            );
                            if ui.button("Cancel").clicked() {
                                ctx.data_mut(|d| {
                                    d.insert_temp(egui::Id::new("cancel_batch_render"), true)
                                });
                            }
                        }
                    });

                    ui.separator();

                    // Scene Browser (collapsible)
                    ui.collapsing("Scene Browser", |ui| {
                        // Use USD stage if available, otherwise empty provider
                        let empty_provider = EmptyPrimProvider;
                        let provider: &dyn PrimDataProvider = match &self.usd_stage {
                            Some(stage) => stage.as_ref(),
                            None => &empty_provider,
                        };

                        // Store selection change request in temp data for processing after egui run
                        if let Some(new_selection) = scene_browser::render_scene_browser(
                            ui,
                            &mut self.scene_browser_state,
                            provider,
                        ) {
                            ctx.data_mut(|d| {
                                d.insert_temp(
                                    egui::Id::new("prim_selection_changed"),
                                    new_selection,
                                );
                            });
                        }
                    });
                });
            left_panel_width = stats_panel.response.rect.width();

            // Property Inspector (right panel)
            let property_panel = egui::SidePanel::right("property_panel")
                .default_width(280.0)
                .show(ctx, |ui| {
                    render_property_inspector(ui, self.selected_prim_properties.as_ref());
                });
            right_panel_width = property_panel.response.rect.width();

            // Timeline panel (always visible, like Houdini/Maya/Blender)
            let timeline_panel = egui::TopBottomPanel::bottom("timeline_panel")
                .exact_height(32.0)
                .show(ctx, |ui| {
                    let has_animation = self.timeline_state.has_range();

                    ui.horizontal_centered(|ui| {
                        // Camera dropdown
                        let cam_display = self.viewport_camera_source.display_name();
                        egui::ComboBox::from_id_salt("viewport_camera")
                            .selected_text(cam_display)
                            .width(100.0)
                            .show_ui(ui, |ui| {
                                // Viewport option
                                if ui
                                    .selectable_label(
                                        matches!(self.viewport_camera_source, CameraSource::Viewport),
                                        "Viewport",
                                    )
                                    .clicked()
                                {
                                    self.viewport_camera_source = CameraSource::Viewport;
                                    self.camera_locked = false;
                                    self.selected_usd_camera = None;
                                }
                                // USD cameras from stage
                                if let Some(ref stage) = self.usd_stage {
                                    if let Ok(paths) = stage.camera_paths() {
                                        for path in paths {
                                            let is_selected = matches!(
                                                &self.viewport_camera_source,
                                                CameraSource::UsdCamera(p) if p == &path
                                            );
                                            if ui.selectable_label(is_selected, &path).clicked() {
                                                self.viewport_camera_source =
                                                    CameraSource::UsdCamera(path.clone());
                                                self.selected_usd_camera = Some(path.clone());
                                                self.camera_locked = true;
                                                // Sync to camera immediately (via egui temp data)
                                                ctx.data_mut(|d| {
                                                    d.insert_temp(
                                                        egui::Id::new("sync_viewport_camera"),
                                                        path,
                                                    )
                                                });
                                            }
                                        }
                                    }
                                }
                            });

                        // Lock/Unlock toggle (only show when USD camera selected)
                        if matches!(self.viewport_camera_source, CameraSource::UsdCamera(_)) {
                            let icon = if self.camera_locked { "Lock" } else { "Free" };
                            if ui.button(icon).clicked() {
                                self.camera_locked = !self.camera_locked;
                            }
                        }

                        ui.separator();

                        // Play/Pause button (disabled if no animation)
                        ui.add_enabled_ui(has_animation, |ui| {
                            let play_text = if self.timeline_state.is_playing {
                                "⏸"
                            } else {
                                "▶"
                            };
                            if ui.button(play_text).clicked() {
                                self.timeline_state.is_playing = !self.timeline_state.is_playing;
                            }

                            // Go to start
                            if ui.button("|◀").clicked() {
                                self.timeline_state.go_to_start();
                            }
                        });

                        // Frame range display (start)
                        let (start, end) = if has_animation {
                            (
                                self.timeline_state.start_frame as f32,
                                self.timeline_state.end_frame as f32,
                            )
                        } else {
                            (1.0, 100.0)
                        };
                        ui.label(format!("{:.0}", start));

                        // Frame slider with frame numbers shown
                        let mut frame = self.timeline_state.current_frame as f32;
                        let slider = egui::Slider::new(&mut frame, start..=end)
                            .show_value(true)
                            .integer();
                        if ui.add_sized([200.0, 18.0], slider).changed() {
                            self.timeline_state.current_frame = frame as f64;
                        }

                        // Frame range display (end)
                        ui.label(format!("{:.0}", end));

                        // Go to end (disabled if no animation)
                        ui.add_enabled_ui(has_animation, |ui| {
                            if ui.button("▶|").clicked() {
                                self.timeline_state.go_to_end();
                            }
                        });

                        // Loop toggle
                        ui.checkbox(&mut self.timeline_state.loop_playback, "Loop");

                        // Integer frame snap toggle
                        ui.checkbox(&mut self.timeline_state.snap_to_frames, "Int");

                        // FPS display
                        ui.label(format!("@{:.0}fps", self.timeline_state.fps));
                    });
                });
            let timeline_height = timeline_panel.response.rect.height();

            // Node Graph (bottom panel)
            let node_graph_panel = egui::TopBottomPanel::bottom("node_graph_panel")
                .default_height(200.0)
                .resizable(true)
                .show(ctx, |ui| {
                    let events = render_node_graph(ui, &mut self.node_graph_state);
                    // Store events for processing after egui frame ends
                    for event in events {
                        ctx.data_mut(|d| {
                            let mut pending: Vec<NodeGraphEvent> = d
                                .get_temp(egui::Id::new("node_graph_events"))
                                .unwrap_or_default();
                            pending.push(event);
                            d.insert_temp(egui::Id::new("node_graph_events"), pending);
                        });
                    }
                });
            bottom_panel_height = node_graph_panel.response.rect.height() + timeline_height;
        });

        // Update gnomon size from UI
        self.gnomon.size = gnomon_size;
        self.ui_left_panel_width = left_panel_width;
        self.ui_right_panel_width = right_panel_width;
        self.ui_bottom_panel_height = bottom_panel_height;

        // Update LOD max polys from UI
        self.lod_max_polys = lod_max_polys;

        // Update render mode from UI - detect mode change
        let mode_changed = self.ivar_state.mode != render_mode;
        self.ivar_state.mode = render_mode;

        // Handle mode switch to Ivar - start render if needed
        if mode_changed && render_mode == RenderMode::Ivar {
            log::info!("Switched to Ivar mode - starting render");
            self.start_ivar_render();
        }

        // Handle rebuild scene request (stored in egui temp data)
        let rebuild_requested = self.egui_ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new("rebuild_scene_requested"))
                .unwrap_or(false)
        });
        if rebuild_requested {
            log::info!("Manual scene rebuild requested");
            self.invalidate_ivar_scene();
            // Clear the flag
            self.egui_ctx
                .data_mut(|d| d.remove::<bool>(egui::Id::new("rebuild_scene_requested")));
        }

        // Handle batch render start request
        let start_batch = self.egui_ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new("start_batch_render"))
                .unwrap_or(false)
        });
        if start_batch {
            self.egui_ctx
                .data_mut(|d| d.remove::<bool>(egui::Id::new("start_batch_render")));
            self.start_batch_render();
        }

        // Handle batch render cancel request
        let cancel_batch = self.egui_ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new("cancel_batch_render"))
                .unwrap_or(false)
        });
        if cancel_batch {
            self.egui_ctx
                .data_mut(|d| d.remove::<bool>(egui::Id::new("cancel_batch_render")));
            if let Some(ref flag) = self.batch_cancel_flag {
                flag.store(true, Ordering::Relaxed);
            }
        }

        // Handle sync viewport to USD camera request (from batch render panel)
        let sync_camera: Option<String> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("sync_viewport_to_usd_camera")));
        if let Some(camera_path) = sync_camera {
            self.egui_ctx
                .data_mut(|d| d.remove::<String>(egui::Id::new("sync_viewport_to_usd_camera")));
            self.sync_viewport_to_usd_camera(&camera_path);
        }

        // Handle viewport camera selection from timeline dropdown
        let sync_viewport_cam: Option<String> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("sync_viewport_camera")));
        if let Some(camera_path) = sync_viewport_cam {
            self.egui_ctx
                .data_mut(|d| d.remove::<String>(egui::Id::new("sync_viewport_camera")));
            self.sync_viewport_to_usd_camera(&camera_path);
        }

        // Handle prim selection from scene browser
        let selected_prim: Option<String> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("prim_selection_changed")));
        if let Some(prim_path) = selected_prim {
            self.egui_ctx
                .data_mut(|d| d.remove::<String>(egui::Id::new("prim_selection_changed")));
            self.selected_prim_path = Some(prim_path.clone());
            let provider: Option<&dyn PrimDataProvider> = self
                .usd_stage
                .as_ref()
                .map(|s| s.as_ref() as &dyn PrimDataProvider);
            if let Some(info) = provider.and_then(|p| p.get_prim_info(&prim_path)) {
                self.selected_prim_properties = Some(PrimProperties::from_display_info(&info));
            } else {
                self.selected_prim_properties = Some(PrimProperties {
                    path: prim_path,
                    ..Default::default()
                });
            }
        }

        // Handle node graph events (USD loading, render start, etc.)
        let node_graph_events: Vec<NodeGraphEvent> = self.egui_ctx.data(|d| {
            d.get_temp(egui::Id::new("node_graph_events"))
                .unwrap_or_default()
        });
        if !node_graph_events.is_empty() {
            // Clear the events
            self.egui_ctx
                .data_mut(|d| d.remove::<Vec<NodeGraphEvent>>(egui::Id::new("node_graph_events")));

            for event in node_graph_events {
                match event {
                    NodeGraphEvent::LoadUsdFile(path) => {
                        log::info!("Node graph: Loading USD file: {}", path);
                        match self.load_usd_scene(&path) {
                            Ok(()) => {
                                // Mark the node as loaded successfully
                                self.node_graph_state.mark_node_loaded(&path);
                                log::info!("USD file loaded successfully: {}", path);
                            }
                            Err(e) => {
                                log::error!("Failed to load USD file: {}", e);
                                self.node_graph_state.mark_node_error(&path, e.to_string());
                            }
                        }
                    }
                    NodeGraphEvent::StartRender { spp } => {
                        log::info!("Node graph: Starting render with {} SPP", spp);
                        self.ivar_state.samples_per_pixel = spp;
                        self.ivar_state.mode = RenderMode::Ivar;
                        self.start_ivar_render();
                    }
                    NodeGraphEvent::ConvertTexturesToTx => {
                        #[cfg(feature = "oiio")]
                        {
                            let paths = self.collect_material_texture_paths();
                            if paths.is_empty() {
                                self.node_graph_state
                                    .mark_tx_conversion_complete("No textures".into());
                            } else {
                                log::info!("Converting {} textures to .tx", paths.len());
                                let (tx, rx) = mpsc::channel();
                                self.tx_conversion_receiver = Some(rx);
                                let base_dir = self.texture_base_dir.clone();
                                std::thread::spawn(move || {
                                    let cache = match base_dir {
                                        Some(dir) => {
                                            bif_core::texture::TextureCache::with_base_dir(dir)
                                        }
                                        None => bif_core::texture::TextureCache::new(),
                                    };
                                    let count = cache.convert_textures_to_tx(&paths);
                                    let status = if count > 0 {
                                        format!("{}/{} converted", count, paths.len())
                                    } else {
                                        "All up to date".into()
                                    };
                                    let _ = tx.send(status);
                                });
                            }
                        }
                        #[cfg(not(feature = "oiio"))]
                        {
                            self.node_graph_state
                                .mark_tx_conversion_complete("OIIO not available".into());
                        }
                    }
                    NodeGraphEvent::LoadHdri {
                        path,
                        rotation,
                        intensity,
                        show_background,
                    } => {
                        log::info!("Node graph: Loading HDRI (async): {}", path);
                        self.node_graph_state.mark_hdri_loading(&path);
                        let (tx, rx) = mpsc::channel();
                        self.ibl_receiver = Some(rx);
                        let rotation_rad = rotation.to_radians();
                        let path_clone = path.clone();
                        std::thread::spawn(move || {
                            match bif_core::hdr::HdrImage::load(&path_clone) {
                                Ok(hdr) => {
                                    // Clone pixels for GPU compute (Ivar takes ownership of HdrImage)
                                    let hdr_pixels = hdr.pixels.clone();
                                    let hdr_width = hdr.width;
                                    let hdr_height = hdr.height;
                                    let ivar_env = bif_renderer::HdriEnvironment::new(
                                        hdr,
                                        rotation_rad,
                                        intensity,
                                    );
                                    let _ = tx.send(IblResult::Success {
                                        hdr_pixels,
                                        hdr_width,
                                        hdr_height,
                                        ivar_env: Arc::new(ivar_env),
                                        path: path_clone,
                                        rotation_rad,
                                        intensity,
                                        show_background,
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(IblResult::Error {
                                        path: path_clone,
                                        message: e.to_string(),
                                    });
                                }
                            }
                        });
                    }
                    NodeGraphEvent::UpdateHdriParams {
                        rotation,
                        intensity,
                        show_background,
                    } => {
                        let rotation_rad = rotation.to_radians();
                        self.update_environment_params(intensity, rotation_rad, show_background);
                    }
                }
            }
        }

        self.egui_state
            .handle_platform_output(window, full_output.platform_output);

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.0, self.size.1],
            pixels_per_point: window.scale_factor() as f32,
        };

        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Upload egui textures
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        // Prepare egui render pass
        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // Main render pass - dispatch based on render mode
        match self.ivar_state.mode {
            RenderMode::Vulkan => {
                // Standard GPU viewport rendering

                // Skybox pass (renders environment background before geometry)
                if self.show_background && self.gpu_environment.params.has_environment != 0 {
                    let mut skybox_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Skybox Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    skybox_pass.set_pipeline(&self.skybox_pipeline);
                    skybox_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    skybox_pass.set_bind_group(1, &self.skybox_bind_group, &[]);
                    skybox_pass.draw(0..3, 0..1);
                }

                // Geometry pass
                {
                    let color_load = if self.show_background
                        && self.gpu_environment.params.has_environment != 0
                    {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(clear_color)
                    };
                    let depth_load = if self.show_background
                        && self.gpu_environment.params.has_environment != 0
                    {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(1.0)
                    };

                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: color_load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: depth_load,
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    // Set common pipeline state
                    render_pass.set_pipeline(&self.pipeline);
                    render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    render_pass.set_bind_group(1, &self.material_bind_group, &[]);
                    render_pass.set_bind_group(2, &self.texture_bind_group, &[]);
                    render_pass.set_bind_group(3, &self.gpu_environment.bind_group, &[]);
                    render_pass.set_bind_group(4, &self.lights.bind_group, &[]);

                    if self.use_multi_draw && !self.prototype_gpu_data.is_empty() {
                        // Multi-draw: iterate over each prototype's GPU data
                        let mut total_instances_drawn = 0u32;
                        let mut buffer_offset = 0u64;

                        for proto_data in &self.prototype_gpu_data {
                            if let Some(instances) =
                                self.instance_groups.get(&proto_data.prototype_id)
                            {
                                if instances.is_empty() {
                                    continue;
                                }

                                // Write instances for this prototype to the instance buffer
                                self.queue.write_buffer(
                                    &self.instance_buffer,
                                    buffer_offset,
                                    bytemuck::cast_slice(instances),
                                );

                                // Set buffers and draw
                                render_pass
                                    .set_vertex_buffer(0, proto_data.vertex_buffer.slice(..));
                                render_pass.set_vertex_buffer(
                                    1,
                                    self.instance_buffer.slice(buffer_offset..),
                                );
                                render_pass.set_index_buffer(
                                    proto_data.index_buffer.slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                render_pass.draw_indexed(
                                    0..proto_data.num_indices,
                                    0,
                                    0..instances.len() as u32,
                                );

                                total_instances_drawn += instances.len() as u32;
                                buffer_offset +=
                                    (instances.len() * std::mem::size_of::<InstanceData>()) as u64;
                            }
                        }

                        log::trace!(
                            "Multi-draw: {} prototypes, {} total instances",
                            self.prototype_gpu_data.len(),
                            total_instances_drawn
                        );
                    } else {
                        // Single-draw: use combined vertex/index buffers
                        log::trace!(
                            "Drawing {} indices x {} near instances + {} LOD box instances (of {} total)",
                            self.num_indices,
                            self.visible_instance_count,
                            self.lod_box_instance_count,
                            self.num_instances
                        );
                        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                        render_pass.set_index_buffer(
                            self.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.draw_indexed(
                            0..self.num_indices,
                            0,
                            0..self.visible_instance_count,
                        );
                    }

                    // Draw far instances as LOD box proxies (from same buffer, offset by near count)
                    if self.lod_box_instance_count > 0 {
                        let far_byte_offset = (self.visible_instance_count as usize
                            * std::mem::size_of::<InstanceData>())
                            as u64;
                        render_pass.set_vertex_buffer(0, self.lod_box_vertex_buffer.slice(..));
                        render_pass
                            .set_vertex_buffer(1, self.instance_buffer.slice(far_byte_offset..));
                        render_pass.set_index_buffer(
                            self.lod_box_index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.draw_indexed(
                            0..self.lod_box_num_indices,
                            0,
                            0..self.lod_box_instance_count,
                        );
                    }
                }

                // Render gnomon in bottom-right corner
                {
                    let mut gnomon_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Gnomon Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load, // Keep existing content
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None, // No depth testing for gnomon
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    // Set viewport to bottom-right corner of the active viewport
                    let gnomon_size = self.gnomon.size as f32;
                    let padding = 16.0;
                    let viewport_left = self.ui_left_panel_width;
                    let viewport_right = (self.size.0 as f32) - self.ui_right_panel_width;
                    let viewport_bottom = (self.size.1 as f32) - self.ui_bottom_panel_height;
                    let viewport_width = (viewport_right - viewport_left).max(0.0);
                    let viewport_height = viewport_bottom.max(0.0);

                    if viewport_width >= gnomon_size + padding
                        && viewport_height >= gnomon_size + padding
                    {
                        let x =
                            (viewport_right - gnomon_size - padding).max(viewport_left + padding);
                        let y = (viewport_bottom - gnomon_size - padding).max(padding);

                        gnomon_pass.set_viewport(
                            x,           // x (right side)
                            y,           // y (bottom, wgpu uses top-left origin)
                            gnomon_size, // width
                            gnomon_size, // height
                            0.0,         // min_depth
                            1.0,         // max_depth
                        );

                        self.gnomon.render(&mut gnomon_pass);
                    }
                }
            }
            RenderMode::Ivar => {
                // Check camera dirty and restart render if needed
                if self.ivar_state.check_camera_dirty(&self.camera)
                    && !self.ivar_state.render_complete
                {
                    log::info!("Camera moved - restarting Ivar render");
                    self.start_ivar_render();
                }

                // Poll for scene build completion
                self.poll_scene_build();

                // Poll for completed buckets
                self.poll_ivar_messages();

                // Upload current image buffer to texture (uses selected AOV channel)
                self.upload_ivar_pixels();

                // Render fullscreen quad with Ivar texture
                {
                    let mut ivar_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Ivar Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    ivar_pass.set_pipeline(&self.ivar_pipeline);
                    ivar_pass.set_bind_group(0, &self.ivar_bind_group, &[]);
                    ivar_pass.draw(0..3, 0..1); // Single fullscreen triangle
                }
            }
        }

        // Render egui on top
        {
            let mut egui_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime(); // Need 'static lifetime for egui renderer

            self.egui_renderer
                .render(&mut egui_pass, &paint_jobs, &screen_descriptor);
        }

        // Free egui textures
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
