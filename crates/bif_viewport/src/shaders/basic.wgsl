// PBR shader with split-sum IBL environment lighting + explicit lights.
// Falls back to headlight when no environment is loaded.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    camera_position: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
    selected_instance_id: u32,
    shading_mode: u32,       // 0 = textured, 1 = display color
    _pad0: u32,
    _pad1: u32,
}

struct MaterialUniform {
    base_color: vec4<f32>,         // [r, g, b, metalness]
    specular_params: vec4<f32>,    // [roughness, ior, weight, pad]
}

struct MaterialGpu {
    base_color: vec4<f32>,         // [r, g, b, metalness]
    specular_params: vec4<f32>,    // [roughness, ior, weight, pad]
    emission: vec4<f32>,           // [r, g, b, luminance]
    extra_params: vec4<f32>,       // [opacity, coat_weight, coat_roughness, pad]
    texture_indices: vec4<u32>,
    extra_indices: vec4<u32>,
}

struct EnvironmentParams {
    intensity: f32,
    rotation: f32,
    has_environment: u32,
    max_mip: f32,
}

// Light types (must match LIGHT_TYPE_* constants in Rust)
const LIGHT_TYPE_DISTANT: u32 = 0u;
const LIGHT_TYPE_POINT: u32 = 1u;
const LIGHT_TYPE_RECT: u32 = 2u;

struct LightGpu {
    position_type: vec4<f32>,      // xyz = position, w = type
    direction_radius: vec4<f32>,   // xyz = direction, w = radius
    color_intensity: vec4<f32>,    // rgb = color, a = intensity
    params: vec4<f32>,             // angle, width, height, unused
}

struct LightsUniform {
    lights: array<LightGpu, 32>,
    light_count: vec4<u32>,        // [0] = count
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

// Lights (bind group 4)
@group(4) @binding(0)
var<uniform> lights_uniform: LightsUniform;

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
    @location(0) normal_ws: vec3<f32>,     // World-space normal
    @location(1) uv: vec2<f32>,
    @location(2) world_pos: vec3<f32>,     // World-space position
    @location(3) @interpolate(flat) instance_material_id: u32,
    @location(4) @interpolate(flat) instance_idx: u32,
    @location(5) @interpolate(flat) tri_mat_offset: u32,
    @location(6) vertex_color: vec3<f32>,  // Display color (primvars:displayColor)
}

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance_index: u32) -> VertexOutput {
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
    out.instance_idx = instance_index;
    out.tri_mat_offset = in.tri_mat_offset;
    out.vertex_color = in.color;

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

// Standard Fresnel-Schlick (no roughness)
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
}

// GGX/Trowbridge-Reitz normal distribution
fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (3.14159265 * denom * denom + 0.0001);
}

// Smith's geometry function
fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = r * r / 8.0;
    return n_dot_v / (n_dot_v * (1.0 - k) + k);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness);
}

