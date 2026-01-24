// PBR shader with split-sum IBL environment lighting.
// Falls back to headlight when no environment is loaded.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    camera_position: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
}

struct MaterialUniform {
    diffuse_color: vec4<f32>,
    metallic_roughness: vec4<f32>,
}

struct MaterialGpu {
    diffuse_color: vec4<f32>,
    metallic_roughness: vec4<f32>,
    texture_indices: vec4<u32>,
    extra_indices: vec4<u32>,
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
var<uniform> material: MaterialUniform;

@group(1) @binding(1)
var<storage, read> material_table: array<MaterialGpu>;

@group(1) @binding(2)
var<storage, read> triangle_materials: array<u32>;

@group(2) @binding(0)
var textures: binding_array<texture_2d<f32>>;

@group(2) @binding(1)
var texture_sampler: sampler;

// Environment IBL (bind group 3)
@group(3) @binding(0)
var irradiance_map: texture_cube<f32>;

@group(3) @binding(1)
var prefiltered_map: texture_cube<f32>;

@group(3) @binding(2)
var brdf_lut: texture_2d<f32>;

@group(3) @binding(3)
var env_sampler: sampler;

@group(3) @binding(4)
var<uniform> env_params: EnvironmentParams;

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
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal_ws: vec3<f32>,     // World-space normal
    @location(1) uv: vec2<f32>,
    @location(2) world_pos: vec3<f32>,     // World-space position
    @location(3) @interpolate(flat) instance_material_id: u32,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let model_matrix = mat4x4<f32>(
        in.model_matrix_0,
        in.model_matrix_1,
        in.model_matrix_2,
        in.model_matrix_3,
    );

    var out: VertexOutput;
    let world_position = model_matrix * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_position;

    // World-space normal via adjugate (correct for non-uniform scale)
    let n = mat3x3<f32>(model_matrix[0].xyz, model_matrix[1].xyz, model_matrix[2].xyz);
    let adj = mat3x3<f32>(cross(n[1], n[2]), cross(n[2], n[0]), cross(n[0], n[1]));
    out.normal_ws = normalize(adj * in.normal);
    out.world_pos = world_position.xyz;
    out.uv = in.uv;
    out.instance_material_id = in.instance_material_id;

    return out;
}

// Rotate a direction around Y axis by given radians
fn rotate_y(dir: vec3<f32>, angle: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec3<f32>(
        c * dir.x + s * dir.z,
        dir.y,
        -s * dir.x + c * dir.z,
    );
}

// ACES filmic tone mapping
fn aces_tonemap(color: vec3<f32>) -> vec3<f32> {
    let a = color * (color * 2.51 + vec3(0.03));
    let b = color * (color * 2.43 + vec3(0.59)) + vec3(0.14);
    return saturate(a / b);
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return pow(c, vec3(1.0 / 2.2));
}

// Fresnel-Schlick approximation
fn fresnel_schlick_roughness(cos_theta: f32, f0: vec3<f32>, roughness: f32) -> vec3<f32> {
    let max_val = max(vec3<f32>(1.0 - roughness), f0);
    return f0 + (max_val - f0) * pow(1.0 - cos_theta, 5.0);
}

@fragment
fn fs_main(
    in: VertexOutput,
    @builtin(primitive_index) primitive_id: u32,
) -> @location(0) vec4<f32> {
    // Material lookup
    var material_id = triangle_materials[primitive_id];
    if (material_id == 0xFFFFFFFFu) {
        material_id = in.instance_material_id;
    }

    let mat = material_table[material_id];
    let metallic = mat.metallic_roughness.x;
    let roughness = mat.metallic_roughness.y;

    var base_color = mat.diffuse_color.rgb;
    let diffuse_tex_index = mat.texture_indices.x;
    if (diffuse_tex_index != 0u) {
        let flipped_uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
        let tex_sample = textureSample(textures[diffuse_tex_index], texture_sampler, flipped_uv);
        base_color = tex_sample.rgb;
    }

    let normal = normalize(in.normal_ws);
    let view_dir = normalize(camera.camera_position.xyz - in.world_pos);
    let n_dot_v = max(dot(normal, view_dir), 0.0);

    // F0: 0.04 for dielectrics, base_color for metals
    let f0 = mix(vec3<f32>(0.04), base_color, metallic);

    // IBL path (when environment is loaded)
    if (env_params.has_environment != 0u) {
        let fresnel = fresnel_schlick_roughness(n_dot_v, f0, roughness);
        let ks = fresnel;
        let kd = (1.0 - ks) * (1.0 - metallic);

        // Diffuse IBL: sample irradiance cubemap
        let irr_dir = rotate_y(normal, env_params.rotation);
        let irradiance = textureSample(irradiance_map, env_sampler, irr_dir).rgb;
        let diffuse_ibl = kd * base_color * irradiance;

        // Specular IBL: sample prefiltered cubemap + BRDF LUT
        let reflect_dir = reflect(-view_dir, normal);
        let pref_dir = rotate_y(reflect_dir, env_params.rotation);
        let prefiltered = textureSampleLevel(prefiltered_map, env_sampler, pref_dir, roughness * env_params.max_mip).rgb;
        let brdf = textureSample(brdf_lut, env_sampler, vec2<f32>(n_dot_v, roughness)).rg;
        let specular_ibl = prefiltered * (fresnel * brdf.x + brdf.y);

        let color = (diffuse_ibl + specular_ibl) * env_params.intensity;
        return vec4<f32>(linear_to_srgb(aces_tonemap(color)), 1.0);
    }

    // Fallback: headlight shading (no environment loaded)
    let normal_vs = normalize((camera.view * vec4<f32>(normal, 0.0)).xyz);
    let view_pos = camera.view * vec4<f32>(in.world_pos, 1.0);
    let view_dir_vs = normalize(-view_pos.xyz);

    let light_dir = view_dir_vs;
    let half_vec = normalize(light_dir + view_dir_vs);
    let n_dot_l = max(dot(normal_vs, light_dir), 0.0);
    let diffuse = base_color * n_dot_l;

    let n_dot_h = max(dot(normal_vs, half_vec), 0.0);
    let shininess = mix(8.0, 256.0, 1.0 - roughness);
    let spec_intensity = pow(n_dot_h, shininess) * mat.metallic_roughness.z;
    let fresnel_hl = f0 + (1.0 - f0) * pow(1.0 - max(dot(view_dir_vs, half_vec), 0.0), 5.0);
    let specular_color = fresnel_hl * spec_intensity;

    let dielectric_contrib = diffuse * (1.0 - metallic);
    let metal_contrib = specular_color * metallic;
    let ambient = base_color * 0.15;
    let lit_color = ambient + dielectric_contrib * 0.7 + metal_contrib * 0.5;

    return vec4<f32>(linear_to_srgb(aces_tonemap(lit_color)), 1.0);
}
