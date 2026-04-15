#include "window_builder.h"
#include "command_palette.h"
#include "first_launch_widget.h"
#include "layer_stack_widget.h"
#include "node_graph_widget.h"
#include "property_inspector_widget.h"
#include "render_settings_widget.h"
#include "render_widget.h"
#include "scene_browser_widget.h"
#include "shortcut_registry.h"
#include "timeline_widget.h"

#include <QAction>
#include <QApplication>
#include <QByteArray>
#include <QDockWidget>
#include <QFileDialog>
#include <QHash>
#include <QKeySequence>
#include <QLabel>
#include <QList>
#include <QMainWindow>
#include <QMenu>
#include <QMenuBar>
#include <QPushButton>
#include <QScreen>
#include <QSettings>
#include <QShortcut>
#include <QStackedWidget>
#include <QStatusBar>
#include <QString>
#include <QStringList>
#include <QTimer>
#include <QToolBar>
#include <QVBoxLayout>
#include <QWidget>

#include "bif_qt/src/main_window.cxxqt.h"

namespace {

// ---------------------------------------------------------------------------
// Workspace presets (B.5)
// ---------------------------------------------------------------------------
namespace ws {
constexpr const char* ASSEMBLY = "assembly";
constexpr const char* LIGHTING = "lighting";
constexpr const char* MATERIALS = "materials";
constexpr const char* RENDER = "render";
constexpr const char* DEFAULT_WORKSPACE = ASSEMBLY;
constexpr const char* SETTINGS_LAST_KEY = "workspaces/last_active";

void apply_default_layout(QMainWindow* window, const QString& name) {
    auto dock = [window](const char* obj_name) -> QDockWidget* {
        return window->findChild<QDockWidget*>(QString::fromLatin1(obj_name));
    };
    auto* scene_browser = dock("dock_scene_browser");
    auto* layer_stack = dock("dock_layer_stack");
    auto* property_inspector = dock("dock_property_inspector");
    auto* node_graph = dock("dock_node_graph");

    auto show_all = [&](bool s) {
        if (scene_browser) scene_browser->setVisible(s);
        if (layer_stack) layer_stack->setVisible(s);
        if (property_inspector) property_inspector->setVisible(s);
        if (node_graph) node_graph->setVisible(s);
    };

    if (name == QLatin1String(ws::ASSEMBLY)) {
        show_all(true);
    } else if (name == QLatin1String(ws::LIGHTING)) {
        if (scene_browser) scene_browser->setVisible(true);
        if (layer_stack) layer_stack->setVisible(false);
        if (property_inspector) property_inspector->setVisible(true);
        if (node_graph) node_graph->setVisible(false);
    } else if (name == QLatin1String(ws::MATERIALS)) {
        if (scene_browser) scene_browser->setVisible(false);
        if (layer_stack) layer_stack->setVisible(false);
        if (property_inspector) property_inspector->setVisible(true);
        if (node_graph) node_graph->setVisible(true);
    } else if (name == QLatin1String(ws::RENDER)) {
        if (scene_browser) scene_browser->setVisible(false);
        if (layer_stack) layer_stack->setVisible(false);
        if (property_inspector) property_inspector->setVisible(true);
        if (node_graph) node_graph->setVisible(false);
    } else {
        show_all(true);
    }
}

QString state_key(const QString& name) {
    return QStringLiteral("workspaces/%1/state").arg(name);
}

void save_current(QMainWindow* window, const QString& current) {
    if (current.isEmpty()) return;
    QSettings settings;
    settings.setValue(state_key(current), window->saveState());
}

void switch_to(
    QMainWindow* window,
    BifShellState* state,
    const QString& target) {
    save_current(window, state->getCurrent_workspace());

    QSettings settings;
    const QByteArray blob = settings.value(state_key(target)).toByteArray();
    if (blob.isEmpty()) {
        apply_default_layout(window, target);
    } else if (!window->restoreState(blob)) {
        apply_default_layout(window, target);
    }

    state->setCurrent_workspace(target);
    settings.setValue(QLatin1String(SETTINGS_LAST_KEY), target);
    state->setStatus_message(QStringLiteral("Workspace: %1").arg(target));
    window->statusBar()->showMessage(state->getStatus_message());
}
}  // namespace ws

// ---------------------------------------------------------------------------
// Menu actions
// ---------------------------------------------------------------------------
struct MenuActions {
    QAction* new_stage;
    QAction* open_stage;
    QAction* save;
    QAction* save_as;
    QAction* exit_app;

