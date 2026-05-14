#include "window_builder.h"
#include "collection_editor_widget.h"
#include "command_palette.h"
#include "first_launch_widget.h"
#include "layer_stack_widget.h"
#include "node_graph_widget.h"
#include "usda_panel_widget.h"
#include "property_inspector_widget.h"
#include "render_settings_widget.h"
#include "render_widget.h"
#include "scene_browser_widget.h"
#include "shortcut_registry.h"
#include "timeline_widget.h"

#include <QAction>
#include <QApplication>
#include <QComboBox>
#include <QByteArray>
#include <QCheckBox>
#include <QColor>
#include <QDockWidget>
#include <QDragEnterEvent>
#include <QDragMoveEvent>
#include <QDropEvent>
#include <QFileDialog>
#include <QFileInfo>
#include <QFrame>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QHash>
#include <QKeySequence>
#include <QLabel>
#include <QList>
#include <QMainWindow>
#include <QMenu>
#include <QMenuBar>
#include <QMimeData>
#include <QMessageBox>
#include <QPointer>
#include <QPushButton>
#include <QScreen>
#include <QSettings>
#include <QShortcut>
#include <QStackedWidget>
#include <QStatusBar>
#include <QString>
#include <QStringList>
#include <QTimer>
#include <vector>
#include <QToolBar>
#include <QUrl>
#include <QVBoxLayout>
#include <QWidget>

#include "bif_qt/src/main_window.cxxqt.h"

namespace {

constexpr auto kNodeGraphPreviewProperty = "_bif_node_graph_preview_enabled";
constexpr auto kNodeGraphPreviewSettingsKey = "experiments/node_graph_preview";

bool node_graph_preview_enabled(QMainWindow* window) {
    return window->property(kNodeGraphPreviewProperty).toBool();
}

void set_node_graph_preview_enabled(QMainWindow* window, bool enabled) {
    window->setProperty(kNodeGraphPreviewProperty, enabled);
    QSettings settings;
    settings.setValue(QLatin1String(kNodeGraphPreviewSettingsKey), enabled);
}

void enforce_node_graph_preview_gate(QMainWindow* window) {
    auto* node_graph = window->findChild<QDockWidget*>(QStringLiteral("dock_node_graph"));
    if (node_graph && !node_graph_preview_enabled(window)) {
        node_graph->hide();
    }
}

bool is_supported_stage_path(const QString& path) {
    const auto suffix = QFileInfo(path).suffix().toLower();
    return suffix == QStringLiteral("usd")
        || suffix == QStringLiteral("usda")
        || suffix == QStringLiteral("usdc")
        || suffix == QStringLiteral("usdz");
}

QString first_supported_stage_path(const QMimeData* mime_data) {
    if (!mime_data) return QString();
    for (const auto& url : mime_data->urls()) {
        if (!url.isLocalFile()) continue;
        const auto path = url.toLocalFile();
        if (is_supported_stage_path(path)) {
            return path;
        }
    }
    return QString();
}

struct CameraChoice {
    QString label;
    QString key;
    bool separator_before = false;
};

QList<CameraChoice> collect_camera_choices(BifShellState* state) {
    QList<CameraChoice> choices;
    choices.append(CameraChoice{QStringLiteral("Perspective"), QStringLiteral("free"), false});

    const struct { const char* label; const char* key; } ortho_views[] = {
        {"Top",    "ortho:Top"},
        {"Bottom", "ortho:Bottom"},
        {"Front",  "ortho:Front"},
        {"Back",   "ortho:Back"},
        {"Right",  "ortho:Right"},
        {"Left",   "ortho:Left"},
    };
    bool first_ortho = true;
    for (const auto& view : ortho_views) {
        choices.append(CameraChoice{
            QString::fromLatin1(view.label),
            QString::fromLatin1(view.key),
            first_ortho,
        });
        first_ortho = false;
    }

    const int usd_count = state->usd_camera_count();
    for (int i = 0; i < usd_count; ++i) {
        const auto path = state->usd_camera_path_at(i);
        const auto leaf = path.split(QLatin1Char('/')).last();
        choices.append(CameraChoice{
            leaf,
            QStringLiteral("usd:") + path,
            i == 0,
        });
    }
    return choices;
}

void sync_camera_picker_selection(QComboBox* picker, const QString& active_key) {
    if (!picker) return;
    for (int i = 0; i < picker->count(); ++i) {
        if (picker->itemData(i).toString() == active_key) {
            picker->setCurrentIndex(i);
            return;
        }
    }
}

bool is_ortho_camera_source(const QString& key) {
    return key.startsWith(QStringLiteral("ortho:"));
}

// ---------------------------------------------------------------------------
// Workspace presets (B.5)
// ---------------------------------------------------------------------------
namespace ws {
constexpr const char* ASSEMBLY = "assembly";
constexpr const char* LIGHTING = "lighting";
constexpr const char* MATERIALS = "materials";
constexpr const char* REVIEW = "review";
constexpr const char* DEFAULT_WORKSPACE = ASSEMBLY;
constexpr const char* SETTINGS_LAST_KEY = "workspaces/last_active";

QString canonical_name(const QString& name) {
    if (name == QStringLiteral("render")) return QString::fromLatin1(REVIEW);
    return name;
}

QString display_name(const QString& name) {
    const auto canonical = canonical_name(name);
    if (canonical == QLatin1String(ASSEMBLY)) return QStringLiteral("Assembly");
    if (canonical == QLatin1String(LIGHTING)) return QStringLiteral("Lighting");
    if (canonical == QLatin1String(MATERIALS)) return QStringLiteral("Materials");
    if (canonical == QLatin1String(REVIEW)) return QStringLiteral("Review");
    return canonical;
}

QString payload_policy_for_workspace(const QString& name) {
    // C3 only has native LoadAll/LoadNone. Lighting keeps LoadAll until the
    // future frustum-based policy exists in bif_core.
    if (canonical_name(name) == QLatin1String(REVIEW)) return QStringLiteral("LoadNone");
    return QStringLiteral("LoadAll");
}

void apply_default_layout(QMainWindow* window, const QString& name) {
    const auto canonical = canonical_name(name);
    auto dock = [window](const char* obj_name) -> QDockWidget* {
        return window->findChild<QDockWidget*>(QString::fromLatin1(obj_name));
    };
    auto* scene_browser = dock("dock_scene_browser");
    auto* layer_stack = dock("dock_layer_stack");
    auto* property_inspector = dock("dock_property_inspector");
    auto* render_settings = dock("dock_render_settings");
    auto* node_graph = dock("dock_node_graph");
    auto* timeline = dock("dock_timeline");
    const bool node_graph_preview = node_graph_preview_enabled(window);

    auto show = [](QDockWidget* dock_widget, bool visible) {
        if (dock_widget) dock_widget->setVisible(visible);
    };

    if (canonical == QLatin1String(ws::ASSEMBLY)) {
        show(scene_browser, true);
        show(layer_stack, true);
        show(property_inspector, true);
        show(render_settings, false);
        show(node_graph, node_graph_preview);
        show(timeline, true);
        if (node_graph && node_graph_preview) {
            node_graph->raise();
        } else if (timeline) {
            timeline->raise();
        }
        if (property_inspector) property_inspector->raise();
    } else if (canonical == QLatin1String(ws::LIGHTING)) {
        show(scene_browser, true);
        show(layer_stack, false);
        show(property_inspector, true);
        show(render_settings, true);
        show(node_graph, false);
        show(timeline, false);
        if (render_settings) render_settings->raise();
    } else if (canonical == QLatin1String(ws::MATERIALS)) {
        show(scene_browser, true);
        show(layer_stack, false);
        show(property_inspector, true);
        show(render_settings, false);
        show(node_graph, node_graph_preview);
        show(timeline, false);
        if (node_graph && node_graph_preview) node_graph->raise();
        if (property_inspector) property_inspector->raise();
    } else if (canonical == QLatin1String(ws::REVIEW)) {
        show(scene_browser, false);
        show(layer_stack, false);
        show(property_inspector, false);
        show(render_settings, true);
        show(node_graph, false);
        show(timeline, false);
        if (render_settings) render_settings->raise();
    } else {
        show(scene_browser, true);
        show(layer_stack, true);
        show(property_inspector, true);
        show(render_settings, false);
        show(node_graph, node_graph_preview);
        show(timeline, true);
    }
}

QString state_key(const QString& name) {
    return QStringLiteral("workspaces/%1/state").arg(canonical_name(name));
}

void save_current(QMainWindow* window, const QString& current) {
    const auto canonical = canonical_name(current);
    if (canonical.isEmpty()) return;
    QSettings settings;
    settings.setValue(state_key(canonical), window->saveState());
}

void finish_switch_to(
    QMainWindow* window,
    BifShellState* state,
    const QString& canonical_target) {
    save_current(window, state->getCurrent_workspace());

    QSettings settings;
    const QByteArray blob = settings.value(state_key(canonical_target)).toByteArray();
    if (blob.isEmpty()) {
        apply_default_layout(window, canonical_target);
    } else if (!window->restoreState(blob)) {
        apply_default_layout(window, canonical_target);
    }
    enforce_node_graph_preview_gate(window);

    state->setCurrent_workspace(canonical_target);
    settings.setValue(QLatin1String(SETTINGS_LAST_KEY), canonical_target);
    state->setStatus_message(
        QStringLiteral("Workspace: %1").arg(display_name(canonical_target)));
    window->statusBar()->showMessage(state->getStatus_message());
}

void switch_to(
    QMainWindow* window,
    BifShellState* state,
    const QString& target) {
    const auto canonical_target = canonical_name(target);
    const auto target_policy = payload_policy_for_workspace(canonical_target);
    const auto current_policy = state->payload_policy_name();
    if (target_policy != current_policy) {
        if (state->has_loaded_stage()) {
            QPointer<QMainWindow> window_guard(window);
            QPointer<BifShellState> state_guard(state);
            QTimer::singleShot(0, window, [window_guard, state_guard, canonical_target,
                                           current_policy, target_policy]() {
                if (!window_guard || !state_guard) return;
                const auto answer = QMessageBox::question(
                    window_guard,
                    QStringLiteral("Reload Stage For Workspace"),
                    QStringLiteral(
                        "Switching to %1 changes payload loading from %2 to %3 and reloads the "
                        "open stage. Continue?")
                        .arg(display_name(canonical_target), current_policy, target_policy),
                    QMessageBox::Yes | QMessageBox::No,
                    QMessageBox::No);
                if (answer != QMessageBox::Yes) {
                    return;
                }
                if (!state_guard->on_set_payload_policy(target_policy)) {
                    window_guard->statusBar()->showMessage(state_guard->getStatus_message());
                    return;
                }
                finish_switch_to(window_guard, state_guard, canonical_target);
            });
            return;
        }
        if (!state->on_set_payload_policy(target_policy)) {
            window->statusBar()->showMessage(state->getStatus_message());
            return;
        }
    }

    finish_switch_to(window, state, canonical_target);
}
}  // namespace ws

// ---------------------------------------------------------------------------
// Menu actions
// ---------------------------------------------------------------------------
struct MenuActions {
    QAction* new_stage;
    QAction* open_stage;
    QAction* close_stage;
    QAction* save;
    QAction* save_as;
    QAction* exit_app;
    QAction* undo;
    QAction* redo;
    QAction* ivar_render;

