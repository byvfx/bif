//! usdview subprocess target — measures USD stage performance via Python.
//!
//! Generates a Python timing script, runs it via the USD toolkit's `python.bat`
//! or system Python with pxr available, and parses JSON output.
//!
//! Requires `USD_TOOLKIT_DIR` env var pointing at the USD toolkit directory,
//! or `USD_PYTHON` env var pointing at a Python with pxr importable.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UsdviewError {
    #[error("USD_TOOLKIT_DIR or USD_PYTHON not set")]
    NoPython,
    #[error("subprocess failed: {0}")]
    Subprocess(String),
    #[error("failed to parse output: {0}")]
    Parse(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result from one usdview measurement run.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct UsdviewTimings {
    pub stage_open_s: f64,
    pub stage_close_s: f64,
    pub prim_count: usize,
}

/// Generate the Python timing script content.
fn timing_script(scene_path: &Path, iterations: usize) -> String {
    let scene = scene_path.display().to_string().replace('\\', "/");
    format!(
        r#"
import json, time, sys
from pxr import Usd

scene_path = r"{scene}"
iterations = {iterations}

results = []
for i in range(iterations):
    t0 = time.perf_counter()
    stage = Usd.Stage.Open(scene_path)
    t1 = time.perf_counter()

    prim_count = len(list(stage.Traverse()))

    del stage
    t2 = time.perf_counter()

    results.append({{
        "stage_open_s": t1 - t0,
        "stage_close_s": t2 - t1,
        "prim_count": prim_count,
    }})

json.dump(results, sys.stdout)
"#
    )
}

/// Find a Python executable that can import pxr.
fn find_python() -> Result<PathBuf, UsdviewError> {
    // Try USD_PYTHON env var first
    if let Ok(python) = std::env::var("USD_PYTHON") {
        return Ok(PathBuf::from(python));
    }

    // Try USD_TOOLKIT_DIR/python.bat (Windows)
    if let Ok(toolkit) = std::env::var("USD_TOOLKIT_DIR") {
        let python_bat = PathBuf::from(&toolkit).join("python.bat");
        if python_bat.exists() {
            return Ok(python_bat);
        }
        let python_exe = PathBuf::from(&toolkit).join("python.exe");
        if python_exe.exists() {
            return Ok(python_exe);
        }
    }

    Err(UsdviewError::NoPython)
}

/// Run the usdview timing script and return parsed results.
pub fn measure(scene_path: &Path, iterations: usize) -> Result<Vec<UsdviewTimings>, UsdviewError> {
    let python = find_python()?;
    let script = timing_script(scene_path, iterations);

    // Write script to temp file
    let tmp_dir = std::env::temp_dir();
    let script_path = tmp_dir.join("bif_perf_usdview.py");
    {
        let mut f = std::fs::File::create(&script_path)?;
        f.write_all(script.as_bytes())?;
    }

    let output = Command::new(&python)
        .arg(&script_path)
        .output()
        .map_err(|e| UsdviewError::Subprocess(format!("{}: {e}", python.display())))?;

    // Clean up
    let _ = std::fs::remove_file(&script_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(UsdviewError::Subprocess(format!(
            "exit {}: {stderr}",
            output.status
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let results: Vec<UsdviewTimings> =
        serde_json::from_str(&stdout).map_err(|e| UsdviewError::Parse(e.to_string()))?;

    Ok(results)
}
