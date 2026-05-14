#include "timeline_widget.h"

#include <QAction>
#include <QColor>
#include <QLabel>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QPainter>
#include <QPainterPath>
#include <QSpinBox>
#include <QToolBar>
#include <QVBoxLayout>

#include "bif_qt/src/main_window.cxxqt.h"

namespace {
constexpr int kRulerTop = 4;
constexpr int kRulerBottom = 20;
constexpr int kTrackTop = 22;
constexpr int kTrackBottom = 40;
constexpr int kHMargin = 8;
}  // namespace

TimelineRuler::TimelineRuler(BifShellState* state, QWidget* parent)
    : QWidget(parent), m_state(state) {
    setMouseTracking(false);
    setMinimumHeight(48);
    if (m_state) {
        QObject::connect(m_state, &BifShellState::current_frameChanged,
            this, &TimelineRuler::on_state_changed);
        QObject::connect(m_state, &BifShellState::start_frameChanged,
            this, &TimelineRuler::on_state_changed);
        QObject::connect(m_state, &BifShellState::end_frameChanged,
            this, &TimelineRuler::on_state_changed);
        // Keyframes are derived per-selection now (Phase E.2 move 9) —
        // re-paint when the selected prim changes.
        QObject::connect(m_state, &BifShellState::selected_prim_pathChanged,
            this, &TimelineRuler::on_state_changed);
    }
}

int TimelineRuler::frame_at_x(int x) const {
    if (!m_state) return 0;
    const int start = m_state->getStart_frame();
    const int end = m_state->getEnd_frame();
    if (end <= start) return start;
    const int w = qMax(1, width() - 2 * kHMargin);
    const double t = static_cast<double>(x - kHMargin) / static_cast<double>(w);
    const int range = end - start;
    int frame = start + static_cast<int>(qBound(0.0, t, 1.0) * range + 0.5);
    return qBound(start, frame, end);
}

int TimelineRuler::x_for_frame(int frame) const {
    if (!m_state) return kHMargin;
    const int start = m_state->getStart_frame();
    const int end = m_state->getEnd_frame();
    if (end <= start) return kHMargin;
    const int w = qMax(1, width() - 2 * kHMargin);
    const double t = static_cast<double>(frame - start) / static_cast<double>(end - start);
    return kHMargin + static_cast<int>(qBound(0.0, t, 1.0) * w);
}

