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
      m_prev_action(nullptr),
      m_play_action(nullptr),
      m_next_action(nullptr),
      m_frame_spin(nullptr),
      m_range_label(nullptr) {
    setObjectName(QStringLiteral("timeline_widget"));

    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_toolbar = new QToolBar(this);
    m_toolbar->setMovable(false);
    m_toolbar->setFloatable(false);
    m_toolbar->setIconSize(QSize(0, 0));
    m_toolbar->setStyleSheet(QStringLiteral(
        "QToolBar { background-color: rgba(42, 47, 54, 255); border: none;"
        "           border-bottom: 1px solid rgba(20, 22, 26, 255);"
        "           padding: 4px 6px; spacing: 4px; }"
        "QToolButton { background-color: transparent;"
        "              color: rgba(220, 222, 226, 255);"
        "              border: 1px solid rgba(60, 65, 75, 180);"
        "              border-radius: 3px; padding: 3px 8px; font-size: 11px; }"
        "QToolButton:hover { background-color: rgba(51, 56, 64, 255); }"
        "QToolButton:checked { background-color: rgba(74, 144, 217, 120);"
        "                      border-color: rgba(74, 144, 217, 240); }"));

    m_prev_action = m_toolbar->addAction(QStringLiteral("◀"));
    m_prev_action->setToolTip(QStringLiteral("Previous frame"));
    QObject::connect(m_prev_action, &QAction::triggered, this, [this]() {
        if (m_state) m_state->step_frame(-1);
    });

    m_play_action = m_toolbar->addAction(QStringLiteral("▶  Play"));
    m_play_action->setCheckable(true);
    m_play_action->setToolTip(QStringLiteral("Play / Pause (Space)"));
    QObject::connect(m_play_action, &QAction::triggered, this, &TimelineWidget::on_play_toggled);

    m_next_action = m_toolbar->addAction(QStringLiteral("▶"));
    m_next_action->setToolTip(QStringLiteral("Next frame"));
    QObject::connect(m_next_action, &QAction::triggered, this, [this]() {
        if (m_state) m_state->step_frame(1);
    });

    m_toolbar->addSeparator();

    auto* frame_label = new QLabel(QStringLiteral(" Frame "), m_toolbar);
    frame_label->setStyleSheet(QStringLiteral("color: rgba(140, 145, 155, 255);"));
    m_toolbar->addWidget(frame_label);

    m_frame_spin = new QSpinBox(m_toolbar);
    m_frame_spin->setStyleSheet(QStringLiteral(
        "QSpinBox { background-color: rgba(30, 34, 40, 255);"
        "           color: rgba(220, 222, 226, 255);"
        "           border: 1px solid rgba(60, 65, 75, 180);"
        "           border-radius: 3px; padding: 2px 4px; min-width: 48px; }"));
    m_frame_spin->setRange(m_state ? m_state->getStart_frame() : 0,
                           m_state ? m_state->getEnd_frame() : 100);
    m_frame_spin->setValue(m_state ? m_state->getCurrent_frame() : 0);
    QObject::connect(m_frame_spin, QOverload<int>::of(&QSpinBox::valueChanged),
        this, &TimelineWidget::on_frame_spin_changed);
    m_toolbar->addWidget(m_frame_spin);

    m_range_label = new QLabel(QStringLiteral(""), m_toolbar);
    m_range_label->setStyleSheet(QStringLiteral(
        "color: rgba(140, 145, 155, 255); padding: 0 10px;"));
    m_toolbar->addWidget(m_range_label);

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
    const bool playing = m_state->getIs_playing();

    {
        QSignalBlocker b(m_frame_spin);
        m_frame_spin->setRange(start, end);
        m_frame_spin->setValue(cur);
    }
    m_range_label->setText(QStringLiteral("[%1 – %2]").arg(start).arg(end));
    {
        QSignalBlocker b(m_play_action);
        m_play_action->setChecked(playing);
        m_play_action->setText(playing
            ? QStringLiteral("⏸  Pause")
            : QStringLiteral("▶  Play"));
    }
}
