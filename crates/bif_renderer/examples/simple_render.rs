//! Simple path tracer example.
//!
//! Renders a basic scene with spheres and saves to PPM format.

use bif_renderer::{
    Camera, BvhNode, Sphere, Hittable,
    Lambertian, Metal, Dielectric,
    RenderConfig, render, color_to_rgba,
    Color, Vec3,
};
use std::fs::File;
use std::io::{BufWriter, Write};

fn main() {
    println!("BIF Path Tracer - Simple Example");
    println!("=================================");
    
    // Build the scene
    let start = std::time::Instant::now();
    let world = build_scene();
    println!("Scene built in {:?}", start.elapsed());
    
    // Set up camera
    let mut camera = Camera::new()
        .with_resolution(800, 450)
        .with_quality(50, 10)
        .with_position(
            Vec3::new(13.0, 2.0, 3.0),  // look_from
            Vec3::new(0.0, 0.0, 0.0),   // look_at
            Vec3::new(0.0, 1.0, 0.0),   // vup
        )
        .with_lens(20.0, 0.6, 10.0);
    camera.initialize();
    
    // Render configuration
    let config = RenderConfig {
        samples_per_pixel: 50,
        max_depth: 10,
        background: Color::new(0.5, 0.7, 1.0),
        use_sky_gradient: true,
    };
    
    println!("Rendering {}x{} @ {} spp...", 
        camera.image_width, camera.image_height, config.samples_per_pixel);
    
    // Render
    let start = std::time::Instant::now();
    let image = render(&camera, &world, &config);
    let render_time = start.elapsed();
    
    println!("Rendered in {:?}", render_time);
    
    // Save as PPM
    let filename = "output.ppm";
    save_ppm(&image, filename).expect("Failed to save image");
    println!("Saved to {}", filename);
}

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
    
    for a in -5..5 {
        for b in -5..5 {
            let center = Vec3::new(
                a as f32 + 0.9 * rng.gen::<f32>(),
                0.2,
                b as f32 + 0.9 * rng.gen::<f32>(),
            );
            
            if (center - Vec3::new(4.0, 0.2, 0.0)).length() > 0.9 {
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

fn save_ppm(image: &bif_renderer::ImageBuffer, filename: &str) -> std::io::Result<()> {
    let file = File::create(filename)?;
    let mut writer = BufWriter::new(file);
    
    writeln!(writer, "P3")?;
    writeln!(writer, "{} {}", image.width, image.height)?;
    writeln!(writer, "255")?;
    
    for y in 0..image.height {
        for x in 0..image.width {
            let color = image.get(x, y);
            let rgba = color_to_rgba(color);
            writeln!(writer, "{} {} {}", rgba[0], rgba[1], rgba[2])?;
        }
    }
    
    Ok(())
}
