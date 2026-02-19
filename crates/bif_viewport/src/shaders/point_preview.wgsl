// Point preview shader for scatter point cloud visualization.
// Renders points as billboard quads with circle masking.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    camera_position: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
    selected_instance_id: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct PointParams {
    color: vec4<f32>,
    point_size: f32,
    viewport_width: f32,
    viewport_height: f32,
    _pad: f32,
}

@group(1) @binding(0)
var<uniform> params: PointParams;

struct PointPosition {
    x: f32,
    y: f32,
    z: f32,
    _pad: f32,
}

@group(1) @binding(1)
var<storage, read> points: array<PointPosition>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let pt = points[instance_index];
    let world_pos = vec4<f32>(pt.x, pt.y, pt.z, 1.0);
    let clip_pos = camera.view_proj * world_pos;

    // Quad corner offsets (0=TL, 1=TR, 2=BL, 3=BR)
    var corner: vec2<f32>;
    switch vertex_index {
        case 0u: { corner = vec2<f32>(-1.0,  1.0); }
        case 1u: { corner = vec2<f32>( 1.0,  1.0); }
        case 2u: { corner = vec2<f32>(-1.0, -1.0); }
        default: { corner = vec2<f32>( 1.0, -1.0); }
    }

    // Convert point_size from pixels to NDC offset
    let pixel_offset = corner * params.point_size;
    let ndc_offset = vec2<f32>(
        pixel_offset.x / params.viewport_width * 2.0,
        pixel_offset.y / params.viewport_height * 2.0,
    );

    out.clip_position = clip_pos + vec4<f32>(ndc_offset * clip_pos.w, 0.0, 0.0);
    out.color = params.color;
    out.uv = corner * 0.5 + 0.5; // Map [-1,1] to [0,1]

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Circle mask: discard outside unit circle
    let centered = in.uv * 2.0 - 1.0;
    if dot(centered, centered) > 1.0 {
        discard;
    }
    return in.color;
}
