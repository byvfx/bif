//! bif_perf — USD performance metrics CLI for BIF.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use bif_perf::audit;
use bif_perf::config::RunConfig;
use bif_perf::metrics;
use bif_perf::report::comparison;
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

        /// Target: bif, usdview, houdini.
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

        /// Auto-save results to benchmarks/results/ with timestamp.
        #[arg(long)]
        save: bool,
    },

    /// Audit a scene against USD maxperf.html best practices.
    Audit {
        /// Scene file to audit.
        scene: PathBuf,
    },

    /// Compare two benchmark result YAML files.
    Compare {
        /// Baseline results file.
        baseline: PathBuf,
        /// Current results file.
        current: PathBuf,
    },

    /// Show download instructions for missing official test assets.
    Download,

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
            save,
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

            // Auto-save YAML to benchmarks/results/ with timestamp
            if save {
                let results_dir = workspace_root.join("benchmarks/results");
                let _ = std::fs::create_dir_all(&results_dir);
                let ts = chrono::Local::now().format("%Y-%m-%d_%H%M%S");
                let filename = format!("{ts}_{target}.yaml");
                let save_path = results_dir.join(&filename);
                let yaml = report.render(&OutputFormat::Yaml);
                match std::fs::write(&save_path, &yaml) {
                    Ok(()) => {
                        log::info!("Results saved to {}", save_path.display());
                        // Write latest pointer file
                        let latest = results_dir.join(format!("latest_{target}.yaml"));
                        let _ = std::fs::write(&latest, &yaml);
                    }
                    Err(e) => log::error!("Failed to save results: {e}"),
                }
            }
        }
        Command::Audit { scene } => {
            let scene_path = if scene.is_absolute() {
                scene
            } else {
                workspace_root.join(&scene)
            };

            if !scene_path.exists() {
                log::error!("Scene not found: {}", scene_path.display());
                std::process::exit(1);
            }

            log::info!("Auditing: {}", scene_path.display());
            let results = audit::run_audit(&scene_path);
            print!("{}", audit::render_audit(&results));
        }
        Command::Compare { baseline, current } => match comparison::compare(&baseline, &current) {
            Ok(table) => println!("{table}"),
            Err(e) => {
                log::error!("Comparison failed: {e}");
                std::process::exit(1);
            }
        },
        Command::Download => {
            let registry = scenes::default_registry();
            let mut missing = false;
            for entry in &registry {
                let Some(ref url) = entry.download_url else {
                    continue;
                };
                let abs = workspace_root.join(&entry.path);
                if abs.exists() {
                    println!("[OK]      {} — already at {}", entry.name, entry.path);
                } else {
                    missing = true;
                    println!("[MISSING] {} — download from:", entry.name);
                    println!("          {url}");
                    println!("          Extract to: {}", entry.path);
                }
            }
            if !missing {
                println!("\nAll official assets present.");
            }
        }
        Command::List => {
            println!("=== Metrics ===");
            for m in metrics::all_bif_metrics() {
                println!("  {:20} {}", m.id(), m.name());
            }

            println!("\n=== Audit Checks ===");
            for c in audit::all_checks() {
                println!("  {:20} {}", c.id(), c.name());
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