void TimelineRuler::paintEvent(QPaintEvent* /*event*/) {
    if (!m_state) return;
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, true);

    // Background track
    p.fillRect(rect(), QColor(34, 38, 44));
    p.fillRect(QRect(kHMargin, kTrackTop, width() - 2 * kHMargin,
                     kTrackBottom - kTrackTop),
               QColor(42, 47, 54));

    const int start = m_state->getStart_frame();
    const int end = m_state->getEnd_frame();
    const int cur = m_state->getCurrent_frame();
    const int range = qMax(1, end - start);

    // Ruler ticks every 10 frames, labels every 20.
    p.setPen(QColor(100, 105, 115));
    QFont small_font = p.font();
    small_font.setPointSize(qMax(7, small_font.pointSize() - 2));
    p.setFont(small_font);
    for (int f = start; f <= end; ++f) {
        if ((f - start) % 10 != 0) continue;
        const int x = x_for_frame(f);
        const bool major = (f - start) % 20 == 0;
        p.setPen(major ? QColor(180, 185, 195) : QColor(100, 105, 115));
        p.drawLine(x, kRulerTop, x, major ? kRulerBottom : kRulerBottom - 4);
        if (major) {
            p.setPen(QColor(140, 145, 155));
            p.drawText(QRect(x - 20, kRulerTop - 2, 40, 14),
                       Qt::AlignHCenter | Qt::AlignTop,
                       QString::number(f));
        }
    }

    // Keyframe diamonds
    p.setPen(QPen(QColor(180, 140, 30), 1));
    p.setBrush(QColor(220, 160, 40));
    const int kf_count = m_state->keyframe_count();
    const int track_mid = (kTrackTop + kTrackBottom) / 2;
    for (int i = 0; i < kf_count; ++i) {
        const int f = m_state->keyframe_at(i);
        if (f < 0) continue;
        const int x = x_for_frame(f);
        QPainterPath diamond;
        diamond.moveTo(x, track_mid - 5);
        diamond.lineTo(x + 4, track_mid);
        diamond.lineTo(x, track_mid + 5);
        diamond.lineTo(x - 4, track_mid);
        diamond.closeSubpath();
        p.drawPath(diamond);
    }

    // Playhead — full-height vertical line + top triangle
    const int px = x_for_frame(cur);
    p.setPen(QPen(QColor(74, 144, 217), 1));
    p.drawLine(px, kRulerTop, px, kTrackBottom + 4);
    QPainterPath head;
    head.moveTo(px, kRulerTop - 2);
    head.lineTo(px + 4, kRulerTop + 4);
    head.lineTo(px - 4, kRulerTop + 4);
    head.closeSubpath();
    p.setBrush(QColor(74, 144, 217));
    p.setPen(Qt::NoPen);
    p.drawPath(head);

    // Muted range summary at the right edge
    p.setPen(QColor(140, 145, 155));
    p.setFont(small_font);
    p.drawText(QRect(0, kTrackBottom + 6, width() - kHMargin, 14),
               Qt::AlignRight | Qt::AlignTop,
               QStringLiteral("frame %1 / %2").arg(cur).arg(end));
    Q_UNUSED(range);
}

void TimelineRuler::mousePressEvent(QMouseEvent* event) {
    if (event->button() != Qt::LeftButton || !m_state) return;
    const int frame = frame_at_x(event->position().x());
    m_state->setCurrent_frame(frame);
}

void TimelineRuler::mouseMoveEvent(QMouseEvent* event) {
    if (!(event->buttons() & Qt::LeftButton) || !m_state) return;
    const int frame = frame_at_x(event->position().x());
    m_state->setCurrent_frame(frame);
}

void TimelineRuler::on_state_changed() {
    update();
}

// ---------------------------------------------------------------------------

