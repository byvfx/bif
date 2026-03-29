//! Scene registry — declarative test scene definitions.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Scene complexity tier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneTier {
    Simple,
    Medium,
    Large,
    Official,
}

impl std::fmt::Display for SceneTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Simple => write!(f, "simple"),
            Self::Medium => write!(f, "medium"),
            Self::Large => write!(f, "large"),
            Self::Official => write!(f, "official"),
        }
    }
}

impl SceneTier {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "simple" => Some(Self::Simple),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            "official" => Some(Self::Official),
            _ => None,
        }
    }
}

/// A registered test scene.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneEntry {
    pub name: String,
    /// Relative path from workspace root.
    pub path: String,
    pub tier: SceneTier,
    /// Download URL for official assets (None for in-repo scenes).
    pub download_url: Option<String>,
}

/// All registered scenes.
pub fn default_registry() -> Vec<SceneEntry> {
    vec![
        // Simple
        SceneEntry {
            name: "Cube".into(),
            path: "assets/cube.usd".into(),
            tier: SceneTier::Simple,
            download_url: None,
        },
        SceneEntry {
            name: "Boxes 100".into(),
            path: "assets/boxes_100.usda".into(),
            tier: SceneTier::Simple,
            download_url: None,
        },
        // Medium
        SceneEntry {
            name: "Lucy 100".into(),
            path: "assets/lucy_100.usda".into(),
            tier: SceneTier::Medium,
            download_url: None,
        },
        SceneEntry {
            name: "Lucy 900".into(),
            path: "assets/lucy_900.usda".into(),
            tier: SceneTier::Medium,
            download_url: None,
        },
        // Large
        SceneEntry {
            name: "Lucy 10000".into(),
            path: "assets/lucy_10000.usda".into(),
            tier: SceneTier::Large,
            download_url: None,
        },
        // Official (download to assets/perf/)
        SceneEntry {
            name: "Kitchen Set".into(),
            path: "assets/perf/Kitchen_set/Kitchen_set.usd".into(),
            tier: SceneTier::Official,
            download_url: Some("https://openusd.org/release/dl_kitchen_set.html".into()),
        },
        SceneEntry {
            name: "ALab".into(),
            path: "assets/perf/ALab/ALab.usd".into(),
            tier: SceneTier::Official,
            download_url: Some("https://animallogic.com/alab/".into()),
        },
        SceneEntry {
            name: "Moore Lane".into(),
            path: "assets/perf/MooreLane/MooreLane_ASWF_0623.usda".into(),
            tier: SceneTier::Official,
            download_url: Some("https://dpel.aswf.io/4004-moore-lane/".into()),
        },
    ]
}

/// Resolve scene entries matching a selector (tier name, "all", or a file path).
/// Returns (name, absolute_path) pairs for scenes that exist on disk.
pub fn resolve_scenes(selector: &str, workspace_root: &Path) -> Vec<(String, PathBuf)> {
    // Direct file path
    let as_path = Path::new(selector);
    if as_path.extension().is_some() {
        let abs = if as_path.is_absolute() {
            as_path.to_path_buf()
        } else {
            workspace_root.join(as_path)
        };
        if abs.exists() {
            let name = abs
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            return vec![(name, abs)];
        }
        log::warn!("Scene not found: {}", abs.display());
        return vec![];
    }

    // Tier or "all"
    let tier_filter = if selector == "all" {
        None
    } else {
        SceneTier::parse(selector)
    };

    let registry = default_registry();
    registry
        .into_iter()
        .filter(|entry| tier_filter.as_ref().is_none_or(|t| &entry.tier == t))
        .filter_map(|entry| {
            let abs = workspace_root.join(&entry.path);
            if abs.exists() {
                Some((entry.name, abs))
            } else {
                log::debug!("Skipping missing scene: {} ({})", entry.name, entry.path);
                None
            }
        })
        .collect()
}
