//! SHARC radiance cache A/B benchmark.
//!
//! Renders a Cornell box with cache OFF then ON, printing timing + stats.
//!
//! Run: `cargo run --example cache_bench -p bif_renderer --release`

use std::sync::Arc;
use std::time::{Duration, Instant};

use rayon::prelude::*;

use bif_renderer::{
    generate_buckets, radiance_cache::auto_cell_size, render_bucket, BvhNode, Camera, Color,
    DiffuseLight, Hittable, Lambertian, LightList, Material, RadianceCache, RadianceCacheConfig,
    RenderConfig, Sphere, Triangle, Vec3,
};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 256;
const PASSES: u32 = 32;
const MAX_DEPTH: u32 = 10;
const BUCKET_SIZE: u32 = 64;

fn main() {
    println!("BIF SHARC Cache Benchmark");
    println!("==========================");

    let world = build_cornell_box();
    let scene_aabb = world.bounding_box();

    let mut camera = Camera::new()
        .with_resolution(WIDTH, HEIGHT)
        .with_quality(1, MAX_DEPTH)
        .with_position(Vec3::new(2.5, 2.5, -8.0), Vec3::new(2.5, 2.5, 2.5), Vec3::Y)
        .with_lens(40.0, 0.0, 10.0);
    camera.initialize();

    let buckets = generate_buckets(WIDTH, HEIGHT, BUCKET_SIZE);

    println!("Scene: Cornell box (12 tris + 2 spheres)");
    println!(
        "Resolution: {}x{}, {} passes, max_depth={}",
        WIDTH, HEIGHT, PASSES, MAX_DEPTH
    );
    println!();

    // --- Cache OFF ---
    println!("--- Cache OFF ---");
    let mut total_off = Duration::ZERO;
    for pass in 0..PASSES {
        let config = RenderConfig {
            samples_per_pixel: 1,
            max_depth: MAX_DEPTH,
            pass_number: pass,
            lights: Arc::new(LightList::new()),
            radiance_cache: None,
            ..Default::default()
        };
        let start = Instant::now();
        buckets.par_iter().for_each(|bucket| {
            render_bucket(bucket, &camera, &world, &config);
        });
        let elapsed = start.elapsed();
        total_off += elapsed;
        println!(
            "Pass {:2}/{}: {:>5}ms",
            pass + 1,
            PASSES,
            elapsed.as_millis()
        );
    }
    println!("Total: {}ms", total_off.as_millis());
    println!();

    // --- Cache ON ---
    println!("--- Cache ON ---");
    let cell_size = auto_cell_size(&scene_aabb);
    let cache = Arc::new(RadianceCache::new(RadianceCacheConfig {
        cell_size,
        ..Default::default()
    }));
    let mut total_on = Duration::ZERO;
    for pass in 0..PASSES {
        let config = RenderConfig {
            samples_per_pixel: 1,
            max_depth: MAX_DEPTH,
            pass_number: pass,
            lights: Arc::new(LightList::new()),
            radiance_cache: Some(Arc::clone(&cache)),
            ..Default::default()
        };
        let start = Instant::now();
        buckets.par_iter().for_each(|bucket| {
            render_bucket(bucket, &camera, &world, &config);
        });
        let elapsed = start.elapsed();
        total_on += elapsed;
        println!(
            "Pass {:2}/{}: {:>5}ms  hit={:.1}%  occ={:.1}%",
            pass + 1,
            PASSES,
            elapsed.as_millis(),
            cache.hit_rate() * 100.0,
            cache.occupancy() * 100.0,
        );
        cache.advance_frame();
    }
    println!("Total: {}ms", total_on.as_millis());
    println!();

    // --- Results ---
    println!("=== RESULTS ===");
    println!("Cache OFF: {}ms", total_off.as_millis());
    println!("Cache ON:  {}ms", total_on.as_millis());
    if total_on.as_nanos() > 0 {
        println!(
            "Speedup:   {:.2}x",
            total_off.as_secs_f64() / total_on.as_secs_f64()
        );
    }
    println!("Hit rate:  {:.1}%", cache.hit_rate() * 100.0);
    println!("Occupancy: {:.1}%", cache.occupancy() * 100.0);
}

