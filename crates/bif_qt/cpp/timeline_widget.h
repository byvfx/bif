// TimelineWidget — Phase D.1 panel.
//
// Composition: toolbar (Prev / Play / Next + frame spin box + range
// label) above a custom-paint TimelineRuler that draws the frame
// range ruler, keyframe diamonds, and the playhead. Scrub by
// dragging across the ruler.
//
// State comes from BifShellState qproperties:
//   current_frame / start_frame / end_frame / is_playing
// plus demo_keyframes exposed via keyframe_count / keyframe_at.
// Phase E replaces the demo data with AnimatedTransform-derived
// keyframes and wires a QTimer to advance current_frame on play.

#pragma once

#include <QWidget>

class BifShellState;
class QAction;
class QLabel;
class QSpinBox;
class QToolBar;

/// Inner widget doing the ruler/keyframe/playhead paint + scrub.
class TimelineRuler : public QWidget {
    Q_OBJECT
public:
    explicit TimelineRuler(BifShellState* state, QWidget* parent = nullptr);

    QSize sizeHint() const override { return QSize(400, 52); }
    QSize minimumSizeHint() const override { return QSize(200, 36); }

protected:
    void paintEvent(QPaintEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;

private slots:
    void on_state_changed();

private:
    int frame_at_x(int x) const;
    int x_for_frame(int frame) const;

    BifShellState* m_state;
};

/// Toolbar + TimelineRuler composition — this is what goes in the dock.
class TimelineWidget : public QWidget {
    Q_OBJECT
public:
    explicit TimelineWidget(BifShellState* state, QWidget* parent = nullptr);
    ~TimelineWidget() override;

private slots:
    void on_play_toggled();
    void on_frame_spin_changed(int value);
    void on_state_changed();

private slots:
    void on_start_spin_changed(int value);
    void on_end_spin_changed(int value);
    void on_fps_spin_changed(int value);
    void on_detect_clicked();

private:
    BifShellState* m_state;
    TimelineRuler* m_ruler;
    QToolBar* m_toolbar;
    QAction* m_prev_keyframe_action;
    QAction* m_prev_action;
    QAction* m_play_action;
    QAction* m_next_action;
    QAction* m_next_keyframe_action;
    QSpinBox* m_frame_spin;
    // Range + fps inline editors — ease-of-use alternative to a
    // modal "Global Animation Options" dialog. `m_detect_action`
    // re-reads the loaded USD stage's time metadata. `m_realtime_action`
    // flips the QTimer between paced (1000/fps ms) and as-fast-as-
    // possible playback modes.
    QSpinBox* m_start_spin;
    QSpinBox* m_end_spin;
    QSpinBox* m_fps_spin;
    QAction* m_realtime_action;
    QAction* m_loop_action;
    QAction* m_detect_action;
};
