#include "app_main.h"
#include "render_widget.h"

#include <QApplication>
#include <QMainWindow>
#include <QStatusBar>

// cxx-generated header — declares SpikeCallbacks and spike_on_*
// trampolines that forward into Rust.
#include "bif_qt_spike/src/bridge.rs.h"

int qt_spike_run(SpikeCallbacks* callbacks) {
    // QApplication requires a long-lived argc/argv. Static is fine
    // for a single-window spike — Qt never parses args in our case.
    static char arg0[] = "bif_qt_spike";
    static char* argv_storage[] = {arg0, nullptr};
    static int argc = 1;

    QApplication app(argc, argv_storage);
    app.setApplicationName(QStringLiteral("bif_qt_spike"));
    app.setOrganizationName(QStringLiteral("BIF"));

    QMainWindow window;
    window.setWindowTitle(QStringLiteral("bif Phase-0 spike — wgpu into Qt"));
    window.resize(960, 640);

    auto* viewport = new RenderWidget(&window);
    window.setCentralWidget(viewport);
    window.statusBar()->showMessage(QStringLiteral("Waiting for wgpu surface..."));

    // Signal -> Rust. Lambdas capture `callbacks` + `viewport` + the
    // status bar. All lambda invocations happen on the Qt main thread,
    // so no additional synchronization is needed.
    QObject::connect(viewport, &RenderWidget::surfaceReady, viewport, [callbacks, viewport, &window]() {
        const auto hwnd = viewport->nativeWinId();
        const auto hinst = viewport->nativeHInstance();
        const auto w = viewport->pixelWidth();
        const auto h = viewport->pixelHeight();
        if (spike_on_surface_ready(*callbacks, hwnd, hinst, w, h)) {
            window.statusBar()->showMessage(
                QStringLiteral("wgpu surface live — triangle should be visible"));
        } else {
            window.statusBar()->showMessage(
                QStringLiteral("FAILED to init wgpu surface — see stderr"));
        }
    });

    QObject::connect(viewport, &RenderWidget::resized, viewport, [callbacks](int w, int h) {
        spike_on_resize(*callbacks, w, h);
    });

    QObject::connect(viewport, &RenderWidget::frameRequested, viewport, [callbacks]() {
        spike_on_frame(*callbacks);
    });

    window.show();
    const int rc = app.exec();
    spike_on_shutdown(*callbacks);
    return rc;
}
