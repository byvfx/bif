use anyhow::Result;
use std::path::Path;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::sync::mpsc;
use std::time::Instant;

use wgpu::{util::DeviceExt, Device, Instance, Queue, Surface, SurfaceConfiguration};

use bif_math::{Camera, Mat4, Vec3};

// Re-export bif_renderer types for Ivar integration
use bif_renderer::{
    Bucket, BucketResult, generate_buckets, render_bucket, DEFAULT_BUCKET_SIZE,
    Triangle, BvhNode, Hittable, Lambertian, Color, RenderConfig, ImageBuffer,
};

/// Render mode selection: GPU viewport or Ivar CPU path tracer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Real-time GPU viewport rendering (wgpu)
    #[default]
    Vulkan,
    /// Ivar CPU path tracer for production quality
    Ivar,
}

impl RenderMode {
    /// Get display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            RenderMode::Vulkan => "Vulkan",
            RenderMode::Ivar => "Ivar",
        }
    }
}

/// Snapshot of camera state for dirty detection
#[derive(Debug, Clone, Copy)]
pub struct CameraSnapshot {
    pub position: Vec3,
    pub target: Vec3,
    pub fov_y: f32,
}

impl CameraSnapshot {
    /// Create snapshot from viewport camera
    pub fn from_camera(camera: &Camera) -> Self {
        Self {
            position: camera.position,
            target: camera.target,
            fov_y: camera.fov_y,
        }
    }
    
    /// Check if camera has changed significantly
    pub fn has_changed(&self, other: &Self) -> bool {
        const EPSILON: f32 = 0.0001;
        (self.position - other.position).length() > EPSILON
            || (self.target - other.target).length() > EPSILON
            || (self.fov_y - other.fov_y).abs() > EPSILON
    }
}

/// Message from Ivar background render thread
#[derive(Debug)]
pub enum IvarMessage {
    /// A bucket has been completed
    BucketComplete(BucketResult),
    /// Entire render is complete
    RenderComplete {
        elapsed_secs: f32,
    },
    /// Render was cancelled
    Cancelled,
}

/// State for Ivar progressive rendering
pub struct IvarState {
    /// Current render mode
    pub mode: RenderMode,
    /// Accumulated image buffer
    pub image_buffer: Option<ImageBuffer>,
    /// List of buckets for current render
    pub buckets: Vec<Bucket>,
    /// Number of buckets completed
    pub buckets_completed: usize,
    /// Whether render is complete
    pub render_complete: bool,
    /// Cancel flag for background thread
    pub cancel_flag: Arc<AtomicBool>,
    /// Receiver for bucket completion messages
    pub receiver: Option<mpsc::Receiver<IvarMessage>>,
    /// Last camera snapshot for dirty detection
    pub last_camera_snapshot: Option<CameraSnapshot>,
    /// Time when render started
    pub render_start_time: Option<Instant>,
    /// Cached world geometry (BVH of triangles)
    /// TODO: Invalidate world cache when scene is reloaded or modified
    /// TODO: Add "Rebuild Scene" button to manually invalidate cached BVH
    pub world: Option<Arc<BvhNode>>,
    /// Samples per pixel for rendering
    /// TODO: Expose SPP in UI
    pub samples_per_pixel: u32,
    /// Max bounce depth
    pub max_depth: u32,
}

impl Default for IvarState {
    fn default() -> Self {
        Self {
            mode: RenderMode::Vulkan,
            image_buffer: None,
            buckets: Vec::new(),
            buckets_completed: 0,
            render_complete: false,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            receiver: None,
            last_camera_snapshot: None,
            render_start_time: None,
            world: None,
            samples_per_pixel: 16,  // Lower for interactive preview
            max_depth: 8,
        }
    }
}

impl IvarState {
    /// Reset render state (call when starting new render)
    pub fn reset_render(&mut self, width: u32, height: u32) {
        // Cancel any existing render
        self.cancel_flag.store(true, Ordering::Relaxed);
        
        // Create new cancel flag
        self.cancel_flag = Arc::new(AtomicBool::new(false));
        
        // Clear state
        self.image_buffer = Some(ImageBuffer::new(width, height));
        self.buckets = generate_buckets(width, height, DEFAULT_BUCKET_SIZE);
        self.buckets_completed = 0;
        self.render_complete = false;
        self.receiver = None;
        self.render_start_time = Some(Instant::now());
    }
    
    /// Check if camera has moved and render needs restart
    pub fn check_camera_dirty(&mut self, camera: &Camera) -> bool {
        let current = CameraSnapshot::from_camera(camera);
        
        match &self.last_camera_snapshot {
            Some(last) if !last.has_changed(&current) => false,
            _ => {
                self.last_camera_snapshot = Some(current);
                true
            }
        }
    }
    