    QMenu* look_through;
    QAction* toggle_orthographic;
    QAction* workspace_assembly;
    QAction* workspace_lighting;
    QAction* workspace_materials;
    QAction* workspace_review;
    QAction* zen_mode;
    QAction* toggle_grid;
    QAction* toggle_lod;
    QAction* toggle_node_graph_experimental;
    QAction* toggle_usda_source;

    // View > Panels submenu (v0.16.6)
    QAction* panel_scene_browser;
    QAction* panel_layer_stack;
    QAction* panel_property_inspector;
    QAction* panel_render_settings;
    QAction* panel_collection_editor;
    QAction* panel_timeline;
    QAction* reset_workspace_layout;

    QAction* about;
};

QDockWidget* make_placeholder_dock(
    const QString& title,
    const QString& object_name,
    Qt::DockWidgetArea initial_area,
    QMainWindow* parent) {
    auto* dock = new QDockWidget(title, parent);
    dock->setObjectName(object_name);
    dock->setAllowedAreas(Qt::AllDockWidgetAreas);
    dock->setFeatures(
        QDockWidget::DockWidgetMovable |
        QDockWidget::DockWidgetFloatable |
        QDockWidget::DockWidgetClosable);

    auto* placeholder = new QLabel(
        QStringLiteral("(%1 panel — Phase C)").arg(title),
        dock);
    placeholder->setAlignment(Qt::AlignCenter);
    placeholder->setMinimumWidth(220);
    placeholder->setStyleSheet(QStringLiteral(
        "color: rgba(140, 145, 155, 255); padding: 24px;"));
    dock->setWidget(placeholder);

    parent->addDockWidget(initial_area, dock);
    return dock;
}

MenuActions build_menu_bar(QMainWindow* window) {
    namespace sc = bif_qt::shortcuts;
    MenuActions a{};
    auto* menu = window->menuBar();

    auto* file = menu->addMenu(QStringLiteral("&File"));
    a.new_stage = file->addAction(QStringLiteral("&New Stage"));
    a.new_stage->setShortcut(QKeySequence::New);
    a.open_stage = file->addAction(QStringLiteral("&Open Stage..."));
    a.open_stage->setShortcut(QKeySequence::Open);
    a.close_stage = file->addAction(QStringLiteral("&Close Stage"));
    a.close_stage->setShortcut(QKeySequence::Close);
    file->addSeparator();
    a.save = file->addAction(QStringLiteral("&Save"));
    a.save->setShortcut(QKeySequence::Save);
    a.save_as = file->addAction(QStringLiteral("Save &As..."));
    a.save_as->setShortcut(QKeySequence::SaveAs);
    file->addSeparator();
    a.exit_app = file->addAction(QStringLiteral("E&xit"));
    a.exit_app->setShortcut(QKeySequence(QStringLiteral("Ctrl+Q")));

    auto* edit = menu->addMenu(QStringLiteral("&Edit"));
    a.undo = edit->addAction(QStringLiteral("&Undo"));
    a.undo->setShortcut(sc::lookup(sc::kEditUndo, QKeySequence(QStringLiteral("Ctrl+Z"))));
    a.undo->setShortcutContext(Qt::ApplicationShortcut);
    a.undo->setEnabled(false);
    a.redo = edit->addAction(QStringLiteral("&Redo"));
    a.redo->setShortcut(sc::lookup(sc::kEditRedo, QKeySequence(QStringLiteral("Ctrl+Shift+Z"))));
    a.redo->setShortcutContext(Qt::ApplicationShortcut);
    a.redo->setEnabled(false);

    auto* view = menu->addMenu(QStringLiteral("&View"));
    a.look_through = view->addMenu(QStringLiteral("Look &Through"));
    a.toggle_orthographic = view->addAction(QStringLiteral("&Orthographic Projection"));
    a.toggle_orthographic->setCheckable(true);
    view->addSeparator();
    a.workspace_assembly = view->addAction(QStringLiteral("&Assembly Workspace"));
    a.workspace_assembly->setShortcut(QKeySequence(QStringLiteral("Ctrl+1")));
    a.workspace_lighting = view->addAction(QStringLiteral("&Lighting Workspace"));
    a.workspace_lighting->setShortcut(QKeySequence(QStringLiteral("Ctrl+2")));
    a.workspace_materials = view->addAction(QStringLiteral("&Materials Workspace"));
    a.workspace_materials->setShortcut(QKeySequence(QStringLiteral("Ctrl+3")));
    a.workspace_review = view->addAction(QStringLiteral("&Review Workspace"));
    a.workspace_review->setShortcut(QKeySequence(QStringLiteral("Ctrl+4")));
    view->addSeparator();
    a.zen_mode = view->addAction(QStringLiteral("&Zen Mode"));
    a.zen_mode->setShortcut(QKeySequence(QStringLiteral("Ctrl+\\")));
    a.zen_mode->setCheckable(true);
    a.toggle_grid = view->addAction(QStringLiteral("&Grid"));
    a.toggle_grid->setCheckable(true);
    a.toggle_grid->setChecked(true);  // DisplaySettings::default = true
    a.toggle_lod = view->addAction(QStringLiteral("Viewport &LOD"));
    a.toggle_lod->setShortcut(QKeySequence(QStringLiteral("Ctrl+L")));
    a.toggle_lod->setCheckable(true);
    a.toggle_lod->setChecked(true);  // DisplaySettings::default = true

    view->addSeparator();
    auto* panels = view->addMenu(QStringLiteral("&Panels"));
    auto add_panel = [panels](const QString& label,
                              const char* sc_id,
                              const QKeySequence& def) -> QAction* {
        auto* act = panels->addAction(label);
        act->setCheckable(true);
        act->setShortcut(sc::lookup(sc_id, def));
        act->setShortcutContext(Qt::ApplicationShortcut);
        return act;
    };
    a.panel_scene_browser = add_panel(
        QStringLiteral("Scene &Browser"),
        sc::kPanelSceneBrowser,
        QKeySequence(QStringLiteral("Ctrl+Shift+1")));
    a.panel_layer_stack = add_panel(
        QStringLiteral("&Layer Stack"),
        sc::kPanelLayerStack,
        QKeySequence(QStringLiteral("Ctrl+Shift+2")));
    a.panel_property_inspector = add_panel(
        QStringLiteral("&Property Inspector"),
        sc::kPanelPropertyInspector,
        QKeySequence(QStringLiteral("Ctrl+Shift+3")));
    a.panel_render_settings = add_panel(
        QStringLiteral("&Render Settings"),
        sc::kPanelRenderSettings,
        QKeySequence(QStringLiteral("Ctrl+Shift+4")));
    a.panel_collection_editor = add_panel(
        QStringLiteral("&Collection Editor"),
        sc::kPanelCollectionEditor,
        QKeySequence(QStringLiteral("Ctrl+Shift+5")));
    a.toggle_node_graph_experimental = add_panel(
        QStringLiteral("&Node Graph (Experimental)"),
        sc::kPanelNodeGraph,
        QKeySequence(QStringLiteral("Ctrl+Shift+6")));
    a.panel_timeline = add_panel(
        QStringLiteral("&Timeline"),
        sc::kPanelTimeline,
        QKeySequence(QStringLiteral("Ctrl+Shift+7")));
    a.toggle_usda_source = add_panel(
        QStringLiteral("&USDA Source"),
        sc::kPanelUsdaSource,
        QKeySequence(QStringLiteral("Ctrl+Shift+8")));
    a.toggle_usda_source->setChecked(false);

    view->addSeparator();
    a.reset_workspace_layout =
        view->addAction(QStringLiteral("&Reset Workspace Layout"));

    auto* render = menu->addMenu(QStringLiteral("&Render"));
    a.ivar_render = render->addAction(QStringLiteral("Ivar &Render"));

    auto* help = menu->addMenu(QStringLiteral("&Help"));
    a.about = help->addAction(QStringLiteral("&About BIF"));

    return a;
}

// ---------------------------------------------------------------------------
// Tier 1 — Edit-target pill + status bar chip + viewport edge tint.
//
// All three surfaces read the same 4 invokables on `BifShellState`
// (`active_edit_target_is_set` / `_name` / `_identifier` / `_color_index`)
// and refresh on `layer_state_revisionChanged`. The pill lives in the
// breadcrumb toolbar (right-aligned), the chip lives in the status bar
// (permanent widget, right-aligned), and the viewport edge tint is a
// 2px inner border on a QFrame that wraps the RenderWidget.
// ---------------------------------------------------------------------------

// Mirror of crates/bif_qt/cpp/{layer_stack,property_inspector,scene_browser}_widget.cpp
// LAYER_PALETTE. TODO(tier2): consolidate into a shared header.
constexpr QColor EDIT_TARGET_PALETTE[8] = {
    QColor(80, 190, 180),
    QColor(180, 120, 220),
    QColor(230, 150, 70),
    QColor(220, 190, 80),
    QColor(230, 130, 180),
    QColor(90, 150, 230),
    QColor(120, 200, 100),
    QColor(220, 100, 100),
};

// Pull the current edit-target label + color from the shell state.
// Returns {"", gray} when nothing is set; caller picks whether to show.
struct EditTargetDisplay {
    QString name;
    QColor color;
    bool set;
};

EditTargetDisplay read_edit_target(BifShellState* state) {
    EditTargetDisplay d{};
    d.set = state->active_edit_target_is_set();
    if (!d.set) {
        d.color = QColor(90, 94, 102);
        return d;
    }
    d.name = state->active_edit_target_name();
    const int ci = state->active_edit_target_color_index();
    d.color = (ci >= 0 && ci < 8) ? EDIT_TARGET_PALETTE[ci]
                                  : QColor(90, 94, 102);
    return d;
}

// Build the edit-target chip. A colored dot + layer name, pill-shaped.
// `compact` shrinks padding + font for the status-bar variant.
QWidget* build_edit_target_chip(QWidget* parent, BifShellState* state,
                                bool compact) {
    auto* chip = new QWidget(parent);
    chip->setObjectName(compact ? QStringLiteral("edit_target_chip")
                                : QStringLiteral("edit_target_pill"));
    auto* layout = new QHBoxLayout(chip);
    const int pad_h = compact ? 6 : 10;
    const int pad_v = compact ? 2 : 4;
    layout->setContentsMargins(pad_h, pad_v, pad_h, pad_v);
    layout->setSpacing(compact ? 5 : 7);

    auto* dot = new QLabel(chip);
    const int dot_d = compact ? 8 : 10;
    dot->setFixedSize(dot_d, dot_d);
    dot->setObjectName(QStringLiteral("edit_target_dot"));
    layout->addWidget(dot);

    auto* label = new QLabel(chip);
    label->setObjectName(QStringLiteral("edit_target_label"));
    label->setStyleSheet(QString::fromLatin1(
        "color: rgba(220, 222, 226, 255); font-size: %1px;")
            .arg(compact ? 11 : 12));
    layout->addWidget(label);

    auto refresh = [chip, dot, label, state, compact, dot_d]() {
        const auto d = read_edit_target(state);
        if (!d.set) {
            chip->setVisible(false);
            return;
        }
        chip->setVisible(true);
        label->setText(QStringLiteral("Editing: %1").arg(d.name));
        const auto bg = QColor(d.color.red(), d.color.green(),
                               d.color.blue(), compact ? 40 : 55);
        chip->setStyleSheet(QString::fromLatin1(
            "QWidget#%1 { background-color: rgba(%2,%3,%4,%5);"
            " border: 1px solid rgba(%6,%7,%8,%9);"
            " border-radius: %10px; }")
                .arg(chip->objectName())
                .arg(bg.red()).arg(bg.green()).arg(bg.blue()).arg(bg.alpha())
                .arg(d.color.red()).arg(d.color.green()).arg(d.color.blue())
                .arg(compact ? 140 : 180)
                .arg(compact ? 9 : 11));
        dot->setStyleSheet(QString::fromLatin1(
            "background-color: rgba(%1,%2,%3,255);"
            " border-radius: %4px;")
                .arg(d.color.red()).arg(d.color.green()).arg(d.color.blue())
                .arg(dot_d / 2));
    };
    refresh();

    QObject::connect(state, &BifShellState::layer_state_revisionChanged,
                     chip, refresh);
    return chip;
}

// ---------------------------------------------------------------------------
// Breadcrumb (B.7) — placeholder bar above the central widget. Phase
// C populates segments from SceneLayerState / selection. For now,
// shows a single "(no stage)" segment.
// ---------------------------------------------------------------------------
QToolBar* build_breadcrumb_bar(QMainWindow* window) {
    auto* bar = new QToolBar(QStringLiteral("Breadcrumb"), window);
    bar->setObjectName(QStringLiteral("breadcrumb_bar"));
    bar->setMovable(false);
    bar->setFloatable(false);
    bar->setIconSize(QSize(0, 0));
    bar->setStyleSheet(QStringLiteral(
        "QToolBar {"
        "  background-color: rgba(34, 38, 44, 255);"
        "  border: none;"
        "  border-bottom: 1px solid rgba(20, 22, 26, 255);"
        "  padding: 4px 8px;"
        "  spacing: 0px;"
        "}"
        "QToolButton {"
        "  background-color: transparent;"
        "  color: rgba(140, 145, 155, 255);"
        "  border: none;"
        "  padding: 4px 6px;"
        "  font-size: 12px;"
        "}"
        "QToolButton:hover { color: rgba(220, 222, 226, 255); }"
        "QLabel { color: rgba(80, 85, 95, 255); padding: 0 2px; }"));

    bar->addAction(QStringLiteral("(no stage)"));
    return bar;
}

// Populate breadcrumb segments. Phase C calls this on prim selection
// changes; Phase B leaves it as a stub.
void breadcrumb_set_path(QToolBar* bar, const QStringList& segments) {
    bar->clear();
    if (segments.isEmpty()) {
        bar->addAction(QStringLiteral("(no stage)"));
        return;
    }
    for (int i = 0; i < segments.size(); ++i) {
        if (i > 0) {
            auto* sep = new QLabel(QStringLiteral(" › "), bar);
            bar->addWidget(sep);
        }
        bar->addAction(segments[i]);
    }
}

// ---------------------------------------------------------------------------
// Central area (B.6 + B.7) — wraps QToolBar + QStackedWidget. Stack
// index 0 = first-launch screen, 1 = wgpu viewport.
// ---------------------------------------------------------------------------
struct CentralArea {
    QWidget* container;
    QToolBar* breadcrumb;
    QWidget* edit_target_pill;
    QFrame* viewport_frame;
    QStackedWidget* stack;
    FirstLaunchWidget* first_launch;
    RenderWidget* viewport;
};

CentralArea build_central_area(QMainWindow* window, BifShellState* state) {
    CentralArea ca{};
    ca.container = new QWidget(window);
    ca.container->setObjectName(QStringLiteral("central_area"));

    auto* layout = new QVBoxLayout(ca.container);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    // Breadcrumb row: [breadcrumb_bar (stretch) | edit-target pill]. The
    // breadcrumb toolbar calls `->clear()` on every selection change,
    // so the pill can't share its action list — it lives alongside
    // instead.
    auto* breadcrumb_row = new QWidget(ca.container);
    breadcrumb_row->setObjectName(QStringLiteral("breadcrumb_row"));
    breadcrumb_row->setStyleSheet(QStringLiteral(
        "QWidget#breadcrumb_row {"
        "  background-color: rgba(34, 38, 44, 255);"
        "  border-bottom: 1px solid rgba(20, 22, 26, 255);"
        "}"));
    auto* row_layout = new QHBoxLayout(breadcrumb_row);
    row_layout->setContentsMargins(0, 0, 8, 0);
    row_layout->setSpacing(0);

    ca.breadcrumb = build_breadcrumb_bar(window);
    ca.breadcrumb->setParent(breadcrumb_row);
    row_layout->addWidget(ca.breadcrumb, 1);

    // Camera picker — QComboBox that lists Free / ortho presets / USD cameras.
    // Repopulates when `camera_list_revision` changes (stage load/close).
    auto* camera_picker = new QComboBox(breadcrumb_row);
    camera_picker->setObjectName(QStringLiteral("camera_picker"));
    camera_picker->setMinimumWidth(130);
    camera_picker->setMaximumWidth(180);
    // Styling inherited from theme.rs `QComboBox` rule (Graphite §2).

    auto populate_camera_picker = [=]() {
        camera_picker->blockSignals(true);
        camera_picker->clear();
        const auto choices = collect_camera_choices(state);
        for (const auto& choice : choices) {
            if (choice.separator_before) {
                camera_picker->insertSeparator(camera_picker->count());
            }
            camera_picker->addItem(choice.label, choice.key);
        }
        // Restore active selection
        sync_camera_picker_selection(camera_picker, state->active_camera_name());
        camera_picker->blockSignals(false);
    };

    populate_camera_picker();

    QObject::connect(state, &BifShellState::camera_list_revisionChanged,
                     camera_picker, populate_camera_picker);

    QObject::connect(camera_picker,
                     QOverload<int>::of(&QComboBox::currentIndexChanged),
                     state, [=](int index) {
                         auto key = camera_picker->itemData(index).toString();
                         if (!key.isEmpty())
                             state->on_select_camera(key);
                     });

    row_layout->addWidget(camera_picker, 0);

    // AOV preview picker — QComboBox listing AovChannel::all() entries.
    // Drives `BifShellState::preview_aov` via `on_select_preview_aov`.
    // Output appears on the next 16ms viewport tick — no repaint signal needed.
    auto* aov_picker = new QComboBox(breadcrumb_row);
    aov_picker->setObjectName(QStringLiteral("aov_picker"));
    aov_picker->setMinimumWidth(130);
    aov_picker->setMaximumWidth(180);
    aov_picker->setToolTip(QStringLiteral("AOV channel shown in the render view"));
    // Styling inherited from theme.rs `QComboBox` rule (Graphite §2).

    {
        aov_picker->blockSignals(true);
        const int count = state->preview_aov_count();
        for (int i = 0; i < count; ++i) {
            aov_picker->addItem(state->preview_aov_name_at(i));
        }
        aov_picker->setCurrentIndex(state->active_preview_aov_index());
        aov_picker->blockSignals(false);
    }

    QObject::connect(aov_picker,
                     QOverload<int>::of(&QComboBox::currentIndexChanged),
                     state, [=](int index) {
                         state->on_select_preview_aov(index);
                     });

    row_layout->addWidget(aov_picker, 0);

    // Render-mode picker — flips IvarState::mode between Vulkan rasterizer and
    // Ivar overlay. The "Ivar Render" button in render_settings still triggers
    // a fresh batch; this combo just controls which path is on screen.
    auto* mode_picker = new QComboBox(breadcrumb_row);
    mode_picker->setObjectName(QStringLiteral("mode_picker"));
    mode_picker->setMinimumWidth(120);
    mode_picker->setMaximumWidth(160);
    mode_picker->setToolTip(QStringLiteral("Render mode shown in the viewport"));
    // Styling inherited from theme.rs `QComboBox` rule (Graphite §2).

    {
        mode_picker->blockSignals(true);
        const int count = state->render_mode_count();
        for (int i = 0; i < count; ++i) {
            mode_picker->addItem(state->render_mode_name_at(i));
        }
        mode_picker->setCurrentIndex(state->active_render_mode_index());
        mode_picker->blockSignals(false);
    }

    QObject::connect(mode_picker,
                     QOverload<int>::of(&QComboBox::currentIndexChanged),
                     state, [=](int index) {
                         state->on_select_render_mode(index);
                     });

    row_layout->addWidget(mode_picker, 0);

    // Sky-gradient toggle — flips IvarState::use_sky_gradient. When off, the
    // Ivar path tracer returns solid background instead of the white→blue
    // gradient (bif_renderer::sky_gradient).
    auto* sky_toggle = new QCheckBox(QStringLiteral("Sky Gradient"), breadcrumb_row);
    sky_toggle->setObjectName(QStringLiteral("sky_toggle"));
    sky_toggle->setToolTip(QStringLiteral(
        "Blue sky-gradient background for the Ivar path tracer (when no HDRI)"));
    sky_toggle->setStyleSheet(QStringLiteral(
        "QCheckBox#sky_toggle {"
        "  color: rgba(180, 185, 195, 255);"
        "  font-size: 11px;"
        "  padding: 2px 6px;"
        "}"
        "QCheckBox#sky_toggle::indicator {"
        "  width: 12px; height: 12px;"
        "  border: 1px solid rgba(85, 92, 105, 255);"
        "  border-radius: 2px;"
        "  background-color: rgba(34, 38, 44, 200);"
        "}"
        "QCheckBox#sky_toggle::indicator:checked {"
        "  background-color: rgba(120, 165, 220, 220);"
        "  border-color: rgba(150, 190, 230, 255);"
        "}"));
    sky_toggle->setChecked(state->sky_gradient_enabled());

    QObject::connect(sky_toggle, &QCheckBox::toggled, state,
                     [=](bool checked) { state->on_set_sky_gradient_enabled(checked); });

    row_layout->addWidget(sky_toggle, 0);

    ca.edit_target_pill = build_edit_target_chip(breadcrumb_row, state, /*compact=*/false);
    row_layout->addWidget(ca.edit_target_pill, 0);

    layout->addWidget(breadcrumb_row);

    // Viewport tint frame — a thin coloured inset around the stacked
    // viewport. Shares the edit-target color via the same refresh hook.
    ca.viewport_frame = new QFrame(ca.container);
    ca.viewport_frame->setObjectName(QStringLiteral("viewport_edge_tint"));
    ca.viewport_frame->setFrameShape(QFrame::NoFrame);
    auto* frame_layout = new QVBoxLayout(ca.viewport_frame);
    frame_layout->setContentsMargins(2, 2, 2, 2);
    frame_layout->setSpacing(0);

    ca.stack = new QStackedWidget(ca.viewport_frame);
    ca.stack->setObjectName(QStringLiteral("central_stack"));
    frame_layout->addWidget(ca.stack, 1);

    layout->addWidget(ca.viewport_frame, 1);

    ca.first_launch = new FirstLaunchWidget(ca.stack);
    ca.viewport = new RenderWidget(ca.stack);
    ca.viewport->setObjectName(QStringLiteral("viewport"));

    ca.stack->addWidget(ca.first_launch);  // index 0
    ca.stack->addWidget(ca.viewport);      // index 1
    ca.stack->setCurrentIndex(0);

    // Edge-tint stylesheet follows the edit-target color.
    auto refresh_tint = [frame = ca.viewport_frame, state]() {
        const auto d = read_edit_target(state);
        if (!d.set) {
            frame->setStyleSheet(QStringLiteral(
                "QFrame#viewport_edge_tint { background-color: transparent; }"));
            return;
        }
        frame->setStyleSheet(QString::fromLatin1(
            "QFrame#viewport_edge_tint {"
            "  background-color: rgba(%1,%2,%3,255);"
            "}")
                .arg(d.color.red()).arg(d.color.green()).arg(d.color.blue()));
    };
    refresh_tint();
    QObject::connect(state, &BifShellState::layer_state_revisionChanged,
                     ca.viewport_frame, refresh_tint);

    return ca;
}

// ---------------------------------------------------------------------------
// Action wiring
// ---------------------------------------------------------------------------

// Shared helper: drive a USD stage load from whichever UI surface
// requested it (menu bar, first-launch screen, recent-stage click).
//
// Pauses the viewport's 16ms render tick around the file picker so the
// native Windows dialog doesn't fight the wgpu paint loop for z-order
// (previously worked around via window->setVisible(false/true) which
// looked awful — the whole app briefly vanished).
//
// `pre_picked_path` short-circuits the picker (used by Recent Stages).
// Empty path = show the native QFileDialog.
static void trigger_open_stage(
    QMainWindow* window,
    BifShellState* shell_state,
    QStackedWidget* central_stack,
    RenderWidget* viewport,
    const QString& pre_picked_path = QString()) {

    auto update_status = [shell_state, window]() {
        window->statusBar()->showMessage(shell_state->getStatus_message());
    };

    QString path = pre_picked_path;
    if (path.isEmpty()) {
        // Pause the viewport render tick — fixes the native-dialog
        // z-order fight caused by 60fps paint events demanding the
        // main-window foreground.
        if (viewport) viewport->pausePainting();
        path = QFileDialog::getOpenFileName(
            window,
            QStringLiteral("Open USD Stage"),
            QString(),
            QStringLiteral("USD files (*.usd *.usda *.usdc *.usdz);;All files (*)"));
        if (viewport) viewport->resumePainting();
    }

    if (path.isEmpty()) {
        // User cancelled — leave the central stack wherever it was.
        return;
    }

    // Record in QSettings recents for the first-launch list.
    QSettings settings;
    auto recents = settings.value(QStringLiteral("recent_stages")).toStringList();
    recents.removeAll(path);
    recents.prepend(path);
    while (recents.size() > 10) recents.removeLast();
    settings.setValue(QStringLiteral("recent_stages"), recents);

    // Swap to viewport FIRST so the RenderWidget actually becomes
    // visible (and fires surfaceReady) before the stage load runs —
    // otherwise with_viewport_mut returns None and the load no-ops.
    central_stack->setCurrentIndex(1);

    shell_state->on_stage_path_opened(path);
    update_status();
}

class StageDropFilter : public QObject {
public:
    StageDropFilter(
        QMainWindow* window,
        BifShellState* shell_state,
        QStackedWidget* central_stack,
        RenderWidget* viewport)
        : QObject(window),
          m_window(window),
          m_shell_state(shell_state),
          m_central_stack(central_stack),
          m_viewport(viewport) {}

protected:
    bool eventFilter(QObject* watched, QEvent* event) override {
        auto* target = qobject_cast<QWidget*>(watched);
        if (!target || !m_window || !m_shell_state || !m_central_stack) {
            return QObject::eventFilter(watched, event);
        }
        if (target != m_window && !m_window->isAncestorOf(target)) {
            return QObject::eventFilter(watched, event);
        }

        switch (event->type()) {
        case QEvent::DragEnter: {
            auto* drag = static_cast<QDragEnterEvent*>(event);
            if (first_supported_stage_path(drag->mimeData()).isEmpty()) {
                return QObject::eventFilter(watched, event);
            }
            drag->acceptProposedAction();
            return true;
        }
        case QEvent::DragMove: {
            auto* drag = static_cast<QDragMoveEvent*>(event);
            if (first_supported_stage_path(drag->mimeData()).isEmpty()) {
                return QObject::eventFilter(watched, event);
            }
            drag->acceptProposedAction();
            return true;
        }
        case QEvent::Drop: {
            auto* drop = static_cast<QDropEvent*>(event);
            const auto path = first_supported_stage_path(drop->mimeData());
            if (path.isEmpty()) {
                return QObject::eventFilter(watched, event);
            }
            drop->acceptProposedAction();
            trigger_open_stage(
                m_window,
                m_shell_state,
                m_central_stack,
                m_viewport,
                path);
            return true;
        }
        default:
            break;
        }
        return QObject::eventFilter(watched, event);
    }

private:
    QMainWindow* m_window;
    BifShellState* m_shell_state;
    QStackedWidget* m_central_stack;
    RenderWidget* m_viewport;
};

void wire_shell_actions(
    MenuActions& actions,
    BifShellState* shell_state,
    QMainWindow* window,
    QStackedWidget* central_stack,
    RenderWidget* viewport) {
    auto update_status = [shell_state, window]() {
        window->statusBar()->showMessage(shell_state->getStatus_message());
    };
    auto* undo_action = actions.undo;
    auto* redo_action = actions.redo;
    auto refresh_edit_actions = [shell_state, undo_action, redo_action]() {
        undo_action->setEnabled(shell_state->getCan_undo());
        redo_action->setEnabled(shell_state->getCan_redo());
    };
    QObject::connect(shell_state, &BifShellState::can_undoChanged,
                     window, refresh_edit_actions);
    QObject::connect(shell_state, &BifShellState::can_redoChanged,
                     window, refresh_edit_actions);
    refresh_edit_actions();

    auto* camera_picker = window->findChild<QComboBox*>(QStringLiteral("camera_picker"));
    auto* look_through_menu = actions.look_through;
    auto* ortho_action = actions.toggle_orthographic;
    auto sync_camera_surfaces = [camera_picker, look_through_menu, ortho_action, shell_state]() {
        const auto active = shell_state->active_camera_name();
        if (camera_picker) {
            camera_picker->blockSignals(true);
            sync_camera_picker_selection(camera_picker, active);
            camera_picker->blockSignals(false);
        }
        if (look_through_menu) {
            for (auto* action : look_through_menu->actions()) {
                if (action->isSeparator()) continue;
                action->setChecked(action->data().toString() == active);
            }
        }
        ortho_action->blockSignals(true);
        ortho_action->setChecked(is_ortho_camera_source(active));
        ortho_action->blockSignals(false);
    };
    auto repopulate_look_through_menu =
        [window, look_through_menu, shell_state, sync_camera_surfaces]() {
            look_through_menu->clear();
            const auto choices = collect_camera_choices(shell_state);
            for (const auto& choice : choices) {
                if (choice.separator_before) {
                    look_through_menu->addSeparator();
                }
                auto* action = look_through_menu->addAction(choice.label);
                action->setData(choice.key);
                action->setCheckable(true);
                const auto key = choice.key;
                QObject::connect(action, &QAction::triggered, window,
                    [shell_state, sync_camera_surfaces, key]() {
                        shell_state->on_select_camera(key);
                        sync_camera_surfaces();
                    });
            }
            sync_camera_surfaces();
        };
    QObject::connect(shell_state, &BifShellState::camera_list_revisionChanged,
                     window, repopulate_look_through_menu);
    if (camera_picker) {
        QObject::connect(camera_picker,
                         QOverload<int>::of(&QComboBox::currentIndexChanged),
                         window,
                         [sync_camera_surfaces](int) { sync_camera_surfaces(); });
    }
    repopulate_look_through_menu();

    auto new_stage_flow = [shell_state, central_stack, update_status]() {
        shell_state->on_new_stage();
        central_stack->setCurrentIndex(1);
        update_status();
    };

    QObject::connect(actions.new_stage, &QAction::triggered, window, new_stage_flow);
    // File → Open Stage — shared helper.
    QObject::connect(actions.open_stage, &QAction::triggered, window,
        [window, shell_state, central_stack, viewport]() {
            trigger_open_stage(window, shell_state, central_stack, viewport);
        });
    // File → Close Stage — reset renderer, clear state, return to first-launch.
    QObject::connect(actions.close_stage, &QAction::triggered, window,
        [shell_state, central_stack, viewport, update_status]() {
            // Pause render tick during teardown so the 16ms paint loop
            // can't fire against a half-reset scene.
            if (viewport) viewport->pausePainting();
            shell_state->close_stage();
            if (viewport) viewport->resumePainting();
            central_stack->setCurrentIndex(0);
            update_status();
        });
    QObject::connect(actions.save, &QAction::triggered, window,
        [shell_state, update_status]() {
            shell_state->on_save();
            update_status();
        });
    QObject::connect(actions.save_as, &QAction::triggered, window,
        [shell_state, window, viewport, update_status]() {
            // No stage / no edit target → status-only fallback. Avoids
            // showing a useless dialog the user can only cancel out of.
            if (!shell_state->active_edit_target_is_set()) {
                shell_state->on_save_as();
                update_status();
                return;
            }

            // Pre-fill with the current working-layer identifier so the
            // native dialog opens to the right directory + filename. User
            // can rename in place.
            QString initial = shell_state->active_edit_target_identifier();

            // Pause the viewport render tick around the native dialog —
            // same fix as trigger_open_stage() (wgpu paint loop vs. z-order).
            if (viewport) viewport->pausePainting();
            QString path = QFileDialog::getSaveFileName(
                window,
                QStringLiteral("Save USD Stage As"),
                initial,
                QStringLiteral("USD ASCII (*.usda);;All files (*)"));
            if (viewport) viewport->resumePainting();

            if (path.isEmpty()) {
                // User cancelled — leave status bar untouched.
                return;
            }
            shell_state->on_save_as_to_path(path);
            update_status();
        });
    QObject::connect(actions.undo, &QAction::triggered, window,
        [shell_state, update_status]() {
            shell_state->on_undo();
            update_status();
        });
    QObject::connect(actions.redo, &QAction::triggered, window,
        [shell_state, update_status]() {
            shell_state->on_redo();
            update_status();
        });
    QObject::connect(actions.ivar_render, &QAction::triggered, window,
        [shell_state, update_status]() {
            shell_state->on_start_ivar_render();
            update_status();
        });
    QObject::connect(actions.exit_app, &QAction::triggered,
        qApp, &QCoreApplication::quit);

    QObject::connect(actions.workspace_assembly, &QAction::triggered, window,
        [window, shell_state]() { ws::switch_to(window, shell_state, QLatin1String(ws::ASSEMBLY)); });
    QObject::connect(actions.workspace_lighting, &QAction::triggered, window,
        [window, shell_state]() { ws::switch_to(window, shell_state, QLatin1String(ws::LIGHTING)); });
    QObject::connect(actions.workspace_materials, &QAction::triggered, window,
        [window, shell_state]() { ws::switch_to(window, shell_state, QLatin1String(ws::MATERIALS)); });
    QObject::connect(actions.workspace_review, &QAction::triggered, window,
        [window, shell_state]() { ws::switch_to(window, shell_state, QLatin1String(ws::REVIEW)); });
    QObject::connect(actions.toggle_orthographic, &QAction::toggled, window,
        [shell_state, sync_camera_surfaces](bool enabled) {
            const auto active = shell_state->active_camera_name();
            if (enabled) {
                if (!is_ortho_camera_source(active)) {
                    shell_state->on_select_camera(QStringLiteral("ortho:Top"));
                }
            } else if (is_ortho_camera_source(active)) {
                shell_state->on_select_camera(QStringLiteral("free"));
            }
            sync_camera_surfaces();
        });

    QObject::connect(actions.zen_mode, &QAction::toggled, window,
        [window, shell_state, update_status](bool zen) {
            const auto docks = window->findChildren<QDockWidget*>();
            for (auto* dock : docks) {
                dock->setVisible(!zen);
            }
            shell_state->setStatus_message(zen
                ? QStringLiteral("Zen mode: ON — docks hidden, Ctrl+\\ to restore")
                : QStringLiteral("Zen mode: OFF — docks restored"));
            update_status();
        });

    QObject::connect(actions.toggle_lod, &QAction::toggled, window,
        [shell_state, update_status](bool enabled) {
            shell_state->on_set_lod_enabled(enabled);
            update_status();
        });
    QObject::connect(actions.toggle_grid, &QAction::toggled, window,
        [shell_state, update_status](bool visible) {
            shell_state->on_set_grid_visible(visible);
            update_status();
        });
    QObject::connect(actions.toggle_node_graph_experimental, &QAction::toggled, window,
        [window, shell_state, update_status](bool enabled) {
            set_node_graph_preview_enabled(window, enabled);
            auto* node_graph =
                window->findChild<QDockWidget*>(QStringLiteral("dock_node_graph"));
            if (node_graph) {
                if (enabled) {
                    node_graph->show();
                    node_graph->raise();
                } else {
                    node_graph->hide();
                }
            }
            shell_state->setStatus_message(enabled
                ? QStringLiteral("Node graph preview: ON")
                : QStringLiteral("Node graph preview: OFF"));
            update_status();
        });

    // ── View ▸ Panels: bidirectional action ↔ dock visibility sync ──
    // Each entry binds a checkable QAction to a QDockWidget so that:
    //   - toggling the action shows/hides + raises the dock
    //   - closing the dock via its X uncheck the action (visibilityChanged)
    //   - workspace switches that call apply_default_layout propagate to the
    //     menu state automatically (also through visibilityChanged).
    struct PanelBinding {
        QAction* action;
        const char* dock_name;
        const char* label;
    };
    const PanelBinding panel_bindings[] = {
        {actions.panel_scene_browser,       "dock_scene_browser",       "Scene Browser"},
        {actions.panel_layer_stack,         "dock_layer_stack",         "Layer Stack"},
        {actions.panel_property_inspector,  "dock_property_inspector",  "Property Inspector"},
        {actions.panel_render_settings,     "dock_render_settings",     "Render Settings"},
        {actions.panel_collection_editor,   "dock_collection_editor",   "Collection Editor"},
        {actions.panel_timeline,            "dock_timeline",            "Timeline"},
        {actions.toggle_usda_source,        "dock_usda_source",         "USDA Source"},
    };
    // action → dock (lazy findChild inside lambda — survives ordering)
    for (const auto& b : panel_bindings) {
        QObject::connect(b.action, &QAction::toggled, window,
            [window, shell_state, update_status, dn = b.dock_name, lbl = b.label](bool on) {
                auto* dock = window->findChild<QDockWidget*>(QString::fromLatin1(dn));
                if (dock) {
                    if (on) { dock->show(); dock->raise(); }
                    else    { dock->hide(); }
                }
                shell_state->setStatus_message(
                    QStringLiteral("%1: %2").arg(QString::fromLatin1(lbl),
                                                 on ? QStringLiteral("ON") : QStringLiteral("OFF")));
                update_status();
            });
    }

    // dock → action wiring + initial-state seeding. Deferred to the next
    // event-loop tick because the docks are created later in build_shell
    // (line ~1388+). At this point in the call site, findChild() would
    // return nullptr.
    QTimer::singleShot(0, window,
        [window, panel_bindings = std::vector(std::begin(panel_bindings), std::end(panel_bindings)),
         ng_action = actions.toggle_node_graph_experimental]() {
            for (const auto& b : panel_bindings) {
                auto* dock = window->findChild<QDockWidget*>(QString::fromLatin1(b.dock_name));
                if (!dock) continue;
                QObject::connect(dock, &QDockWidget::visibilityChanged, b.action,
                    [act = b.action](bool visible) {
                        if (act->isChecked() != visible) {
                            QSignalBlocker block(act);
                            act->setChecked(visible);
                        }
                    });
                QSignalBlocker block(b.action);
                b.action->setChecked(dock->isVisible());
            }
            // Node Graph (Experimental) keeps its own toggled-handler (sets the
            // preview gate) — only add the back-direction sync here.
            if (auto* ng_dock = window->findChild<QDockWidget*>(QStringLiteral("dock_node_graph"))) {
                QObject::connect(ng_dock, &QDockWidget::visibilityChanged, ng_action,
                    [ng_action](bool visible) {
                        if (ng_action->isChecked() != visible) {
                            QSignalBlocker block(ng_action);
                            ng_action->setChecked(visible);
                        }
                    });
            }
        });

    // View ▸ Reset Workspace Layout — restore default dock arrangement.
    QObject::connect(actions.reset_workspace_layout, &QAction::triggered, window,
        [window, shell_state, update_status]() {
            const QString ws = shell_state->getCurrent_workspace();
            const QString display = ws::display_name(ws);
            const auto answer = QMessageBox::question(
                window,
                QStringLiteral("Reset Workspace Layout"),
                QStringLiteral(
                    "Reset the %1 workspace to its default dock layout? "
                    "Your saved layout for this workspace will be lost.").arg(display),
                QMessageBox::Yes | QMessageBox::No,
                QMessageBox::No);
            if (answer != QMessageBox::Yes) return;
            QSettings settings;
            settings.remove(ws::state_key(ws::canonical_name(ws)));
            ws::apply_default_layout(window, ws);
            shell_state->setStatus_message(
                QStringLiteral("Reset %1 layout.").arg(display));
            update_status();
        });

    QObject::connect(actions.about, &QAction::triggered, window,
        [shell_state, window, update_status]() {
            // Status bar gets the short "BIF x.y.z — ..." line; the modal
            // QMessageBox carries the richer body with version + repo link.
            shell_state->on_about();
            QMessageBox::about(
                window,
                QStringLiteral("About BIF"),
                shell_state->about_dialog_body());
            update_status();
        });
}

// First-launch screen → action plumbing. Clicking the cards routes
// through the same invokables as the menu, then flips the central
// stack to the viewport.
void wire_first_launch(
    FirstLaunchWidget* first_launch,
    BifShellState* shell_state,
    QStackedWidget* central_stack,
    RenderWidget* viewport,
    QMainWindow* window) {
    auto update_status = [shell_state, window]() {
        window->statusBar()->showMessage(shell_state->getStatus_message());
    };

    QObject::connect(first_launch, &FirstLaunchWidget::newStageClicked, window,
        [shell_state, central_stack, update_status]() {
            shell_state->on_new_stage();
            central_stack->setCurrentIndex(1);
            update_status();
        });
    // Open Stage from the first-launch screen → same flow as File → Open.
    QObject::connect(first_launch, &FirstLaunchWidget::openStageClicked, window,
        [window, shell_state, central_stack, viewport]() {
            trigger_open_stage(window, shell_state, central_stack, viewport);
        });
    // Recent-stage click → load directly without picker.
    QObject::connect(first_launch, &FirstLaunchWidget::recentStageActivated, window,
        [window, shell_state, central_stack, viewport](const QString& path) {
            trigger_open_stage(window, shell_state, central_stack, viewport, path);
        });
}

void connect_viewport_signals(
    RenderWidget* viewport,
    ViewportCallbacks* cb,
    BifShellState* shell_state,
    QMainWindow* window) {
    QObject::connect(
        viewport, &RenderWidget::surfaceReady, viewport,
        [viewport, cb, shell_state, window]() {
            const auto hwnd = viewport->nativeWinId();
            const auto hinst = viewport->nativeHInstance();
            const auto w = viewport->pixelWidth();
            const auto h = viewport->pixelHeight();
            const auto scale = static_cast<float>(viewport->devicePixelRatioF());
            const auto ok = viewport_on_surface_ready(*cb, hwnd, hinst, w, h, scale);
            shell_state->setStatus_message(ok
                ? QStringLiteral("wgpu viewport live")
                : QStringLiteral("FAILED to init wgpu viewport — see stderr"));
            shell_state->sync_ivar_status();
            window->statusBar()->showMessage(shell_state->getStatus_message());
        });

    QObject::connect(
        viewport, &RenderWidget::resized, viewport,
        [cb, viewport](int w, int h) {
            const auto scale = static_cast<float>(viewport->devicePixelRatioF());
            viewport_on_resize(*cb, w, h, scale);
        });

    QObject::connect(
        viewport, &RenderWidget::frameRequested, viewport,
        [cb, shell_state]() {
            viewport_on_frame(*cb);
            shell_state->sync_undo_redo_state();
            shell_state->sync_ivar_status();
        });

    // Camera + selection input — Phase E.1 routes deltas to
    // BifShellState invokables that update the status bar. Phase E.2
    // dispatches real AppEvent::Camera* to the bif_renderer Renderer.
    auto update_status = [shell_state, window]() {
        window->statusBar()->showMessage(shell_state->getStatus_message());
    };

    QObject::connect(
        viewport, &RenderWidget::cameraOrbit, shell_state,
        [shell_state, update_status](int dx, int dy) {
            shell_state->on_camera_orbit(dx, dy);
            update_status();
        });
    QObject::connect(
        viewport, &RenderWidget::cameraPan, shell_state,
        [shell_state, update_status](int dx, int dy) {
            shell_state->on_camera_pan(dx, dy);
            update_status();
        });
    QObject::connect(
        viewport, &RenderWidget::cameraZoom, shell_state,
        [shell_state, update_status](int d) {
            shell_state->on_camera_zoom(d);
            update_status();
        });
    QObject::connect(
        viewport, &RenderWidget::primPickRequested, shell_state,
        [shell_state, update_status](int x, int y) {
            shell_state->on_prim_pick(x, y);
            update_status();
        });
    QObject::connect(
        viewport, &RenderWidget::transformGizmoMoved, shell_state,
        [shell_state](int x, int y) {
            shell_state->on_transform_gizmo_move(x, y);
        });
    QObject::connect(
        viewport, &RenderWidget::transformGizmoReleased, shell_state,
        [shell_state, update_status](int x, int y) {
            if (shell_state->on_transform_gizmo_release(x, y)) {
                update_status();
            }
        });
}

}  // namespace

