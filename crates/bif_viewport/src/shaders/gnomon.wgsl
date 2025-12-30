// Gnomon shader - draws XYZ axis indicator in corner
// Uses camera view rotation to orient axes with viewport

struct GnomonUniform {
    view_rotation: mat4x4<f32>,  // Camera view matrix rotation only (no translation)
}

@group(0) @binding(0)
var<uniform> gnomon: GnomonUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Apply camera rotation to axis direction (use w=0 for pure direction transform)
    let rotated = (gnomon.view_rotation * vec4<f32>(in.position, 0.0)).xyz;
    
    // The gnomon viewport is set to bottom-left corner
    // Output in NDC: scale to fit nicely and keep it centered
    out.clip_position = vec4<f32>(rotated.xy * 0.7, 0.0, 1.0);
    out.color = in.color;
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
