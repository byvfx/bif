// Basic shader for rendering solid color geometry with camera
// Grey placeholder material with headlight diffuse lighting

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) model_matrix_0: vec4<f32>,
    @location(4) model_matrix_1: vec4<f32>,
    @location(5) model_matrix_2: vec4<f32>,
    @location(6) model_matrix_3: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal_vs: vec3<f32>,  // View-space normal for headlight
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    // Reconstruct model matrix from instance data
    let model_matrix = mat4x4<f32>(
        in.model_matrix_0,
        in.model_matrix_1,
        in.model_matrix_2,
        in.model_matrix_3,
    );
    
    var out: VertexOutput;
    let world_position = model_matrix * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_position;
    
    // Transform normal to world space, then to view space for headlight
    let world_normal = normalize((model_matrix * vec4<f32>(in.normal, 0.0)).xyz);
    out.normal_vs = normalize((camera.view * vec4<f32>(world_normal, 0.0)).xyz);
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Base grey color
    let base_color = vec3<f32>(0.5, 0.5, 0.5);
    
    // Headlight diffuse: light from camera direction (positive Z in view space)
    // In view space, the camera looks down -Z, so light comes from +Z
    let normal = normalize(in.normal_vs);
    let diffuse = max(normal.z, 0.0);  // Dot with (0, 0, 1)
    
    // Ambient + diffuse lighting
    let ambient = 0.2;
    let lit_color = base_color * (ambient + diffuse * 0.8);
    
    return vec4<f32>(lit_color, 1.0);
}
