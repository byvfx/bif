// GGX importance-sampled prefilter compute shader.
// Generates prefiltered environment map for specular IBL.
// Dispatched per mip level as (mip_size/8, mip_size/8, 6).

struct PrefilterParams {
    roughness: f32,
    sample_count: u32,
    face_size: u32,
    _pad: u32,
}

@group(0) @binding(0) var env_cube: texture_cube<f32>;
@group(0) @binding(1) var env_sampler: sampler;
@group(0) @binding(2) var output: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(3) var<uniform> params: PrefilterParams;

const PI: f32 = 3.14159265358979;

fn cube_dir(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let u = uv.x * 2.0 - 1.0;
    let v = uv.y * 2.0 - 1.0;
    switch face {
        case 0u: { return normalize(vec3( 1.0, -v,   -u)); }
        case 1u: { return normalize(vec3(-1.0, -v,    u)); }
        case 2u: { return normalize(vec3( u,    1.0,  v)); }
        case 3u: { return normalize(vec3( u,   -1.0, -v)); }
        case 4u: { return normalize(vec3( u,   -v,   1.0)); }
        default: { return normalize(vec3(-u,   -v,  -1.0)); }
    }
}

fn build_tbn(n: vec3<f32>) -> mat3x3<f32> {
    var up = vec3(0.0, 1.0, 0.0);
    if abs(n.y) > 0.999 {
        up = vec3(1.0, 0.0, 0.0);
    }
    let tangent = normalize(cross(up, n));
    let bitangent = cross(n, tangent);
    return mat3x3(tangent, bitangent, n);
}

// Van der Corput radical inverse (base 2).
fn radical_inverse_vdc(bits_in: u32) -> f32 {
    var bits = bits_in;
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return f32(bits) * 2.3283064365386963e-10;
}

fn hammersley(i: u32, n: u32) -> vec2<f32> {
    return vec2(f32(i) / f32(n), radical_inverse_vdc(i));
}

// GGX importance sampling: sample half-vector from NDF.
fn importance_sample_ggx(xi: vec2<f32>, roughness: f32, n: vec3<f32>) -> vec3<f32> {
    let a = roughness * roughness;
    let a2 = a * a;

    let phi = 2.0 * PI * xi.x;
    let cos_theta = sqrt((1.0 - xi.y) / (1.0 + (a2 - 1.0) * xi.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);

    let h_local = vec3(
        sin_theta * cos(phi),
        sin_theta * sin(phi),
        cos_theta,
    );

    let tbn = build_tbn(n);
    return normalize(tbn * h_local);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = params.face_size;
    if id.x >= size || id.y >= size || id.z >= 6u {
        return;
    }

    let uv = (vec2<f32>(id.xy) + 0.5) / f32(size);
    let n = cube_dir(id.z, uv);
    let v = n; // View = Normal for prefilter (isotropic assumption)

    var prefiltered = vec3(0.0);
    var total_weight = 0.0;

    let sample_count = params.sample_count;
    for (var i = 0u; i < sample_count; i++) {
        let xi = hammersley(i, sample_count);
        let h = importance_sample_ggx(xi, params.roughness, n);
        let l = normalize(2.0 * dot(v, h) * h - v);

        let n_dot_l = max(dot(n, l), 0.0);
        if n_dot_l > 0.0 {
            let sample_color = textureSampleLevel(env_cube, env_sampler, l, 0.0).rgb;
            prefiltered += sample_color * n_dot_l;
            total_weight += n_dot_l;
        }
    }

    if total_weight > 0.0 {
        prefiltered /= total_weight;
    }

    textureStore(output, vec2<i32>(id.xy), i32(id.z), vec4(prefiltered, 1.0));
}
