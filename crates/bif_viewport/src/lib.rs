use anyhow::Result;
use wgpu::{util::DeviceExt, Device, Instance, Queue, Surface, SurfaceConfiguration};
use bif_math::{Camera, Mat4, Vec3};
use std::path::Path;

/// Mesh data loaded from file
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
}

/// Camera uniform data for GPU
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
        }
    }
    
    fn update_view_proj(&mut self, camera: &Camera) {
        self.view_proj = camera.view_projection_matrix().to_cols_array_2d();
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
    pub camera: Camera,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    mesh_bounds_min: Vec3,
    mesh_bounds_max: Vec3,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
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
            present_mode: wgpu::PresentMode::Fifo, // VSync
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
        camera.far = camera_distance * 10.0;   // 10x distance for safety
        
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
                buffers: &[Vertex::desc()],
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
                front_face: wgpu::FrontFace::Ccw,
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
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            mesh_bounds_min: mesh_data.bounds_min,
            mesh_bounds_max: mesh_data.bounds_max,
            depth_texture,
            depth_view,
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
    
    /// Render a frame with the given clear color
    pub fn render(&self, clear_color: wgpu::Color) -> Result<()> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        
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
            
            // Draw the mesh
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }
        
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        
        Ok(())
    }
}
