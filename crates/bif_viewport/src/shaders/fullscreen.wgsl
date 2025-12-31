// Fullscreen quad shader for displaying Ivar render output.
//
// Uses a single oversized triangle to cover the entire screen,
// which is more efficient than a traditional two-triangle quad.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle technique:
    // Generate 3 vertices that form a triangle covering the entire screen.
    // This avoids the need for a vertex buffer.
    //
    // Vertex 0: (-1, -1) -> (0, 1)  bottom-left
    // Vertex 1: ( 3, -1) -> (2, 1)  far right
    // Vertex 2: (-1,  3) -> (0, -1) far top
    
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    
    // UV coordinates: flip Y so (0,0) is top-left to match image layout
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    
    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@group(0) @binding(0) var ivar_texture: texture_2d<f32>;
@group(0) @binding(1) var ivar_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(ivar_texture, ivar_sampler, in.uv);
}
