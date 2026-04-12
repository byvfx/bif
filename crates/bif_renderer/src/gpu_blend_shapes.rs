//! GPU blend shape evaluation stubs (v0.13.6).
//!
//! CPU-only for now. These types reserve the data layout so a future GPU
//! skinning pass can upload delta textures / storage buffers without
//! refactoring the `Mesh` struct.

/// Placeholder for future GPU blend shape evaluation.
///
/// v0.13.6: CPU path only. This struct captures the dimensions a GPU
/// implementation would need (target count, vertex count) so the buffer
/// layout can be designed before any shader work begins.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct GpuBlendShapeLayout {
    /// Number of blend shape targets for this mesh.
    pub target_count: u32,
    /// Number of vertices (must match mesh vertex buffer length).
    pub vertex_count: u32,
    // Future fields:
    // pub delta_texture_width: u32,
    // pub delta_texture_height: u32,
    // pub storage_buffer_offset: u64,
}
