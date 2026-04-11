//! BIF project file persistence (.bif / .bifa).
//!
//! `.bif` — binary (bincode), fast load/save. **Not forward-compatible**: any
//! field addition/removal/reorder in serialized types breaks existing files.
//! Treat `.bif` as a fast cache format, not a durable archive.
//!
//! `.bifa` — ASCII (JSON pretty-print), human-readable/diffable. More tolerant
//! of schema changes via `#[serde(default)]`.
//!
//! Modeled after Maya's `.mb` / `.ma` dual format.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use bif_math::ProjectionMode;
use egui_snarl::Snarl;

use crate::ivar_state::BatchRenderSettings;
use crate::node_graph::{GraphNodeId, SceneNode};
use crate::DisplaySettings;

/// Current format version. Bump on breaking schema changes.
pub const FORMAT_VERSION: u32 = 1;

/// Result of the "Save changes?" dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavePromptResult {
    /// User chose "Yes" — save then proceed.
    Save,
    /// User chose "No" — discard changes and proceed.
    Discard,
    /// User chose "Cancel" — abort the action.
    Cancel,
}

/// Maximum number of recent files to track.
const MAX_RECENT_FILES: usize = 8;

// ---------------------------------------------------------------------------
// EvalMode (persisted in project file, used by Phase 5)
// ---------------------------------------------------------------------------

/// Node graph evaluation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EvalMode {
    /// Re-evaluate immediately on any change (current behavior).
    #[default]
    Auto,
    /// Only evaluate when user clicks "Cook" button.
    Manual,
    /// Re-evaluate when mouse button is released (not during drag).
    OnMouseRelease,
}

// ---------------------------------------------------------------------------
// CameraData (serializable subset of bif_math::Camera)
// ---------------------------------------------------------------------------

/// Camera state persisted in project file.
///
/// Uses `[f32; 3]` arrays instead of `glam::Vec3` to avoid requiring
/// the `glam/serde` feature. Converted to/from `bif_math::Camera` at
/// save/load boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraData {
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub move_speed: f32,
    pub projection: ProjectionMode,
}

impl From<&bif_math::Camera> for CameraData {
    fn from(cam: &bif_math::Camera) -> Self {
        Self {
            position: cam.position.into(),
            target: cam.target.into(),
            up: cam.up.into(),
            fov_y: cam.fov_y,
            near: cam.near,
            far: cam.far,
            yaw: cam.yaw,
            pitch: cam.pitch,
            distance: cam.distance,
            move_speed: cam.move_speed,
            projection: cam.projection,
        }
    }
}

impl CameraData {
    /// Apply this data onto an existing Camera, preserving `aspect`.
    pub fn apply_to(&self, cam: &mut bif_math::Camera) {
        cam.position = self.position.into();
        cam.target = self.target.into();
        cam.up = self.up.into();
        cam.fov_y = self.fov_y;
        cam.near = self.near;
        cam.far = self.far;
        cam.yaw = self.yaw;
        cam.pitch = self.pitch;
        cam.distance = self.distance;
        cam.move_speed = self.move_speed;
        cam.projection = self.projection;
        // aspect is window-dependent — caller must set it separately
    }
}

// ---------------------------------------------------------------------------
// ProjectFile
// ---------------------------------------------------------------------------

/// Top-level project file structure.
#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    /// Format version for forward compatibility.
    pub version: u32,
    /// Node graph (nodes + connections + positions via egui-snarl).
    pub graph: Snarl<SceneNode>,
    /// Display node (which node feeds viewport/export).
    pub display_node: Option<GraphNodeId>,
    /// Viewport camera state.
    pub camera: CameraData,
    /// Render settings for batch rendering.
    pub render_settings: BatchRenderSettings,
    /// Display settings (purpose mode, LOD).
    pub display_settings: DisplaySettings,
    /// Evaluation mode.
    pub eval_mode: EvalMode,
}

// ---------------------------------------------------------------------------
// Path relativization
// ---------------------------------------------------------------------------

