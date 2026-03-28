//! Individual best-practice checks from USD maxperf.html.

use std::path::Path;

use bif_core::usd::cpp_bridge::UsdStage;

use super::{AuditCheck, AuditResult, AuditStatus};

/// Evaluate file extension for binary format check (testable without USD).
fn check_binary_format(ext: &str) -> (AuditStatus, String, Option<String>) {
    match ext {
        "usdc" => (
            AuditStatus::Pass,
            "Binary crate format (.usdc)".into(),
            None,
        ),
        "usd" => (
            AuditStatus::Pass,
            "Auto-detect format (.usd) — defaults to binary".into(),
            None,
        ),
        "usda" => (
            AuditStatus::Warn,
            "ASCII format (.usda) — slower to parse, higher memory".into(),
            Some("Convert to .usdc with `usdcat --out scene.usdc scene.usda`".into()),
        ),
        _ => (
            AuditStatus::Skip,
            format!("Unknown extension: .{ext}"),
            None,
        ),
    }
}

// ============================================================================
// Check 1: Binary format (.usdc) preferred over .usda for production
// ============================================================================

pub struct BinaryFormatCheck;

impl AuditCheck for BinaryFormatCheck {
    fn id(&self) -> &str {
        "binary_format"
    }
    fn name(&self) -> &str {
        "Binary Format"
    }
    fn needs_payloads(&self) -> bool {
        false
    }
    fn run(&self, scene_path: &Path, _stage: &UsdStage) -> AuditResult {
        let ext = scene_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let (status, detail, rec) = check_binary_format(ext);
        self.result(status, detail, rec)
    }
}

// ============================================================================
// Check 2: Payload usage for large scenes
// ============================================================================

pub struct PayloadUsageCheck;

impl AuditCheck for PayloadUsageCheck {
    fn id(&self) -> &str {
        "payload_usage"
    }
    fn name(&self) -> &str {
        "Payload Usage"
    }
    fn run(&self, _scene_path: &Path, stage: &UsdStage) -> AuditResult {
        let count = stage.prim_count().unwrap_or(0);
        let mut payload_count = 0;
        let mut root_prim_count = 0;
        for i in 0..count {
            if let Ok(info) = stage.get_prim_info(i) {
                if info.has_payload {
                    payload_count += 1;
                }
                // Count top-level prims (depth 1 = one slash after root)
                if info.path.matches('/').count() == 1 {
                    root_prim_count += 1;
                }
            }
        }

        // Check payload usage against root prim count, not expanded total.
        // After load_payloads(), total prim count includes expanded children.
        if root_prim_count > 5 && payload_count == 0 && count > 1000 {
            self.result(
                AuditStatus::Warn,
                format!("{count} prims ({root_prim_count} root), no payload arcs"),
                Some("Package asset geometry behind payload arcs for deferred loading".into()),
            )
        } else if payload_count > 0 {
            self.result(
                AuditStatus::Pass,
                format!("{payload_count} payload arcs, {count} total prims"),
                None,
            )
        } else {
            self.result(
                AuditStatus::Pass,
                format!("{count} prims (small scene, payloads not needed)"),
                None,
            )
        }
    }
}

// ============================================================================
// Check 3: Prim count threshold
// ============================================================================

pub struct PrimCountCheck;

impl AuditCheck for PrimCountCheck {
    fn id(&self) -> &str {
        "prim_count"
    }
    fn name(&self) -> &str {
        "Prim Count"
    }
    fn run(&self, _scene_path: &Path, stage: &UsdStage) -> AuditResult {
        let count = stage.prim_count().unwrap_or(0);

        if count > 100_000 {
            self.result(
                AuditStatus::Warn,
                format!("{count} prims — very large (instanced prims are expected, unique prims are the concern)"),
                Some("Consider higher-level instancing to reduce unique prim count".into()),
            )
        } else if count > 10_000 {
            self.result(
                AuditStatus::Pass,
                format!("{count} prims — moderate scene"),
                None,
            )
        } else {
            self.result(AuditStatus::Pass, format!("{count} prims"), None)
        }
    }
}

