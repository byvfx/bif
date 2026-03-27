//! bif_perf — USD performance metrics CLI for BIF.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use bif_perf::config::RunConfig;
use bif_perf::metrics;
use bif_perf::report::{OutputFormat, Report};
use bif_perf::runner;
use bif_perf::scenes;
use bif_perf::targets;

#[derive(Parser)]
#[command(name = "bif_perf", about = "USD performance metrics for BIF")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run performance metrics.
    Run {
        /// Scene file path or tier (simple/medium/large/official/all).
        #[arg(default_value = "all")]
        scene: String,

        /// Target: bif (usdview/houdini in Phase 4).
        #[arg(short, long, default_value = "bif")]
        target: String,

        /// Number of iterations per metric (minimum 1).
        #[arg(short = 'n', long, default_value = "100")]
        iterations: usize,

        /// Warmup iterations before measurement.
        #[arg(short = 'w', long, default_value = "3")]
        warmup: usize,

        /// Output format: terminal, yaml, csv.
        #[arg(short, long, default_value = "terminal")]
        format: String,

        /// Output file (stdout if omitted).
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Run only these metrics (comma-separated IDs).
        #[arg(short, long)]
        metrics: Option<String>,
    },

    /// List available scenes, metrics, and targets.
    List,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let cli = Cli::parse();
    let workspace_root = find_workspace_root();

    match cli.command {
        Command::Run {
            scene,
            target,
            iterations,
            warmup,
            format,
            output,
            metrics: metric_filter,
        } => {
            if iterations == 0 {
                log::error!("iterations must be >= 1");
                std::process::exit(1);
            }

            let fmt = OutputFormat::parse(&format);
            let config = RunConfig {
                iterations,
                warmup_iterations: warmup,
                metric_filter: metric_filter
                    .map(|s| s.split(',').map(|m| m.trim().to_string()).collect()),
            };

            let all_metrics = metrics::all_bif_metrics();
            let resolved = scenes::resolve_scenes(&scene, &workspace_root);

            if resolved.is_empty() {
                log::error!("No scenes found for selector: {scene}");
                std::process::exit(1);
            }

            log::info!(
                "Running {} metrics on {} scene(s), {} iterations each",
                all_metrics.len(),
                resolved.len(),
                config.iterations,
            );

            let mut results = Vec::new();
            for (name, path) in &resolved {
                match runner::run_scene(name, path, &all_metrics, &config, &target) {
                    Ok(r) => results.push(r),
                    Err(e) => log::error!("Failed on {name}: {e}"),
                }
            }

            let report = Report::new(results);
            let rendered = report.render(&fmt);

            if let Some(out_path) = output {
                std::fs::write(&out_path, &rendered).unwrap_or_else(|e| {
                    log::error!("Failed to write {}: {e}", out_path.display());
                    std::process::exit(1);
                });
                log::info!("Results saved to {}", out_path.display());
            } else {
                println!("{rendered}");
            }
        }
        Command::List => {
            println!("=== Metrics ===");
            for m in metrics::all_bif_metrics() {
                println!("  {:20} {}", m.id(), m.name());
            }

            println!("\n=== Targets ===");
            for t in targets::available_targets() {
                println!("  {t}");
            }

            println!("\n=== Scenes ===");
            let resolved = scenes::resolve_scenes("all", &workspace_root);
            let registry = scenes::default_registry();
            for entry in &registry {
                let exists = resolved.iter().any(|(n, _)| n == &entry.name);
                let status = if exists { "OK" } else { "MISSING" };
                println!(
                    "  [{:7}] {:20} {:8} {}",
                    status, entry.name, entry.tier, entry.path
                );
            }
        }
    }
}

/// Find workspace root using CARGO_MANIFEST_DIR (compile-time) with cwd fallback.
fn find_workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points to benchmarks/ at compile time — parent is workspace root
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let from_manifest = PathBuf::from(manifest_dir)
        .parent()
        .map(|p| p.to_path_buf());

    if let Some(ref root) = from_manifest {
        if root.join("Cargo.toml").exists() {
            return root.clone();
        }
    }

    // Fallback: walk up from cwd
    let mut dir = std::env::current_dir().expect("cannot get current directory");
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return dir;
                }
            }
        }
        if !dir.pop() {
            return std::env::current_dir().expect("cannot get current directory");
        }
    }
}