// Evaluate direct lighting from explicit lights
fn evaluate_direct_lights(
    world_pos: vec3<f32>,
    normal: vec3<f32>,
    view_dir: vec3<f32>,
    base_color: vec3<f32>,
    metallic: f32,
    roughness: f32,
    f0: vec3<f32>,
) -> vec3<f32> {
    var result = vec3<f32>(0.0);
    let light_count = lights_uniform.light_count[0];

    for (var i = 0u; i < light_count; i = i + 1u) {
        let light = lights_uniform.lights[i];
        let light_type = u32(light.position_type.w);
        let light_color = light.color_intensity.rgb;
        let light_intensity = light.color_intensity.a;

        var light_dir: vec3<f32>;
        var attenuation: f32 = 1.0;

        if (light_type == LIGHT_TYPE_DISTANT) {
            // Directional light
            light_dir = normalize(-light.direction_radius.xyz);
        } else if (light_type == LIGHT_TYPE_POINT) {
            // Point light with distance attenuation
            let light_pos = light.position_type.xyz;
            let to_light = light_pos - world_pos;
            let distance = length(to_light);
            light_dir = to_light / distance;
            // Inverse square falloff
            attenuation = 1.0 / (distance * distance + 0.01);
        } else if (light_type == LIGHT_TYPE_RECT) {
            // Area light - simplified as point at center
            let light_pos = light.position_type.xyz;
            let to_light = light_pos - world_pos;
            let distance = length(to_light);
            light_dir = to_light / distance;
            attenuation = 1.0 / (distance * distance + 0.01);
        } else {
            continue;
        }

        let n_dot_l = max(dot(normal, light_dir), 0.0);
        if (n_dot_l <= 0.0) {
            continue;
        }

        // Cook-Torrance BRDF
        let half_vec = normalize(view_dir + light_dir);
        let n_dot_h = max(dot(normal, half_vec), 0.0);
        let n_dot_v = max(dot(normal, view_dir), 0.001);
        let h_dot_v = max(dot(half_vec, view_dir), 0.0);

        // Specular — clamp to avoid extreme values at grazing angles
        let clamped_n_dot_v = max(n_dot_v, 0.001);
        let clamped_n_dot_l = max(n_dot_l, 0.001);
        let d = distribution_ggx(n_dot_h, roughness);
        let g = geometry_smith(clamped_n_dot_v, clamped_n_dot_l, roughness);
        let f = fresnel_schlick(h_dot_v, f0);

        let specular = (d * g * f) / (4.0 * clamped_n_dot_v * clamped_n_dot_l + 0.0001);

        // Diffuse (energy conserving)
        let ks = f;
        let kd = (1.0 - ks) * (1.0 - metallic);
        let diffuse = kd * base_color / 3.14159265;

        // Combine
        let radiance = light_color * light_intensity * attenuation;
        result += (diffuse + specular) * radiance * n_dot_l;
    }

    return result;
}

