//! Individual best-practice checks from USD maxperf.html.

use std::path::Path;

use bif_core::usd::cpp_bridge::UsdStage;

use super::{AuditCheck, AuditResult, AuditStatus};

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
    fn run(&self, scene_path: &Path, _stage: &UsdStage) -> AuditResult {
        let ext = scene_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        match ext {
            "usdc" => AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Pass,
                detail: "Binary crate format (.usdc)".into(),
                recommendation: None,
            },
            "usd" => AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Pass,
                detail: "Auto-detect format (.usd) — defaults to binary".into(),
                recommendation: None,
            },
            "usda" => AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Warn,
                detail: "ASCII format (.usda) — slower to parse, higher memory".into(),
                recommendation: Some(
                    "Convert to .usdc with `usdcat --out scene.usdc scene.usda`".into(),
                ),
            },
            _ => AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Skip,
                detail: format!("Unknown extension: .{ext}"),
                recommendation: None,
            },
        }
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
        for i in 0..count {
            if let Ok(info) = stage.get_prim_info(i) {
                if info.has_payload {
                    payload_count += 1;
                }
            }
        }

        if count > 1000 && payload_count == 0 {
            AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Warn,
                detail: format!("{count} prims but no payloads — entire scene loaded eagerly"),
                recommendation: Some(
                    "Package asset geometry behind payload arcs for deferred loading".into(),
                ),
            }
        } else if payload_count > 0 {
            AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Pass,
                detail: format!("{payload_count}/{count} prims use payloads"),
                recommendation: None,
            }
        } else {
            AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Pass,
                detail: format!("{count} prims (small scene, payloads not needed)"),
                recommendation: None,
            }
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
            AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Warn,
                detail: format!("{count} prims — very large scene"),
                recommendation: Some(
                    "Consider higher-level instancing to reduce prim count".into(),
                ),
            }
        } else if count > 10_000 {
            AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Pass,
                detail: format!("{count} prims — moderate scene"),
                recommendation: None,
            }
        } else {
            AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Pass,
                detail: format!("{count} prims"),
                recommendation: None,
            }
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
        let mut instancer_count = 0usize;

        for i in 0..count {
            if let Ok(info) = stage.get_prim_info(i) {
                match info.type_name.as_str() {
                    "Mesh" => mesh_count += 1,
                    "PointInstancer" => instancer_count += 1,
                    _ => {}
                }
            }
        }

        if mesh_count > 100 && instancer_count == 0 {
            AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Warn,
                detail: format!("{mesh_count} meshes, 0 instancers — all unique geometry"),
                recommendation: Some(
                    "Use PointInstancer or native instancing for repeated geometry".into(),
                ),
            }
        } else {
            AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Pass,
                detail: format!("{mesh_count} meshes, {instancer_count} instancers"),
                recommendation: None,
            }
        }
    }
}

// ============================================================================
// Check 5: Alembic usage (scan prim paths for .abc references)
// ============================================================================

pub struct AlembicUsageCheck;

impl AuditCheck for AlembicUsageCheck {
    fn id(&self) -> &str {
        "alembic_usage"
    }
    fn name(&self) -> &str {
        "Alembic Usage"
    }
    fn run(&self, _scene_path: &Path, stage: &UsdStage) -> AuditResult {
        let count = stage.prim_count().unwrap_or(0);
        let mut abc_refs = 0usize;

        for i in 0..count {
            if let Ok(info) = stage.get_prim_info(i) {
                // Heuristic: .abc in prim path suggests Alembic reference
                if info.path.contains(".abc") {
                    abc_refs += 1;
                }
            }
        }

        if abc_refs > 0 {
            AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Warn,
                detail: format!("{abc_refs} prims reference Alembic (.abc) data"),
                recommendation: Some(
                    "Prefer native USD (.usdc) over Alembic for better performance".into(),
                ),
            }
        } else {
            AuditResult {
                check_id: self.id().into(),
                check_name: self.name().into(),
                status: AuditStatus::Pass,
                detail: "No Alembic references detected".into(),
                recommendation: None,
            }
        }
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
    fn run(&self, _scene_path: &Path, _stage: &UsdStage) -> AuditResult {
        // TODO: Add usd_bridge_get_used_layer_count() to C++ bridge
        AuditResult {
            check_id: self.id().into(),
            check_name: self.name().into(),
            status: AuditStatus::Skip,
            detail: "No C++ bridge function for layer count yet".into(),
            recommendation: Some("Add usd_bridge_get_used_layer_count() to cpp bridge".into()),
        }
    }
}
