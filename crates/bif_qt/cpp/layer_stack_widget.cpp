#include "layer_stack_widget.h"
#include "layer_stack_model.h"

#include <QAction>
#include <QColor>
#include <QListView>
#include <QPainter>
#include <QStyledItemDelegate>
#include <QToolBar>
#include <QVBoxLayout>

#include "bif_qt/src/main_window.cxxqt.h"

namespace {

// 8-color layer palette mirroring bif_qt::theme::LAYER_COLORS /
// bif_viewport::theme::LAYER_COLORS. Kept in sync by duplication
// for now — a Phase D follow-up can bridge the Rust theme table
// through cxx if this drifts.
constexpr QColor LAYER_PALETTE[8] = {
    QColor(80, 190, 180),   // teal
    QColor(180, 120, 220),  // purple
    QColor(230, 150, 70),   // orange
    QColor(220, 190, 80),   // gold
    QColor(230, 130, 180),  // pink
    QColor(90, 150, 230),   // blue
    QColor(120, 200, 100),  // green
    QColor(220, 100, 100),  // red
};

// Delegate that paints a color dot + indent on the left, then
// delegates to the base paint with a left-shifted rect so the
// checkbox + text appear after the dot. The editorEvent override
// shifts the rect identically so checkbox hit-testing aligns with
// the visible checkbox position (Qt's default uses option.rect to
// compute SE_ItemViewItemCheckIndicator).
class LayerRowDelegate : public QStyledItemDelegate {
public:
    using QStyledItemDelegate::QStyledItemDelegate;

    static constexpr int kIndentPerLevel = 14;
    static constexpr int kDotDiameter = 8;
    static constexpr int kDotPad = 8;

    static int left_reserved_for(const QModelIndex& index) {
        const int depth = qMax(0, index.data(LayerStackModel::DepthRole).toInt());
        return depth * kIndentPerLevel + kDotDiameter + kDotPad;
    }

    void paint(QPainter* painter,
               const QStyleOptionViewItem& option,
               const QModelIndex& index) const override {
        const int depth = qMax(0, index.data(LayerStackModel::DepthRole).toInt());
        const int color_index = index.data(LayerStackModel::ColorIndexRole).toInt();
        const int indent_px = depth * kIndentPerLevel;
        const int left_reserved = left_reserved_for(index);

        QStyleOptionViewItem adjusted = option;
        adjusted.rect.setLeft(option.rect.left() + left_reserved);
        QStyledItemDelegate::paint(painter, adjusted, index);

        if (color_index >= 0 && color_index < 8) {
            painter->save();
            painter->setRenderHint(QPainter::Antialiasing, true);
            const auto color = LAYER_PALETTE[color_index];
            const int cy = option.rect.center().y();
            const int cx = option.rect.left() + indent_px + kDotDiameter / 2 + 2;
            painter->setPen(Qt::NoPen);
            painter->setBrush(color);
            painter->drawEllipse(QPoint(cx, cy), kDotDiameter / 2, kDotDiameter / 2);
            painter->restore();
        }
    }

    bool editorEvent(QEvent* event,
                     QAbstractItemModel* model,
                     const QStyleOptionViewItem& option,
                     const QModelIndex& index) override {
        // Mirror the rect shift from paint() so checkbox hit-testing
        // lines up with where the checkbox actually rendered. Without
        // this, clicks miss the visible checkbox intermittently.
        QStyleOptionViewItem adjusted = option;
        adjusted.rect.setLeft(option.rect.left() + left_reserved_for(index));
        return QStyledItemDelegate::editorEvent(event, model, adjusted, index);
    }

    QSize sizeHint(const QStyleOptionViewItem& option,
                   const QModelIndex& index) const override {
        QSize base = QStyledItemDelegate::sizeHint(option, index);
        base.setHeight(qMax(28, base.height()));
        // Account for indent + dot reservation in width hint.
        base.setWidth(base.width() + left_reserved_for(index));
        return base;
    }
};

}  // namespace

LayerStackWidget::LayerStackWidget(BifShellState* state, QWidget* parent)
    : QWidget(parent),
      m_state(state),
      m_model(nullptr),
      m_view(nullptr),
      m_toolbar(nullptr),
      m_isolation_action(nullptr) {
    setObjectName(QStringLiteral("layer_stack_widget"));

    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_toolbar = new QToolBar(this);
    m_toolbar->setMovable(false);
    m_toolbar->setFloatable(false);
    m_toolbar->setIconSize(QSize(0, 0));
    // QToolBar chrome comes from theme.rs (Graphite). Per-button overrides
    // for the toggle/check state stay local.
    m_toolbar->setStyleSheet(QStringLiteral(
        "QToolButton { background-color: transparent;"
        "              color: rgba(229, 226, 225, 255);"
        "              border: 1px solid rgba(65, 71, 82, 180);"
        "              border-radius: 6px; padding: 3px 8px; font-size: 11px; }"
        "QToolButton:hover { background-color: rgba(53, 53, 53, 255); }"
        "QToolButton:checked { background-color: rgba(74, 158, 255, 120);"
        "                      border-color: rgba(74, 158, 255, 240); }"));

    m_isolation_action = m_toolbar->addAction(QStringLiteral("Isolation"));
    m_isolation_action->setCheckable(true);
    m_isolation_action->setToolTip(
        QStringLiteral("Isolate the working layer's opinions visually (Phase E wires the render hook)."));
    QObject::connect(m_isolation_action, &QAction::toggled, this,
        [this](bool) {
            if (m_state) m_state->toggle_isolation_mode();
        });

    layout->addWidget(m_toolbar);

    m_view = new QListView(this);
    m_view->setObjectName(QStringLiteral("layer_stack_view"));
    m_view->setAlternatingRowColors(false);
    m_view->setSelectionMode(QAbstractItemView::SingleSelection);
    m_view->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_view->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_view->setUniformItemSizes(true);
    m_view->setItemDelegate(new LayerRowDelegate(m_view));
    m_view->setStyleSheet(QStringLiteral(
        "QListView {"
        "  background-color: rgba(34, 38, 44, 255);"
        "  color: rgba(220, 222, 226, 255);"
        "  border: none; padding: 2px; outline: none;"
        "}"
        "QListView::item:selected { background-color: rgba(74, 144, 217, 80); }"));
    layout->addWidget(m_view, 1);

    m_model = new LayerStackModel(m_state, this);
    m_view->setModel(m_model);

    QObject::connect(m_view, &QListView::doubleClicked,
        this, &LayerStackWidget::on_row_double_clicked);

    if (m_state) {
        QObject::connect(m_state, &BifShellState::layer_state_revisionChanged,
            this, &LayerStackWidget::on_state_revision_changed);
        on_state_revision_changed();
    }
}

LayerStackWidget::~LayerStackWidget() = default;

void LayerStackWidget::on_row_double_clicked(const QModelIndex& index) {
    if (m_model) m_model->activate_as_working(index);
}

void LayerStackWidget::on_state_revision_changed() {
    if (!m_state || !m_isolation_action) return;
    const bool iso = m_state->isolation_mode_active();
    if (m_isolation_action->isChecked() != iso) {
        QSignalBlocker blocker(m_isolation_action);
        m_isolation_action->setChecked(iso);
    }
}
