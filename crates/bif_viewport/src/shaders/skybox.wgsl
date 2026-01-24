// Skybox fullscreen pass: samples prefiltered cubemap mip 0 at camera ray direction.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    camera_position: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
}

struct EnvironmentParams {
    intensity: f32,
    rotation: f32,
    has_environment: u32,
    max_mip: f32,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var prefiltered_map: texture_cube<f32>;

@group(1) @binding(1)
var env_sampler: sampler;

@group(1) @binding(2)
var<uniform> env_params: EnvironmentParams;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

// Fullscreen triangle (3 vertices, no vertex buffer needed)
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index & 2u) * 2 - 1);
    out.clip_position = vec4<f32>(x, y, 1.0, 1.0);
    out.ndc = vec2<f32>(x, y);
    return out;
}

fn rotate_y(dir: vec3<f32>, angle: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec3<f32>(
        c * dir.x + s * dir.z,
        dir.y,
        -s * dir.x + c * dir.z,
    );
}

fn aces_tonemap(color: vec3<f32>) -> vec3<f32> {
    let a = color * (color * 2.51 + vec3(0.03));
    let b = color * (color * 2.43 + vec3(0.59)) + vec3(0.14);
    return saturate(a / b);
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return pow(c, vec3(1.0 / 2.2));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Reconstruct world-space ray from NDC via inverse view-projection
    let near_point = camera.inv_view_proj * vec4<f32>(in.ndc, -1.0, 1.0);
    let far_point = camera.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let world_near = near_point.xyz / near_point.w;
    let world_far = far_point.xyz / far_point.w;
    let view_dir = normalize(world_far - world_near);

    let sample_dir = rotate_y(view_dir, env_params.rotation);
    let color = textureSampleLevel(prefiltered_map, env_sampler, sample_dir, 0.0).rgb * env_params.intensity;

    return vec4<f32>(linear_to_srgb(aces_tonemap(color)), 1.0);
}
