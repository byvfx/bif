//! SHARC radiance cache A/B benchmark.
//!
//! Renders a Cornell box with cache OFF then ON, printing timing + stats.
//! Uses deferred thread-local writes to eliminate write-lock contention.
//!
//! Run: `cargo run --example cache_bench -p bif_renderer --release`

use std::hint::black_box;
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
const TOTAL_PASSES: u32 = 32;
const COLD_PASSES: u32 = 5;
const WARMUP_PASSES: u32 = 3;
const MAX_DEPTH: u32 = 10;
const BUCKET_SIZE: u32 = 64;

/// Per-pass measurements.
struct PassStats {
    elapsed: Duration,
    energy: f64,
    /// Per-pass hit rate (delta, not cumulative). None for cache OFF.
    hit_rate: Option<f32>,
    occupancy: Option<f32>,
}

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
    let lights = Arc::new(LightList::new());
    let cell_size = auto_cell_size(&scene_aabb);

    println!("Scene: Cornell box (12 tris + 2 spheres)");
    println!(
        "Resolution: {}x{}, {} passes ({} cold + {} warm), max_depth={}",
        WIDTH,
        HEIGHT,
        TOTAL_PASSES,
        COLD_PASSES,
        TOTAL_PASSES - COLD_PASSES,
        MAX_DEPTH
    );
    println!("Cache: deferred thread-local writes, cell_size={cell_size:.4}");
    println!();

    // --- Warmup before OFF ---
    print!("Warming up ({WARMUP_PASSES} passes)...");
    run_warmup(&buckets, &camera, &world, &lights);
    println!(" done");
    println!();

    // --- Cache OFF ---
    println!("--- Cache OFF ---");
    let off_stats = run_off(&buckets, &camera, &world, &lights);
    print_pass_table(&off_stats);

    // --- Warmup before ON ---
    print!("Warming up ({WARMUP_PASSES} passes)...");
    run_warmup(&buckets, &camera, &world, &lights);
    println!(" done");
    println!();

    // --- Cache ON ---
    println!("--- Cache ON (deferred writes) ---");
    let cache = Arc::new(RadianceCache::new(RadianceCacheConfig {
        cell_size,
        deferred_writes: true,
        ..Default::default()
    }));
    let on_stats = run_on(&buckets, &camera, &world, &lights, &cache);
    print_pass_table(&on_stats);

    // --- Results ---
    print_results(&off_stats, &on_stats, &cache);
}

// ---------------------------------------------------------------------------
// Run helpers
// ---------------------------------------------------------------------------

fn run_warmup(
    buckets: &[bif_renderer::Bucket],
    camera: &Camera,
    world: &BvhNode,
    lights: &Arc<LightList>,
) {
    for pass in 0..WARMUP_PASSES {
        let config = RenderConfig {
            samples_per_pixel: 1,
            max_depth: MAX_DEPTH,
            pass_number: 1000 + pass, // distinct seed from timed passes
            lights: Arc::clone(lights),
            ..Default::default()
        };
        buckets.par_iter().for_each(|bucket| {
            black_box(render_bucket(bucket, camera, world, &config));
        });
    }
}

fn run_off(
    buckets: &[bif_renderer::Bucket],
    camera: &Camera,
    world: &BvhNode,
    lights: &Arc<LightList>,
) -> Vec<PassStats> {
    let mut stats = Vec::with_capacity(TOTAL_PASSES as usize);
    for pass in 0..TOTAL_PASSES {
        let config = RenderConfig {
            samples_per_pixel: 1,
            max_depth: MAX_DEPTH,
            pass_number: pass,
            lights: Arc::clone(lights),
            ..Default::default()
        };
        let start = Instant::now();
        let energy: f64 = buckets
            .par_iter()
            .map(|bucket| {
                let pixels = render_bucket(bucket, camera, world, &config);
                pixels.iter().map(|c| (c.x + c.y + c.z) as f64).sum::<f64>()
            })
            .sum();
        let elapsed = start.elapsed();
        stats.push(PassStats {
            elapsed,
            energy,
            hit_rate: None,
            occupancy: None,
        });
    }
    stats
}

