// Silhouette outline pass for selection highlight.
//
// Technique: normal-expanded back-face rendering.
// Renders back faces of a slightly enlarged version of the selected mesh.
// Back faces that protrude beyond the original silhouette are the only ones
// that pass the depth test, producing a clean outline with no internal edges.
//
// Clip-space normal expansion gives a consistent screen-pixel outline width
// regardless of the mesh's distance from the camera.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    // Remaining fields (view, camera_position, etc.) not used by this shader.
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct OutlineParams {
    color: vec4<f32>,
    width_ndc: f32,
    _pad0: vec3<f32>,
}

@group(1) @binding(0)
var<uniform> outline: OutlineParams;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) vertex_material_id: u32,
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,
    @location(9) instance_material_id: u32,
    @location(10) tri_mat_offset: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let model = mat4x4<f32>(
        in.model_matrix_0,
        in.model_matrix_1,
        in.model_matrix_2,
        in.model_matrix_3,
    );

    // World-space normal via adjugate (correct under non-uniform scale).
    let m3 = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    let adj = mat3x3<f32>(cross(m3[1], m3[2]), cross(m3[2], m3[0]), cross(m3[0], m3[1]));
    let world_normal = normalize(adj * in.normal);

    let world_pos = model * vec4<f32>(in.position, 1.0);
    var clip = camera.view_proj * world_pos;

    // Expand along normal in clip space for consistent screen-pixel outline width.
    // Multiplying by clip.w converts NDC offset to clip-space offset.
    let clip_normal = normalize((camera.view_proj * vec4<f32>(world_normal, 0.0)).xy);
    clip.x += clip_normal.x * clip.w * outline.width_ndc;
    clip.y += clip_normal.y * clip.w * outline.width_ndc;

    return VertexOutput(clip);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return outline.color;
}