/// Make `abs_path` relative to `base_dir`. Returns the original if it
/// can't be made relative (e.g. different drive on Windows).
fn make_relative(abs_path: &str, base_dir: &Path) -> String {
    if abs_path.is_empty() {
        return String::new();
    }
    let p = Path::new(abs_path);
    match pathdiff_relative(p, base_dir) {
        Some(rel) => rel.to_string_lossy().into_owned(),
        None => abs_path.to_string(),
    }
}

/// Resolve `rel_path` against `base_dir`. If already absolute, return as-is.
fn make_absolute(rel_path: &str, base_dir: &Path) -> String {
    if rel_path.is_empty() {
        return String::new();
    }
    let p = Path::new(rel_path);
    if p.is_absolute() {
        return rel_path.to_string();
    }
    base_dir.join(p).to_string_lossy().into_owned()
}

/// Simple relative-path computation (avoids external `pathdiff` crate).
fn pathdiff_relative(path: &Path, base: &Path) -> Option<PathBuf> {
    // Canonicalize-lite: just use components
    let path_parts: Vec<_> = path.components().collect();
    let base_parts: Vec<_> = base.components().collect();

    // Strip common prefix
    let mut common = 0;
    for (a, b) in path_parts.iter().zip(base_parts.iter()) {
        if a == b {
            common += 1;
        } else {
            break;
        }
    }

    if common == 0 {
        return None; // No common prefix (e.g. different drives)
    }

    let ups = base_parts.len() - common;
    let mut result = PathBuf::new();
    for _ in 0..ups {
        result.push("..");
    }
    for part in &path_parts[common..] {
        result.push(part.as_os_str());
    }
    Some(result)
}

/// Convert absolute paths in SceneNode fields to relative (for saving).
fn relativize_paths(graph: &mut Snarl<SceneNode>, project_dir: &Path) {
    let node_ids: Vec<_> = graph.node_ids().map(|(id, _)| id).collect();
    for nid in node_ids {
        match &mut graph[nid] {
            SceneNode::UsdRead { file_path, .. } => {
                *file_path = make_relative(file_path, project_dir);
            }
            SceneNode::HdriEnvironment { file_path, .. } => {
                *file_path = make_relative(file_path, project_dir);
            }
            SceneNode::UsdExport { output_path, .. } => {
                *output_path = make_relative(output_path, project_dir);
            }
            _ => {}
        }
    }
}

/// Convert relative paths back to absolute (after loading).
fn absolutize_paths(graph: &mut Snarl<SceneNode>, project_dir: &Path) {
    let node_ids: Vec<_> = graph.node_ids().map(|(id, _)| id).collect();
    for nid in node_ids {
        match &mut graph[nid] {
            SceneNode::UsdRead { file_path, .. } => {
                *file_path = make_absolute(file_path, project_dir);
            }
            SceneNode::HdriEnvironment { file_path, .. } => {
                *file_path = make_absolute(file_path, project_dir);
            }
            SceneNode::UsdExport { output_path, .. } => {
                *output_path = make_absolute(output_path, project_dir);
            }
            _ => {}
        }
    }
}

/// Also relativize paths in BatchRenderSettings.
fn relativize_render_paths(settings: &mut BatchRenderSettings, project_dir: &Path) {
    settings.output_directory = make_relative(&settings.output_directory, project_dir);
}

/// Also absolutize paths in BatchRenderSettings.
fn absolutize_render_paths(settings: &mut BatchRenderSettings, project_dir: &Path) {
    settings.output_directory = make_absolute(&settings.output_directory, project_dir);
}

// ---------------------------------------------------------------------------
// Save / Load
// ---------------------------------------------------------------------------

/// Save a project file. Format determined by extension:
/// `.bif` → bincode, `.bifa` → JSON pretty-print.
pub fn save_project(file: &ProjectFile, path: &Path) -> Result<()> {
    let project_dir = path
        .parent()
        .context("project path has no parent directory")?;

    // Clone and relativize paths before serializing
    let mut graph = file.graph.clone();
    relativize_paths(&mut graph, project_dir);

    let mut render_settings = file.render_settings.clone();
    relativize_render_paths(&mut render_settings, project_dir);

    let to_save = ProjectFile {
        version: file.version,
        graph,
        display_node: file.display_node,
        camera: file.camera.clone(),
        render_settings,
        display_settings: file.display_settings.clone(),
        eval_mode: file.eval_mode,
    };

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("bifa");

    match ext {
        "bif" => {
            let bytes = bincode::serialize(&to_save).context("bincode serialize failed")?;
            std::fs::write(path, bytes).context("write .bif failed")?;
        }
        _ => {
            // Default to JSON (.bifa or unknown)
            let json = serde_json::to_string_pretty(&to_save).context("JSON serialize failed")?;
            std::fs::write(path, json).context("write .bifa failed")?;
        }
    }

    log::info!("Saved project to {}", path.display());
    Ok(())
}