TimelineWidget::TimelineWidget(BifShellState* state, QWidget* parent)
    : QWidget(parent),
      m_state(state),
      m_ruler(nullptr),
      m_toolbar(nullptr),
      m_prev_keyframe_action(nullptr),
      m_prev_action(nullptr),
      m_play_action(nullptr),
      m_next_action(nullptr),
      m_next_keyframe_action(nullptr),
      m_frame_spin(nullptr),
      m_start_spin(nullptr),
      m_end_spin(nullptr),
      m_fps_spin(nullptr),
      m_realtime_action(nullptr),
      m_loop_action(nullptr),
      m_detect_action(nullptr) {
    setObjectName(QStringLiteral("timeline_widget"));

    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_toolbar = new QToolBar(this);
    m_toolbar->setMovable(false);
    m_toolbar->setFloatable(false);
    m_toolbar->setIconSize(QSize(0, 0));
    // QToolBar chrome comes from theme.rs (Graphite). Per-button overrides
    // for the play/loop toggles stay local.
    m_toolbar->setStyleSheet(QStringLiteral(
        "QToolButton { background-color: transparent;"
        "              color: rgba(229, 226, 225, 255);"
        "              border: 1px solid rgba(65, 71, 82, 180);"
        "              border-radius: 6px; padding: 3px 8px; font-size: 11px; }"
        "QToolButton:hover { background-color: rgba(53, 53, 53, 255); }"
        "QToolButton:checked { background-color: rgba(74, 158, 255, 120);"
        "                      border-color: rgba(74, 158, 255, 240); }"));

    // Nuke-style three-zone layout:
    //   LEFT  — playback config (fps, real-time, loop)
    //   CENTER — transport + frame counter (visually dominant)
    //   RIGHT — timeline range scope (start / end / detect)
    // QToolBar is LTR; expanding QWidget spacers enforce the zones.

    const QString kSpinQss = QStringLiteral(
        "QSpinBox { background-color: rgba(30, 34, 40, 255);"
        "           color: rgba(220, 222, 226, 255);"
        "           border: 1px solid rgba(60, 65, 75, 180);"
        "           border-radius: 3px; padding: 2px 4px; min-width: 52px; }");
    const QString kTagQss = QStringLiteral(
        "color: rgba(140, 145, 155, 255); padding: 0 2px; font-size: 11px;");

    // ---- LEFT zone: playback config --------------------------------
    m_fps_spin = new QSpinBox(m_toolbar);
    m_fps_spin->setStyleSheet(kSpinQss);
    m_fps_spin->setRange(1, 240);
    m_fps_spin->setSuffix(QStringLiteral(" fps"));
    m_fps_spin->setValue(m_state ? m_state->getPlayback_fps() : 24);
    m_fps_spin->setToolTip(QStringLiteral("Playback frame rate (frames per second)"));
    QObject::connect(m_fps_spin, QOverload<int>::of(&QSpinBox::valueChanged),
        this, &TimelineWidget::on_fps_spin_changed);
    m_toolbar->addWidget(m_fps_spin);

    m_realtime_action = m_toolbar->addAction(QStringLiteral("RT"));
    m_realtime_action->setCheckable(true);
    m_realtime_action->setChecked(m_state ? m_state->getRealtime_playback() : true);
    m_realtime_action->setToolTip(QStringLiteral(
        "Real-time playback\n"
        "ON: paced at FPS.\n"
        "OFF: as-fast-as-possible (every frame rendered)."));
    QObject::connect(m_realtime_action, &QAction::toggled, this, [this](bool on) {
        if (m_state) m_state->setRealtime_playback(on);
    });

    m_loop_action = m_toolbar->addAction(QStringLiteral("↻ Loop"));
    m_loop_action->setCheckable(true);
    m_loop_action->setChecked(m_state ? m_state->getLoop_playback() : true);
    m_loop_action->setToolTip(QStringLiteral(
        "Loop playback\n"
        "ON: wrap back to start at end frame.\n"
        "OFF: stop at end frame.\n"
        "(Future: Repeat / Bounce / Stop / Continue modes à la Nuke.)"));
    QObject::connect(m_loop_action, &QAction::toggled, this, [this](bool on) {
        if (m_state) m_state->setLoop_playback(on);
    });

    auto* left_spacer = new QWidget(m_toolbar);
    left_spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    m_toolbar->addWidget(left_spacer);

    // ---- CENTER zone: transport + frame counter -------------------
    m_prev_keyframe_action = m_toolbar->addAction(QStringLiteral("⏮"));
    m_prev_keyframe_action->setToolTip(QStringLiteral("Previous keyframe (Shift+Left)"));
    QObject::connect(m_prev_keyframe_action, &QAction::triggered, this, [this]() {
        if (m_state) m_state->jump_to_prev_keyframe();
    });

    m_prev_action = m_toolbar->addAction(QStringLiteral("◀"));
    m_prev_action->setToolTip(QStringLiteral("Previous frame (Left)"));
    QObject::connect(m_prev_action, &QAction::triggered, this, [this]() {
        if (m_state) m_state->step_frame(-1);
    });

    m_play_action = m_toolbar->addAction(QStringLiteral("▶  Play"));
    m_play_action->setCheckable(true);
    m_play_action->setToolTip(QStringLiteral("Play / Pause (Space)"));
    QObject::connect(m_play_action, &QAction::triggered, this, &TimelineWidget::on_play_toggled);

    m_next_action = m_toolbar->addAction(QStringLiteral("▶"));
    m_next_action->setToolTip(QStringLiteral("Next frame (Right)"));
    QObject::connect(m_next_action, &QAction::triggered, this, [this]() {
        if (m_state) m_state->step_frame(1);
    });

    m_next_keyframe_action = m_toolbar->addAction(QStringLiteral("⏭"));
    m_next_keyframe_action->setToolTip(QStringLiteral("Next keyframe (Shift+Right)"));
    QObject::connect(m_next_keyframe_action, &QAction::triggered, this, [this]() {
        if (m_state) m_state->jump_to_next_keyframe();
    });

    // Visually dominant frame counter — slightly bigger, accent-colored
    // border so the eye lands on it like Nuke's orange current frame.
    m_frame_spin = new QSpinBox(m_toolbar);
    m_frame_spin->setStyleSheet(QStringLiteral(
        "QSpinBox { background-color: rgba(30, 34, 40, 255);"
        "           color: rgba(230, 180, 80, 255);"
        "           border: 1px solid rgba(90, 110, 140, 200);"
        "           border-radius: 3px; padding: 3px 6px;"
        "           min-width: 64px; font-size: 13px; font-weight: 600; }"
        "QSpinBox:focus { border-color: rgba(230, 180, 80, 200); }"));
    m_frame_spin->setAlignment(Qt::AlignCenter);
    m_frame_spin->setButtonSymbols(QAbstractSpinBox::NoButtons);
    m_frame_spin->setRange(m_state ? m_state->getStart_frame() : 0,
                           m_state ? m_state->getEnd_frame() : 100);
    m_frame_spin->setValue(m_state ? m_state->getCurrent_frame() : 0);
    m_frame_spin->setToolTip(QStringLiteral("Current frame"));
    QObject::connect(m_frame_spin, QOverload<int>::of(&QSpinBox::valueChanged),
        this, &TimelineWidget::on_frame_spin_changed);
    m_toolbar->addWidget(m_frame_spin);

    auto* right_spacer = new QWidget(m_toolbar);
    right_spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    m_toolbar->addWidget(right_spacer);

    // ---- RIGHT zone: timeline range + detect ----------------------
    auto* start_tag = new QLabel(QStringLiteral("Start"), m_toolbar);
    start_tag->setStyleSheet(kTagQss);
    m_toolbar->addWidget(start_tag);
    m_start_spin = new QSpinBox(m_toolbar);
    m_start_spin->setStyleSheet(kSpinQss);
    m_start_spin->setRange(-10000, 100000);
    m_start_spin->setValue(m_state ? m_state->getStart_frame() : 0);
    m_start_spin->setToolTip(QStringLiteral("Start frame (timeline range)"));
    QObject::connect(m_start_spin, QOverload<int>::of(&QSpinBox::valueChanged),
        this, &TimelineWidget::on_start_spin_changed);
    m_toolbar->addWidget(m_start_spin);

    auto* end_tag = new QLabel(QStringLiteral("End"), m_toolbar);
    end_tag->setStyleSheet(kTagQss);
    m_toolbar->addWidget(end_tag);
    m_end_spin = new QSpinBox(m_toolbar);
    m_end_spin->setStyleSheet(kSpinQss);
    m_end_spin->setRange(-10000, 100000);
    m_end_spin->setValue(m_state ? m_state->getEnd_frame() : 100);
    m_end_spin->setToolTip(QStringLiteral("End frame (timeline range)"));
    QObject::connect(m_end_spin, QOverload<int>::of(&QSpinBox::valueChanged),
        this, &TimelineWidget::on_end_spin_changed);
    m_toolbar->addWidget(m_end_spin);

    m_detect_action = m_toolbar->addAction(QStringLiteral("⇅"));
    m_detect_action->setToolTip(QStringLiteral(
        "Detect range + FPS from the loaded USD stage\n"
        "(reads startTimeCode / endTimeCode / timeCodesPerSecond)"));
    QObject::connect(m_detect_action, &QAction::triggered,
        this, &TimelineWidget::on_detect_clicked);

    layout->addWidget(m_toolbar);

    m_ruler = new TimelineRuler(m_state, this);
    layout->addWidget(m_ruler, 1);

    if (m_state) {
        QObject::connect(m_state, &BifShellState::current_frameChanged,
            this, &TimelineWidget::on_state_changed);
        QObject::connect(m_state, &BifShellState::start_frameChanged,
            this, &TimelineWidget::on_state_changed);
        QObject::connect(m_state, &BifShellState::end_frameChanged,
            this, &TimelineWidget::on_state_changed);
        QObject::connect(m_state, &BifShellState::playback_fpsChanged,
            this, &TimelineWidget::on_state_changed);
        QObject::connect(m_state, &BifShellState::realtime_playbackChanged,
            this, &TimelineWidget::on_state_changed);
        QObject::connect(m_state, &BifShellState::loop_playbackChanged,
            this, &TimelineWidget::on_state_changed);
        QObject::connect(m_state, &BifShellState::is_playingChanged,
            this, &TimelineWidget::on_state_changed);
    }
    on_state_changed();
}

