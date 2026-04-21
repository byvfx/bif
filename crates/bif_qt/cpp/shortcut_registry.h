// ShortcutRegistry — centralized QKeySequence lookup with
// user-override support via QSettings.
//
// Every user-facing keyboard shortcut in the shell routes through
// this registry. Callers pass a stable string action ID and a
// compiled-in default sequence; the registry checks QSettings
// `shortcuts/<action_id>` for an override and returns whichever is
// set. This means:
//
//   1. Shortcuts have a canonical ID that survives code rewrites.
//   2. Defaults are compiled in (no dependency on user settings
//      being present).
//   3. A future Preferences dialog (v0.16) can enumerate registered
//      IDs, display a QKeySequenceEdit per entry, and write through
//      to `shortcuts/<id>` in QSettings.
//
// Phase E.1 only adds the lookup path; the dialog is v0.16 scope.
// New shortcuts introduced after Phase E.1 SHOULD go through here
// so the Preferences dialog works uniformly when it lands.
//
// Design note: action IDs use dot-namespaced `category.action` form
// so the dialog can group them (Timeline / File / Camera / etc.).

#pragma once

#include <QHash>
#include <QKeySequence>
#include <QString>

namespace bif_qt::shortcuts {

// ---- Timeline ------------------------------------------------------
inline constexpr const char* kTimelinePrevFrame = "timeline.prev_frame";
inline constexpr const char* kTimelineNextFrame = "timeline.next_frame";
inline constexpr const char* kTimelinePrevKeyframe = "timeline.prev_keyframe";
inline constexpr const char* kTimelineNextKeyframe = "timeline.next_keyframe";
inline constexpr const char* kTimelineTogglePlayback = "timeline.toggle_playback";

// ---- Camera / Viewport --------------------------------------------
inline constexpr const char* kCameraFrameSelected = "camera.frame_selected";

// ---- File ----------------------------------------------------------
inline constexpr const char* kFileNewStage = "file.new_stage";
inline constexpr const char* kFileOpenStage = "file.open_stage";
inline constexpr const char* kFileSave = "file.save";
inline constexpr const char* kFileSaveAs = "file.save_as";
inline constexpr const char* kFileExit = "file.exit";

// ---- Workspace -----------------------------------------------------
inline constexpr const char* kWorkspaceAssembly = "workspace.assembly";
inline constexpr const char* kWorkspaceLighting = "workspace.lighting";
inline constexpr const char* kWorkspaceMaterials = "workspace.materials";
inline constexpr const char* kWorkspaceRender = "workspace.render";
inline constexpr const char* kWorkspaceZenMode = "workspace.zen_mode";

// ---- Command Palette ----------------------------------------------
inline constexpr const char* kPaletteOpen = "palette.open";

/// Returns the QKeySequence for `action_id`. Consults QSettings
/// (`shortcuts/<action_id>`) first; falls back to `default_sequence`.
/// A non-empty QSettings value that fails to parse also falls back.
QKeySequence lookup(const char* action_id, const QKeySequence& default_sequence);

/// Write a user override for `action_id` to QSettings. Empty sequence
/// clears the override (falls back to default on next lookup). The
/// v0.16 Preferences dialog will be the sole caller.
void set_override(const char* action_id, const QKeySequence& sequence);

/// Clear a user override (equivalent to `set_override(id, {})`).
void clear_override(const char* action_id);

/// All registered action IDs + their current default. Used by the
/// v0.16 Preferences dialog to enumerate configurable shortcuts.
/// Registration happens at first call via side effect in `lookup`.
QHash<QString, QKeySequence> registered_defaults();

}  // namespace bif_qt::shortcuts
