// Irradiance cubemap convolution compute shader.
// Convolves the environment cubemap with a cosine lobe for diffuse IBL.
// Dispatched as (irr_size/8, irr_size/8, 6) workgroups.

@group(0) @binding(0) var env_cube: texture_cube<f32>;
@group(0) @binding(1) var env_sampler: sampler;
@group(0) @binding(2) var output: texture_storage_2d_array<rgba16float, write>;

const PI: f32 = 3.14159265358979;
const SAMPLE_DELTA: f32 = 0.05;

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

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(output);
    let size = dims.x;
    if id.x >= size || id.y >= size || id.z >= 6u {
        return;
    }

    let uv = (vec2<f32>(id.xy) + 0.5) / f32(size);
    let normal = cube_dir(id.z, uv);
    let tbn = build_tbn(normal);

    var irradiance = vec3(0.0);
    var sample_count = 0.0;

    // Hemisphere convolution with uniform sampling
    var phi = 0.0;
    while phi < 2.0 * PI {
        var theta = 0.0;
        while theta < 0.5 * PI {
            let sin_theta = sin(theta);
            let cos_theta = cos(theta);
            // Tangent-space direction
            let local_dir = vec3(
                sin_theta * cos(phi),
                sin_theta * sin(phi),
                cos_theta,
            );
            // World-space direction
            let sample_dir = tbn * local_dir;
            let sample_color = textureSampleLevel(env_cube, env_sampler, sample_dir, 0.0).rgb;
            // cos(theta) * sin(theta) for hemisphere solid angle weighting
            irradiance += sample_color * cos_theta * sin_theta;
            sample_count += 1.0;
            theta += SAMPLE_DELTA;
        }
        phi += SAMPLE_DELTA;
    }

    irradiance = PI * irradiance / sample_count;
    textureStore(output, vec2<i32>(id.xy), i32(id.z), vec4(irradiance, 1.0));
}
