use anyhow::Result;
use std::path::Path;
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
    generate_buckets, render_bucket, Bucket, BucketResult, BvhNode, Color, EmbreeScene, Hittable,
    ImageBuffer, Lambertian, RenderConfig, DEFAULT_BUCKET_SIZE,
};

// Scene browser and property inspector modules
pub mod node_graph;
pub mod property_inspector;
pub mod scene_browser;

pub use node_graph::{render_node_graph, NodeGraphEvent, NodeGraphState, SceneNode};
pub use property_inspector::{render_property_inspector, PrimProperties};
pub use scene_browser::{EmptyPrimProvider, PrimDataProvider, PrimDisplayInfo, SceneBrowserState};

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

/// Scene build status for async scene construction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildStatus {
    /// Scene has not been built yet
    #[default]
    NotStarted,
    /// Scene is currently being built in background thread
    Building,
    /// Scene build completed successfully
    Complete,
    /// Scene build failed
    Failed,
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
    RenderComplete { elapsed_secs: f32 },
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
    /// Scene build status for async construction
    pub build_status: BuildStatus,
    /// Receiver for scene build completion
    pub build_receiver: Option<mpsc::Receiver<Arc<BvhNode>>>,
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
            build_status: BuildStatus::NotStarted,
            build_receiver: None,
            samples_per_pixel: 16, // Lower for interactive preview
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

    /// Create a box mesh from AABB (for LOD proxy rendering)
    ///
    /// Generates a simple box with 8 vertices, 36 indices (12 triangles).
    /// Uses clockwise winding to match USD mesh convention.
    #[allow(clippy::vec_init_then_push)]
    pub fn from_aabb(aabb: &Aabb) -> Self {
        let min = aabb.min_point();
        let max = aabb.max_point();

        // 8 corner vertices of the box
        let corners = [
            Vec3::new(min.x, min.y, min.z), // 0: front-bottom-left
            Vec3::new(max.x, min.y, min.z), // 1: front-bottom-right
            Vec3::new(max.x, max.y, min.z), // 2: front-top-right
            Vec3::new(min.x, max.y, min.z), // 3: front-top-left
            Vec3::new(min.x, min.y, max.z), // 4: back-bottom-left
            Vec3::new(max.x, min.y, max.z), // 5: back-bottom-right
            Vec3::new(max.x, max.y, max.z), // 6: back-top-right
            Vec3::new(min.x, max.y, max.z), // 7: back-top-left
        ];

        // Face normals
        let normals = [
            Vec3::new(0.0, 0.0, -1.0), // front (negative Z)
            Vec3::new(0.0, 0.0, 1.0),  // back (positive Z)
            Vec3::new(-1.0, 0.0, 0.0), // left (negative X)
            Vec3::new(1.0, 0.0, 0.0),  // right (positive X)
            Vec3::new(0.0, -1.0, 0.0), // bottom (negative Y)
            Vec3::new(0.0, 1.0, 0.0),  // top (positive Y)
        ];

        let grey = [0.4, 0.4, 0.4]; // Slightly darker grey for LOD boxes

        // Build vertices with per-face normals (24 vertices = 6 faces x 4 corners)
        let mut vertices = Vec::with_capacity(24);

        // Front face (z = min) - vertices 0,1,2,3, normal -Z
        vertices.push(Vertex {
            position: corners[0].into(),
            normal: normals[0].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[1].into(),
            normal: normals[0].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[2].into(),
            normal: normals[0].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[3].into(),
            normal: normals[0].into(),
            color: grey,
        });

        // Back face (z = max) - vertices 5,4,7,6, normal +Z
        vertices.push(Vertex {
            position: corners[5].into(),
            normal: normals[1].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[4].into(),
            normal: normals[1].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[7].into(),
            normal: normals[1].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[6].into(),
            normal: normals[1].into(),
            color: grey,
        });

        // Left face (x = min) - vertices 4,0,3,7, normal -X
        vertices.push(Vertex {
            position: corners[4].into(),
            normal: normals[2].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[0].into(),
            normal: normals[2].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[3].into(),
            normal: normals[2].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[7].into(),
            normal: normals[2].into(),
            color: grey,
        });

        // Right face (x = max) - vertices 1,5,6,2, normal +X
        vertices.push(Vertex {
            position: corners[1].into(),
            normal: normals[3].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[5].into(),
            normal: normals[3].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[6].into(),
            normal: normals[3].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[2].into(),
            normal: normals[3].into(),
            color: grey,
        });

        // Bottom face (y = min) - vertices 4,5,1,0, normal -Y
        vertices.push(Vertex {
            position: corners[4].into(),
            normal: normals[4].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[5].into(),
            normal: normals[4].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[1].into(),
            normal: normals[4].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[0].into(),
            normal: normals[4].into(),
            color: grey,
        });

        // Top face (y = max) - vertices 3,2,6,7, normal +Y
        vertices.push(Vertex {
            position: corners[3].into(),
            normal: normals[5].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[2].into(),
            normal: normals[5].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[6].into(),
            normal: normals[5].into(),
            color: grey,
        });
        vertices.push(Vertex {
            position: corners[7].into(),
            normal: normals[5].into(),
            color: grey,
        });

        // Indices for 6 faces (clockwise winding for USD convention)
        // Each face has 4 vertices and 2 triangles (6 indices)
        let mut indices = Vec::with_capacity(36);
        for face in 0..6 {
            let base = face * 4;
            // CW winding: 0,2,1 and 0,3,2
            indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
        }

        Self {
            vertices,
            indices,
            bounds_min: min,
            bounds_max: max,
        }
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
                let len =
                    (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
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
            let color = [normal[0].abs(), normal[1].abs(), normal[2].abs()];

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
            let normal = mesh
                .normals
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

/// Scratch buffers for frustum culling to avoid per-frame allocations
struct CullingScratch {
    visible_with_distance: Vec<(f32, usize)>,
    near_instances: Vec<InstanceData>,
    far_instances: Vec<InstanceData>,
}

impl CullingScratch {
    fn new(max_instances: usize) -> Self {
        Self {
            visible_with_distance: Vec::with_capacity(max_instances),
            near_instances: Vec::with_capacity(max_instances),
            far_instances: Vec::with_capacity(max_instances),
        }
    }

    fn clear(&mut self) {
        self.visible_with_distance.clear();
        self.near_instances.clear();
        self.far_instances.clear();
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
    /// Instance buffer for LOD box proxies (far instances)
    lod_box_instance_buffer: wgpu::Buffer,
    /// Number of instances rendered as box proxies
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
    usd_stage: Option<UsdStage>,

    // Node graph state for scene assembly
    pub node_graph_state: NodeGraphState,
}

impl Renderer {
    /// Create a depth texture for the given size
    fn create_depth_texture(
        device: &Device,
        size: (u32, u32),
    ) -> (wgpu::Texture, wgpu::TextureView) {
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
    fn create_ivar_texture(
        device: &Device,
        size: (u32, u32),
    ) -> (wgpu::Texture, wgpu::TextureView) {
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
                buffers: &[], // No vertex buffer needed for fullscreen triangle
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
            depth_stencil: None, // No depth for fullscreen quad
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

        // Start with blank scene - no default mesh
        log::info!("Initializing blank scene (no default geometry)");

        // Create empty mesh data
        let mesh_data = MeshData {
            vertices: vec![],
            indices: vec![],
            bounds_min: Vec3::new(0.0, 0.0, 0.0),
            bounds_max: Vec3::new(0.0, 0.0, 0.0),
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
            Self::create_depth_texture(&device, (size.width, size.height));

        // No instances by default - empty scene
        let dummy_instance = InstanceData {
            model_matrix: Mat4::IDENTITY.to_cols_array_2d(),
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

        // LOD box instance buffer (shares capacity with main instance buffer)
        let lod_box_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LOD Box Instance Buffer (Dynamic)"),
            size: instance_buffer_size as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &lod_box_instance_buffer,
            0,
            bytemuck::cast_slice(&[dummy_instance]),
        );

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

        let gnomon_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let gnomon_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
                cull_mode: None, // No culling for lines
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None, // No depth for gnomon overlay
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        log::info!("Gnomon initialized");

        // Calculate stats - empty scene has 0 triangles
        let num_triangles = 0;

        // Create Ivar resources for CPU path tracer display
        let (ivar_texture, ivar_texture_view) =
            Self::create_ivar_texture(&device, (size.width, size.height));

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

        let (ivar_pipeline, ivar_bind_group, _) =
            Self::create_ivar_pipeline(&device, config.format, &ivar_texture_view, &ivar_sampler);

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
            instance_transforms: vec![], // Empty scene - no instances
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
            lod_box_instance_buffer,
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
            Self::create_depth_texture(&device, (size.width, size.height));

        // Generate instances from scene - collect both GPU data and transforms for Ivar
        let mut instance_transforms: Vec<Mat4> = Vec::with_capacity(scene.instances.len());
        let instances: Vec<InstanceData> = scene
            .instances
            .iter()
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

        // LOD box instance buffer (shares capacity with main instance buffer)
        let lod_box_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LOD Box Instance Buffer (Dynamic)"),
            size: instance_buffer_size as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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

        let gnomon_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let gnomon_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
        let (ivar_texture, ivar_texture_view) =
            Self::create_ivar_texture(&device, (size.width, size.height));

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

        let (ivar_pipeline, ivar_bind_group, _) =
            Self::create_ivar_pipeline(&device, config.format, &ivar_texture_view, &ivar_sampler);

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
            max_instances: MAX_INSTANCES,
            instance_aabbs,
            prototype_aabb,
            visible_instance_count: instances.len() as u32,
            lod_distance_threshold: 100.0,
            lod_max_polys: 5_000_000, // 5M poly budget default
            triangles_per_instance: num_triangles,
            lod_box_vertex_buffer,
            lod_box_index_buffer,
            lod_box_num_indices: lod_box_mesh.indices.len() as u32,
            lod_box_instance_buffer,
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
        })
    }

    /// Create a new renderer with scene AND USD stage for scene browser
    pub async fn new_with_scene_and_stage(
        window: std::sync::Arc<winit::window::Window>,
        scene: &bif_core::Scene,
        stage: UsdStage,
    ) -> Result<Self> {
        let mut renderer = Self::new_with_scene(window, scene).await?;
        renderer.usd_stage = Some(stage);
        Ok(renderer)
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
            let (ivar_texture, ivar_texture_view) =
                Self::create_ivar_texture(&self.device, new_size);
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
            self.culling_scratch.visible_with_distance.push((distance_sq, idx));
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
            let transform = &self.instance_transforms[idx];
            self.culling_scratch.near_instances.push(InstanceData {
                model_matrix: transform.to_cols_array_2d(),
            });
        }

        for &(_distance_sq, idx) in &self.culling_scratch.visible_with_distance[split_point..] {
            let transform = &self.instance_transforms[idx];
            self.culling_scratch.far_instances.push(InstanceData {
                model_matrix: transform.to_cols_array_2d(),
            });
        }

        // Update GPU buffers with visible instances
        if !self.culling_scratch.near_instances.is_empty() {
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.culling_scratch.near_instances),
            );
        }

        if !self.culling_scratch.far_instances.is_empty() {
            self.queue.write_buffer(
                &self.lod_box_instance_buffer,
                0,
                bytemuck::cast_slice(&self.culling_scratch.far_instances),
            );
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

        let path = path.as_ref();
        log::info!("Loading USD scene: {:?}", path);

        // Load USD file
        let (scene, stage) =
            load_usd_with_stage(path).map_err(|e| anyhow::anyhow!("Failed to load USD: {}", e))?;

        if scene.prototypes.is_empty() {
            return Err(anyhow::anyhow!("Scene has no geometry"));
        }

        // Convert first prototype to MeshData
        let proto = &scene.prototypes[0];
        let mesh_data = MeshData::from_core_mesh(&proto.mesh);

        log::info!(
            "Loaded {} vertices, {} indices from USD scene",
            mesh_data.vertices.len(),
            mesh_data.indices.len()
        );

        // Create new vertex buffer
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(&mesh_data.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        // Create new index buffer
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(&mesh_data.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        // Generate instances from scene
        let mut instance_transforms: Vec<Mat4> = Vec::with_capacity(scene.instances.len());
        let instances: Vec<InstanceData> = scene
            .instances
            .iter()
            .map(|inst| {
                let model_matrix = inst.model_matrix();
                instance_transforms.push(model_matrix);
                InstanceData {
                    model_matrix: model_matrix.to_cols_array_2d(),
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
        self.instance_transforms = instance_transforms;
        self.instance_aabbs = instance_aabbs;
        self.prototype_aabb = prototype_aabb;
        self.triangles_per_instance = self.num_indices / 3;
        self.num_triangles = self.triangles_per_instance * self.num_instances;
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

        // Update USD stage for scene browser
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

        log::info!(
            "USD scene loaded successfully: {} triangles x {} instances",
            self.num_indices / 3,
            self.num_instances
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

        // Clone data needed for background thread
        let mesh_data = self.mesh_data.clone();
        let transforms = self.instance_transforms.clone();

        // Create channel for build completion
        let (tx, rx) = mpsc::channel();
        self.ivar_state.build_receiver = Some(rx);

        // Spawn background thread to build scene
        std::thread::spawn(move || {
            let start_time = Instant::now();

            log::info!(
                "Background thread: Building Embree scene ({} triangles, {} instances)...",
                mesh_data.indices.len() / 3,
                transforms.len()
            );

            // Extract triangle vertices for Embree
            let mut triangle_vertices = Vec::with_capacity(mesh_data.indices.len() / 3);
            for i in (0..mesh_data.indices.len()).step_by(3) {
                let i0 = mesh_data.indices[i] as usize;
                let i1 = mesh_data.indices[i + 1] as usize;
                let i2 = mesh_data.indices[i + 2] as usize;

                // Get vertices in LOCAL space (no transformation)
                let v0 = Vec3::from_array(mesh_data.vertices[i0].position);
                let v1 = Vec3::from_array(mesh_data.vertices[i1].position);
                let v2 = Vec3::from_array(mesh_data.vertices[i2].position);

                triangle_vertices.push([v0, v1, v2]);
            }

            log::info!("Background thread: Extracted {} triangles, creating acceleration structure with {} instances...",
                triangle_vertices.len(), transforms.len());

            // Try to create Embree scene first, fall back to CPU BVH if unavailable
            let world = if let Some(embree_scene) = EmbreeScene::try_new(
                &triangle_vertices,
                transforms.clone(),
                Lambertian::new(Color::new(0.7, 0.7, 0.7)),
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
    pub fn render(
        &mut self,
        clear_color: wgpu::Color,
        window: &winit::window::Window,
    ) -> Result<()> {
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
        let mut gnomon_size = self.gnomon_size;
        let mut lod_max_polys = self.lod_max_polys;

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

                    // Scene Browser (collapsible)
                    ui.collapsing("Scene Browser", |ui| {
                        // Use USD stage if available, otherwise empty provider
                        let empty_provider = EmptyPrimProvider;
                        let provider: &dyn PrimDataProvider = match &self.usd_stage {
                            Some(stage) => stage,
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

            // Property Inspector (right panel)
            egui::SidePanel::right("property_panel")
                .default_width(280.0)
                .show(ctx, |ui| {
                    render_property_inspector(ui, self.selected_prim_properties.as_ref());
                });

            // Node Graph (bottom panel)
            egui::TopBottomPanel::bottom("node_graph_panel")
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
        });

        // Update gnomon size from UI
        self.gnomon_size = gnomon_size;

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

                    // Draw near instances (full mesh, after frustum culling + LOD split)
                    log::trace!(
                        "Drawing {} indices x {} near instances + {} LOD box instances (of {} total)",
                        self.num_indices,
                        self.visible_instance_count,
                        self.lod_box_instance_count,
                        self.num_instances
                    );
                    render_pass.set_pipeline(&self.pipeline);
                    render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                    render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                    render_pass
                        .set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(
                        0..self.num_indices,
                        0,
                        0..self.visible_instance_count,
                    );

                    // Draw far instances as LOD box proxies
                    if self.lod_box_instance_count > 0 {
                        render_pass.set_vertex_buffer(0, self.lod_box_vertex_buffer.slice(..));
                        render_pass.set_vertex_buffer(1, self.lod_box_instance_buffer.slice(..));
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

                    // Set viewport to bottom-right corner
                    let gnomon_size = self.gnomon_size as f32;
                    gnomon_pass.set_viewport(
                        (self.size.0 as f32) - gnomon_size, // x (right side)
                        (self.size.1 as f32) - gnomon_size, // y (bottom, wgpu uses top-left origin)
                        gnomon_size,                        // width
                        gnomon_size,                        // height
                        0.0,                                // min_depth
                        1.0,                                // max_depth
                    );

                    gnomon_pass.set_pipeline(&self.gnomon_pipeline);
                    gnomon_pass.set_bind_group(0, &self.gnomon_bind_group, &[]);
                    gnomon_pass.set_vertex_buffer(0, self.gnomon_vertex_buffer.slice(..));
                    gnomon_pass.draw(0..6, 0..1); // 6 vertices (3 lines)
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
