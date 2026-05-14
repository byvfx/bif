// bif_qt theme — Graphite design system ("Quiet Confidence").
//
// Values track `assets/stitch_bif_ui/obsidian_graphite/DESIGN.md`:
//   - Tonal Architecture: hierarchy through surface depth, not borders
//   - No-Line Rule: 1px solid borders are prohibited for sectioning
//   - Surface tiers: surface (#131313) -> container (#20201f) -> highest (#353535)
//   - Text: ON_SURFACE (#e5e2e1) never pure white
//   - Accents: primary #a4c9ff (text) / #4a9eff (focus glow)
//
// Public color names follow the old BIF "BG_BASE / BG_PANEL / BG_SURFACE..."
// taxonomy so existing call sites keep compiling — the RGB values now map
// onto the Graphite surface tiers.

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
// Graphite surface tiers (DESIGN.md §2)
// ---------------------------------------------------------------------------

pub const SURFACE: Color = Color::rgb(0x13, 0x13, 0x13);
pub const SURFACE_CONTAINER_LOWEST: Color = Color::rgb(0x0e, 0x0e, 0x0e);
pub const SURFACE_CONTAINER_LOW: Color = Color::rgb(0x1b, 0x1b, 0x1b);
pub const SURFACE_CONTAINER: Color = Color::rgb(0x20, 0x20, 0x1f);
pub const SURFACE_CONTAINER_HIGH: Color = Color::rgb(0x2a, 0x2a, 0x2a);
pub const SURFACE_CONTAINER_HIGHEST: Color = Color::rgb(0x35, 0x35, 0x35);

// Legacy aliases — old constants mapped onto Graphite tiers.
pub const BG_BASE: Color = SURFACE;
pub const BG_PANEL: Color = SURFACE_CONTAINER;
pub const BG_SURFACE: Color = SURFACE_CONTAINER_HIGH;
pub const BG_OVERLAY: Color = SURFACE_CONTAINER_HIGHEST;
pub const BG_INPUT: Color = SURFACE_CONTAINER_LOWEST;

// ---------------------------------------------------------------------------
// Text (DESIGN.md §3 + §6)
// ---------------------------------------------------------------------------

pub const ON_SURFACE: Color = Color::rgb(0xe5, 0xe2, 0xe1);
pub const ON_SURFACE_VARIANT: Color = Color::rgb(0xc0, 0xc7, 0xd4);
pub const OUTLINE: Color = Color::rgb(0x8a, 0x91, 0x9e);
pub const OUTLINE_VARIANT: Color = Color::rgb(0x41, 0x47, 0x52);

pub const TEXT_PRIMARY: Color = ON_SURFACE;
pub const TEXT_SECONDARY: Color = OUTLINE;
pub const TEXT_DISABLED: Color = Color::rgb(0x55, 0x59, 0x62);
pub const TEXT_ACCENT: Color = Color::rgb(0xa4, 0xc9, 0xff);

// ---------------------------------------------------------------------------
// Accents (DESIGN.md §2 §5)
// ---------------------------------------------------------------------------

pub const PRIMARY: Color = Color::rgb(0xa4, 0xc9, 0xff);
pub const PRIMARY_CONTAINER: Color = Color::rgb(0x4a, 0x9e, 0xff);
pub const ON_PRIMARY: Color = Color::rgb(0x00, 0x31, 0x5d);
pub const SECONDARY: Color = Color::rgb(0x5d, 0xd9, 0xd0);

pub const ACCENT_PRIMARY: Color = PRIMARY_CONTAINER;
pub const ACCENT_HOVER: Color = PRIMARY;
pub const ACCENT_DIM: Color = Color::rgb(0x2c, 0x5a, 0x90);

// ---------------------------------------------------------------------------
// Semantic status colors
// ---------------------------------------------------------------------------

pub const STATUS_OK: Color = SECONDARY;
pub const STATUS_WARNING: Color = Color::rgb(0xdc, 0xaa, 0x32);
pub const STATUS_ERROR: Color = Color::rgb(0xc8, 0x41, 0x41);
pub const STATUS_INFO: Color = PRIMARY_CONTAINER;

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

pub const SELECTION_BG: Color = Color::rgba(0x4a, 0x9e, 0xff, 60);
pub const SELECTION_ROW: Color = Color::rgba(0x4a, 0x9e, 0xff, 30);

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
    Color::rgb(80, 190, 180),
    Color::rgb(180, 120, 220),
    Color::rgb(230, 150, 70),
    Color::rgb(220, 190, 80),
    Color::rgb(230, 130, 180),
    Color::rgb(90, 150, 230),
    Color::rgb(120, 200, 100),
    Color::rgb(220, 100, 100),
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

