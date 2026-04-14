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
//   emits request_frame()  — Qt update() path triggers a paintEvent,
//                            we forward it as a render request to Rust
//
// The Rust side drives wgpu::Surface creation/render/resize entirely;
// this widget is a glorified HWND carrier with a render tick.

#pragma once

#include <QWidget>
#include <cstdint>

class RenderWidget : public QWidget {
    Q_OBJECT
public:
    explicit RenderWidget(QWidget* parent = nullptr);
    ~RenderWidget() override;

    // Returns the native window handle as u64.
    // Windows: HWND. Linux: Window/xcb_window_t. macOS: NSView*.
    std::uint64_t nativeWinId() const;

    // Windows-only: module HINSTANCE (GetModuleHandle(NULL)).
    // Returns 0 on non-Windows.
    std::uint64_t nativeHInstance() const;

    // Widget size in device pixels (DPR-scaled).
    int pixelWidth() const;
    int pixelHeight() const;

protected:
    QPaintEngine* paintEngine() const override { return nullptr; }
    void paintEvent(QPaintEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;
    void showEvent(QShowEvent* event) override;

signals:
    void resized(int width, int height);
    void frameRequested();
    void surfaceReady();
};