    /// Get render progress as percentage
    pub fn progress(&self) -> f32 {
        if self.buckets.is_empty() {
            return 0.0;
        }
        (self.buckets_completed as f32 / self.buckets.len() as f32) * 100.0
    }
    
    /// Get elapsed render time in seconds
    pub fn elapsed_secs(&self) -> f32 {
        self.render_start_time
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0)
    }
}

#[derive(Clone)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

impl MeshData {
    /// Get mesh center
    pub fn center(&self) -> Vec3 {
        (self.bounds_min + self.bounds_max) * 0.5
    }
    
    /// Get mesh size (diagonal of bounding box)
    pub fn size(&self) -> f32 {
        (self.bounds_max - self.bounds_min).length()
    }
    
    /// Load an OBJ file into mesh data
    pub fn load_obj<P: AsRef<Path>>(path: P) -> Result<Self> {
        let (models, _materials) = tobj::load_obj(
            path.as_ref(),
            &tobj::LoadOptions {
                single_index: true,
                triangulate: true,
                ..Default::default()
            },
        )?;
        
        if models.is_empty() {
            anyhow::bail!("No models found in OBJ file");
        }
        
        // Take first model
        let model = &models[0];
        let mesh = &model.mesh;
        
        // Build vertices with normals
        let mut vertices = Vec::new();
        let vertex_count = mesh.positions.len() / 3;
        
        let has_normals = !mesh.normals.is_empty();
        log::info!("Mesh has normals: {}", has_normals);
        
        // If no normals, compute per-face normals
        let computed_normals = if !has_normals {
            log::info!("Computing per-face normals...");
            let mut normals = vec![[0.0f32; 3]; vertex_count];
            
            // Compute face normals and accumulate at vertices
            for face in mesh.indices.chunks(3) {
                let i0 = face[0] as usize;
                let i1 = face[1] as usize;
                let i2 = face[2] as usize;
                
                let p0 = Vec3::from_slice(&mesh.positions[i0 * 3..i0 * 3 + 3]);
                let p1 = Vec3::from_slice(&mesh.positions[i1 * 3..i1 * 3 + 3]);
                let p2 = Vec3::from_slice(&mesh.positions[i2 * 3..i2 * 3 + 3]);
                
                let edge1 = p1 - p0;
                let edge2 = p2 - p0;
                let face_normal = edge1.cross(edge2).normalize();
                
                // Accumulate at each vertex
                for &idx in &[i0, i1, i2] {
                    normals[idx][0] += face_normal.x;
                    normals[idx][1] += face_normal.y;
                    normals[idx][2] += face_normal.z;
                }
            }
            
            // Normalize accumulated normals
            for normal in &mut normals {
                let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
                if len > 0.0 {
                    normal[0] /= len;
                    normal[1] /= len;
                    normal[2] /= len;
                }
            }
            
            Some(normals)
        } else {
            None
        };
        
        for i in 0..vertex_count {
            let pos_idx = i * 3;
            
            // Use computed normals if available, otherwise from file
            let normal = if let Some(ref computed) = computed_normals {
                computed[i]
            } else if has_normals {
                let norm_idx = i * 3;
                [
                    mesh.normals[norm_idx],
                    mesh.normals[norm_idx + 1],
                    mesh.normals[norm_idx + 2],
                ]
            } else {
                [0.0, 1.0, 0.0]
            };
            
            // Color from normal (not needed anymore, shader uses normal directly)
            let color = [
                normal[0].abs(),
                normal[1].abs(),
                normal[2].abs(),
            ];
            
            vertices.push(Vertex {
                position: [
                    mesh.positions[pos_idx],
                    mesh.positions[pos_idx + 1],
                    mesh.positions[pos_idx + 2],
                ],
                normal,
                color,
            });
        }
        
        // Calculate bounding box
        let mut bounds_min = Vec3::splat(f32::INFINITY);
        let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);
        
        for vertex in &vertices {
            let pos = Vec3::from_array(vertex.position);
            bounds_min = bounds_min.min(pos);
            bounds_max = bounds_max.max(pos);
        }
        