/// Load a project file. Format determined by extension.
pub fn load_project(path: &Path) -> Result<ProjectFile> {
    let project_dir = path
        .parent()
        .context("project path has no parent directory")?;

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("bifa");

    let mut project: ProjectFile = match ext {
        "bif" => {
            let bytes = std::fs::read(path).context("read .bif failed")?;
            bincode::deserialize(&bytes).context("bincode deserialize failed")?
        }
        _ => {
            let json = std::fs::read_to_string(path).context("read .bifa failed")?;
            serde_json::from_str(&json).context("JSON deserialize failed")?
        }
    };

    if project.version > FORMAT_VERSION {
        bail!(
            "Project file version {} is newer than supported version {}. Please update BIF.",
            project.version,
            FORMAT_VERSION
        );
    }

    absolutize_paths(&mut project.graph, project_dir);
    absolutize_render_paths(&mut project.render_settings, project_dir);

    log::info!("Loaded project from {}", path.display());
    Ok(project)
}

// ---------------------------------------------------------------------------
// ProjectState (runtime, not serialized)
// ---------------------------------------------------------------------------

/// Runtime project state — tracks open file and dirty flag.
#[derive(Debug, Default)]
pub struct ProjectState {
    /// Path to the currently open project file (None = untitled).
    pub file_path: Option<PathBuf>,
    /// Whether the project has unsaved changes.
    pub dirty: bool,
}

impl ProjectState {
    /// Mark the project as having unsaved changes.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Clear the dirty flag (after save).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Window title string: "BIF - filename.bif *" or "BIF - Untitled".
    pub fn window_title(&self) -> String {
        let name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        if self.dirty {
            format!("BIF - {} *", name)
        } else {
            format!("BIF - {}", name)
        }
    }
}

// ---------------------------------------------------------------------------
// Recent Files
// ---------------------------------------------------------------------------

/// Recent file list, persisted to `~/.bif/recent.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RecentFiles {
    pub paths: Vec<PathBuf>,
}

impl RecentFiles {
    /// Add a path to the front of the list. Deduplicates and caps at MAX_RECENT_FILES.
    pub fn add(&mut self, path: &Path) {
        let normalized = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.paths.retain(|p| p != &normalized);
        self.paths.insert(0, normalized);
        self.paths.truncate(MAX_RECENT_FILES);
    }

    /// Remove paths that no longer exist on disk.
    pub fn prune(&mut self) {
        self.paths.retain(|p| p.exists());
    }
}

/// Config directory: `~/.bif/`
fn config_dir() -> Option<PathBuf> {
    dirs_or_home().map(|h| h.join(".bif"))
}

/// Get user home directory (cross-platform fallback).
fn dirs_or_home() -> Option<PathBuf> {
    // Try HOME env first (Unix + Git Bash on Windows), then USERPROFILE (Windows)
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Load recent files from `~/.bif/recent.json`.
pub fn load_recent_files() -> RecentFiles {
    let Some(dir) = config_dir() else {
        return RecentFiles::default();
    };
    let path = dir.join("recent.json");
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => RecentFiles::default(),
    }
}