fn run_on(
    buckets: &[bif_renderer::Bucket],
    camera: &Camera,
    world: &BvhNode,
    lights: &Arc<LightList>,
    cache: &Arc<RadianceCache>,
) -> Vec<PassStats> {
    let mut stats = Vec::with_capacity(TOTAL_PASSES as usize);
    for pass in 0..TOTAL_PASSES {
        let config = RenderConfig {
            samples_per_pixel: 1,
            max_depth: MAX_DEPTH,
            pass_number: pass,
            lights: Arc::clone(lights),
            radiance_cache: Some(Arc::clone(cache)),
            ..Default::default()
        };

        let reads_before = cache.stat_reads();
        let hits_before = cache.stat_hits();

        let start = Instant::now();
        let energy: f64 = buckets
            .par_iter()
            .map(|bucket| {
                let pixels = render_bucket(bucket, camera, world, &config);
                pixels.iter().map(|c| (c.x + c.y + c.z) as f64).sum::<f64>()
            })
            .sum();

        // Flush deferred writes from all rayon workers + main thread
        rayon::broadcast(|_| cache.flush_thread_writes());
        cache.flush_thread_writes();

        let elapsed = start.elapsed();

        // Per-pass delta hit rate
        let pass_reads = cache.stat_reads() - reads_before;
        let pass_hits = cache.stat_hits() - hits_before;
        let pass_hr = if pass_reads > 0 {
            pass_hits as f32 / pass_reads as f32
        } else {
            0.0
        };

        stats.push(PassStats {
            elapsed,
            energy,
            hit_rate: Some(pass_hr),
            occupancy: Some(cache.occupancy()),
        });

        cache.advance_frame();
    }
    stats
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

fn print_pass_table(stats: &[PassStats]) {
    let total = stats.len() as u32;
    for (i, s) in stats.iter().enumerate() {
        let pass = i as u32 + 1;
        let phase = if pass <= COLD_PASSES { " [cold]" } else { "" };
        match (s.hit_rate, s.occupancy) {
            (Some(hr), Some(occ)) => println!(
                "Pass {:2}/{}: {:>5}ms  hit={:.1}%  occ={:.1}%{}",
                pass,
                total,
                s.elapsed.as_millis(),
                hr * 100.0,
                occ * 100.0,
                phase,
            ),
            _ => println!(
                "Pass {:2}/{}: {:>5}ms{}",
                pass,
                total,
                s.elapsed.as_millis(),
                phase,
            ),
        }
    }
    let total_ms: u128 = stats.iter().map(|s| s.elapsed.as_millis()).sum();
    let cold_ms: u128 = stats
        .iter()
        .take(COLD_PASSES as usize)
        .map(|s| s.elapsed.as_millis())
        .sum();
    let warm_ms = total_ms - cold_ms;
    println!("Total: {total_ms}ms  (cold: {cold_ms}ms, warm: {warm_ms}ms)");
    println!();
}

fn print_results(off: &[PassStats], on: &[PassStats], cache: &RadianceCache) {
    let cold = COLD_PASSES as usize;

    let off_total: Duration = off.iter().map(|s| s.elapsed).sum();
    let on_total: Duration = on.iter().map(|s| s.elapsed).sum();
    let off_cold: Duration = off.iter().take(cold).map(|s| s.elapsed).sum();
    let on_cold: Duration = on.iter().take(cold).map(|s| s.elapsed).sum();
    let off_warm: Duration = off.iter().skip(cold).map(|s| s.elapsed).sum();
    let on_warm: Duration = on.iter().skip(cold).map(|s| s.elapsed).sum();

    let speedup = |a: Duration, b: Duration| -> f64 {
        if b.as_nanos() > 0 {
            a.as_secs_f64() / b.as_secs_f64()
        } else {
            0.0
        }
    };

    // Median pass time
    let median = |stats: &[PassStats]| -> Duration {
        let mut times: Vec<Duration> = stats.iter().map(|s| s.elapsed).collect();
        times.sort();
        times[times.len() / 2]
    };

    println!("=== RESULTS ===");
    println!(
        "              {:>8} {:>8} {:>8} {:>8}",
        "Total", "Cold", "Warm", "Median"
    );
    println!(
        "Cache OFF:    {:>5}ms {:>5}ms {:>5}ms {:>5}ms",
        off_total.as_millis(),
        off_cold.as_millis(),
        off_warm.as_millis(),
        median(off).as_millis(),
    );
    println!(
        "Cache ON:     {:>5}ms {:>5}ms {:>5}ms {:>5}ms",
        on_total.as_millis(),
        on_cold.as_millis(),
        on_warm.as_millis(),
        median(on).as_millis(),
    );
    println!(
        "Speedup:      {:>5.2}x {:>5.2}x {:>5.2}x",
        speedup(off_total, on_total),
        speedup(off_cold, on_cold),
        speedup(off_warm, on_warm),
    );
    println!();

    // Energy validation
    let energy_off: f64 = off.iter().map(|s| s.energy).sum();
    let energy_on: f64 = on.iter().map(|s| s.energy).sum();
    let divergence = if energy_off.abs() > 1e-10 {
        ((energy_on - energy_off) / energy_off).abs() * 100.0
    } else {
        0.0
    };
    println!("Energy OFF: {energy_off:.2}");
    println!("Energy ON:  {energy_on:.2}");
    println!("Divergence: {divergence:.1}%");
    println!();

    // Final cache stats
    println!("Hit rate:  {:.1}% (cumulative)", cache.hit_rate() * 100.0);
    println!("Occupancy: {:.1}%", cache.occupancy() * 100.0);
    if let Some(last) = on.last() {
        if let Some(hr) = last.hit_rate {
            println!("Last pass:  hit={:.1}%", hr * 100.0);
        }
    }
}

// ---------------------------------------------------------------------------
// Scene
// ---------------------------------------------------------------------------

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
/// Background is black (closed box, no sky).
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
