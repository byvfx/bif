//! Centralized color constants and theme configuration for the BIF UI.
//!
//! Call [`apply_theme`] once after creating an [`egui::Context`] to set the
//! application-wide dark theme with tightened spacing and VFX-friendly colors.

use egui::{Color32, Stroke, Vec2};

// ---------------------------------------------------------------------------
// Backgrounds
// ---------------------------------------------------------------------------

/// Deepest background (viewport area behind panels).
pub const BG_BASE: Color32 = Color32::from_rgb(26, 29, 33);
/// Panel backgrounds.
pub const BG_PANEL: Color32 = Color32::from_rgb(34, 38, 44);
/// Elevated surfaces (headers, hover rows).
pub const BG_SURFACE: Color32 = Color32::from_rgb(42, 47, 54);
/// Popups, tooltips, menus.
pub const BG_OVERLAY: Color32 = Color32::from_rgb(51, 56, 64);
/// Text fields, slider tracks.
pub const BG_INPUT: Color32 = Color32::from_rgb(30, 34, 40);

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// Primary text color.
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(220, 222, 226);
/// Secondary / dimmed text.
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(140, 145, 155);
/// Disabled text.
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(80, 85, 95);
/// Accent-colored text (links, labels).
pub const TEXT_ACCENT: Color32 = Color32::from_rgb(74, 144, 217);

// ---------------------------------------------------------------------------
// Semantic status colors
// ---------------------------------------------------------------------------

/// Success / OK.
pub const STATUS_OK: Color32 = Color32::from_rgb(60, 180, 80);
/// Warning.
pub const STATUS_WARNING: Color32 = Color32::from_rgb(220, 170, 50);
/// Error.
pub const STATUS_ERROR: Color32 = Color32::from_rgb(200, 65, 65);
/// Informational.
pub const STATUS_INFO: Color32 = Color32::from_rgb(100, 150, 220);

// ---------------------------------------------------------------------------
// Accent
// ---------------------------------------------------------------------------

/// Primary accent — selections, focus rings, primary buttons.
pub const ACCENT_PRIMARY: Color32 = Color32::from_rgb(74, 144, 217);
/// Hovered accent.
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(90, 158, 228);
/// Dimmed accent (active press state).
pub const ACCENT_DIM: Color32 = Color32::from_rgb(50, 95, 145);

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

/// Selection background (accent at ~15% opacity).
pub const SELECTION_BG: Color32 = Color32::from_rgba_premultiplied(74, 144, 217, 40);
/// Subtler row highlight.
pub const SELECTION_ROW: Color32 = Color32::from_rgba_premultiplied(74, 144, 217, 25);

// ---------------------------------------------------------------------------
// Node-graph pin colors
// ---------------------------------------------------------------------------

/// Scene data pin.
pub const PIN_SCENE: Color32 = Color32::from_rgb(100, 200, 100);
/// Image data pin.
pub const PIN_IMAGE: Color32 = Color32::from_rgb(200, 150, 50);
/// Environment data pin.
pub const PIN_ENVIRONMENT: Color32 = Color32::from_rgb(100, 150, 255);

// ---------------------------------------------------------------------------
// Transform axis colors
// ---------------------------------------------------------------------------

/// X axis (red).
pub const AXIS_X: Color32 = Color32::from_rgb(220, 50, 50);
/// Y axis (green).
pub const AXIS_Y: Color32 = Color32::from_rgb(50, 200, 50);
/// Z axis (blue).
pub const AXIS_Z: Color32 = Color32::from_rgb(50, 100, 230);

// ---------------------------------------------------------------------------
// Animation
// ---------------------------------------------------------------------------

/// Keyframe diamond fill color.
pub const KEYFRAME_FILL: Color32 = Color32::from_rgb(220, 160, 40);
/// Keyframe diamond stroke/outline color.
pub const KEYFRAME_STROKE: Color32 = Color32::from_rgb(180, 140, 30);

// ---------------------------------------------------------------------------
// Overlay
// ---------------------------------------------------------------------------

/// Semi-transparent backdrop for floating overlays (BG_BASE at ~78% opacity).
pub const BG_OVERLAY_BACKDROP: Color32 = Color32::from_rgba_premultiplied(26, 29, 33, 200);

// ---------------------------------------------------------------------------
// Theme application
// ---------------------------------------------------------------------------

/// Apply the BIF dark theme to an egui context.
///
/// Sets panel fills, widget styles, selection colors, and tighter spacing
/// suitable for a VFX DCC application. Call once after creating the context.
pub fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // -- Visuals --------------------------------------------------------
    let vis = &mut style.visuals;
    vis.dark_mode = true;
    vis.panel_fill = BG_PANEL;
    vis.window_fill = BG_PANEL;
    vis.extreme_bg_color = BG_BASE;
    vis.faint_bg_color = BG_INPUT;

    // Widgets — inactive
    vis.widgets.inactive.bg_fill = BG_SURFACE;
    vis.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);

    // Widgets — hovered
    vis.widgets.hovered.bg_fill = BG_OVERLAY;
    vis.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

    // Widgets — active (pressed)
    vis.widgets.active.bg_fill = ACCENT_DIM;
    vis.widgets.active.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

    // Widgets — non-interactive (labels, separators)
    vis.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);
    vis.widgets.noninteractive.bg_fill = BG_PANEL;

    // Selection
    vis.selection.bg_fill = SELECTION_BG;
    vis.selection.stroke = Stroke::new(1.0, ACCENT_PRIMARY);

    // -- Spacing --------------------------------------------------------
    style.spacing.item_spacing = Vec2::new(6.0, 3.0);
    style.spacing.button_padding = Vec2::new(6.0, 2.0);

    ctx.set_style(style);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_theme_does_not_panic() {
        let ctx = egui::Context::default();
        apply_theme(&ctx);

        let style = ctx.style();
        assert!(style.visuals.dark_mode);
        assert_eq!(style.visuals.panel_fill, BG_PANEL);
        assert_eq!(style.visuals.extreme_bg_color, BG_BASE);
        assert_eq!(style.visuals.selection.bg_fill, SELECTION_BG);
        assert_eq!(style.spacing.item_spacing, Vec2::new(6.0, 3.0));
    }

    #[test]
    fn color_constants_are_correct() {
        assert_eq!(BG_BASE, Color32::from_rgb(26, 29, 33));
        assert_eq!(ACCENT_PRIMARY, Color32::from_rgb(74, 144, 217));
        assert_eq!(TEXT_PRIMARY, Color32::from_rgb(220, 222, 226));
        assert_eq!(
            SELECTION_BG,
            Color32::from_rgba_premultiplied(74, 144, 217, 40)
        );
    }
}
