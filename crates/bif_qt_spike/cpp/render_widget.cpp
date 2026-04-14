#include "render_widget.h"

#include <QPaintEvent>
#include <QResizeEvent>
#include <QShowEvent>
#include <QTimer>

#ifdef _WIN32
#include <windows.h>
#endif

RenderWidget::RenderWidget(QWidget* parent) : QWidget(parent) {
    // Native window + own the pixels. Order matters: WA_NativeWindow
    // must be set before anything asks for winId().
    setAttribute(Qt::WA_NativeWindow, true);
    setAttribute(Qt::WA_DontCreateNativeAncestors, true);
    setAttribute(Qt::WA_PaintOnScreen, true);
    setAttribute(Qt::WA_OpaquePaintEvent, true);
    setAttribute(Qt::WA_NoSystemBackground, true);
    // Accept focus so the viewport can handle key events later.
    setFocusPolicy(Qt::StrongFocus);
    // Reasonable default, shell will override.
    setMinimumSize(320, 240);
}

RenderWidget::~RenderWidget() = default;

std::uint64_t RenderWidget::nativeWinId() const {
    return static_cast<std::uint64_t>(winId());
}

std::uint64_t RenderWidget::nativeHInstance() const {
#ifdef _WIN32
    return reinterpret_cast<std::uint64_t>(GetModuleHandleW(nullptr));
#else
    return 0;
#endif
}

int RenderWidget::pixelWidth() const {
    return static_cast<int>(width() * devicePixelRatioF());
}

int RenderWidget::pixelHeight() const {
    return static_cast<int>(height() * devicePixelRatioF());
}

void RenderWidget::paintEvent(QPaintEvent* event) {
    // Qt paint event is our tick — forward to Rust. Never call base.
    Q_UNUSED(event);
    emit frameRequested();
}

void RenderWidget::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    emit resized(pixelWidth(), pixelHeight());
}

void RenderWidget::showEvent(QShowEvent* event) {
    QWidget::showEvent(event);
    // First show — wgpu::Surface creation needs a valid HWND, which
    // only exists after the widget has been shown and native-created.
    emit surfaceReady();
    // Kick off continuous rendering via a ~60fps timer. A real shell
    // will drive this from the scene-dirty event instead.
    static QTimer* tick = nullptr;
    if (!tick) {
        tick = new QTimer(this);
        connect(tick, &QTimer::timeout, this, QOverload<>::of(&QWidget::update));
        tick->start(16);
    }
}
