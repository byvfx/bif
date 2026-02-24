//! USD validation — run usdchecker/usdcat/usdtree from the toolkit.
//!
//! Shells out to the USD toolkit `.bat` scripts to validate exported files.

use std::path::Path;
use std::process::Command;

/// Path to the USD toolkit scripts directory.
const USD_TOOLKIT: &str = r"D:\__projects\_programming\usd_25_11\scripts";

/// Run `usdchecker` on a USD file.
///
/// Returns `(passed, output)` — `passed` is true if the file passes validation.
pub fn run_usdchecker(path: &str) -> (bool, String) {
    let script = Path::new(USD_TOOLKIT).join("usdchecker.bat");
    match Command::new(&script).arg(path).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let combined = if stderr.is_empty() {
                stdout
            } else {
                format!("{}\n{}", stdout, stderr)
            };
            (output.status.success(), combined)
        }
        Err(e) => (false, format!("Failed to run usdchecker: {}", e)),
    }
}

/// Run `usdcat` on a USD file — returns the composed stage as text.
pub fn run_usdcat(path: &str) -> String {
    let script = Path::new(USD_TOOLKIT).join("usdcat.bat");
    match Command::new(&script).arg(path).output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
        Err(e) => format!("Failed to run usdcat: {}", e),
    }
}

/// Run `usdtree` on a USD file — returns the hierarchy tree.
pub fn run_usdtree(path: &str) -> String {
    let script = Path::new(USD_TOOLKIT).join("usdtree.bat");
    match Command::new(&script).arg(path).output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
        Err(e) => format!("Failed to run usdtree: {}", e),
    }
}
