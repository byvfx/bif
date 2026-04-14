#include "window_builder.h"
#include "render_widget.h"

#include <QAction>
#include <QApplication>
#include <QDockWidget>
#include <QKeySequence>
#include <QLabel>
#include <QList>
#include <QMainWindow>
#include <QMenu>
#include <QMenuBar>
#include <QShortcut>
#include <QStatusBar>
#include <QWidget>

// cxx-qt-generated header for BifShellState + ViewportCallbacks
// + viewport_on_* trampolines declared in src/main_window.rs.
#include "bif_qt/src/main_window.cxxqt.h"

namespace {

// ---------------------------------------------------------------------------
// Menu actions — owned by QMainWindow (parent-child); we keep
// pointers in this struct for post-construction wiring.
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
    // QKeySequence::Quit is empty on Windows — force Ctrl+Q.
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

// Connect each action to a lambda that invokes the corresponding
// BifShellState invokable then shows the resulting status message.
// Keeps the reaction chain: user → QAction → BifShellState (cxx-qt
// invokable updates status_message) → QStatusBar shows message.
void wire_shell_actions(
    MenuActions& actions,
    BifShellState* shell_state,
    QMainWindow* window) {
    auto update_status = [shell_state, window]() {
        window->statusBar()->showMessage(shell_state->getStatus_message());
    };

    QObject::connect(actions.new_stage, &QAction::triggered, window,
        [shell_state, update_status]() {
            shell_state->on_new_stage();
            update_status();
        });
    QObject::connect(actions.open_stage, &QAction::triggered, window,
        [shell_state, update_status]() {
            shell_state->on_open_stage();
            update_status();
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

    // Workspace actions — Phase B.5 wires to QMainWindow::saveState
    // presets in QSettings. For now, update status only.
    auto workspace_stub = [shell_state, update_status, window](const char* name) {
        shell_state->setStatus_message(QStringLiteral("Switched to %1 workspace (presets in Phase B.5)")
            .arg(QString::fromLatin1(name)));
        update_status();
    };
    QObject::connect(actions.workspace_assembly, &QAction::triggered, window,
        [workspace_stub]() { workspace_stub("Assembly"); });
    QObject::connect(actions.workspace_lighting, &QAction::triggered, window,
        [workspace_stub]() { workspace_stub("Lighting"); });
    QObject::connect(actions.workspace_materials, &QAction::triggered, window,
        [workspace_stub]() { workspace_stub("Materials"); });
    QObject::connect(actions.workspace_render, &QAction::triggered, window,
        [workspace_stub]() { workspace_stub("Render"); });

    // Zen mode — toggles visibility of every QDockWidget child. Menu
    // bar + status bar stay visible (only docks vanish).
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
                ? QStringLiteral("wgpu viewport live — Phase B shell")
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
}

}  // namespace

int bif_qt_run_shell(ViewportCallbacks* viewport_cb, ::rust::Str stylesheet) {
    // argc/argv — static storage so QApplication can hold the int&.
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
    wire_shell_actions(menu_actions, shell_state, &window);

    auto* viewport = new RenderWidget(&window);
    viewport->setObjectName(QStringLiteral("viewport"));
    window.setCentralWidget(viewport);
    if (viewport_cb != nullptr) {
        connect_viewport_signals(viewport, viewport_cb, shell_state, &window);
    }

    make_placeholder_dock(
        QStringLiteral("Scene Browser"),
        QStringLiteral("dock_scene_browser"),
        Qt::LeftDockWidgetArea, &window);
    make_placeholder_dock(
        QStringLiteral("Layer Stack"),
        QStringLiteral("dock_layer_stack"),
        Qt::LeftDockWidgetArea, &window);
    make_placeholder_dock(
        QStringLiteral("Property Inspector"),
        QStringLiteral("dock_property_inspector"),
        Qt::RightDockWidgetArea, &window);
    make_placeholder_dock(
        QStringLiteral("Node Graph"),
        QStringLiteral("dock_node_graph"),
        Qt::BottomDockWidgetArea, &window);

    window.statusBar()->showMessage(
        QStringLiteral("Phase B shell ready — actions wired, panels placeholders."));

    window.show();
    const int rc = app.exec();
    if (viewport_cb != nullptr) {
        viewport_on_shutdown(*viewport_cb);
    }
    return rc;
}
