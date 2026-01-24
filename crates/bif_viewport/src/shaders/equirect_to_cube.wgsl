// Equirectangular HDR → Cubemap compute shader.
// Dispatched as (face_size/8, face_size/8, 6) workgroups.

@group(0) @binding(0) var equirect: texture_2d<f32>;
@group(0) @binding(1) var output: texture_storage_2d_array<rgba16float, write>;

const PI: f32 = 3.14159265358979;

// Convert cubemap face + UV to world direction.
fn cube_dir(face: u32, uv: vec2<f32>) -> vec3<f32> {
    // Map [0,1] UV to [-1,1]
    let u = uv.x * 2.0 - 1.0;
    let v = uv.y * 2.0 - 1.0;

    switch face {
        case 0u: { return normalize(vec3( 1.0, -v,   -u)); }  // +X
        case 1u: { return normalize(vec3(-1.0, -v,    u)); }  // -X
        case 2u: { return normalize(vec3( u,    1.0,  v)); }  // +Y
        case 3u: { return normalize(vec3( u,   -1.0, -v)); }  // -Y
        case 4u: { return normalize(vec3( u,   -v,   1.0)); } // +Z
        default: { return normalize(vec3(-u,   -v,  -1.0)); } // -Z
    }
}

// Convert world direction to equirectangular UV.
fn dir_to_equirect(dir: vec3<f32>) -> vec2<f32> {
    let u = atan2(dir.z, dir.x) / (2.0 * PI) + 0.5;
    let v = asin(clamp(dir.y, -1.0, 1.0)) / PI + 0.5;
    return vec2(u, 1.0 - v);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(output);
    let size = dims.x;
    if id.x >= size || id.y >= size || id.z >= 6u {
        return;
    }

    let uv = (vec2<f32>(id.xy) + 0.5) / f32(size);
    let dir = cube_dir(id.z, uv);
    let equirect_uv = dir_to_equirect(dir);

    // Convert UV to integer texel coords (nearest-neighbor)
    let tex_dims = textureDimensions(equirect);
    let coord = vec2<i32>(
        i32(equirect_uv.x * f32(tex_dims.x)) % i32(tex_dims.x),
        clamp(i32(equirect_uv.y * f32(tex_dims.y)), 0, i32(tex_dims.y) - 1),
    );
    let color = textureLoad(equirect, coord, 0);
    textureStore(output, vec2<i32>(id.xy), i32(id.z), color);
}