    QAction* workspace_assembly;
    QAction* workspace_lighting;
    QAction* workspace_materials;
    QAction* workspace_render;
    QAction* zen_mode;

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
    MenuActions a{};
    auto* menu = window->menuBar();

    auto* file = menu->addMenu(QStringLiteral("&File"));
    a.new_stage = file->addAction(QStringLiteral("&New Stage"));
    a.new_stage->setShortcut(QKeySequence::New);
    a.open_stage = file->addAction(QStringLiteral("&Open Stage..."));
    a.open_stage->setShortcut(QKeySequence::Open);
    file->addSeparator();
    a.save = file->addAction(QStringLiteral("&Save"));
    a.save->setShortcut(QKeySequence::Save);
    a.save_as = file->addAction(QStringLiteral("Save &As..."));
    a.save_as->setShortcut(QKeySequence::SaveAs);
    file->addSeparator();
    a.exit_app = file->addAction(QStringLiteral("E&xit"));
    a.exit_app->setShortcut(QKeySequence(QStringLiteral("Ctrl+Q")));

    auto* view = menu->addMenu(QStringLiteral("&View"));
    a.workspace_assembly = view->addAction(QStringLiteral("&Assembly Workspace"));
    a.workspace_assembly->setShortcut(QKeySequence(QStringLiteral("Ctrl+1")));
    a.workspace_lighting = view->addAction(QStringLiteral("&Lighting Workspace"));
    a.workspace_lighting->setShortcut(QKeySequence(QStringLiteral("Ctrl+2")));
    a.workspace_materials = view->addAction(QStringLiteral("&Materials Workspace"));
    a.workspace_materials->setShortcut(QKeySequence(QStringLiteral("Ctrl+3")));
    a.workspace_render = view->addAction(QStringLiteral("&Render Workspace"));
    a.workspace_render->setShortcut(QKeySequence(QStringLiteral("Ctrl+4")));
    view->addSeparator();
    a.zen_mode = view->addAction(QStringLiteral("&Zen Mode"));
    a.zen_mode->setShortcut(QKeySequence(QStringLiteral("Ctrl+\\")));
    a.zen_mode->setCheckable(true);

    auto* help = menu->addMenu(QStringLiteral("&Help"));
    a.about = help->addAction(QStringLiteral("&About BIF"));