/// Save recent files to `~/.bif/recent.json`.
pub fn save_recent_files(recent: &RecentFiles) {
    let Some(dir) = config_dir() else {
        return;
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Failed to create config dir {}: {}", dir.display(), e);
        return;
    }
    let path = dir.join("recent.json");
    if let Ok(json) = serde_json::to_string_pretty(recent) {
        if let Err(e) = std::fs::write(&path, &json) {
            log::warn!("Failed to save recent files: {}", e);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use egui_snarl::{InPinId, OutPinId};

    fn sample_project() -> ProjectFile {
        let mut graph = Snarl::new();
        let read_id = graph.insert_node(
            egui::pos2(100.0, 200.0),
            SceneNode::UsdRead {
                // Relative literal — round-trip serde test doesn't touch the filesystem.
                file_path: "scene.usda".into(),
                is_loaded: true,
                error: None,
            },
        );
        let render_id = graph.insert_node(egui::pos2(400.0, 200.0), SceneNode::ivar_render());
        graph.connect(
            OutPinId {
                node: read_id,
                output: 0,
            },
            InPinId {
                node: render_id,
                input: 0,
            },
        );

        ProjectFile {
            version: FORMAT_VERSION,
            graph,
            display_node: Some(GraphNodeId(1)),
            camera: CameraData {
                position: [0.0, 5.0, 10.0],
                target: [0.0, 0.0, 0.0],
                up: [0.0, 1.0, 0.0],
                fov_y: 0.785,
                near: 0.1,
                far: 10000.0,
                yaw: 1.0,
                pitch: 0.3,
                distance: 11.18,
                move_speed: 2.0,
                projection: ProjectionMode::Perspective,
            },
            render_settings: BatchRenderSettings::default(),
            display_settings: DisplaySettings::default(),
            eval_mode: EvalMode::Auto,
        }
    }

    #[test]
    fn save_load_bifa_round_trip() {
        let project = sample_project();
        let dir = std::env::temp_dir().join("bif_test_bifa");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.bifa");

        save_project(&project, &path).unwrap();
        let loaded = load_project(&path).unwrap();

        assert_eq!(loaded.version, FORMAT_VERSION);
        assert_eq!(loaded.display_node, Some(GraphNodeId(1)));
        assert_eq!(loaded.graph.node_ids().count(), 2);
        assert_eq!(loaded.eval_mode, EvalMode::Auto);

        // Verify connection preserved
        let render_nid = loaded.graph.node_ids().nth(1).unwrap().0;
        let in_pin = loaded.graph.in_pin(InPinId {
            node: render_nid,
            input: 0,
        });
        assert_eq!(in_pin.remotes.len(), 1);

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_load_bif_round_trip() {
        let project = sample_project();
        let dir = std::env::temp_dir().join("bif_test_bif");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.bif");

        save_project(&project, &path).unwrap();
        let loaded = load_project(&path).unwrap();

        assert_eq!(loaded.version, FORMAT_VERSION);
        assert_eq!(loaded.graph.node_ids().count(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // Platform-appropriate absolute paths for path-relativization tests.
    // `Path::components()` is separator-sensitive, so Windows tests use `D:\\...`
    // and Unix tests use `/...`. The tests below exercise string manipulation
    // only — no filesystem I/O — so we just swap the literals per target.
    #[cfg(windows)]
    const TEST_BASE: &str = "D:\\projects\\my_scene";
    #[cfg(windows)]
    const TEST_ABS_CHILD: &str = "D:\\projects\\my_scene\\assets\\model.usda";
    #[cfg(windows)]
    const TEST_EXPECTED_REL: &str = "assets\\model.usda";
    #[cfg(windows)]
    const TEST_ABS_SIBLING: &str = "D:\\projects\\shared\\textures\\env.hdr";
    #[cfg(windows)]
    const TEST_BASE_SHORT: &str = "D:\\projects";

    #[cfg(not(windows))]
    const TEST_BASE: &str = "/projects/my_scene";
    #[cfg(not(windows))]
    const TEST_ABS_CHILD: &str = "/projects/my_scene/assets/model.usda";
    #[cfg(not(windows))]
    const TEST_EXPECTED_REL: &str = "assets/model.usda";
    #[cfg(not(windows))]
    const TEST_ABS_SIBLING: &str = "/projects/shared/textures/env.hdr";
    #[cfg(not(windows))]
    const TEST_BASE_SHORT: &str = "/projects";

    #[test]
    fn path_relativization_round_trip() {
        let base = Path::new(TEST_BASE);
        let rel = make_relative(TEST_ABS_CHILD, base);
        assert_eq!(rel, TEST_EXPECTED_REL);

        let back = make_absolute(&rel, base);
        assert_eq!(back, TEST_ABS_CHILD);
    }

    #[test]
    fn path_relativization_parent_dir() {
        let base = Path::new(TEST_BASE);
        let rel = make_relative(TEST_ABS_SIBLING, base);
        assert!(rel.contains(".."), "should have .. for parent: {}", rel);

        let back = make_absolute(&rel, base);
        // Path may not be canonical but should resolve correctly
        assert!(back.contains("shared"));
    }

    #[test]
    fn path_relativization_empty() {
        let base = Path::new(TEST_BASE_SHORT);
        assert_eq!(make_relative("", base), "");
        assert_eq!(make_absolute("", base), "");
    }

    #[test]
    fn version_check_rejects_future() {
        let dir = std::env::temp_dir().join("bif_test_ver");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("future.bifa");

        let mut project = sample_project();
        project.version = 999;
        // Write directly (bypass save_project which uses FORMAT_VERSION)
        let json = serde_json::to_string_pretty(&project).unwrap();
        std::fs::write(&path, &json).unwrap();

        match load_project(&path) {
            Err(e) => assert!(
                e.to_string().contains("999"),
                "error should mention version: {}",
                e
            ),
            Ok(_) => panic!("should reject future version"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recent_files_add_and_cap() {
        let mut recent = RecentFiles::default();
        for i in 0..10 {
            recent.add(Path::new(&format!("file_{}.bif", i)));
        }
        assert_eq!(recent.paths.len(), MAX_RECENT_FILES);
        // Most recent is first
        assert_eq!(recent.paths[0], PathBuf::from("file_9.bif"));
    }

    #[test]
    fn recent_files_dedup() {
        let mut recent = RecentFiles::default();
        recent.add(Path::new("a.bif"));
        recent.add(Path::new("b.bif"));
        recent.add(Path::new("a.bif")); // re-add
        assert_eq!(recent.paths.len(), 2);
        assert_eq!(recent.paths[0], PathBuf::from("a.bif"));
    }

    #[test]
    fn project_state_window_title() {
        let mut state = ProjectState::default();
        assert_eq!(state.window_title(), "BIF - Untitled");

        state.file_path = Some(PathBuf::from("scene.bifa"));
        assert_eq!(state.window_title(), "BIF - scene.bifa");

        state.mark_dirty();
        assert_eq!(state.window_title(), "BIF - scene.bifa *");

        state.mark_clean();
        assert_eq!(state.window_title(), "BIF - scene.bifa");
    }

    #[test]
    fn camera_data_round_trip() {
        let cam = bif_math::Camera::new(
            bif_math::Vec3::new(1.0, 2.0, 3.0),
            bif_math::Vec3::ZERO,
            16.0 / 9.0,
        );
        let data = CameraData::from(&cam);
        let mut restored = bif_math::Camera::new(bif_math::Vec3::ZERO, bif_math::Vec3::ZERO, 1.0);
        data.apply_to(&mut restored);

        assert!((restored.position - cam.position).length() < 1e-5);
        assert!((restored.target - cam.target).length() < 1e-5);
        assert_eq!(restored.fov_y, cam.fov_y);
        assert_eq!(restored.distance, cam.distance);
        // aspect is NOT restored (window-dependent)
    }

    #[test]
    fn serde_eval_mode_round_trip() {
        for mode in [EvalMode::Auto, EvalMode::Manual, EvalMode::OnMouseRelease] {
            let json = serde_json::to_string(&mode).unwrap();
            let back: EvalMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }

    #[test]
    fn skipped_fields_reset_after_load() {
        let project = sample_project();
        let dir = std::env::temp_dir().join("bif_test_skip");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_skip.bifa");

        save_project(&project, &path).unwrap();
        let loaded = load_project(&path).unwrap();

        // Runtime fields should be default (false/None) after deserialization
        let read_node = loaded.graph.node_ids().next().unwrap().0;
        if let SceneNode::UsdRead {
            is_loaded, error, ..
        } = &loaded.graph[read_node]
        {
            assert!(!is_loaded);
            assert!(error.is_none());
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