int bif_qt_run_shell(ViewportCallbacks* viewport_cb, ::rust::Str stylesheet) {
    static char arg0[] = "bif_qt_shell";
    static char* argv_storage[] = {arg0, nullptr};
    static int argc = 1;

    QApplication app(argc, argv_storage);
    app.setApplicationName(QStringLiteral("BIF"));
    app.setApplicationDisplayName(QStringLiteral("BIF — USD Orchestration"));
    app.setOrganizationName(QStringLiteral("BIF"));

    if (!stylesheet.empty()) {
        app.setStyleSheet(QString::fromUtf8(
            stylesheet.data(),
            static_cast<int>(stylesheet.size())));
    }

    QMainWindow window;
    window.setObjectName(QStringLiteral("bif_main_window"));
    window.setWindowTitle(QStringLiteral("BIF — USD Orchestration (Qt, Phase B shell)"));
    window.resize(1440, 900);

    auto* shell_state = new BifShellState(&window);
    window.setProperty("bif_shell_state", QVariant::fromValue(shell_state));

    auto menu_actions = build_menu_bar(&window);

    // Central area: breadcrumb toolbar + QStackedWidget(first-launch | viewport).
    auto central = build_central_area(&window, shell_state);
    window.setCentralWidget(central.container);
    window.setAcceptDrops(true);
    central.container->setAcceptDrops(true);
    central.viewport_frame->setAcceptDrops(true);
    central.stack->setAcceptDrops(true);
    central.first_launch->setAcceptDrops(true);
    central.viewport->setAcceptDrops(true);
    auto* stage_drop_filter = new StageDropFilter(
        &window, shell_state, central.stack, central.viewport);
    app.installEventFilter(stage_drop_filter);

    wire_shell_actions(menu_actions, shell_state, &window, central.stack, central.viewport);
    wire_first_launch(central.first_launch, shell_state, central.stack, central.viewport, &window);
    if (viewport_cb != nullptr) {
        connect_viewport_signals(central.viewport, viewport_cb, shell_state, &window);
    }

    // Breadcrumb bar ← selected_prim_path (Phase E.2 move 6) + layer
    // segment (Tier 1 item #6). Format:
    //   stage_name > layer_name (edit) > prim > path > segments
    // Refreshes on both `selected_prim_pathChanged` and
    // `layer_state_revisionChanged` so the edit-target segment stays
    // in sync with the Layer Stack panel's working-layer radio.
    auto refresh_breadcrumb = [breadcrumb = central.breadcrumb, shell_state]() {
        QStringList segments;
        const auto stage_name = shell_state->current_stage_display();
        if (!stage_name.isEmpty()) {
            segments << stage_name;
        }
        if (shell_state->active_edit_target_is_set()) {
            segments << QStringLiteral("%1 (edit)")
                .arg(shell_state->active_edit_target_name());
        }
        const auto prim_path = shell_state->getSelected_prim_path();
        if (!prim_path.isEmpty()) {
            segments.append(prim_path.split(QChar('/'), Qt::SkipEmptyParts));
        }
        breadcrumb_set_path(breadcrumb, segments);
    };
    // Stage path itself has no dedicated signal (plain Rust field, not
    // a qproperty); `layer_state_revisionChanged` fires right after
    // `scene_layer_state` populates on successful load, so it covers
    // the stage-name-appearance case too.
    QObject::connect(shell_state, &BifShellState::selected_prim_pathChanged,
                     &window, refresh_breadcrumb);
    QObject::connect(shell_state, &BifShellState::layer_state_revisionChanged,
                     &window, refresh_breadcrumb);
    refresh_breadcrumb();

    // Scene Browser dock — real panel (Phase C.2). Demo prim tree
    // until Phase E wires CompositeProvider.
    {
        auto* dock = new QDockWidget(QStringLiteral("Scene Browser"), &window);
        dock->setObjectName(QStringLiteral("dock_scene_browser"));
        dock->setAllowedAreas(Qt::AllDockWidgetAreas);
        dock->setFeatures(
            QDockWidget::DockWidgetMovable |
            QDockWidget::DockWidgetFloatable |
            QDockWidget::DockWidgetClosable);
        auto* panel = new SceneBrowserWidget(shell_state, dock);
        dock->setWidget(panel);
        window.addDockWidget(Qt::LeftDockWidgetArea, dock);
    }
    // Layer Stack dock — real panel (Phase C.1). Other docks stay
    // placeholders until their phase lands.
    {
        auto* dock = new QDockWidget(QStringLiteral("Layer Stack"), &window);
        dock->setObjectName(QStringLiteral("dock_layer_stack"));
        dock->setAllowedAreas(Qt::AllDockWidgetAreas);
        dock->setFeatures(
            QDockWidget::DockWidgetMovable |
            QDockWidget::DockWidgetFloatable |
            QDockWidget::DockWidgetClosable);
        auto* panel = new LayerStackWidget(shell_state, dock);
        dock->setWidget(panel);
        window.addDockWidget(Qt::LeftDockWidgetArea, dock);
    }
    // Property Inspector dock — real panel (Phase C.3). Fake
    // attributes until Phase E wires UsdPrim::GetAttributes.
    {
        auto* dock = new QDockWidget(QStringLiteral("Property Inspector"), &window);
        dock->setObjectName(QStringLiteral("dock_property_inspector"));
        dock->setAllowedAreas(Qt::AllDockWidgetAreas);
        dock->setFeatures(
            QDockWidget::DockWidgetMovable |
            QDockWidget::DockWidgetFloatable |
            QDockWidget::DockWidgetClosable);
        auto* panel = new PropertyInspectorWidget(shell_state, dock);
        dock->setWidget(panel);
        window.addDockWidget(Qt::RightDockWidgetArea, dock);
    }
    // Bottom area: Node Graph and Timeline (Phase D.1) tabified together.
    QDockWidget* node_graph_dock = nullptr;
    QDockWidget* timeline_dock = nullptr;
    {
        node_graph_dock = new QDockWidget(QStringLiteral("Node Graph"), &window);
        node_graph_dock->setObjectName(QStringLiteral("dock_node_graph"));
        node_graph_dock->setAllowedAreas(Qt::AllDockWidgetAreas);
        node_graph_dock->setFeatures(
            QDockWidget::DockWidgetMovable |
            QDockWidget::DockWidgetFloatable |
            QDockWidget::DockWidgetClosable);
        auto* panel = new NodeGraphWidget(shell_state, node_graph_dock);
        node_graph_dock->setWidget(panel);
        window.addDockWidget(Qt::BottomDockWidgetArea, node_graph_dock);
    }
    {
        timeline_dock = new QDockWidget(QStringLiteral("Timeline"), &window);
        timeline_dock->setObjectName(QStringLiteral("dock_timeline"));
        timeline_dock->setAllowedAreas(Qt::AllDockWidgetAreas);
        timeline_dock->setFeatures(
            QDockWidget::DockWidgetMovable |
            QDockWidget::DockWidgetFloatable |
            QDockWidget::DockWidgetClosable);
        auto* panel = new TimelineWidget(shell_state, timeline_dock);
        timeline_dock->setWidget(panel);
        window.addDockWidget(Qt::BottomDockWidgetArea, timeline_dock);
        window.tabifyDockWidget(node_graph_dock, timeline_dock);
    }

    // Render Settings dock — tabified with Property Inspector on the right.
    {
        auto* prop_dock = window.findChild<QDockWidget*>(
            QStringLiteral("dock_property_inspector"));
        auto* dock = new QDockWidget(QStringLiteral("Render Settings"), &window);
        dock->setObjectName(QStringLiteral("dock_render_settings"));
        dock->setAllowedAreas(Qt::AllDockWidgetAreas);
        dock->setFeatures(
            QDockWidget::DockWidgetMovable |
            QDockWidget::DockWidgetFloatable |
            QDockWidget::DockWidgetClosable);
        auto* panel = new RenderSettingsWidget(shell_state, dock);
        dock->setWidget(panel);
        window.addDockWidget(Qt::RightDockWidgetArea, dock);
        if (prop_dock) {
            window.tabifyDockWidget(prop_dock, dock);
            prop_dock->raise();
        }
    }

    // USDA Source dock (C4b-2) — tabified with Node Graph at the
    // bottom; hidden by default. View → USDA Source toggles it.
    {
        auto* dock = new QDockWidget(QStringLiteral("USDA Source"), &window);
        dock->setObjectName(QStringLiteral("dock_usda_source"));
        dock->setAllowedAreas(Qt::AllDockWidgetAreas);
        dock->setFeatures(
            QDockWidget::DockWidgetMovable |
            QDockWidget::DockWidgetFloatable |
            QDockWidget::DockWidgetClosable);
        auto* panel = new UsdaPanelWidget(shell_state, dock);
        dock->setWidget(panel);
        window.addDockWidget(Qt::BottomDockWidgetArea, dock);
        if (node_graph_dock) {
            window.tabifyDockWidget(node_graph_dock, dock);
        }
        dock->hide();
    }

    // Collection Editor dock (v0.16.5) — tabified with Property
    // Inspector on the right.
    {
        auto* prop_dock = window.findChild<QDockWidget*>(
            QStringLiteral("dock_property_inspector"));
        auto* dock = new QDockWidget(QStringLiteral("Collection Editor"), &window);
        dock->setObjectName(QStringLiteral("dock_collection_editor"));
        dock->setAllowedAreas(Qt::AllDockWidgetAreas);
        dock->setFeatures(
            QDockWidget::DockWidgetMovable |
            QDockWidget::DockWidgetFloatable |
            QDockWidget::DockWidgetClosable);
        auto* panel = new CollectionEditorWidget(shell_state, dock);
        dock->setWidget(panel);
        window.addDockWidget(Qt::RightDockWidgetArea, dock);
        if (prop_dock) {
            window.tabifyDockWidget(prop_dock, dock);
            prop_dock->raise();
        }
    }

    {
        QSettings settings;
        const bool node_graph_preview = settings
            .value(QLatin1String(kNodeGraphPreviewSettingsKey), false)
            .toBool();
        window.setProperty(kNodeGraphPreviewProperty, node_graph_preview);
        menu_actions.toggle_node_graph_experimental->blockSignals(true);
        menu_actions.toggle_node_graph_experimental->setChecked(node_graph_preview);
        menu_actions.toggle_node_graph_experimental->blockSignals(false);
    }

    {
        QSettings settings;
        const QString last = settings
            .value(QLatin1String(ws::SETTINGS_LAST_KEY),
                   QLatin1String(ws::DEFAULT_WORKSPACE))
            .toString();
        ws::switch_to(&window, shell_state, last);
    }

    QObject::connect(shell_state, &BifShellState::status_messageChanged, &window,
        [&window, shell_state]() {
            window.statusBar()->showMessage(shell_state->getStatus_message());
        });

    // Command palette (B.8) — Ctrl+P opens a centered overlay
    // listing the menu actions. Phase C widens to prims/layers/nodes.
    {
        QHash<QString, QAction*> commands;
        commands.insert(QStringLiteral("File: New Stage"), menu_actions.new_stage);
        commands.insert(QStringLiteral("File: Open Stage..."), menu_actions.open_stage);
        commands.insert(QStringLiteral("File: Close Stage"), menu_actions.close_stage);
        commands.insert(QStringLiteral("File: Save"), menu_actions.save);
        commands.insert(QStringLiteral("File: Save As..."), menu_actions.save_as);
        commands.insert(QStringLiteral("File: Exit"), menu_actions.exit_app);
        commands.insert(QStringLiteral("Edit: Undo"), menu_actions.undo);
        commands.insert(QStringLiteral("Edit: Redo"), menu_actions.redo);
        commands.insert(
            QStringLiteral("View: Toggle Orthographic Projection"),
            menu_actions.toggle_orthographic);
        commands.insert(QStringLiteral("Workspace: Assembly"), menu_actions.workspace_assembly);
        commands.insert(QStringLiteral("Workspace: Lighting"), menu_actions.workspace_lighting);
        commands.insert(QStringLiteral("Workspace: Materials"), menu_actions.workspace_materials);
        commands.insert(QStringLiteral("Workspace: Review"), menu_actions.workspace_review);
        commands.insert(QStringLiteral("View: Toggle Zen Mode"), menu_actions.zen_mode);
        commands.insert(QStringLiteral("View: Toggle Grid"), menu_actions.toggle_grid);
        commands.insert(QStringLiteral("View: Toggle Viewport LOD"), menu_actions.toggle_lod);
        commands.insert(
            QStringLiteral("View: Toggle Scene Browser"),
            menu_actions.panel_scene_browser);
        commands.insert(
            QStringLiteral("View: Toggle Layer Stack"),
            menu_actions.panel_layer_stack);
        commands.insert(
            QStringLiteral("View: Toggle Property Inspector"),
            menu_actions.panel_property_inspector);
        commands.insert(
            QStringLiteral("View: Toggle Render Settings"),
            menu_actions.panel_render_settings);
        commands.insert(
            QStringLiteral("View: Toggle Collection Editor"),
            menu_actions.panel_collection_editor);
        commands.insert(
            QStringLiteral("View: Toggle Node Graph (Experimental)"),
            menu_actions.toggle_node_graph_experimental);
        commands.insert(
            QStringLiteral("View: Toggle Timeline"),
            menu_actions.panel_timeline);
        commands.insert(
            QStringLiteral("View: Toggle USDA Source"),
            menu_actions.toggle_usda_source);
        commands.insert(
            QStringLiteral("View: Reset Workspace Layout"),
            menu_actions.reset_workspace_layout);
        commands.insert(QStringLiteral("Render: Ivar Render"), menu_actions.ivar_render);
        commands.insert(QStringLiteral("Help: About BIF"), menu_actions.about);

        // Owned by `window` via Qt parent-child; deleted on shutdown.
        auto* palette = new CommandPalette(&window, commands);

        auto* shortcut = new QShortcut(
            QKeySequence(QStringLiteral("Ctrl+P")), &window);
        shortcut->setContext(Qt::ApplicationShortcut);
        QObject::connect(shortcut, &QShortcut::activated, &window,
            [&window, palette]() {
                // Center the palette over the main window before showing.
                const auto win_center = window.geometry().center();
                palette->move(
                    win_center.x() - palette->width() / 2,
                    window.geometry().top() + 80);
                palette->show();
                palette->raise();
                palette->activateWindow();
            });
    }

    // Timeline QTimer — advances current_frame while is_playing.
    // Interval driven by playback_fps + realtime_playback: paced at
    // 1000/fps ms in realtime mode, 0ms (as-fast-as-possible) when
    // realtime is off. Phase E.2 swaps for bif_core TimelineState.
    auto* timeline_timer = new QTimer(&window);
    auto compute_timer_interval = [shell_state]() {
        if (!shell_state->getRealtime_playback()) return 0;
        const int fps = qMax(1, shell_state->getPlayback_fps());
        return qMax(1, 1000 / fps);
    };
    auto should_run_timeline = [shell_state, &window]() {
        return shell_state->getIs_playing()
            && window.isVisible()
            && !window.isMinimized()
            && QApplication::applicationState() == Qt::ApplicationActive;
    };
    timeline_timer->setInterval(compute_timer_interval());
    QObject::connect(timeline_timer, &QTimer::timeout, &window,
        [shell_state, &window]() {
            if (!window.isVisible()
                || window.isMinimized()
                || QApplication::applicationState() != Qt::ApplicationActive) {
                return;
            }
            const int cur = shell_state->getCurrent_frame();
            const int start = shell_state->getStart_frame();
            const int end = shell_state->getEnd_frame();
            const bool loop = shell_state->getLoop_playback();
            int next = cur + 1;
            if (next > end) {
                if (loop) {
                    next = start;
                } else {
                    shell_state->setIs_playing(false);
                    shell_state->setCurrent_frame(end);
                    return;
                }
            }
            shell_state->setCurrent_frame(next);
        });
    auto sync_timeline_timer =
        [timeline_timer, compute_timer_interval, should_run_timeline]() {
        timeline_timer->setInterval(compute_timer_interval());
        if (should_run_timeline()) {
            timeline_timer->start();
        } else {
            timeline_timer->stop();
        }
    };
    QObject::connect(shell_state, &BifShellState::is_playingChanged, &window,
                     sync_timeline_timer);
    // Re-tune the interval on fps / realtime change.
    QObject::connect(shell_state, &BifShellState::playback_fpsChanged,
                     &window, sync_timeline_timer);
    QObject::connect(shell_state, &BifShellState::realtime_playbackChanged,
                     &window, sync_timeline_timer);
    QObject::connect(qApp, &QGuiApplication::applicationStateChanged, &window,
        [sync_timeline_timer](Qt::ApplicationState) {
            sync_timeline_timer();
        });

    // Push every frame change into `Renderer::set_time` so animated
    // prims actually move. Covers both the QTimer tick above (playback)
    // and the timeline slider / jump-to-keyframe paths (scrubbing).
    QObject::connect(shell_state, &BifShellState::current_frameChanged,
                     &window, [shell_state]() {
                         shell_state->on_frame_changed(
                             shell_state->getCurrent_frame());
                     });

    // Keyboard shortcuts — all routed through ShortcutRegistry so a
    // future Preferences dialog can rebind them. Register each with a
    // stable ID + default sequence; the registry consults QSettings
    // `shortcuts/<id>` for user overrides on lookup.
    //
    // Context choice: Qt::ApplicationShortcut for F (fires everywhere,
    // matches standard DCC "frame selected"). Qt::WindowShortcut for
    // arrow keys + Space so QLineEdit / QSpinBox edits keep arrow
    // navigation + spacebar typing.
    {
        namespace sc = bif_qt::shortcuts;
        auto bind = [&window](const char* id, const QKeySequence& def,
                              Qt::ShortcutContext ctx, auto&& handler) {
            auto* shortcut = new QShortcut(sc::lookup(id, def), &window);
            shortcut->setContext(ctx);
            QObject::connect(shortcut, &QShortcut::activated, &window, handler);
            return shortcut;
        };

        bind(sc::kCameraFrameSelected, QKeySequence(Qt::Key_F),
             Qt::ApplicationShortcut,
             [shell_state, &window]() {
                 shell_state->on_frame_selected();
                 window.statusBar()->showMessage(shell_state->getStatus_message());
             });

        bind(sc::kTimelineTogglePlayback, QKeySequence(Qt::Key_Space),
             Qt::WindowShortcut,
             [shell_state]() { shell_state->toggle_playback(); });

        bind(sc::kTimelinePrevFrame, QKeySequence(Qt::Key_Left),
             Qt::WindowShortcut,
             [shell_state]() { shell_state->step_frame(-1); });

        bind(sc::kTimelineNextFrame, QKeySequence(Qt::Key_Right),
             Qt::WindowShortcut,
             [shell_state]() { shell_state->step_frame(1); });

        bind(sc::kTimelinePrevKeyframe,
             QKeySequence(Qt::ShiftModifier | Qt::Key_Left),
             Qt::WindowShortcut,
             [shell_state]() { shell_state->jump_to_prev_keyframe(); });

        bind(sc::kTimelineNextKeyframe,
             QKeySequence(Qt::ShiftModifier | Qt::Key_Right),
             Qt::WindowShortcut,
             [shell_state]() { shell_state->jump_to_next_keyframe(); });
    }

    // Tier 1 — status-bar edit-target chip (permanent, right-aligned).
    // Hides itself when no stage is loaded via the internal refresh hook.
    {
        auto* ivar_status = new QLabel(window.statusBar());
        ivar_status->setObjectName(QStringLiteral("ivar_status_chip"));
        ivar_status->setStyleSheet(QStringLiteral(
            "color: rgba(180, 185, 195, 255);"
            "padding: 0 10px;"
            "border-left: 1px solid rgba(60, 65, 75, 180);"));
        auto refresh_ivar_status = [shell_state, ivar_status]() {
            const auto text = shell_state->getIvar_status();
            ivar_status->setVisible(!text.isEmpty());
            ivar_status->setText(text);
        };
        QObject::connect(shell_state, &BifShellState::ivar_statusChanged,
                         &window, refresh_ivar_status);
        refresh_ivar_status();
        window.statusBar()->addPermanentWidget(ivar_status);
    }
    {
        auto* chip = build_edit_target_chip(window.statusBar(), shell_state, /*compact=*/true);
        window.statusBar()->addPermanentWidget(chip);
    }

    // Tier 1 item #4 — window title follows stage + edit target + dirt.
    // Latent dirty asterisk until the v0.16 write path flips is_dirty.
    auto refresh_title = [&window, shell_state]() {
        window.setWindowTitle(shell_state->compose_title());
    };
    QObject::connect(shell_state, &BifShellState::layer_state_revisionChanged,
                     &window, refresh_title);
    refresh_title();

    window.show();
    const int rc = app.exec();
    ws::save_current(&window, shell_state->getCurrent_workspace());
    if (viewport_cb != nullptr) {
        viewport_on_shutdown(*viewport_cb);
    }
    return rc;
}
