// Box-filter mipmap downsample compute shader.
//
// Reads 2x2 texels from the source mip level (as texture_2d),
// averages them, and writes to the destination mip level (as storage texture).

@group(0) @binding(0) var src_texture: texture_2d<f32>;
@group(0) @binding(1) var dst_texture: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dst_size = textureDimensions(dst_texture);
    if gid.x >= dst_size.x || gid.y >= dst_size.y {
        return;
    }

    // Source coordinates (2x2 block)
    let src_x = gid.x * 2u;
    let src_y = gid.y * 2u;
    let src_size = textureDimensions(src_texture, 0);

    // Clamp source coordinates
    let x0 = src_x;
    let y0 = src_y;
    let x1 = min(src_x + 1u, src_size.x - 1u);
    let y1 = min(src_y + 1u, src_size.y - 1u);

    // Box filter: average 4 texels
    let p00 = textureLoad(src_texture, vec2<i32>(i32(x0), i32(y0)), 0);
    let p10 = textureLoad(src_texture, vec2<i32>(i32(x1), i32(y0)), 0);
    let p01 = textureLoad(src_texture, vec2<i32>(i32(x0), i32(y1)), 0);
    let p11 = textureLoad(src_texture, vec2<i32>(i32(x1), i32(y1)), 0);

    let avg = (p00 + p10 + p01 + p11) * 0.25;

    textureStore(dst_texture, vec2<i32>(i32(gid.x), i32(gid.y)), avg);
}
