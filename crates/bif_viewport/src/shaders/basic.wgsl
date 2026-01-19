// Basic PBR shader for rendering textured geometry with camera
// Supports material properties: diffuse color, metallic, roughness
// Uses primitive_index for per-triangle material lookup (GeomSubsets)

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
}

struct MaterialUniform {
    diffuse_color: vec4<f32>,  // RGB + padding
    metallic_roughness: vec4<f32>,  // metallic, roughness, specular, padding
}

struct MaterialGpu {
    diffuse_color: vec4<f32>,
    metallic_roughness: vec4<f32>,
    texture_indices: vec4<u32>,
    extra_indices: vec4<u32>,
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

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) vertex_material_id: u32,  // Unused now - kept for vertex layout compatibility
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,
    @location(9) instance_material_id: u32,  // Per-instance fallback
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal_vs: vec3<f32>,   // View-space normal
    @location(1) uv: vec2<f32>,          // UV coordinates for texturing
    @location(2) view_dir: vec3<f32>,    // View direction for specular
    @location(3) @interpolate(flat) instance_material_id: u32,  // Fallback when no triangle material
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

    // Transform normal to view space for camera-locked headlight
    let normal_ws = (model_matrix * vec4<f32>(in.normal, 0.0)).xyz;
    out.normal_vs = normalize((camera.view * vec4<f32>(normal_ws, 0.0)).xyz);

    // Pass through UV coordinates
    out.uv = in.uv;

    // Compute view direction (camera position is at inverse view translation)
    // For headlight, we use the view-space Z direction
    let view_pos = camera.view * world_position;
    out.view_dir = normalize(-view_pos.xyz);

    // Pass instance material as fallback
    out.instance_material_id = in.instance_material_id;

    return out;
}

@fragment
fn fs_main(
    in: VertexOutput,
    @builtin(primitive_index) primitive_id: u32,
) -> @location(0) vec4<f32> {
    // Look up per-triangle material, fall back to instance material if sentinel value
    var material_id = triangle_materials[primitive_id];
    if (material_id == 0xFFFFFFFFu) {
        material_id = in.instance_material_id;
    }

    let mat = material_table[material_id];
    let metallic = mat.metallic_roughness.x;
    let roughness = mat.metallic_roughness.y;
    let specular = mat.metallic_roughness.z;

    var base_color = mat.diffuse_color.rgb;
    let diffuse_tex_index = mat.texture_indices.x;
    if (diffuse_tex_index != 0u) {
        // Use texture color directly (not multiplied by diffuse_color which may be grey default)
        let tex_sample = textureSample(textures[diffuse_tex_index], texture_sampler, in.uv);
        base_color = tex_sample.rgb;

        // DEBUG: Uncomment to see raw texture without lighting
        // return vec4<f32>(base_color, 1.0);

        // DEBUG: Uncomment to see UV coordinates
        // return vec4<f32>(in.uv.x, in.uv.y, 0.0, 1.0);

        // DEBUG: Uncomment to see material_id as color
        // return vec4<f32>(f32(material_id) / 14.0, 0.0, 0.0, 1.0);
    }

    // Simple PBR-inspired shading
    let normal = normalize(in.normal_vs);
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
