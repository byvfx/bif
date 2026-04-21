// RenderWidget — QWidget subclass configured for external GPU rendering.
//
// Critical attributes so Qt gets out of wgpu's way:
//   WA_NativeWindow        — forces this widget to own a native HWND
//   WA_PaintOnScreen       — tells Qt we paint ourselves, no backing store
//   WA_OpaquePaintEvent    — skip background fill
//   WA_NoSystemBackground  — skip system background
//
// paintEngine() returns nullptr so Qt never tries to paint this widget
// through the raster engine — wgpu owns all pixels.
//
// Exposed to Rust:
//   native_win_id()        — returns HWND as u64 for raw-window-handle
//   native_hinstance()     — returns HINSTANCE as u64
//   emits resized(w, h)    — whenever the widget's pixel size changes
//   emits frameRequested() — Qt update() path triggers a paintEvent
//   emits cameraOrbit / cameraPan / cameraZoom — camera input (Phase E.1)
//   emits primPickRequested(x, y) — LMB click for ray-cast (Phase E.1)
//
// The Rust side drives wgpu::Surface creation/render/resize entirely;
// this widget is a glorified HWND carrier with a render tick + input.

#pragma once

#include <QPoint>
#include <QWidget>
#include <cstdint>

class QTimer;

class RenderWidget : public QWidget {
    Q_OBJECT
public:
    explicit RenderWidget(QWidget* parent = nullptr);
    ~RenderWidget() override;

    std::uint64_t nativeWinId() const;
    std::uint64_t nativeHInstance() const;
    int pixelWidth() const;
    int pixelHeight() const;

    /// Stop the 16ms render tick. Call before modal dialogs so the
    /// paint loop doesn't fight for z-order / focus with the dialog.
    /// Safe to call even when not yet initialized.
    void pausePainting();

    /// Resume the 16ms render tick. Pair with pausePainting().
    void resumePainting();

protected:
    QPaintEngine* paintEngine() const override { return nullptr; }
    void paintEvent(QPaintEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;
    void showEvent(QShowEvent* event) override;

    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void wheelEvent(QWheelEvent* event) override;

signals:
    void resized(int width, int height);
    void frameRequested();
    void surfaceReady();

    /// Camera orbit delta (Alt+LMB drag). Raw pixel deltas; a
    /// downstream handler applies ORBIT_SENSITIVITY.
    void cameraOrbit(int dx, int dy);
    /// Camera pan delta (middle-mouse drag).
    void cameraPan(int dx, int dy);
    /// Camera wheel zoom — matches QWheelEvent::angleDelta().y().
    void cameraZoom(int angle_delta);
    /// Unmodified LMB click — Phase E.2 ray-casts into selection.rs.
    void primPickRequested(int x, int y);

private:
    QTimer* m_tick = nullptr;
    QPoint m_last_mouse_pos;
    bool m_orbit_active = false;
    bool m_pan_active = false;
};
