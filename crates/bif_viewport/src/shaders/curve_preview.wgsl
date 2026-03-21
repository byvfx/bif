// Curve preview shader — renders curves as line segments.

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

struct CurveParams {
    color: vec4<f32>,
}

@group(1) @binding(0)
var<uniform> params: CurveParams;

struct Vertex {
    x: f32,
    y: f32,
    z: f32,
    _pad: f32,
}

@group(1) @binding(1)
var<storage, read> vertices: array<Vertex>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let v = vertices[vertex_index];
    let world_pos = vec4<f32>(v.x, v.y, v.z, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.color = params.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
