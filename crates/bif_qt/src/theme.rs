// bif_qt theme — port of bif_viewport/src/theme.rs to Qt stylesheet
// + QPalette format.
//
// Naming mirrors the egui theme constants 1:1 so panel code can be
// ported mechanically. Values are RGBA u8 tuples (UI-agnostic); the
// `to_qt_rgba_string` helper renders them into `rgba(r,g,b,a)` for
// Qt stylesheets.
//
// The stylesheet is injected at QApplication setup via
// `QApplication::setStyleSheet`. The palette is installed via
// `QApplication::setPalette` — QPalette handles the default widget
// colors where stylesheets don't reach.

/// RGBA color in 0-255 sRGB. UI-framework-agnostic.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(r, g, b, 255)
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(r, g, b, a)
    }

    /// Qt stylesheet `rgba(r, g, b, a)` representation.
    pub fn to_qt_rgba_string(self) -> String {
        format!("rgba({}, {}, {}, {})", self.0, self.1, self.2, self.3)
    }
}

// ---------------------------------------------------------------------------
// Backgrounds
// ---------------------------------------------------------------------------

pub const BG_BASE: Color = Color::rgb(26, 29, 33);
pub const BG_PANEL: Color = Color::rgb(34, 38, 44);
pub const BG_SURFACE: Color = Color::rgb(42, 47, 54);
pub const BG_OVERLAY: Color = Color::rgb(51, 56, 64);
pub const BG_INPUT: Color = Color::rgb(30, 34, 40);

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

pub const TEXT_PRIMARY: Color = Color::rgb(220, 222, 226);
pub const TEXT_SECONDARY: Color = Color::rgb(140, 145, 155);
pub const TEXT_DISABLED: Color = Color::rgb(80, 85, 95);
pub const TEXT_ACCENT: Color = Color::rgb(74, 144, 217);

// ---------------------------------------------------------------------------
// Semantic status colors
// ---------------------------------------------------------------------------

pub const STATUS_OK: Color = Color::rgb(60, 180, 80);
pub const STATUS_WARNING: Color = Color::rgb(220, 170, 50);
pub const STATUS_ERROR: Color = Color::rgb(200, 65, 65);
pub const STATUS_INFO: Color = Color::rgb(100, 150, 220);

// ---------------------------------------------------------------------------
// Accent
// ---------------------------------------------------------------------------

pub const ACCENT_PRIMARY: Color = Color::rgb(74, 144, 217);
pub const ACCENT_HOVER: Color = Color::rgb(90, 158, 228);
pub const ACCENT_DIM: Color = Color::rgb(50, 95, 145);

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

pub const SELECTION_BG: Color = Color::rgba(74, 144, 217, 40);
pub const SELECTION_ROW: Color = Color::rgba(74, 144, 217, 25);

// ---------------------------------------------------------------------------
// Node-graph pin colors
// ---------------------------------------------------------------------------

pub const PIN_SCENE: Color = Color::rgb(100, 200, 100);
pub const PIN_IMAGE: Color = Color::rgb(200, 150, 50);
pub const PIN_ENVIRONMENT: Color = Color::rgb(100, 150, 255);

// ---------------------------------------------------------------------------
// Layer-Aware Stage colors
// ---------------------------------------------------------------------------

pub const LAYER_COLORS: [Color; 8] = [
    Color::rgb(80, 190, 180),  // teal
    Color::rgb(180, 120, 220), // purple
    Color::rgb(230, 150, 70),  // orange
    Color::rgb(220, 190, 80),  // gold
    Color::rgb(230, 130, 180), // pink
    Color::rgb(90, 150, 230),  // blue
    Color::rgb(120, 200, 100), // green
    Color::rgb(220, 100, 100), // red
];

#[inline]
pub fn layer_color(index: usize) -> Color {
    LAYER_COLORS[index % LAYER_COLORS.len()]
}

// ---------------------------------------------------------------------------
// Transform axis colors
// ---------------------------------------------------------------------------

pub const AXIS_X: Color = Color::rgb(220, 50, 50);
pub const AXIS_Y: Color = Color::rgb(50, 200, 50);
pub const AXIS_Z: Color = Color::rgb(50, 100, 230);

// ---------------------------------------------------------------------------
// Animation
// ---------------------------------------------------------------------------

pub const KEYFRAME_FILL: Color = Color::rgb(220, 160, 40);
pub const KEYFRAME_STROKE: Color = Color::rgb(180, 140, 30);

// ---------------------------------------------------------------------------
// Overlay
// ---------------------------------------------------------------------------

pub const BG_OVERLAY_BACKDROP: Color = Color::rgba(26, 29, 33, 200);

// ---------------------------------------------------------------------------
// Qt stylesheet generation
// ---------------------------------------------------------------------------