// ============================================================================
// Check 4: Instance usage (PointInstancer or native instances)
// ============================================================================

pub struct InstanceUsageCheck;

impl AuditCheck for InstanceUsageCheck {
    fn id(&self) -> &str {
        "instance_usage"
    }
    fn name(&self) -> &str {
        "Instance Usage"
    }
    fn run(&self, _scene_path: &Path, stage: &UsdStage) -> AuditResult {
        let count = stage.prim_count().unwrap_or(0);
        let mut mesh_count = 0usize;
        let mut point_instancer_count = 0usize;

        for i in 0..count {
            if let Ok(info) = stage.get_prim_info(i) {
                match info.type_name.as_str() {
                    "Mesh" => mesh_count += 1,
                    "PointInstancer" => point_instancer_count += 1,
                    _ => {}
                }
            }
        }

        let native_count = stage.native_instance_count().unwrap_or(0);
        let total_instancers = point_instancer_count + native_count;

        if mesh_count > 100 && total_instancers == 0 {
            self.result(
                AuditStatus::Warn,
                format!("{mesh_count} meshes, 0 instancers — all unique geometry"),
                Some("Use PointInstancer or native instancing (instanceable=true) for repeated geometry".into()),
            )
        } else {
            self.result(
                AuditStatus::Pass,
                format!(
                    "{mesh_count} meshes, {point_instancer_count} point instancers, {native_count} native instances"
                ),
                None,
            )
        }
    }
}

// ============================================================================
// Check 5: Alembic usage (requires C++ bridge for reference asset paths)
// ============================================================================

pub struct AlembicUsageCheck;

impl AuditCheck for AlembicUsageCheck {
    fn id(&self) -> &str {
        "alembic_usage"
    }
    fn name(&self) -> &str {
        "Alembic Usage"
    }
    fn needs_payloads(&self) -> bool {
        false
    }
    fn run(&self, _scene_path: &Path, _stage: &UsdStage) -> AuditResult {
        // Alembic references live in composition arcs (asset paths), not prim paths.
        // Detecting them requires querying reference/sublayer asset paths from
        // SdfLayer, which the C++ bridge doesn't expose yet.
        self.result(
            AuditStatus::Skip,
            "Requires C++ bridge to query reference asset paths".into(),
            Some("Add usd_bridge_get_sublayer_paths() to cpp bridge".into()),
        )
    }
}

// ============================================================================
// Check 6: Layer count (requires C++ bridge function — skip for now)
// ============================================================================

pub struct LayerCountCheck;

impl AuditCheck for LayerCountCheck {
    fn id(&self) -> &str {
        "layer_count"
    }
    fn name(&self) -> &str {
        "Layer Count"
    }
    fn needs_payloads(&self) -> bool {
        false
    }
    fn run(&self, _scene_path: &Path, _stage: &UsdStage) -> AuditResult {
        // TODO: Add usd_bridge_get_used_layer_count() to C++ bridge
        self.result(
            AuditStatus::Skip,
            "Requires C++ bridge for UsdStage::GetUsedLayers()".into(),
            Some("Add usd_bridge_get_used_layer_count() to cpp bridge".into()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_format_usdc() {
        let (status, _, _) = check_binary_format("usdc");
        assert_eq!(status, AuditStatus::Pass);
    }

    #[test]
    fn test_binary_format_usd() {
        let (status, _, _) = check_binary_format("usd");
        assert_eq!(status, AuditStatus::Pass);
    }

    #[test]
    fn test_binary_format_usda() {
        let (status, _, rec) = check_binary_format("usda");
        assert_eq!(status, AuditStatus::Warn);
        assert!(rec.is_some());
    }

    #[test]
    fn test_binary_format_unknown() {
        let (status, _, _) = check_binary_format("abc");
        assert_eq!(status, AuditStatus::Skip);
    }

    #[test]
    fn test_binary_format_empty() {
        let (status, _, _) = check_binary_format("");
        assert_eq!(status, AuditStatus::Skip);
    }
}
