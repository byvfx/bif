//! Houdini subprocess target — measures USD stage performance via hython.
//!
//! Generates a Python timing script, runs it via `hython`, and parses JSON output.
//!
//! Requires `HOUDINI_DIR` env var pointing at the Houdini install directory
//! (e.g., `C:/Program Files/Side Effects Software/Houdini 20.5.100`),
//! or `hython` must be in PATH.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HoudiniError {
    #[error("hython not found — set HOUDINI_DIR or add hython to PATH")]
    NoHython,
    #[error("subprocess failed: {0}")]
    Subprocess(String),
    #[error("failed to parse output: {0}")]
    Parse(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result from one Houdini measurement run.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct HoudiniTimings {
    pub stage_open_s: f64,
    pub stage_close_s: f64,
    pub node_cook_s: f64,
    pub prim_count: usize,
}

/// Generate the hython timing script content.
fn timing_script(scene_path: &Path, iterations: usize) -> String {
    let scene = scene_path.display().to_string().replace('\\', "/");
    format!(
        r#"
import json, time, sys, hou

scene_path = r"{scene}"
iterations = {iterations}

results = []
for i in range(iterations):
    # Method 1: Direct USD stage open via hou.usd
    t0 = time.perf_counter()
    stage = hou.usd.openStage(scene_path)
    t1 = time.perf_counter()

    prim_count = len(list(stage.Traverse())) if stage else 0

    # Method 2: SOP-level import (measures node cook time)
    obj = hou.node("/obj")
    geo = obj.createNode("geo", "bif_perf_test")
    usd_import = geo.createNode("usdimport")
    usd_import.parm("filepath1").set(scene_path)

    t2 = time.perf_counter()
    usd_import.cook(force=True)
    t3 = time.perf_counter()

    # Cleanup
    geo.destroy()
    del stage
    t4 = time.perf_counter()

    results.append({{
        "stage_open_s": t1 - t0,
        "node_cook_s": t3 - t2,
        "stage_close_s": t4 - t3,
        "prim_count": prim_count,
    }})

json.dump(results, sys.stdout)
"#
    )
}

/// Find hython executable.
fn find_hython() -> Result<PathBuf, HoudiniError> {
    // Try HOUDINI_DIR env var
    if let Ok(houdini_dir) = std::env::var("HOUDINI_DIR") {
        let hython = PathBuf::from(&houdini_dir).join("bin/hython.exe");
        if hython.exists() {
            return Ok(hython);
        }
        let hython_nix = PathBuf::from(&houdini_dir).join("bin/hython");
        if hython_nix.exists() {
            return Ok(hython_nix);
        }
    }

    // Try PATH
    if let Ok(output) = Command::new("where").arg("hython").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    Err(HoudiniError::NoHython)
}

/// Run the hython timing script and return parsed results.
pub fn measure(scene_path: &Path, iterations: usize) -> Result<Vec<HoudiniTimings>, HoudiniError> {
    let hython = find_hython()?;
    let script = timing_script(scene_path, iterations);

    // Write script to temp file
    let tmp_dir = std::env::temp_dir();
    let script_path = tmp_dir.join("bif_perf_houdini.py");
    {
        let mut f = std::fs::File::create(&script_path)?;
        f.write_all(script.as_bytes())?;
    }

    let output = Command::new(&hython)
        .arg(&script_path)
        .output()
        .map_err(|e| HoudiniError::Subprocess(format!("{}: {e}", hython.display())))?;

    let _ = std::fs::remove_file(&script_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HoudiniError::Subprocess(format!(
            "exit {}: {stderr}",
            output.status
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let results: Vec<HoudiniTimings> =
        serde_json::from_str(&stdout).map_err(|e| HoudiniError::Parse(e.to_string()))?;

    Ok(results)
}
