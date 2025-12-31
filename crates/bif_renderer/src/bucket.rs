//! Bucket-based tile rendering for Ivar.
//!
//! Divides the image into tiles (buckets) that can be rendered
//! independently and in parallel using rayon.

use crate::{Camera, Hittable, RenderConfig, Color};
use crate::renderer::render_pixel;

/// A rectangular region of the image to render.
#[derive(Debug, Clone, Copy)]
pub struct Bucket {
    /// X coordinate of bucket's top-left corner
    pub x: u32,
    /// Y coordinate of bucket's top-left corner
    pub y: u32,
    /// Width of the bucket in pixels
    pub width: u32,
    /// Height of the bucket in pixels
    pub height: u32,
    /// Index of this bucket in the render order
    pub index: usize,
}

impl Bucket {
    /// Create a new bucket.
    pub fn new(x: u32, y: u32, width: u32, height: u32, index: usize) -> Self {
        Self { x, y, width, height, index }
    }
    
    /// Get the total number of pixels in this bucket.
    pub fn pixel_count(&self) -> u32 {
        self.width * self.height
    }
}

/// Default bucket size in pixels.
/// TODO: Expose bucket_size in UI (currently hardcoded to 64)
pub const DEFAULT_BUCKET_SIZE: u32 = 64;

/// Generate buckets for an image, sorted in spiral order from center.
/// 
/// This mimics the rendering pattern of production renderers like
/// V-Ray and RenderMan, where buckets are rendered from the center
/// outward so artists see the most important parts first.
pub fn generate_buckets(width: u32, height: u32, bucket_size: u32) -> Vec<Bucket> {
    let mut buckets = Vec::new();
    let mut index = 0;
    
    // Generate grid of buckets
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            let bw = bucket_size.min(width - x);
            let bh = bucket_size.min(height - y);
            buckets.push(Bucket::new(x, y, bw, bh, index));
            index += 1;
            x += bucket_size;
        }
        y += bucket_size;
    }
    
    // Sort by distance from center (spiral order)
    sort_spiral(&mut buckets, width, height);
    
    // Update indices after sorting
    for (i, bucket) in buckets.iter_mut().enumerate() {
        bucket.index = i;
    }
    
    buckets
}

/// Sort buckets by distance from image center (spiral order).
/// 
/// Buckets closer to the center are rendered first, so the artist
/// sees the most visually important part of the image early.
fn sort_spiral(buckets: &mut [Bucket], width: u32, height: u32) {
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    
    buckets.sort_by(|a, b| {
        let a_center_x = a.x as f32 + a.width as f32 / 2.0;
        let a_center_y = a.y as f32 + a.height as f32 / 2.0;
        let b_center_x = b.x as f32 + b.width as f32 / 2.0;
        let b_center_y = b.y as f32 + b.height as f32 / 2.0;
        
        let a_dist = (a_center_x - center_x).powi(2) + (a_center_y - center_y).powi(2);
        let b_dist = (b_center_x - center_x).powi(2) + (b_center_y - center_y).powi(2);
        
        a_dist.partial_cmp(&b_dist).unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Render a single bucket to a vector of colors.
/// 
/// Returns pixels in row-major order within the bucket.
pub fn render_bucket(
    bucket: &Bucket,
    camera: &Camera,
    world: &dyn Hittable,
    config: &RenderConfig,
) -> Vec<Color> {
    let mut pixels = Vec::with_capacity((bucket.width * bucket.height) as usize);
    
    for local_y in 0..bucket.height {
        for local_x in 0..bucket.width {
            let global_x = bucket.x + local_x;
            let global_y = bucket.y + local_y;
            let color = render_pixel(camera, world, global_x, global_y, config);
            pixels.push(color);
        }
    }
    
    pixels
}

/// Result of rendering a bucket.
#[derive(Debug, Clone)]
pub struct BucketResult {
    /// The bucket that was rendered
    pub bucket: Bucket,
    /// Pixel colors in row-major order
    pub pixels: Vec<Color>,
}

impl BucketResult {
    /// Create a new bucket result.
    pub fn new(bucket: Bucket, pixels: Vec<Color>) -> Self {
        Self { bucket, pixels }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_buckets_exact_fit() {
        let buckets = generate_buckets(128, 128, 64);
        assert_eq!(buckets.len(), 4); // 2x2 grid
        
        // Total pixels should equal image size
        let total_pixels: u32 = buckets.iter().map(|b| b.pixel_count()).sum();
        assert_eq!(total_pixels, 128 * 128);
    }
    
    #[test]
    fn test_generate_buckets_partial_fit() {
        let buckets = generate_buckets(100, 100, 64);
        assert_eq!(buckets.len(), 4); // 2x2 grid with partial buckets
        
        // Total pixels should equal image size
        let total_pixels: u32 = buckets.iter().map(|b| b.pixel_count()).sum();
        assert_eq!(total_pixels, 100 * 100);
    }
    
    #[test]
    fn test_spiral_order() {
        let buckets = generate_buckets(192, 192, 64);
        assert_eq!(buckets.len(), 9); // 3x3 grid
        
        // First bucket should be the center one
        let first = &buckets[0];
        assert_eq!(first.x, 64);
        assert_eq!(first.y, 64);
    }
}
