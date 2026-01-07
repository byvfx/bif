//! Example: Load and inspect a USDA file.
//!
//! Run with: cargo run --example load_usda -- assets/test_cube.usda

use std::env;

use bif_core::usd::load_usda;

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage: load_usda <path-to-usda-file>");
        println!("\nExamples:");
        println!("  cargo run --example load_usda -- assets/test_cube.usda");
        println!("  cargo run --example load_usda -- assets/test_grid.usda");
        println!("  cargo run --example load_usda -- assets/test_transform.usda");
        return;
    }

    let path = &args[1];
    println!("Loading USDA file: {}", path);

    match load_usda(path) {
        Ok(scene) => {
            println!("\n=== Scene: {} ===", scene.name);
            println!("Prototypes: {}", scene.prototype_count());
            println!("Instances: {}", scene.instance_count());
            println!("Total triangles: {}", scene.total_triangle_count());

            println!("\n--- Prototypes ---");
            for proto in &scene.prototypes {
                println!(
                    "  [{}] {} - {} vertices, {} triangles",
                    proto.id,
                    proto.name,
                    proto.mesh.vertex_count(),
                    proto.mesh.triangle_count()
                );
                println!(
                    "       Bounds: ({:.2}, {:.2}, {:.2}) to ({:.2}, {:.2}, {:.2})",
                    proto.bounds.x.min,
                    proto.bounds.y.min,
                    proto.bounds.z.min,
                    proto.bounds.x.max,
                    proto.bounds.y.max,
                    proto.bounds.z.max
                );
                println!("       Has normals: {}", proto.mesh.has_normals());
            }

            println!("\n--- Instances ---");
            for (i, instance) in scene.instances.iter().enumerate() {
                let matrix = instance.model_matrix();
                let pos = matrix.transform_point3(bif_math::Vec3::ZERO);
                println!(
                    "  [{}] Proto {} at ({:.2}, {:.2}, {:.2})",
                    i, instance.prototype_id, pos.x, pos.y, pos.z
                );
            }

            let world_bounds = scene.world_bounds();
            println!("\n--- World Bounds ---");
            println!(
                "  Min: ({:.2}, {:.2}, {:.2})",
                world_bounds.x.min, world_bounds.y.min, world_bounds.z.min
            );
            println!(
                "  Max: ({:.2}, {:.2}, {:.2})",
                world_bounds.x.max, world_bounds.y.max, world_bounds.z.max
            );
        }
        Err(e) => {
            eprintln!("Error loading USDA file: {}", e);
        }
    }
}
