// Basic PBR shader for rendering textured geometry with camera
// Supports material properties: diffuse color, metallic, roughness

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
}

struct MaterialUniform {
    diffuse_color: vec4<f32>,  // RGB + padding
    metallic_roughness: vec4<f32>,  // metallic, roughness, specular, padding
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<uniform> material: MaterialUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) model_matrix_0: vec4<f32>,
    @location(5) model_matrix_1: vec4<f32>,
    @location(6) model_matrix_2: vec4<f32>,
    @location(7) model_matrix_3: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal_ws: vec3<f32>,   // World-space normal
    @location(1) uv: vec2<f32>,          // UV coordinates for texturing
    @location(2) view_dir: vec3<f32>,    // View direction for specular
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

    // Transform normal to world space
    out.normal_ws = normalize((model_matrix * vec4<f32>(in.normal, 0.0)).xyz);

    // Pass through UV coordinates
    out.uv = in.uv;

    // Compute view direction (camera position is at inverse view translation)
    // For headlight, we use the view-space Z direction
    let view_pos = camera.view * world_position;
    out.view_dir = normalize(-view_pos.xyz);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Material properties from uniform
    let base_color = material.diffuse_color.rgb;
    let metallic = material.metallic_roughness.x;
    let roughness = material.metallic_roughness.y;
    let specular = material.metallic_roughness.z;

    // Simple PBR-inspired shading
    let normal = normalize(in.normal_ws);
    let view_dir = normalize(in.view_dir);

    // Headlight: light from camera direction
    let light_dir = view_dir;
    let half_vec = normalize(light_dir + view_dir);

    // Diffuse (Lambertian)
    let n_dot_l = max(dot(normal, light_dir), 0.0);
    let diffuse = base_color * n_dot_l;

    // Specular (Blinn-Phong approximation)
    let n_dot_h = max(dot(normal, half_vec), 0.0);
    let shininess = mix(8.0, 256.0, 1.0 - roughness);
    let spec_intensity = pow(n_dot_h, shininess) * specular;

    // Fresnel approximation (Schlick)
    let f0 = mix(vec3<f32>(0.04), base_color, metallic);
    let fresnel = f0 + (1.0 - f0) * pow(1.0 - max(dot(view_dir, half_vec), 0.0), 5.0);
    let specular_color = fresnel * spec_intensity;

    // Combine: diffuse for dielectrics, specular tinted by base_color for metals
    let dielectric_contrib = diffuse * (1.0 - metallic);
    let metal_contrib = specular_color * metallic;

    // Ambient
    let ambient = base_color * 0.15;

    // Final color
    let lit_color = ambient + dielectric_contrib * 0.7 + metal_contrib * 0.5;

    return vec4<f32>(lit_color, 1.0);
}
