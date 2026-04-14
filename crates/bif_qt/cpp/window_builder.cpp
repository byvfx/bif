#include "window_builder.h"

#include <QApplication>
#include <QDockWidget>
#include <QLabel>
#include <QMainWindow>
#include <QMenu>
#include <QMenuBar>
#include <QStatusBar>
#include <QWidget>

// cxx-qt-generated header for the BifShellState QObject declared in
// src/main_window.rs. Lives under cxxqtbuild/include/bif_qt/src/.
#include "bif_qt/src/main_window.cxxqt.h"

namespace {

// Build one dock widget with a titled placeholder label. Phase B
// replaces the QLabel with the real panel widget (QTreeView,
// QTableView, QGraphicsView, etc.).
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
        QStringLiteral("(%1 panel — Phase B)").arg(title),
        dock);
    placeholder->setAlignment(Qt::AlignCenter);
    placeholder->setMinimumWidth(220);
    placeholder->setStyleSheet(QStringLiteral(
        "color: rgba(140, 145, 155, 255); padding: 24px;"));
    dock->setWidget(placeholder);

    parent->addDockWidget(initial_area, dock);
    return dock;
}

void build_menu_bar(QMainWindow* window) {
    auto* menu = window->menuBar();

    auto* file = menu->addMenu(QStringLiteral("&File"));
    file->addAction(QStringLiteral("&New Stage\tCtrl+N"));
    file->addAction(QStringLiteral("&Open Stage...\tCtrl+O"));
    file->addSeparator();
    file->addAction(QStringLiteral("&Save\tCtrl+S"));
    file->addAction(QStringLiteral("Save &As...\tCtrl+Shift+S"));
    file->addSeparator();
    file->addAction(QStringLiteral("E&xit\tCtrl+Q"), qApp, &QCoreApplication::quit);

    auto* view = menu->addMenu(QStringLiteral("&View"));
    view->addAction(QStringLiteral("&Assembly Workspace\tCtrl+1"));
    view->addAction(QStringLiteral("&Lighting Workspace\tCtrl+2"));
    view->addAction(QStringLiteral("&Materials Workspace\tCtrl+3"));
    view->addAction(QStringLiteral("&Render Workspace\tCtrl+4"));
    view->addSeparator();
    view->addAction(QStringLiteral("&Zen Mode\tCtrl+\\"));

    auto* help = menu->addMenu(QStringLiteral("&Help"));
    help->addAction(QStringLiteral("&About BIF"));
}

}  // namespace

int bif_qt_run_shell() {
    // argc/argv — static storage so QApplication can hold the int&.
    static char arg0[] = "bif_qt_shell";
    static char* argv_storage[] = {arg0, nullptr};
    static int argc = 1;

    QApplication app(argc, argv_storage);
    app.setApplicationName(QStringLiteral("BIF"));
    app.setApplicationDisplayName(QStringLiteral("BIF — USD Orchestration"));
    app.setOrganizationName(QStringLiteral("BIF"));

    QMainWindow window;
    window.setObjectName(QStringLiteral("bif_main_window"));
    window.setWindowTitle(QStringLiteral("BIF — USD Orchestration (Qt, Phase A shell)"));
    window.resize(1440, 900);

    // Construct the Rust-backed BifShellState QObject and parent it
    // to the main window for lifetime management. The cxx-qt macros
    // in main_window.rs expand to a default constructor; Qt parent-
    // child semantics delete the object when `window` goes out of
    // scope.
    auto* shell_state = new BifShellState(&window);
    // Tag the window with the shell-state via dynamic property so
    // children can retrieve it later without global lookups.
    window.setProperty("bif_shell_state", QVariant::fromValue(shell_state));

    build_menu_bar(&window);

    // Central viewport placeholder — Phase B swaps in RenderWidget.
    auto* central = new QLabel(
        QStringLiteral("(viewport — Phase B wires up wgpu RenderWidget)"),
        &window);
    central->setAlignment(Qt::AlignCenter);
    central->setMinimumSize(640, 480);
    central->setStyleSheet(QStringLiteral(
        "background-color: rgba(26, 29, 33, 255);"
        "color: rgba(140, 145, 155, 255);"
        "font-size: 14px;"));
    window.setCentralWidget(central);

    // Four dock slots — Phase B fills these in.
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
        QStringLiteral("Phase A shell ready — panels are placeholders."));

    window.show();
    return app.exec();
}