/// Build the global Qt stylesheet string for the BIF dark theme.
///
/// Applied via `QApplication::setStyleSheet`. Covers QMainWindow,
/// QDockWidget, QMenuBar, QStatusBar, QPushButton, QLineEdit,
/// QTreeView, QTableView, QTabWidget. Row height 32px per UI_DESIGN.
/// Returns a `String` so the exact stylesheet is introspectable at
/// runtime (useful for the future Theme Editor panel).
pub fn qt_stylesheet() -> String {
    format!(
        "
        QMainWindow, QWidget {{
            background-color: {bg_panel};
            color: {text_primary};
            font-family: 'Segoe UI', 'Inter', sans-serif;
            font-size: 12px;
        }}

        QDockWidget {{
            background-color: {bg_panel};
            color: {text_primary};
            titlebar-close-icon: url(close.png);
        }}

        QDockWidget::title {{
            background-color: {bg_surface};
            padding: 4px 8px;
            text-align: left;
        }}

        QMenuBar {{
            background-color: {bg_panel};
            color: {text_primary};
            border-bottom: 1px solid {bg_base};
        }}

        QMenuBar::item:selected {{
            background-color: {accent_dim};
        }}

        QMenu {{
            background-color: {bg_overlay};
            color: {text_primary};
            border: 1px solid {bg_base};
        }}

        QMenu::item:selected {{
            background-color: {accent_dim};
        }}

        QStatusBar {{
            background-color: {bg_panel};
            color: {text_secondary};
            border-top: 1px solid {bg_base};
        }}

        QPushButton {{
            background-color: {bg_surface};
            color: {text_primary};
            border: 1px solid {bg_base};
            padding: 4px 10px;
            min-height: 24px;
        }}

        QPushButton:hover  {{ background-color: {bg_overlay}; }}
        QPushButton:pressed {{ background-color: {accent_dim}; }}

        QLineEdit, QTextEdit, QPlainTextEdit, QSpinBox, QDoubleSpinBox {{
            background-color: {bg_input};
            color: {text_primary};
            border: 1px solid {bg_base};
            padding: 2px 4px;
            selection-background-color: {accent_primary};
        }}

        QTreeView, QTableView, QListView {{
            background-color: {bg_panel};
            color: {text_primary};
            alternate-background-color: {bg_surface};
            border: none;
            outline: none;
        }}

        QTreeView::item, QTableView::item, QListView::item {{
            min-height: 28px;
            padding: 2px 4px;
        }}

        QTreeView::item:hover,
        QTableView::item:hover,
        QListView::item:hover {{
            background-color: {selection_row};
        }}

        QTreeView::item:selected,
        QTableView::item:selected,
        QListView::item:selected {{
            background-color: {selection_bg};
            color: {text_primary};
        }}

        QHeaderView::section {{
            background-color: {bg_surface};
            color: {text_secondary};
            padding: 4px 6px;
            border: none;
            border-right: 1px solid {bg_base};
        }}

        QTabWidget::pane {{
            background-color: {bg_panel};
            border: 1px solid {bg_base};
        }}

        QTabBar::tab {{
            background-color: {bg_surface};
            color: {text_secondary};
            padding: 6px 12px;
            border: 1px solid {bg_base};
        }}

        QTabBar::tab:selected {{
            background-color: {bg_panel};
            color: {text_primary};
        }}

        QTabBar::tab:hover {{ color: {text_primary}; }}

        QScrollBar:vertical {{
            background-color: {bg_panel};
            width: 10px;
            margin: 0;
        }}

        QScrollBar::handle:vertical {{
            background-color: {bg_overlay};
            min-height: 24px;
            border-radius: 2px;
        }}

        QScrollBar::handle:vertical:hover {{
            background-color: {accent_dim};
        }}

        QScrollBar::add-line:vertical,
        QScrollBar::sub-line:vertical {{
            height: 0;
        }}
        ",
        bg_base = BG_BASE.to_qt_rgba_string(),
        bg_panel = BG_PANEL.to_qt_rgba_string(),
        bg_surface = BG_SURFACE.to_qt_rgba_string(),
        bg_overlay = BG_OVERLAY.to_qt_rgba_string(),
        bg_input = BG_INPUT.to_qt_rgba_string(),
        text_primary = TEXT_PRIMARY.to_qt_rgba_string(),
        text_secondary = TEXT_SECONDARY.to_qt_rgba_string(),
        accent_primary = ACCENT_PRIMARY.to_qt_rgba_string(),
        accent_dim = ACCENT_DIM.to_qt_rgba_string(),
        selection_bg = SELECTION_BG.to_qt_rgba_string(),
        selection_row = SELECTION_ROW.to_qt_rgba_string(),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_constants_match_egui_theme() {
        assert_eq!(BG_BASE, Color::rgb(26, 29, 33));
        assert_eq!(ACCENT_PRIMARY, Color::rgb(74, 144, 217));
        assert_eq!(TEXT_PRIMARY, Color::rgb(220, 222, 226));
        assert_eq!(SELECTION_BG, Color::rgba(74, 144, 217, 40));
    }

    #[test]
    fn layer_color_wraps_palette() {
        assert_eq!(layer_color(0), LAYER_COLORS[0]);
        assert_eq!(layer_color(8), LAYER_COLORS[0]);
        assert_eq!(layer_color(9), LAYER_COLORS[1]);
    }

    #[test]
    fn qt_rgba_string_format() {
        assert_eq!(BG_BASE.to_qt_rgba_string(), "rgba(26, 29, 33, 255)");
        assert_eq!(SELECTION_BG.to_qt_rgba_string(), "rgba(74, 144, 217, 40)");
    }

    #[test]
    fn stylesheet_includes_key_selectors() {
        let s = qt_stylesheet();
        assert!(s.contains("QMainWindow"));
        assert!(s.contains("QDockWidget"));
        assert!(s.contains("QTreeView"));
        assert!(s.contains("QTabBar::tab"));
        // The palette reference should have been interpolated in.
        assert!(s.contains("rgba(26, 29, 33, 255)")); // BG_BASE
    }
}