pub const BG_OVERLAY_BACKDROP: Color = Color::rgba(0x13, 0x13, 0x13, 200);

// ---------------------------------------------------------------------------
// Qt stylesheet generation — Graphite "Quiet Confidence"
// ---------------------------------------------------------------------------

/// Build the global Qt stylesheet for the Graphite theme.
///
/// Tonal Architecture: panels distinguish themselves by `surface_container`
/// against the `surface` base. The 1px "optical gutter" is rendered as a
/// dark `surface_container_lowest` strip rather than a border. Inputs sit
/// at `surface_container_lowest` and grow a 1px primary-container "glow"
/// on focus. Section labels are tagged `QLabel#sectionHeader` for the
/// uppercase letter-spaced "instrument-panel" treatment (DESIGN.md §3).
pub fn qt_stylesheet() -> String {
    format!(
        "
        QMainWindow, QWidget {{
            background-color: {surface};
            color: {on_surface};
            font-family: 'Inter', 'Segoe UI', system-ui, sans-serif;
            font-size: 13px;
        }}

        QDockWidget {{
            background-color: {surface_container};
            color: {on_surface};
            border: none;
        }}
        QDockWidget::title {{
            background-color: {surface_container_low};
            padding: 6px 10px;
            text-align: left;
            font-family: 'Manrope', 'Inter', sans-serif;
            font-weight: 500;
        }}
        QDockWidget > QWidget {{ background-color: {surface_container}; }}

        QMenuBar {{
            background-color: {surface};
            color: {on_surface};
            padding: 2px 4px;
        }}
        QMenuBar::item {{
            background-color: transparent;
            padding: 4px 10px;
            border-radius: 4px;
        }}
        QMenuBar::item:selected {{ background-color: {surface_container_high}; }}

        QMenu {{
            background-color: {surface_container_highest};
            color: {on_surface};
            border: none;
            padding: 4px;
            border-radius: 6px;
        }}
        QMenu::item {{
            padding: 6px 18px;
            border-radius: 4px;
        }}
        QMenu::item:selected {{ background-color: {primary_container_dim}; }}

        QStatusBar {{
            background-color: {surface_container};
            color: {outline};
            border-top: 1px solid {surface_container_lowest};
            padding: 2px 8px;
        }}

        QToolBar {{
            background-color: {surface_container};
            border: none;
            spacing: 4px;
            padding: 4px;
        }}
        QToolBar::separator {{
            background-color: {surface_container_lowest};
            width: 1px;
            margin: 4px 6px;
        }}

        QPushButton {{
            background-color: {surface_container_high};
            color: {on_surface};
            border: none;
            border-radius: 6px;
            padding: 5px 12px;
            min-height: 24px;
            font-family: 'Inter', 'Segoe UI', sans-serif;
        }}
        QPushButton:hover    {{ background-color: {surface_container_highest}; }}
        QPushButton:pressed  {{ background-color: {primary_container_dim}; }}
        QPushButton:disabled {{ color: {text_disabled}; }}

        QLineEdit, QTextEdit, QPlainTextEdit, QSpinBox, QDoubleSpinBox {{
            background-color: {surface_container_lowest};
            color: {on_surface};
            border: none;
            border-radius: 6px;
            padding: 4px 8px;
            selection-background-color: {primary_container};
            selection-color: {on_primary};
            font-family: 'JetBrains Mono', 'Cascadia Code', Consolas, monospace;
            font-size: 13px;
        }}
        QLineEdit:focus, QTextEdit:focus, QPlainTextEdit:focus,
        QSpinBox:focus, QDoubleSpinBox:focus {{
            border: 1px solid {primary_container};
            padding: 3px 7px;
        }}

        QComboBox {{
            background-color: {surface_container_lowest};
            color: {on_surface};
            border: none;
            border-radius: 6px;
            padding: 4px 10px;
            min-height: 22px;
        }}
        QComboBox:hover {{ background-color: {surface_container_low}; }}
        QComboBox:focus {{ border: 1px solid {primary_container}; padding: 3px 9px; }}
        QComboBox::drop-down {{ border: none; width: 16px; }}
        QComboBox QAbstractItemView {{
            background-color: {surface_container_highest};
            color: {on_surface};
            border: none;
            selection-background-color: {primary_container_dim};
            outline: none;
        }}

        QTreeView, QTableView, QListView, QListWidget {{
            background-color: {surface_container};
            color: {on_surface};
            alternate-background-color: {surface_container_low};
            border: none;
            outline: none;
        }}
        QTreeView::item, QTableView::item, QListView::item, QListWidget::item {{
            min-height: 26px;
            padding: 2px 6px;
        }}
        QTreeView::item:hover, QTableView::item:hover,
        QListView::item:hover, QListWidget::item:hover {{
            background-color: {selection_row};
        }}
        QTreeView::item:selected, QTableView::item:selected,
        QListView::item:selected, QListWidget::item:selected {{
            background-color: {selection_bg};
            color: {on_surface};
        }}

        QHeaderView::section {{
            background-color: {surface_container_low};
            color: {outline};
            padding: 6px 8px;
            border: none;
            font-family: 'Manrope', 'Inter', sans-serif;
            font-weight: 500;
        }}

        QLabel#sectionHeader {{
            color: {outline};
            font-size: 11px;
            letter-spacing: 1px;
            font-weight: 600;
            font-family: 'Manrope', 'Inter', sans-serif;
            padding: 4px 0px;
        }}

        QGroupBox {{
            background-color: transparent;
            border: none;
            margin-top: 14px;
            padding-top: 6px;
        }}
        QGroupBox::title {{
            color: {outline};
            font-size: 11px;
            letter-spacing: 1px;
            font-weight: 600;
            font-family: 'Manrope', 'Inter', sans-serif;
            subcontrol-origin: margin;
            subcontrol-position: top left;
            padding: 2px 0px;
        }}

        QTabWidget::pane {{
            background-color: {surface_container};
            border: none;
        }}
        QTabBar::tab {{
            background-color: {surface_container_low};
            color: {outline};
            padding: 6px 14px;
            border: none;
            border-top-left-radius: 6px;
            border-top-right-radius: 6px;
            font-family: 'Inter', 'Segoe UI', sans-serif;
        }}
        QTabBar::tab:selected {{
            background-color: {surface_container};
            color: {on_surface};
        }}
        QTabBar::tab:hover {{ color: {on_surface}; }}

        QScrollBar:vertical {{
            background-color: {surface_container};
            width: 10px;
            margin: 0;
            border: none;
        }}
        QScrollBar::handle:vertical {{
            background-color: {surface_container_high};
            min-height: 24px;
            border-radius: 3px;
        }}
        QScrollBar::handle:vertical:hover {{ background-color: {surface_container_highest}; }}
        QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{ height: 0; }}

        QScrollBar:horizontal {{
            background-color: {surface_container};
            height: 10px;
            margin: 0;
            border: none;
        }}
        QScrollBar::handle:horizontal {{
            background-color: {surface_container_high};
            min-width: 24px;
            border-radius: 3px;
        }}
        QScrollBar::handle:horizontal:hover {{ background-color: {surface_container_highest}; }}
        QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {{ width: 0; }}

        QSplitter::handle {{ background-color: {surface_container_lowest}; }}
        QSplitter::handle:horizontal {{ width: 1px; }}
        QSplitter::handle:vertical   {{ height: 1px; }}

        /* ── Section blocks (Collection Editor, future inspectors) ── */
        QFrame#sectionBlock {{
            background-color: {surface_container};
            border: none;
            border-radius: 6px;
        }}
        QFrame#sectionHeaderRow {{
            background-color: {surface_container_low};
            border: none;
            border-radius: 6px;
        }}
        QFrame#sectionHeaderRow:hover {{
            background-color: {surface_container_high};
        }}
        QToolButton#sectionIcon {{
            background-color: transparent;
            color: {on_surface_variant};
            border: none;
            border-radius: 4px;
            padding: 2px 6px;
            font-size: 14px;
            font-weight: 600;
        }}
        QToolButton#sectionIcon:hover {{
            background-color: {surface_container_highest};
            color: {on_surface};
        }}
        QLabel#sectionEmptyHint {{
            color: {outline};
            font-style: italic;
            font-size: 12px;
            padding: 8px 12px;
        }}
        QLabel#sectionChevron {{
            color: {outline};
            font-size: 11px;
            padding: 0px 6px 0px 4px;
        }}

        /* Collection Editor lists — left accent stripe per role */
        QListWidget#includesList {{
            background-color: transparent;
            border: none;
            outline: none;
        }}
        QListWidget#includesList::item {{
            background-color: transparent;
            border-left: 4px solid {primary_container};
            padding: 6px 8px 6px 10px;
            min-height: 22px;
        }}
        QListWidget#includesList::item:hover {{
            background-color: {surface_container_high};
        }}
        QListWidget#includesList::item:selected {{
            background-color: {selection_bg};
        }}

        QListWidget#excludesList {{
            background-color: transparent;
            border: none;
            outline: none;
        }}
        QListWidget#excludesList::item {{
            background-color: transparent;
            border-left: 4px solid {outline};
            padding: 6px 8px 6px 10px;
            min-height: 22px;
        }}
        QListWidget#excludesList::item:hover {{
            background-color: {surface_container_high};
        }}
        QListWidget#excludesList::item:selected {{
            background-color: {selection_bg};
        }}

        QListWidget#membersList {{
            background-color: transparent;
            border: none;
            outline: none;
        }}
        QListWidget#membersList::item {{
            background-color: transparent;
            border-left: 4px solid transparent;
            padding: 6px 8px 6px 10px;
            min-height: 22px;
            color: {outline};
            font-family: 'JetBrains Mono', 'Cascadia Code', Consolas, monospace;
            font-size: 12px;
        }}
        QListWidget#membersList::item:hover {{
            background-color: {surface_container_high};
            color: {on_surface};
        }}
        ",
        surface = SURFACE.to_qt_rgba_string(),
        surface_container_lowest = SURFACE_CONTAINER_LOWEST.to_qt_rgba_string(),
        surface_container_low = SURFACE_CONTAINER_LOW.to_qt_rgba_string(),
        surface_container = SURFACE_CONTAINER.to_qt_rgba_string(),
        surface_container_high = SURFACE_CONTAINER_HIGH.to_qt_rgba_string(),
        surface_container_highest = SURFACE_CONTAINER_HIGHEST.to_qt_rgba_string(),
        on_surface = ON_SURFACE.to_qt_rgba_string(),
        on_surface_variant = ON_SURFACE_VARIANT.to_qt_rgba_string(),
        on_primary = ON_PRIMARY.to_qt_rgba_string(),
        outline = OUTLINE.to_qt_rgba_string(),
        text_disabled = TEXT_DISABLED.to_qt_rgba_string(),
        primary_container = PRIMARY_CONTAINER.to_qt_rgba_string(),
        primary_container_dim = ACCENT_DIM.to_qt_rgba_string(),
        selection_bg = SELECTION_BG.to_qt_rgba_string(),
        selection_row = SELECTION_ROW.to_qt_rgba_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphite_surface_tiers_present() {
        assert_eq!(SURFACE, Color::rgb(0x13, 0x13, 0x13));
        assert_eq!(SURFACE_CONTAINER, Color::rgb(0x20, 0x20, 0x1f));
        assert_eq!(SURFACE_CONTAINER_HIGHEST, Color::rgb(0x35, 0x35, 0x35));
        assert_eq!(SURFACE_CONTAINER_LOWEST, Color::rgb(0x0e, 0x0e, 0x0e));
    }

    #[test]
    fn legacy_aliases_map_to_graphite_tiers() {
        assert_eq!(BG_BASE, SURFACE);
        assert_eq!(BG_PANEL, SURFACE_CONTAINER);
        assert_eq!(BG_OVERLAY, SURFACE_CONTAINER_HIGHEST);
    }

    #[test]
    fn never_pure_white_text() {
        assert_ne!(ON_SURFACE, Color::rgb(0xff, 0xff, 0xff));
        assert_eq!(ON_SURFACE, Color::rgb(0xe5, 0xe2, 0xe1));
    }

    #[test]
    fn layer_color_wraps_palette() {
        assert_eq!(layer_color(0), LAYER_COLORS[0]);
        assert_eq!(layer_color(8), LAYER_COLORS[0]);
        assert_eq!(layer_color(9), LAYER_COLORS[1]);
    }

    #[test]
    fn qt_rgba_string_format() {
        assert_eq!(SURFACE.to_qt_rgba_string(), "rgba(19, 19, 19, 255)");
    }

    #[test]
    fn stylesheet_applies_no_line_rule() {
        let s = qt_stylesheet();
        assert!(s.contains("QDockWidget"));
        assert!(s.contains("border: none"));
        assert!(s.contains("rgba(19, 19, 19, 255)"));
        assert!(s.contains("rgba(32, 32, 31, 255)"));
        assert!(s.contains("border-radius: 6px"));
        assert!(s.contains("Inter"));
        assert!(s.contains("JetBrains Mono"));
        assert!(s.contains("Manrope"));
        assert!(s.contains("QLabel#sectionHeader"));
        assert!(s.contains("letter-spacing"));
        assert!(s.contains("QComboBox {"));
    }

    #[test]
    fn stylesheet_exposes_collection_editor_selectors() {
        let s = qt_stylesheet();
        // Section-block primitives reused across inspectors
        assert!(s.contains("QFrame#sectionBlock"));
        assert!(s.contains("QFrame#sectionHeaderRow"));
        assert!(s.contains("QToolButton#sectionIcon"));
        assert!(s.contains("QLabel#sectionEmptyHint"));
        // Collection Editor list variants — left accent stripe per role
        assert!(s.contains("QListWidget#includesList"));
        assert!(s.contains("QListWidget#excludesList"));
        assert!(s.contains("QListWidget#membersList"));
        assert!(s.contains("border-left: 4px solid"));
    }
}
