use anyhow::Result;
use std::path::Path;

use wgpu::{util::DeviceExt, Device, Instance, Queue, Surface, SurfaceConfiguration};

use bif_math::{Camera,Mat4, Vec3};

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
                front_face: wgpu::FrontFace::Cw,  // USD/Houdini uses clockwise winding
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
                front_face: wgpu::FrontFace::Cw,  // USD/Houdini uses clockwise winding
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
        
        // Generate instances from scene
        let instances: Vec<InstanceData> = scene.instances.iter()
            .map(|inst| {
                let model_matrix = inst.model_matrix();
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
        
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            if !show_ui {
                return;
            }
            
            egui::SidePanel::left("stats_panel")
                .default_width(300.0)
                .show(ctx, |ui| {
                    ui.heading("BIF Viewer");
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
        
        // Main render pass
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
