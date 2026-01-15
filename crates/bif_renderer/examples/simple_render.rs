//! Simple path tracer example.
//!
//! Renders the classic "Ray Tracing in One Weekend" scene.

use bif_renderer::{
    color_to_rgba, render, BvhNode, Camera, Color, Dielectric, Hittable, Lambertian, Metal,
    RenderConfig, Sphere, Vec3,
};
use rand::SeedableRng;

fn main() {
    println!("BIF Path Tracer - Simple Example");
    println!("=================================");

    // Build the scene
    let start = std::time::Instant::now();
    let world = build_scene();
    println!("Scene built in {:?}", start.elapsed());

    // Set up camera - classic RTIOW view
    let mut camera = Camera::new()
        .with_resolution(800, 450)
        .with_quality(100, 50)
        .with_position(
            Vec3::new(13.0, 2.0, 3.0), // look_from
            Vec3::new(0.0, 0.0, 0.0),  // look_at
            Vec3::new(0.0, 1.0, 0.0),  // vup
        )
        .with_lens(20.0, 0.0, 10.0); // No DOF blur
    camera.initialize();

    // Render configuration
    let config = RenderConfig {
        samples_per_pixel: 100,
        max_depth: 50,
        background: Color::new(0.7, 0.8, 1.0),
        use_sky_gradient: true,
    };

    println!(
        "Rendering {}x{} @ {} spp...",
        camera.image_width, camera.image_height, config.samples_per_pixel
    );

    // Render
    let start = std::time::Instant::now();
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let image = render(&camera, &world, &config, &mut rng);
    let render_time = start.elapsed();

    println!("Rendered in {:?}", render_time);

    // Save as PNG
    let filename = "output.png";
    save_png(&image, filename).expect("Failed to save image");
    println!("Saved to {}", filename);
}

#[allow(clippy::vec_init_then_push)]
fn build_scene() -> BvhNode {
    let mut objects: Vec<Box<dyn Hittable + Send + Sync>> = Vec::new();

    // Ground
    objects.push(Box::new(Sphere::new(
        Vec3::new(0.0, -1000.0, 0.0),
        1000.0,
        Lambertian::new(Color::new(0.5, 0.5, 0.5)),
    )));

    // Three main spheres
    objects.push(Box::new(Sphere::new(
        Vec3::new(0.0, 1.0, 0.0),
        1.0,
        Dielectric::new(1.5),
    )));

    objects.push(Box::new(Sphere::new(
        Vec3::new(-4.0, 1.0, 0.0),
        1.0,
        Lambertian::new(Color::new(0.4, 0.2, 0.1)),
    )));

    objects.push(Box::new(Sphere::new(
        Vec3::new(4.0, 1.0, 0.0),
        1.0,
        Metal::new(Color::new(0.7, 0.6, 0.5), 0.0),
    )));

    // Small random spheres
    use rand::Rng;
    let mut rng = rand::thread_rng();

    for a in -11..11 {
        for b in -11..11 {
            let center = Vec3::new(
                a as f32 + 0.9 * rng.gen::<f32>(),
                0.2,
                b as f32 + 0.9 * rng.gen::<f32>(),
            );

            // Skip if too close to main spheres
            if (center - Vec3::new(4.0, 0.2, 0.0)).length() > 0.9
                && (center - Vec3::new(0.0, 0.2, 0.0)).length() > 0.9
                && (center - Vec3::new(-4.0, 0.2, 0.0)).length() > 0.9
            {
                let choose_mat: f32 = rng.gen();

                if choose_mat < 0.8 {
                    // Diffuse
                    let albedo = Color::new(
                        rng.gen::<f32>() * rng.gen::<f32>(),
                        rng.gen::<f32>() * rng.gen::<f32>(),
                        rng.gen::<f32>() * rng.gen::<f32>(),
                    );
                    objects.push(Box::new(Sphere::new(center, 0.2, Lambertian::new(albedo))));
                } else if choose_mat < 0.95 {
                    // Metal
                    let albedo = Color::new(
                        0.5 + 0.5 * rng.gen::<f32>(),
                        0.5 + 0.5 * rng.gen::<f32>(),
                        0.5 + 0.5 * rng.gen::<f32>(),
                    );
                    let fuzz = 0.5 * rng.gen::<f32>();
                    objects.push(Box::new(Sphere::new(center, 0.2, Metal::new(albedo, fuzz))));
                } else {
                    // Glass
                    objects.push(Box::new(Sphere::new(center, 0.2, Dielectric::new(1.5))));
                }
            }
        }
    }

    println!("Created {} objects", objects.len());
    BvhNode::new(objects)
}

fn save_png(
    image_buf: &bif_renderer::ImageBuffer,
    filename: &str,
) -> Result<(), image::ImageError> {
    let mut img = image::RgbImage::new(image_buf.width, image_buf.height);

    for y in 0..image_buf.height {
        for x in 0..image_buf.width {
            let color = image_buf.get(x, y);
            let rgba = color_to_rgba(color);
            img.put_pixel(x, y, image::Rgb([rgba[0], rgba[1], rgba[2]]));
        }
    }

    img.save(filename)
}
