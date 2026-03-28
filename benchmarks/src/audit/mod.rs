//! Best-practices audit — checks from USD maxperf.html (scoped to v25).

pub mod checks;

use std::path::Path;

use serde::{Deserialize, Serialize};

use bif_core::usd::cpp_bridge::UsdStage;

/// Result of one best-practice check.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditResult {
    pub check_id: String,
    pub check_name: String,
    pub status: AuditStatus,
    pub detail: String,
    pub recommendation: Option<String>,
}

/// Status of an audit check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditStatus {
    Pass,
    Warn,
    Fail,
    /// Cannot determine (e.g., missing C++ bridge function).
    Skip,
}

impl std::fmt::Display for AuditStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "PASS"),
            Self::Warn => write!(f, "WARN"),
            Self::Fail => write!(f, "FAIL"),
            Self::Skip => write!(f, "SKIP"),
        }
    }
}

/// A best-practice check against USD maxperf.html recommendations.
pub trait AuditCheck: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn run(&self, scene_path: &Path, stage: &UsdStage) -> AuditResult;
}

/// All available audit checks.
pub fn all_checks() -> Vec<Box<dyn AuditCheck>> {
    vec![
        Box::new(checks::BinaryFormatCheck),
        Box::new(checks::PayloadUsageCheck),
        Box::new(checks::PrimCountCheck),
        Box::new(checks::InstanceUsageCheck),
        Box::new(checks::AlembicUsageCheck),
        Box::new(checks::LayerCountCheck),
    ]
}

/// Run all audit checks on a scene. Opens and loads the stage internally.
pub fn run_audit(scene_path: &Path) -> Vec<AuditResult> {
    let checks = all_checks();

    let stage = match UsdStage::open(scene_path) {
        Ok(s) => s,
        Err(e) => {
            return vec![AuditResult {
                check_id: "stage_open".into(),
                check_name: "Stage Open".into(),
                status: AuditStatus::Fail,
                detail: format!("Cannot open stage: {e}"),
                recommendation: Some("Check file path and USD environment".into()),
            }];
        }
    };

    if let Err(e) = stage.load_payloads() {
        return vec![AuditResult {
            check_id: "payload_load".into(),
            check_name: "Payload Load".into(),
            status: AuditStatus::Fail,
            detail: format!("Cannot load payloads: {e}"),
            recommendation: None,
        }];
    }

    checks.iter().map(|c| c.run(scene_path, &stage)).collect()
}

/// Render audit results as a terminal string.
pub fn render_audit(results: &[AuditResult]) -> String {
    let mut out = String::new();
    for r in results {
        let icon = match r.status {
            AuditStatus::Pass => "+",
            AuditStatus::Warn => "!",
            AuditStatus::Fail => "X",
            AuditStatus::Skip => "-",
        };
        out.push_str(&format!("[{}] {} — {}\n", icon, r.check_name, r.detail));
        if let Some(ref rec) = r.recommendation {
            out.push_str(&format!("      Recommendation: {rec}\n"));
        }
    }

    let pass = results
        .iter()
        .filter(|r| r.status == AuditStatus::Pass)
        .count();
    let warn = results
        .iter()
        .filter(|r| r.status == AuditStatus::Warn)
        .count();
    let fail = results
        .iter()
        .filter(|r| r.status == AuditStatus::Fail)
        .count();
    let skip = results
        .iter()
        .filter(|r| r.status == AuditStatus::Skip)
        .count();
    out.push_str(&format!(
        "\nSummary: {pass} pass, {warn} warn, {fail} fail, {skip} skip\n"
    ));
    out
}