        Ok(Self {
            vertices,
            indices: mesh.indices.clone(),
            bounds_min,
            bounds_max,
        })
    }
    
    /// Convert a bif_core::Mesh to GPU-ready MeshData
    pub fn from_core_mesh(mesh: &bif_core::Mesh) -> Self {
        let mut vertices = Vec::with_capacity(mesh.positions.len());
        
        // Get normals (should already be computed by loader)
        let default_normal = Vec3::Y;
        
        for (i, pos) in mesh.positions.iter().enumerate() {
            let normal = mesh.normals
                .as_ref()
                .and_then(|n| n.get(i))
                .unwrap_or(&default_normal);
            
            // Color from normal for visualization
            let color = [normal.x.abs(), normal.y.abs(), normal.z.abs()];
            
            vertices.push(Vertex {
                position: [pos.x, pos.y, pos.z],
                normal: [normal.x, normal.y, normal.z],
                color,
            });
        }
        
        // Get bounds from Aabb
        let bounds_min = Vec3::new(mesh.bounds.x.min, mesh.bounds.y.min, mesh.bounds.z.min);
        let bounds_max = Vec3::new(mesh.bounds.x.max, mesh.bounds.y.max, mesh.bounds.z.max);
        
        Self {
            vertices,
            indices: mesh.indices.clone(),
            bounds_min,
            bounds_max,
        }
    }
}

/// Camera uniform data for GPU
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    view: [[f32; 4]; 4],
}

impl CameraUniform {
    fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            view: Mat4::IDENTITY.to_cols_array_2d(),
        }
    }
    
    fn update_view_proj(&mut self, camera: &Camera) {
        self.view_proj = camera.view_projection_matrix().to_cols_array_2d();
        self.view = camera.view_matrix().to_cols_array_2d();
    }
}

/// Gnomon uniform data for GPU (camera rotation only)
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GnomonUniform {
    view_rotation: [[f32; 4]; 4],
}

impl GnomonUniform {
    fn new() -> Self {
        Self {
            view_rotation: Mat4::IDENTITY.to_cols_array_2d(),
        }
    }
    
    fn update_from_camera(&mut self, camera: &Camera) {
        // Extract rotation from view matrix (zero out translation)
        let view = camera.view_matrix();
        // The view matrix is [R | t], we want just R with no translation
        let rotation = Mat4::from_cols(
            view.col(0),
            view.col(1),
            view.col(2),
            bif_math::Vec4::new(0.0, 0.0, 0.0, 1.0),
        );
        self.view_rotation = rotation.to_cols_array_2d();
    }
}

/// Gnomon vertex (position + color)
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GnomonVertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl GnomonVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GnomonVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
    
    /// Create gnomon axis vertices (origin to X, Y, Z with colors)
    fn create_axes() -> Vec<Self> {
        vec![
            // X axis (red)
            GnomonVertex { position: [0.0, 0.0, 0.0], color: [1.0, 0.2, 0.2] },
            GnomonVertex { position: [1.0, 0.0, 0.0], color: [1.0, 0.2, 0.2] },
            // Y axis (green)
            GnomonVertex { position: [0.0, 0.0, 0.0], color: [0.2, 1.0, 0.2] },
            GnomonVertex { position: [0.0, 1.0, 0.0], color: [0.2, 1.0, 0.2] },
            // Z axis (blue)
            GnomonVertex { position: [0.0, 0.0, 0.0], color: [0.2, 0.5, 1.0] },
            GnomonVertex { position: [0.0, 0.0, 1.0], color: [0.2, 0.5, 1.0] },
        ]
    }
}

/// Vertex data for rendering
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Instance data for GPU instancing
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceData {
    pub model_matrix: [[f32; 4]; 4],
}

impl InstanceData {
    const ATTRIBS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceData>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
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
    mesh_bounds_min: Vec3,
    mesh_bounds_max: Vec3,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    
    // Gnomon resources
    gnomon_pipeline: wgpu::RenderPipeline,
    gnomon_vertex_buffer: wgpu::Buffer,
    gnomon_uniform: GnomonUniform,
    gnomon_buffer: wgpu::Buffer,
    gnomon_bind_group: wgpu::BindGroup,
    
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
    num_triangles: u32,
    pub gnomon_size: u32,
    
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
    instance_transforms: Vec<Mat4>,
}

impl Renderer {
    /// Create a depth texture for the given size
    fn create_depth_texture(device: &Device, size: (u32, u32)) -> (wgpu::Texture, wgpu::TextureView) {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        (depth_texture, depth_view)
    }
    
