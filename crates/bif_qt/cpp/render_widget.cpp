#include "render_widget.h"

#include <QMouseEvent>
#include <QPaintEvent>
#include <QResizeEvent>
#include <QShowEvent>
#include <QTimer>
#include <QWheelEvent>

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
    setMouseTracking(false);
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
    Q_UNUSED(event);
    emit frameRequested();
}

void RenderWidget::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    emit resized(pixelWidth(), pixelHeight());
}

void RenderWidget::showEvent(QShowEvent* event) {
    QWidget::showEvent(event);
    emit surfaceReady();
    if (!m_tick) {
        m_tick = new QTimer(this);
        connect(m_tick, &QTimer::timeout, this, QOverload<>::of(&QWidget::update));
        m_tick->start(16);
    } else if (!m_tick->isActive()) {
        // Resume if paused (e.g. by a modal dialog flow).
        m_tick->start(16);
    }
}

void RenderWidget::pausePainting() {
    if (m_tick && m_tick->isActive()) {
        m_tick->stop();
    }
}

void RenderWidget::resumePainting() {
    if (m_tick && !m_tick->isActive()) {
        m_tick->start(16);
    }
}

// ---------------------------------------------------------------------------
// Camera input (Phase E.1)
// LMB (no mods)      → primPickRequested (Phase E.2 ray-cast)
// Alt+LMB drag       → cameraOrbit (dx, dy pixels)
// MMB drag           → cameraPan   (dx, dy pixels)
// Wheel              → cameraZoom  (QWheelEvent::angleDelta().y())
// Raw pixel deltas are emitted; a downstream handler scales by the
// ORBIT_SENSITIVITY / PAN_SENSITIVITY constants from bif_viewer/main.
// ---------------------------------------------------------------------------

void RenderWidget::mousePressEvent(QMouseEvent* event) {
    m_last_mouse_pos = event->pos();
    if (event->button() == Qt::LeftButton) {
        if (event->modifiers() & Qt::AltModifier) {
            m_orbit_active = true;
        } else {
            // Emit framebuffer (physical) pixel coords so the ray-cast
            // matches the wgpu surface dims (which are DPR-scaled).
            const auto dpr = devicePixelRatioF();
            emit primPickRequested(
                static_cast<int>(event->pos().x() * dpr),
                static_cast<int>(event->pos().y() * dpr));
        }
    } else if (event->button() == Qt::MiddleButton) {
        m_pan_active = true;
    }
    setFocus(Qt::MouseFocusReason);
    event->accept();
}

void RenderWidget::mouseMoveEvent(QMouseEvent* event) {
    const QPoint delta = event->pos() - m_last_mouse_pos;
    m_last_mouse_pos = event->pos();
    if (m_orbit_active) {
        emit cameraOrbit(delta.x(), delta.y());
    } else if (m_pan_active) {
        emit cameraPan(delta.x(), delta.y());
    }
    event->accept();
}

void RenderWidget::mouseReleaseEvent(QMouseEvent* event) {
    if (event->button() == Qt::LeftButton) {
        m_orbit_active = false;
    } else if (event->button() == Qt::MiddleButton) {
        m_pan_active = false;
    }
    event->accept();
}

void RenderWidget::wheelEvent(QWheelEvent* event) {
    emit cameraZoom(event->angleDelta().y());
    event->accept();
}