TimelineWidget::~TimelineWidget() = default;

void TimelineWidget::on_play_toggled() {
    if (m_state) m_state->toggle_playback();
}

void TimelineWidget::on_frame_spin_changed(int value) {
    if (m_state && value != m_state->getCurrent_frame()) {
        m_state->setCurrent_frame(value);
    }
}

void TimelineWidget::on_state_changed() {
    if (!m_state) return;
    const int start = m_state->getStart_frame();
    const int end = m_state->getEnd_frame();
    const int cur = m_state->getCurrent_frame();
    const int fps = m_state->getPlayback_fps();
    const bool realtime = m_state->getRealtime_playback();
    const bool playing = m_state->getIs_playing();

    {
        QSignalBlocker b(m_frame_spin);
        m_frame_spin->setRange(start, end);
        m_frame_spin->setValue(cur);
    }
    if (m_start_spin) {
        QSignalBlocker b(m_start_spin);
        m_start_spin->setValue(start);
    }
    if (m_end_spin) {
        QSignalBlocker b(m_end_spin);
        m_end_spin->setValue(end);
    }
    if (m_fps_spin) {
        QSignalBlocker b(m_fps_spin);
        m_fps_spin->setValue(fps);
    }
    if (m_realtime_action) {
        QSignalBlocker b(m_realtime_action);
        m_realtime_action->setChecked(realtime);
    }
    if (m_loop_action) {
        QSignalBlocker b(m_loop_action);
        m_loop_action->setChecked(m_state->getLoop_playback());
    }
    {
        QSignalBlocker b(m_play_action);
        m_play_action->setChecked(playing);
        m_play_action->setText(playing
            ? QStringLiteral("⏸  Pause")
            : QStringLiteral("▶  Play"));
    }
}

void TimelineWidget::on_start_spin_changed(int value) {
    if (!m_state || value == m_state->getStart_frame()) return;
    m_state->setStart_frame(value);
    // Keep end >= start; bump end if user pushed start past it.
    if (value > m_state->getEnd_frame()) {
        m_state->setEnd_frame(value);
    }
    // Clamp current_frame into the new range.
    if (m_state->getCurrent_frame() < value) {
        m_state->setCurrent_frame(value);
    }
}

void TimelineWidget::on_end_spin_changed(int value) {
    if (!m_state || value == m_state->getEnd_frame()) return;
    m_state->setEnd_frame(value);
    if (value < m_state->getStart_frame()) {
        m_state->setStart_frame(value);
    }
    if (m_state->getCurrent_frame() > value) {
        m_state->setCurrent_frame(value);
    }
}

void TimelineWidget::on_fps_spin_changed(int value) {
    if (m_state && value != m_state->getPlayback_fps()) {
        m_state->setPlayback_fps(value);
    }
}

void TimelineWidget::on_detect_clicked() {
    if (m_state) m_state->detect_timeline_from_stage();
}