@fragment
fn fs_main(
    in: VertexOutput,
    @builtin(primitive_index) primitive_id: u32,
) -> @location(0) vec4<f32> {
    // Wireframe selection overlay: solid gold, no lighting
    if (camera.shading_mode == 2u) {
        return vec4<f32>(1.0, 0.84, 0.0, 1.0); // Gold (#FFD700)
    }

    // Material lookup
    var material_id = triangle_materials[primitive_id + in.tri_mat_offset];
    if (material_id == 0xFFFFFFFFu) {
        material_id = in.instance_material_id;
    }

    let mat = material_table[material_id];
    let metalness = mat.base_color.w;
    let roughness = mat.specular_params.x;
    let ior = mat.specular_params.y;
    let spec_weight = mat.specular_params.z;

    // Per-tile UDIM: compute tile offset from UV, then sample with fract UV.
    // IMPORTANT: ALL texture lookups must add tex_offset to their index —
    // each UDIM tile is a separate texture in a contiguous array block.
    var sample_uv: vec2<f32>;
    var tex_offset = 0u;

    let udim_packed = mat.extra_indices.y;
    if (udim_packed != 0u) {
        let grid_cols = udim_packed >> 16u;
        let grid_rows = udim_packed & 0xFFFFu;
        let offset_packed = mat.extra_indices.z;
        let min_col = offset_packed >> 16u;
        let min_row = offset_packed & 0xFFFFu;

        let raw_col = i32(floor(in.uv.x)) - i32(min_col);
        let raw_row = i32(floor(in.uv.y)) - i32(min_row);
        let col = u32(clamp(raw_col, 0, i32(grid_cols) - 1));
        let row = u32(clamp(raw_row, 0, i32(grid_rows) - 1));

        tex_offset = row * grid_cols + col;
        sample_uv = vec2<f32>(fract(in.uv.x), 1.0 - fract(in.uv.y));
    } else {
        sample_uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
    }

    var base_color = mat.base_color.rgb;
    if (camera.shading_mode == 1u) {
        // Display Color mode: always use vertex color (primvars:displayColor)
        base_color = in.vertex_color;
    } else {
        // Textured mode: use texture if available, then display color, then material
        let diffuse_tex_index = mat.texture_indices.x;
        if (diffuse_tex_index != 0u) {
            let tex_sample = textureSample(textures[diffuse_tex_index + tex_offset], texture_sampler, sample_uv);
            base_color = tex_sample.rgb;
        } else if (in.vertex_color.r > 0.001 || in.vertex_color.g > 0.001 || in.vertex_color.b > 0.001) {
            base_color = in.vertex_color;
        }
    }

    var normal = normalize(in.normal_ws);
    let view_dir = normalize(camera.camera_position.xyz - in.world_pos);
    // Two-sided lighting: flip normal if facing away from camera
    if (dot(normal, view_dir) < 0.0) {
        normal = -normal;
    }
    let n_dot_v = max(dot(normal, view_dir), 0.0);

    // IOR-based F0: ((ior-1)/(ior+1))^2 for dielectrics, base_color for metals
    let ior_ratio = (ior - 1.0) / (ior + 1.0);
    let f0_dielectric = ior_ratio * ior_ratio * spec_weight;
    let f0 = mix(vec3<f32>(f0_dielectric), base_color, metalness);

    // Evaluate direct lighting from explicit lights (USD lights)
    let direct_light = evaluate_direct_lights(
        in.world_pos,
        normal,
        view_dir,
        base_color,
        metalness,
        roughness,
        f0,
    );

    // Compute lit color from one of three paths
    var lit_color: vec3<f32>;

    // IBL path (when environment is loaded)
    if (env_params.has_environment != 0u) {
        let fresnel = fresnel_schlick_roughness(n_dot_v, f0, roughness);
        let ks = fresnel;
        let kd = (1.0 - ks) * (1.0 - metalness);

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

        // Combine IBL + direct lights
        lit_color = (diffuse_ibl + specular_ibl) * env_params.intensity + direct_light;
    } else if (lights_uniform.light_count[0] > 0u) {
        // Fallback: direct lights only (no IBL)
        let ambient = base_color * 0.05;
        lit_color = ambient + direct_light;
    } else {
        // No environment and no lights - use headlight fallback
        let normal_vs = normalize((camera.view * vec4<f32>(normal, 0.0)).xyz);
        let view_pos = camera.view * vec4<f32>(in.world_pos, 1.0);
        let view_dir_vs = normalize(-view_pos.xyz);

        let light_dir = view_dir_vs;
        let half_vec = normalize(light_dir + view_dir_vs);
        let n_dot_l = max(dot(normal_vs, light_dir), 0.0);
        let diffuse = base_color * n_dot_l;

        let n_dot_h = max(dot(normal_vs, half_vec), 0.0);
        let shininess = mix(8.0, 256.0, 1.0 - roughness);
        let spec_intensity = pow(n_dot_h, shininess) * spec_weight;
        let fresnel_hl = f0 + (1.0 - f0) * pow(1.0 - max(dot(view_dir_vs, half_vec), 0.0), 5.0);
        let specular_color = fresnel_hl * spec_intensity;

        let dielectric_contrib = diffuse * (1.0 - metalness);
        let metal_contrib = specular_color * metalness;
        let ambient = base_color * 0.15;
        lit_color = ambient + dielectric_contrib * 0.7 + metal_contrib * 0.5;
    }

    // Selection highlight: subtle warm tint (wireframe overlay provides main highlight)
    if (in.instance_idx == camera.selected_instance_id) {
        lit_color = mix(lit_color, lit_color * vec3<f32>(1.2, 1.1, 0.9), 0.15);
    }

    return vec4<f32>(linear_to_srgb(aces_tonemap(lit_color)), 1.0);
}