    /// Create Ivar texture for displaying path tracer output
    fn create_ivar_texture(device: &Device, size: (u32, u32)) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Ivar Output Texture"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        (texture, view)
    }
    
    /// Create Ivar fullscreen pipeline and bind group
    fn create_ivar_pipeline(
        device: &Device,
        surface_format: wgpu::TextureFormat,
        texture_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> (wgpu::RenderPipeline, wgpu::BindGroup, wgpu::BindGroupLayout) {
        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Ivar Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        
        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Ivar Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        
        // Create shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Ivar Fullscreen Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/fullscreen.wgsl").into()),
        });
        
        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Ivar Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        
        // Create render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Ivar Fullscreen Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],  // No vertex buffer needed for fullscreen triangle
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,  // No depth for fullscreen quad
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });
        
        (pipeline, bind_group, bind_group_layout)
    }
    
    /// Upload Ivar image buffer to GPU texture
    fn upload_ivar_pixels(&self, image: &ImageBuffer) {
        let rgba = image.to_rgba();
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
                bytes_per_row: Some(4 * image.width),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
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
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
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
        
        // Load Lucy model first to get mesh bounds
        let mesh_path = std::env::current_dir()?
            .join("legacy/go-raytracing/assets/models/lucy_low.obj");
        
        log::info!("Loading mesh from: {:?}", mesh_path);
        let mesh_data = MeshData::load_obj(&mesh_path)?;
        log::info!("Loaded {} vertices, {} indices", mesh_data.vertices.len(), mesh_data.indices.len());
        log::info!("Mesh bounds: min={:?}, max={:?}", mesh_data.bounds_min, mesh_data.bounds_max);
        log::info!("Mesh center: {:?}, size: {:.2}", mesh_data.center(), mesh_data.size());
        
        // Calculate proper camera distance to frame the mesh
        let mesh_center = mesh_data.center();
        let mesh_size = mesh_data.size();
        let camera_distance = mesh_size * 1.5;
        
        // Create camera positioned to view the mesh
        let aspect = size.width as f32 / size.height as f32;
        let camera = Camera::new(
            mesh_center + Vec3::new(0.0, 0.0, camera_distance),
            mesh_center,
            aspect,
        );
        
        // Adjust camera near/far planes for mesh size
        let mut camera = camera;
        camera.near = camera_distance * 0.01; // 1% of distance
        camera.far = camera_distance * 20.0;   // 20x distance for safety
        
        log::info!("Camera positioned at {:?}, looking at {:?}, distance {:.2}", 
                   camera.position, camera.target, camera.distance);
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
        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
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
        
        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Basic Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/basic.wgsl").into()),
        });
        
        // Create render pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
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
                front_face: wgpu::FrontFace::Ccw,  // Standard CCW winding
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
        
        // Create vertex and index buffers from loaded mesh data
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
        let (depth_texture, depth_view) = Self::create_depth_texture(&device, (size.width, size.height));
        
        // Generate 100 instances in a 10x10 grid
        let grid_size = 10;
        let spacing = mesh_size * 1.5; // 1.5x mesh size spacing
        let mut instances = Vec::new();
        
        for x in 0..grid_size {
            for z in 0..grid_size {
                let offset_x = (x as f32 - grid_size as f32 / 2.0) * spacing;
                let offset_z = (z as f32 - grid_size as f32 / 2.0) * spacing;
                
                // Create translation matrix
                let model_matrix = Mat4::from_translation(Vec3::new(offset_x, 0.0, offset_z));
                
                instances.push(InstanceData {
                    model_matrix: model_matrix.to_cols_array_2d(),
                });
            }
        }
        
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        log::info!("Created {} instances in {}x{} grid with spacing {:.2}", instances.len(), grid_size, grid_size, spacing);
        
        // Initialize egui
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,  // max_texture_side (use default)
        );
        
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            None,  // No depth testing for egui
            1,
            false,  // allow_srgb_render_target
        );
        
        log::info!("egui initialized");
        
        // Create gnomon resources
        let gnomon_vertices = GnomonVertex::create_axes();
        let gnomon_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Gnomon Vertex Buffer"),
            contents: bytemuck::cast_slice(&gnomon_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        let mut gnomon_uniform = GnomonUniform::new();
        gnomon_uniform.update_from_camera(&camera);
        
        let gnomon_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Gnomon Buffer"),
            contents: bytemuck::cast_slice(&[gnomon_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        
        let gnomon_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Gnomon Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        
        let gnomon_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Gnomon Bind Group"),
            layout: &gnomon_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: gnomon_buffer.as_entire_binding(),
            }],
        });
        
        let gnomon_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Gnomon Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/gnomon.wgsl").into()),
        });
        
        let gnomon_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Gnomon Pipeline Layout"),
            bind_group_layouts: &[&gnomon_bind_group_layout],
            push_constant_ranges: &[],
        });
        
        let gnomon_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Gnomon Pipeline"),
            layout: Some(&gnomon_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &gnomon_shader,
                entry_point: "vs_main",
                buffers: &[GnomonVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &gnomon_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,  // No culling for lines
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,  // No depth for gnomon overlay
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });
        
        log::info!("Gnomon initialized");
        
        // Calculate stats - TODO: Track polygon count from source mesh for accuracy
        let num_triangles = mesh_data.indices.len() as u32 / 3;
        
        // Create Ivar resources for CPU path tracer display
        let (ivar_texture, ivar_texture_view) = Self::create_ivar_texture(&device, (size.width, size.height));
        
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
        
        let (ivar_pipeline, ivar_bind_group, _) = Self::create_ivar_pipeline(
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
            num_indices: mesh_data.indices.len() as u32,
            instance_buffer,
            num_instances: instances.len() as u32,
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            mesh_bounds_min: mesh_data.bounds_min,
            mesh_bounds_max: mesh_data.bounds_max,
            depth_texture,
            depth_view,
            gnomon_pipeline,
            gnomon_vertex_buffer,
            gnomon_uniform,
            gnomon_buffer,
            gnomon_bind_group,
            egui_ctx,
            egui_state,
            egui_renderer,
            show_ui: true,
            fps: 0.0,
            frame_count: 0,
            fps_update_timer: 0.0,
            num_triangles,
            gnomon_size: 80,
            ivar_state: IvarState::default(),
            ivar_texture,
            ivar_texture_view,
            ivar_sampler,
            ivar_bind_group,
            ivar_pipeline,
            mesh_data,
            instance_transforms: vec![Mat4::IDENTITY],  // Default single instance at origin
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
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
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
        
        log::info!("Loaded {} vertices, {} indices from USD scene", 
                   mesh_data.vertices.len(), mesh_data.indices.len());
        log::info!("Mesh bounds: min={:?}, max={:?}", mesh_data.bounds_min, mesh_data.bounds_max);
        log::info!("Scene has {} prototypes, {} instances", 
                   scene.prototype_count(), scene.instance_count());
        
        // Calculate proper camera distance to frame the scene
        // TODO: Add frame_scene() method that can be called to re-frame based on world_bounds
        let world_bounds = scene.world_bounds();
        log::info!("World bounds: min=({:.1}, {:.1}, {:.1}), max=({:.1}, {:.1}, {:.1})",
            world_bounds.x.min, world_bounds.y.min, world_bounds.z.min,
            world_bounds.x.max, world_bounds.y.max, world_bounds.z.max);
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
        
        log::info!("Camera positioned at {:?}, looking at {:?}", camera.position, camera.target);
        
        // Create camera uniform buffer
        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&camera);
        
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        
        // Create bind group layout for camera
        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
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
        
        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Basic Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/basic.wgsl").into()),
        });
        
        // Create render pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
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
                front_face: wgpu::FrontFace::Ccw,  // Standard CCW winding
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
        let (depth_texture, depth_view) = Self::create_depth_texture(&device, (size.width, size.height));
        
        // Generate instances from scene - collect both GPU data and transforms for Ivar
        let mut instance_transforms: Vec<Mat4> = Vec::with_capacity(scene.instances.len());
        let instances: Vec<InstanceData> = scene.instances.iter()
            .enumerate()
            .map(|(i, inst)| {
                let model_matrix = inst.model_matrix();
                instance_transforms.push(model_matrix);
                // Debug: log first few instance transforms
                if i < 5 || i == scene.instances.len() - 1 {
                    let translation = model_matrix.w_axis.truncate();
                    log::info!("Instance {}: translation = {:?}", i, translation);
                }
                InstanceData {
                    model_matrix: model_matrix.to_cols_array_2d(),
                }
            })
            .collect();
        
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        log::info!("Created {} instances from USD scene", instances.len());
        
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
        
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            None,
            1,
            false,
        );
        
        log::info!("Renderer initialized with USD scene");
        
        // Create gnomon resources
        let gnomon_vertices = GnomonVertex::create_axes();
        let gnomon_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Gnomon Vertex Buffer"),
            contents: bytemuck::cast_slice(&gnomon_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        let mut gnomon_uniform = GnomonUniform::new();
        gnomon_uniform.update_from_camera(&camera);
        
        let gnomon_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Gnomon Buffer"),
            contents: bytemuck::cast_slice(&[gnomon_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        
        let gnomon_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Gnomon Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        
        let gnomon_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Gnomon Bind Group"),
            layout: &gnomon_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: gnomon_buffer.as_entire_binding(),
            }],
        });
        
        let gnomon_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Gnomon Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/gnomon.wgsl").into()),
        });
        
        let gnomon_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Gnomon Pipeline Layout"),
            bind_group_layouts: &[&gnomon_bind_group_layout],
            push_constant_ranges: &[],
        });
        
        let gnomon_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Gnomon Pipeline"),
            layout: Some(&gnomon_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &gnomon_shader,
                entry_point: "vs_main",
                buffers: &[GnomonVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &gnomon_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });
        
        log::info!("Gnomon initialized");
        
        // Calculate stats - TODO: Track polygon count from source mesh for accuracy
        let num_triangles = mesh_data.indices.len() as u32 / 3;
        
        // Create Ivar resources for CPU path tracer display
        let (ivar_texture, ivar_texture_view) = Self::create_ivar_texture(&device, (size.width, size.height));
        
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
        
        let (ivar_pipeline, ivar_bind_group, _) = Self::create_ivar_pipeline(
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
            num_indices: mesh_data.indices.len() as u32,
            instance_buffer,
            num_instances: instances.len() as u32,
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            mesh_bounds_min: mesh_data.bounds_min,
            mesh_bounds_max: mesh_data.bounds_max,
            depth_texture,
            depth_view,
            gnomon_pipeline,
            gnomon_vertex_buffer,
            gnomon_uniform,
            gnomon_buffer,
            gnomon_bind_group,
            egui_ctx,
            egui_state,
            egui_renderer,
            show_ui: true,
            fps: 0.0,
            frame_count: 0,
            fps_update_timer: 0.0,
            num_triangles,
            gnomon_size: 80,
            ivar_state: IvarState::default(),
            ivar_texture,
            ivar_texture_view,
            ivar_sampler,
            ivar_bind_group,
            ivar_pipeline,
            mesh_data,
            instance_transforms,
        })
    }
    
    /// Handle window resize
    pub fn resize(&mut self, new_size: (u32, u32)) {
        if new_size.0 > 0 && new_size.1 > 0 {
            self.size = new_size;
            self.config.width = new_size.0;
            self.config.height = new_size.1;
            self.surface.configure(&self.device, &self.config);
            
            // Recreate depth texture with new size
            let (depth_texture, depth_view) = Self::create_depth_texture(&self.device, new_size);
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;
            
            // Recreate Ivar texture with new size
            let (ivar_texture, ivar_texture_view) = Self::create_ivar_texture(&self.device, new_size);
            self.ivar_texture = ivar_texture;
            self.ivar_texture_view = ivar_texture_view;
            
            // Recreate Ivar bind group with new texture view
            let (_, ivar_bind_group, _) = Self::create_ivar_pipeline(
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
        self.gnomon_uniform.update_from_camera(&self.camera);
        self.queue.write_buffer(
            &self.gnomon_buffer,
            0,
            bytemuck::cast_slice(&[self.gnomon_uniform]),
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
        log::info!("Framed mesh at center {:?}, distance {:.2}", mesh_center, camera_distance);
    }
    
    /// Handle egui window event - returns true if event was consumed by egui
    pub fn handle_egui_event(&mut self, window: &winit::window::Window, event: &winit::event::WindowEvent) -> bool {
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
    
    /// Build Ivar scene from viewport mesh data
    fn build_ivar_scene(&mut self) {
        if self.ivar_state.world.is_some() {
            // Scene already built, skip
            return;
        }
        
        log::info!("Building Ivar scene from mesh data ({} instances)...", self.instance_transforms.len());
        
        // Build list of hittable objects
        let mut objects: Vec<Box<dyn Hittable + Send + Sync>> = Vec::new();
        
        // Create triangles for each instance by transforming the prototype mesh vertices
        for (inst_idx, transform) in self.instance_transforms.iter().enumerate() {
            for i in (0..self.mesh_data.indices.len()).step_by(3) {
                let i0 = self.mesh_data.indices[i] as usize;
                let i1 = self.mesh_data.indices[i + 1] as usize;
                let i2 = self.mesh_data.indices[i + 2] as usize;
                
                // Get local-space vertices
                let v0_local = Vec3::from_array(self.mesh_data.vertices[i0].position);
                let v1_local = Vec3::from_array(self.mesh_data.vertices[i1].position);
                let v2_local = Vec3::from_array(self.mesh_data.vertices[i2].position);
                
                // Transform to world space
                let v0 = transform.transform_point3(v0_local);
                let v1 = transform.transform_point3(v1_local);
                let v2 = transform.transform_point3(v2_local);
                
                let tri = Triangle::new(v0, v1, v2, Lambertian::new(Color::new(0.7, 0.7, 0.7)));
                objects.push(Box::new(tri));
            }
            
            if inst_idx == 0 || inst_idx == self.instance_transforms.len() - 1 {
                let translation = transform.w_axis.truncate();
                log::debug!("Ivar instance {}: translation = {:?}", inst_idx, translation);
            }
        }
        
        log::info!("Created {} triangles for Ivar ({} instances x {} tris/instance)", 
            objects.len(), 
            self.instance_transforms.len(),
            self.mesh_data.indices.len() / 3);
        
        // Build BVH from objects
        let world = BvhNode::new(objects);
        self.ivar_state.world = Some(Arc::new(world));
        
        log::info!("Ivar BVH built successfully");
    }
    
    /// Create Ivar camera from viewport camera
    fn create_ivar_camera(&self) -> bif_renderer::Camera {
        let mut camera = bif_renderer::Camera::new()
            .with_resolution(self.size.0, self.size.1)
            .with_position(
                self.camera.position,
                self.camera.target,
                Vec3::Y,
            )
            .with_lens(
                self.camera.fov_y.to_degrees(),
                0.0,  // No DOF for preview
                (self.camera.target - self.camera.position).length(),
            )
            .with_quality(self.ivar_state.samples_per_pixel, self.ivar_state.max_depth);
        
        camera.initialize();
        camera
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
        };
        
        let start_time = Instant::now();
        
        log::info!("Starting Ivar render: {}x{} @ {} SPP, {} buckets",
            self.size.0, self.size.1,
            self.ivar_state.samples_per_pixel,
            buckets.len());
        
        // Spawn background render thread
        std::thread::spawn(move || {
            use rayon::prelude::*;
            
            // Process buckets in parallel
            buckets.par_iter().for_each(|bucket| {
                // Check for cancellation
                if cancel_flag.load(Ordering::Relaxed) {
                    return;
                }
                
                // Render bucket
                let pixels = render_bucket(bucket, &ivar_camera, world.as_ref(), &config);
                
                // Send result
                let result = BucketResult::new(*bucket, pixels);
                let _ = tx.send(IvarMessage::BucketComplete(result));
            });
            
            // Check if cancelled
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(IvarMessage::Cancelled);
            } else {
                let elapsed = start_time.elapsed().as_secs_f32();
                let _ = tx.send(IvarMessage::RenderComplete { elapsed_secs: elapsed });
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
                    // Copy pixels to image buffer
                    if let Some(ref mut image) = self.ivar_state.image_buffer {
                        for local_y in 0..result.bucket.height {
                            for local_x in 0..result.bucket.width {
                                let global_x = result.bucket.x + local_x;
                                let global_y = result.bucket.y + local_y;
                                let pixel_idx = (local_y * result.bucket.width + local_x) as usize;
                                if pixel_idx < result.pixels.len() {
                                    image.set(global_x, global_y, result.pixels[pixel_idx]);
                                }
                            }
                        }
                    }
                    self.ivar_state.buckets_completed += 1;
                }
                IvarMessage::RenderComplete { elapsed_secs } => {
                    self.ivar_state.render_complete = true;
                    log::info!("Ivar render complete in {:.2}s", elapsed_secs);
                }
                IvarMessage::Cancelled => {
                    log::info!("Ivar render cancelled");
                }
            }
        }
    }
    
    /// Render a frame with the given clear color
    pub fn render(&mut self, clear_color: wgpu::Color, window: &winit::window::Window) -> Result<()> {
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
        let num_triangles = self.num_triangles;
        let mesh_bounds_min = self.mesh_bounds_min;
        let mesh_bounds_max = self.mesh_bounds_max;
        let size = self.size;
        let mut gnomon_size = self.gnomon_size;
        
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
                return;
            }
            
            egui::SidePanel::left("stats_panel")
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
                        
                        // Progress bar
                        let progress_bar = egui::ProgressBar::new(ivar_progress / 100.0)
                            .text(format!("{:.1}%", ivar_progress));
                        ui.add(progress_bar);
                        
                        // Stats
                        ui.label(format!("Buckets: {} / {}", ivar_buckets_completed, ivar_total_buckets));
                        ui.label(format!("SPP: {}", ivar_spp));
                        ui.label(format!("Time: {:.1}s", ivar_elapsed));
                        
                        if ivar_render_complete {
                            ui.colored_label(egui::Color32::GREEN, "✓ Render Complete");
                        } else if ivar_buckets_completed > 0 {
                            ui.colored_label(egui::Color32::YELLOW, "⟳ Rendering...");
                        }
                        
                        // TODO: Add progressive multi-pass rendering (1 SPP preview → full SPP)
                    }
                    
                    ui.separator();
                    
                    // FPS Counter
                    ui.label(format!("FPS: {:.1}", fps));
                    ui.separator();
                    
                    // Scene Stats
                    ui.collapsing("Scene Stats", |ui| {
                        ui.label(format!("Instances: {}", num_instances));
                        ui.label(format!("Triangles: {}", num_triangles * num_instances));
                        // TODO: Track actual polygon count from source mesh for accuracy
                        // For now estimate: quads ≈ triangles * 2/3
                        let estimated_polys = (num_triangles as f32 * 0.67) as u32 * num_instances;
                        ui.label(format!("Polygons (est): {}", estimated_polys));
                        ui.label(format!("Triangles/Instance: {}", num_triangles));
                    });
                    
                    ui.separator();
                    
                    // Camera Stats
                    ui.collapsing("Camera", |ui| {
                        ui.label(format!("Position: ({:.2}, {:.2}, {:.2})", 
                            camera.position.x, 
                            camera.position.y, 
                            camera.position.z));
                        ui.label(format!("Target: ({:.2}, {:.2}, {:.2})", 
                            camera.target.x, 
                            camera.target.y, 
                            camera.target.z));
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
                        
                        ui.label(format!("Bounds Min: ({:.2}, {:.2}, {:.2})", 
                            mesh_bounds_min.x, 
                            mesh_bounds_min.y, 
                            mesh_bounds_min.z));
                        ui.label(format!("Bounds Max: ({:.2}, {:.2}, {:.2})", 
                            mesh_bounds_max.x, 
                            mesh_bounds_max.y, 
                            mesh_bounds_max.z));
                        ui.label(format!("Center: ({:.2}, {:.2}, {:.2})", 
                            mesh_center.x, 
                            mesh_center.y, 
                            mesh_center.z));
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
                });
        });
        
        // Update gnomon size from UI
        self.gnomon_size = gnomon_size;
        
        // Update render mode from UI - detect mode change
        let mode_changed = self.ivar_state.mode != render_mode;
        self.ivar_state.mode = render_mode;
        
        // Handle mode switch to Ivar - start render if needed
        if mode_changed && render_mode == RenderMode::Ivar {
            log::info!("Switched to Ivar mode - starting render");
            self.start_ivar_render();
        }
        
        self.egui_state.handle_platform_output(
            window,
            full_output.platform_output,
        );
        
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.0, self.size.1],
            pixels_per_point: window.scale_factor() as f32,
        };
        
        let paint_jobs = self.egui_ctx.tessellate(
            full_output.shapes,
            full_output.pixels_per_point,
        );
        
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        
        // Upload egui textures
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer.update_texture(
                &self.device,
                &self.queue,
                *id,
                image_delta,
            );
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
                {
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Render Pass"),
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
                    
                    // Draw all instances with one draw call
                    log::trace!("Drawing {} indices x {} instances", self.num_indices, self.num_instances);
                    render_pass.set_pipeline(&self.pipeline);
                    render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                    render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                    render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..self.num_indices, 0, 0..self.num_instances);
                }
                
                // Render gnomon in bottom-right corner
                {
                    let mut gnomon_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Gnomon Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,  // Keep existing content
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,  // No depth testing for gnomon
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    
                    // Set viewport to bottom-right corner
                    let gnomon_size = self.gnomon_size as f32;
                    gnomon_pass.set_viewport(
                        (self.size.0 as f32) - gnomon_size,  // x (right side)
                        (self.size.1 as f32) - gnomon_size,  // y (bottom, wgpu uses top-left origin)
                        gnomon_size,  // width
                        gnomon_size,  // height
                        0.0,  // min_depth
                        1.0,  // max_depth
                    );
                    
                    gnomon_pass.set_pipeline(&self.gnomon_pipeline);
                    gnomon_pass.set_bind_group(0, &self.gnomon_bind_group, &[]);
                    gnomon_pass.set_vertex_buffer(0, self.gnomon_vertex_buffer.slice(..));
                    gnomon_pass.draw(0..6, 0..1);  // 6 vertices (3 lines)
                }
            }
            RenderMode::Ivar => {
                // Check camera dirty and restart render if needed
                if self.ivar_state.check_camera_dirty(&self.camera) && !self.ivar_state.render_complete {
                    log::info!("Camera moved - restarting Ivar render");
                    self.start_ivar_render();
                }
                
                // Poll for completed buckets
                self.poll_ivar_messages();
                
                // Upload current image buffer to texture
                if let Some(ref image) = self.ivar_state.image_buffer {
                    self.upload_ivar_pixels(image);
                }
                
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
                    ivar_pass.draw(0..3, 0..1);  // Single fullscreen triangle
                }
            }
        }
        
        // Render egui on top
        {
            let mut egui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            }).forget_lifetime();  // Need 'static lifetime for egui renderer
            
            self.egui_renderer.render(&mut egui_pass, &paint_jobs, &screen_descriptor);
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