/// Push two triangles forming a quad.
///
/// Vertices must be in CCW order when viewed from the front (normal side).
/// The `edge2.cross(edge1)` convention in `Triangle::new` produces the
/// correct inward-facing normal for this winding.
fn push_quad<M: Material + 'static>(
    objects: &mut Vec<Box<dyn Hittable + Send + Sync>>,
    a: Vec3,
    b: Vec3,
    c: Vec3,
    d: Vec3,
    mat: impl Fn() -> M,
) {
    objects.push(Box::new(Triangle::new(a, b, c, mat())));
    objects.push(Box::new(Triangle::new(c, d, a, mat())));
}

/// Build a Cornell box: 5 walls + ceiling light + 2 spheres.
///
/// Box spans (0,0,0) to (5,5,5). Camera looks in from z < 0.
fn build_cornell_box() -> BvhNode {
    let mut objects: Vec<Box<dyn Hittable + Send + Sync>> = Vec::new();

    let white = || Lambertian::new(Color::new(0.73, 0.73, 0.73));
    let red = || Lambertian::new(Color::new(0.65, 0.05, 0.05));
    let green = || Lambertian::new(Color::new(0.12, 0.45, 0.15));
    let light_mat = || DiffuseLight::new(Color::new(15.0, 15.0, 15.0));

    // Floor (y=0, normal +Y)
    push_quad(
        &mut objects,
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(5.0, 0.0, 0.0),
        Vec3::new(5.0, 0.0, 5.0),
        Vec3::new(0.0, 0.0, 5.0),
        white,
    );

    // Ceiling (y=5, normal -Y)
    push_quad(
        &mut objects,
        Vec3::new(0.0, 5.0, 5.0),
        Vec3::new(5.0, 5.0, 5.0),
        Vec3::new(5.0, 5.0, 0.0),
        Vec3::new(0.0, 5.0, 0.0),
        white,
    );

    // Back wall (z=5, normal -Z)
    push_quad(
        &mut objects,
        Vec3::new(0.0, 0.0, 5.0),
        Vec3::new(5.0, 0.0, 5.0),
        Vec3::new(5.0, 5.0, 5.0),
        Vec3::new(0.0, 5.0, 5.0),
        white,
    );

    // Left wall (x=0, normal +X) — red
    push_quad(
        &mut objects,
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 5.0),
        Vec3::new(0.0, 5.0, 5.0),
        Vec3::new(0.0, 5.0, 0.0),
        red,
    );

    // Right wall (x=5, normal -X) — green
    push_quad(
        &mut objects,
        Vec3::new(5.0, 0.0, 5.0),
        Vec3::new(5.0, 0.0, 0.0),
        Vec3::new(5.0, 5.0, 0.0),
        Vec3::new(5.0, 5.0, 5.0),
        green,
    );

    // Ceiling light (y=4.99, normal -Y)
    push_quad(
        &mut objects,
        Vec3::new(1.5, 4.99, 3.5),
        Vec3::new(3.5, 4.99, 3.5),
        Vec3::new(3.5, 4.99, 1.5),
        Vec3::new(1.5, 4.99, 1.5),
        light_mat,
    );

    // Two white spheres
    objects.push(Box::new(Sphere::new(
        Vec3::new(1.75, 1.0, 2.5),
        1.0,
        Lambertian::new(Color::new(0.73, 0.73, 0.73)),
    )));
    objects.push(Box::new(Sphere::new(
        Vec3::new(3.5, 1.0, 3.5),
        1.0,
        Lambertian::new(Color::new(0.73, 0.73, 0.73)),
    )));

    BvhNode::new(objects)
}
