// Infinite ground grid on XZ plane.
// Fullscreen triangle + ray-plane intersection, anti-aliased lines with distance fade.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    camera_position: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

// Fullscreen triangle (3 vertices, no vertex buffer needed)
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index & 2u) * 2 - 1);
    out.clip_position = vec4<f32>(x, y, 1.0, 1.0);
    out.ndc = vec2<f32>(x, y);
    return out;
}

struct FragOutput {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

@fragment
fn fs_main(in: VertexOutput) -> FragOutput {
    var out: FragOutput;

    // Reconstruct world ray from NDC
    let near_point = camera.inv_view_proj * vec4<f32>(in.ndc, 0.0, 1.0);
    let far_point = camera.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let world_near = near_point.xyz / near_point.w;
    let world_far = far_point.xyz / far_point.w;
    let ray_dir = normalize(world_far - world_near);
    let ray_origin = camera.camera_position.xyz;

    // Ray-plane intersection at Y=0
    // origin.y + t * dir.y = 0  =>  t = -origin.y / dir.y
    if abs(ray_dir.y) < 1e-6 {
        discard;
    }
    let t = -ray_origin.y / ray_dir.y;
    if t < 0.0 {
        discard;
    }

    let world_pos = ray_origin + t * ray_dir;
    let world_xz = vec2<f32>(world_pos.x, world_pos.z);

    // Distance from camera for fade
    let dist = length(world_pos - ray_origin);
    let max_dist = 200.0;
    if dist > max_dist {
        discard;
    }

    // Anti-aliased grid lines using fwidth
    // Minor grid: 1m spacing
    let minor_coord = fract(world_xz + 0.5) - 0.5; // center on integer
    let minor_fw = fwidth(world_xz);
    let minor_line = smoothstep(minor_fw * 0.5, minor_fw * 1.5, abs(minor_coord));
    let minor_grid = 1.0 - min(minor_line.x, minor_line.y);

    // Major grid: 10m spacing
    let major_xz = world_xz / 10.0;
    let major_coord = fract(major_xz + 0.5) - 0.5;
    let major_fw = fwidth(major_xz);
    let major_line = smoothstep(major_fw * 0.5, major_fw * 1.5, abs(major_coord));
    let major_grid = 1.0 - min(major_line.x, major_line.y);

    // Axis lines: X-axis red (Z≈0), Z-axis blue (X≈0)
    let axis_fw = minor_fw;
    let x_axis = smoothstep(axis_fw.y * 1.5, axis_fw.y * 0.5, abs(world_xz.y)); // Z ≈ 0
    let z_axis = smoothstep(axis_fw.x * 1.5, axis_fw.x * 0.5, abs(world_xz.x)); // X ≈ 0

    // Combine: axis > major > minor
    let base_gray = vec3<f32>(0.4, 0.4, 0.4);
    let major_gray = vec3<f32>(0.55, 0.55, 0.55);
    let axis_red = vec3<f32>(0.8, 0.2, 0.2);
    let axis_blue = vec3<f32>(0.2, 0.2, 0.8);

    var color = base_gray * minor_grid;
    color = mix(color, major_gray, major_grid);
    color = mix(color, axis_red, x_axis);
    color = mix(color, axis_blue, z_axis);

    let grid_alpha = max(minor_grid, max(major_grid, max(x_axis, z_axis)));

    // Distance fade
    let fade = 1.0 - smoothstep(max_dist * 0.5, max_dist, dist);
    let alpha = grid_alpha * fade * 0.7;

    if alpha < 0.001 {
        discard;
    }

    out.color = vec4<f32>(color, alpha);

    // Write correct depth so geometry occludes grid
    let clip_pos = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.depth = clip_pos.z / clip_pos.w;

    return out;
}
