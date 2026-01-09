// Quick debug tool to inspect USD mesh data
// Run with: cargo run --release --bin debug_usd_mesh -- <path_to.usd>

use bif_core::usd::cpp_bridge::UsdStage;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path_to.usd>", args[0]);
        std::process::exit(1);
    }

    let path = &args[1];
    println!("Loading USD: {}", path);

    let stage = UsdStage::open(path)?;
    let meshes = stage.meshes()?;

    println!("\nFound {} mesh(es)", meshes.len());

    for (i, mesh) in meshes.iter().enumerate() {
        println!("\n=== Mesh {} ===", i);
        println!("Path: {}", mesh.path);
        println!("Vertices: {}", mesh.vertices.len());
        println!("Indices: {}", mesh.indices.len());
        println!("Triangles: {}", mesh.indices.len() / 3);

        if let Some(ref normals) = mesh.normals {
            println!("Normals: {} (has normals)", normals.len());
        } else {
            println!("Normals: None (will be computed)");
        }

        // Show first few triangles (skip degenerate ones)
        let num_tris = (mesh.indices.len() / 3).min(1000);
        println!(
            "\nChecking first {} triangles (skipping degenerate):",
            num_tris
        );

        for tri_idx in 0..num_tris {
            let i0 = mesh.indices[tri_idx * 3] as usize;
            let i1 = mesh.indices[tri_idx * 3 + 1] as usize;
            let i2 = mesh.indices[tri_idx * 3 + 2] as usize;

            if i0 < mesh.vertices.len() && i1 < mesh.vertices.len() && i2 < mesh.vertices.len() {
                let v0 = &mesh.vertices[i0];
                let v1 = &mesh.vertices[i1];
                let v2 = &mesh.vertices[i2];

                // Compute both cross products to see which is correct
                let edge1 = *v1 - *v0;
                let edge2 = *v2 - *v0;
                let normal_ccw = edge1.cross(edge2);
                let normal_cw = edge2.cross(edge1);

                let area = normal_ccw.length();

                // Skip degenerate triangles
                if area < 0.0001 {
                    continue;
                }

                println!("\n  Triangle {} indices: [{}, {}, {}]", tri_idx, i0, i1, i2);
                println!("    v0: ({:.3}, {:.3}, {:.3})", v0.x, v0.y, v0.z);
                println!("    v1: ({:.3}, {:.3}, {:.3})", v1.x, v1.y, v1.z);
                println!("    v2: ({:.3}, {:.3}, {:.3})", v2.x, v2.y, v2.z);
                println!(
                    "    edge1 × edge2 (CCW): ({:.3}, {:.3}, {:.3})",
                    normal_ccw.x, normal_ccw.y, normal_ccw.z
                );
                println!(
                    "    edge2 × edge1 (CW):  ({:.3}, {:.3}, {:.3})",
                    normal_cw.x, normal_cw.y, normal_cw.z
                );

                if let Some(ref normals) = mesh.normals {
                    if i0 < normals.len() {
                        let n0 = &normals[i0];
                        println!(
                            "    Stored normal at v0: ({:.3}, {:.3}, {:.3})",
                            n0.x, n0.y, n0.z
                        );

                        // Check which computed normal matches stored
                        let dist_ccw = (normal_ccw.normalize() - n0.normalize()).length();
                        let dist_cw = (normal_cw.normalize() - n0.normalize()).length();

                        if dist_ccw < dist_cw {
                            println!("    → CCW cross product matches stored normal better");
                        } else {
                            println!("    → CW cross product matches stored normal better");
                        }
                    }
                }

                // Only show first valid triangle
                break;
            }
        }

        // Show bounds
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut min_z = f32::INFINITY;
        let mut max_z = f32::NEG_INFINITY;

        for v in &mesh.vertices {
            min_x = min_x.min(v.x);
            max_x = max_x.max(v.x);
            min_y = min_y.min(v.y);
            max_y = max_y.max(v.y);
            min_z = min_z.min(v.z);
            max_z = max_z.max(v.z);
        }

        println!("\nBounds:");
        println!("  X: [{:.3}, {:.3}]", min_x, max_x);
        println!("  Y: [{:.3}, {:.3}]", min_y, max_y);
        println!("  Z: [{:.3}, {:.3}]", min_z, max_z);
    }

    Ok(())
}