    return a;
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
    QStackedWidget* stack;
    FirstLaunchWidget* first_launch;
    RenderWidget* viewport;
};

CentralArea build_central_area(QMainWindow* window) {
    CentralArea ca{};
    ca.container = new QWidget(window);
    ca.container->setObjectName(QStringLiteral("central_area"));

    auto* layout = new QVBoxLayout(ca.container);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    ca.breadcrumb = build_breadcrumb_bar(window);
    ca.breadcrumb->setParent(ca.container);
    layout->addWidget(ca.breadcrumb);

    ca.stack = new QStackedWidget(ca.container);
    ca.stack->setObjectName(QStringLiteral("central_stack"));
    layout->addWidget(ca.stack, 1);

    ca.first_launch = new FirstLaunchWidget(ca.stack);
    ca.viewport = new RenderWidget(ca.stack);
    ca.viewport->setObjectName(QStringLiteral("viewport"));

    ca.stack->addWidget(ca.first_launch);  // index 0
    ca.stack->addWidget(ca.viewport);      // index 1
    ca.stack->setCurrentIndex(0);

    return ca;
}

// ---------------------------------------------------------------------------
// Action wiring
// ---------------------------------------------------------------------------
void wire_shell_actions(
    MenuActions& actions,
    BifShellState* shell_state,
    QMainWindow* window,
    QStackedWidget* central_stack) {
    auto update_status = [shell_state, window]() {
        window->statusBar()->showMessage(shell_state->getStatus_message());
    };

    auto open_stage_flow = [shell_state, central_stack, update_status]() {
        shell_state->on_open_stage();
        // Swap to viewport so user sees something. Phase C replaces
        // this with: only swap on actual successful stage load.
        central_stack->setCurrentIndex(1);
        update_status();
    };

    auto new_stage_flow = [shell_state, central_stack, update_status]() {
        shell_state->on_new_stage();
        central_stack->setCurrentIndex(1);
        update_status();
    };

    QObject::connect(actions.new_stage, &QAction::triggered, window, new_stage_flow);
    // Open Stage now actually shows a QFileDialog (Phase E.1). Phase
    // E.2 parses the returned path + calls bif_core::scene_loader.
    QObject::connect(actions.open_stage, &QAction::triggered, window,
        [window, shell_state, central_stack, update_status]() {
            const QString path = QFileDialog::getOpenFileName(
                window,
                QStringLiteral("Open USD Stage"),
                QString(),
                QStringLiteral("USD files (*.usd *.usda *.usdc *.usdz);;All files (*)"));
            if (!path.isEmpty()) {
                // Store in recents for the first-launch list.
                QSettings settings;
                auto recents = settings.value(QStringLiteral("recent_stages"))
                                    .toStringList();
                recents.removeAll(path);
                recents.prepend(path);
                while (recents.size() > 10) recents.removeLast();
                settings.setValue(QStringLiteral("recent_stages"), recents);

                shell_state->on_stage_path_opened(path);
                central_stack->setCurrentIndex(1);
                update_status();
            } else {
                shell_state->on_open_stage();
                update_status();
            }
        });
    QObject::connect(actions.save, &QAction::triggered, window,
        [shell_state, update_status]() {
            shell_state->on_save();
            update_status();
        });
    QObject::connect(actions.save_as, &QAction::triggered, window,
        [shell_state, update_status]() {
            shell_state->on_save_as();
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
    QObject::connect(actions.workspace_render, &QAction::triggered, window,
        [window, shell_state]() { ws::switch_to(window, shell_state, QLatin1String(ws::RENDER)); });

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

    QObject::connect(actions.about, &QAction::triggered, window,
        [shell_state, update_status]() {
            shell_state->on_about();
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
    QObject::connect(first_launch, &FirstLaunchWidget::openStageClicked, window,
        [shell_state, central_stack, update_status]() {
            shell_state->on_open_stage();
            central_stack->setCurrentIndex(1);
            update_status();
        });
    QObject::connect(first_launch, &FirstLaunchWidget::recentStageActivated, window,
        [shell_state, central_stack, update_status](const QString& path) {
            shell_state->setStatus_message(QStringLiteral("Recent: %1 (Phase C wires file open)").arg(path));
            central_stack->setCurrentIndex(1);
            update_status();
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
            const auto ok = viewport_on_surface_ready(*cb, hwnd, hinst, w, h);
            shell_state->setStatus_message(ok
                ? QStringLiteral("wgpu viewport live")
                : QStringLiteral("FAILED to init wgpu viewport — see stderr"));
            window->statusBar()->showMessage(shell_state->getStatus_message());
        });

    QObject::connect(
        viewport, &RenderWidget::resized, viewport,
        [cb](int w, int h) {
            viewport_on_resize(*cb, w, h);
        });

    QObject::connect(
        viewport, &RenderWidget::frameRequested, viewport,
        [cb]() {
            viewport_on_frame(*cb);
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
            // Phase E.2 builds a ray and hit-tests; for now just
            // surface the click coords.
            shell_state->setStatus_message(QStringLiteral(
                "Viewport pick at (%1,%2) — Phase E.2 wires ray-cast").arg(x).arg(y));
            update_status();
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
    auto central = build_central_area(&window);
    window.setCentralWidget(central.container);

    wire_shell_actions(menu_actions, shell_state, &window, central.stack);
    wire_first_launch(central.first_launch, shell_state, central.stack, &window);
    if (viewport_cb != nullptr) {
        connect_viewport_signals(central.viewport, viewport_cb, shell_state, &window);
    }

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
    // Bottom area: Node Graph (still a placeholder — Phase D.2) and
    // Timeline (Phase D.1) tabified together.
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
        auto* panel = new NodeGraphWidget(node_graph_dock);
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
        auto* panel = new RenderSettingsWidget(dock);
        dock->setWidget(panel);
        window.addDockWidget(Qt::RightDockWidgetArea, dock);
        if (prop_dock) {
            window.tabifyDockWidget(prop_dock, dock);
            prop_dock->raise();
        }
    }

    {
        QSettings settings;
        const QString last = settings
            .value(QLatin1String(ws::SETTINGS_LAST_KEY),
                   QLatin1String(ws::DEFAULT_WORKSPACE))
            .toString();
        ws::switch_to(&window, shell_state, last);
    }

    // Phase C.1 demo data — seed a 3-layer fake stack so the
    // Layer Stack panel has something to show. Phase E replaces
    // this with real USD stage load.
    shell_state->seed_demo_layer_stack();

    // Command palette (B.8) — Ctrl+P opens a centered overlay
    // listing the menu actions. Phase C widens to prims/layers/nodes.
    {
        QHash<QString, QAction*> commands;
        commands.insert(QStringLiteral("File: New Stage"), menu_actions.new_stage);
        commands.insert(QStringLiteral("File: Open Stage..."), menu_actions.open_stage);
        commands.insert(QStringLiteral("File: Save"), menu_actions.save);
        commands.insert(QStringLiteral("File: Save As..."), menu_actions.save_as);
        commands.insert(QStringLiteral("File: Exit"), menu_actions.exit_app);
        commands.insert(QStringLiteral("Workspace: Assembly"), menu_actions.workspace_assembly);
        commands.insert(QStringLiteral("Workspace: Lighting"), menu_actions.workspace_lighting);
        commands.insert(QStringLiteral("Workspace: Materials"), menu_actions.workspace_materials);
        commands.insert(QStringLiteral("Workspace: Render"), menu_actions.workspace_render);
        commands.insert(QStringLiteral("View: Toggle Zen Mode"), menu_actions.zen_mode);
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
    timeline_timer->setInterval(compute_timer_interval());
    QObject::connect(timeline_timer, &QTimer::timeout, &window,
        [shell_state]() {
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
    QObject::connect(shell_state, &BifShellState::is_playingChanged, &window,
        [shell_state, timeline_timer]() {
            if (shell_state->getIs_playing()) {
                timeline_timer->start();
            } else {
                timeline_timer->stop();
            }
        });
    // Re-tune the interval on fps / realtime change.
    auto retune_timer = [timeline_timer, compute_timer_interval]() {
        timeline_timer->setInterval(compute_timer_interval());
    };
    QObject::connect(shell_state, &BifShellState::playback_fpsChanged,
                     &window, retune_timer);
    QObject::connect(shell_state, &BifShellState::realtime_playbackChanged,
                     &window, retune_timer);

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

    window.show();
    const int rc = app.exec();
    ws::save_current(&window, shell_state->getCurrent_workspace());
    if (viewport_cb != nullptr) {
        viewport_on_shutdown(*viewport_cb);
    }
    return rc;
}
