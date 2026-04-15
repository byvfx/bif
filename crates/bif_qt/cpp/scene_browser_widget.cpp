#include "scene_browser_widget.h"
#include "scene_browser_model.h"

#include <QColor>
#include <QHeaderView>
#include <QLineEdit>
#include <QPainter>
#include <QSortFilterProxyModel>
#include <QStyledItemDelegate>
#include <QTreeView>
#include <QVBoxLayout>

#include "bif_qt/src/main_window.cxxqt.h"

namespace {

// Mirror layer_stack_widget's palette; future Phase D bridge can
// share this through a single source of truth in Rust theme.rs.
constexpr QColor LAYER_PALETTE[8] = {
    QColor(80, 190, 180),
    QColor(180, 120, 220),
    QColor(230, 150, 70),
    QColor(220, 190, 80),
    QColor(230, 130, 180),
    QColor(90, 150, 230),
    QColor(120, 200, 100),
    QColor(220, 100, 100),
};

// Tree-row delegate that paints a small color dot before the
// branch decoration. Indent is handled natively by QTreeView so
// we only reserve `kDotPad + kDotDiameter` on the left.
class PrimRowDelegate : public QStyledItemDelegate {
public:
    using QStyledItemDelegate::QStyledItemDelegate;

    static constexpr int kDotDiameter = 6;
    static constexpr int kDotPad = 6;
    static constexpr int kReserved = kDotDiameter + kDotPad;

    void paint(QPainter* painter,
               const QStyleOptionViewItem& option,
               const QModelIndex& index) const override {
        const int color_index = index.data(SceneBrowserModel::ColorIndexRole).toInt();

        QStyleOptionViewItem adjusted = option;
        adjusted.rect.setLeft(option.rect.left() + kReserved);
        QStyledItemDelegate::paint(painter, adjusted, index);

        if (color_index >= 0 && color_index < 8) {
            painter->save();
            painter->setRenderHint(QPainter::Antialiasing, true);
            const auto color = LAYER_PALETTE[color_index];
            const int cy = option.rect.center().y();
            const int cx = option.rect.left() + kDotDiameter / 2 + 2;
            painter->setPen(Qt::NoPen);
            painter->setBrush(color);
            painter->drawEllipse(QPoint(cx, cy), kDotDiameter / 2, kDotDiameter / 2);
            painter->restore();
        }
    }

    QSize sizeHint(const QStyleOptionViewItem& option,
                   const QModelIndex& index) const override {
        QSize base = QStyledItemDelegate::sizeHint(option, index);
        base.setHeight(qMax(26, base.height()));
        base.setWidth(base.width() + kReserved);
        return base;
    }
};

// QSortFilterProxyModel that accepts a row if it OR any descendant
// matches the filter — keeps parents visible during filtering so
// hierarchy context survives.
class HierarchicalFilter : public QSortFilterProxyModel {
public:
    using QSortFilterProxyModel::QSortFilterProxyModel;

protected:
    bool filterAcceptsRow(int source_row,
                          const QModelIndex& source_parent) const override {
        if (filter_accepts_row_self(source_row, source_parent)) {
            return true;
        }
        const auto idx = sourceModel()->index(source_row, 0, source_parent);
        const int n = sourceModel()->rowCount(idx);
        for (int i = 0; i < n; ++i) {
            if (filterAcceptsRow(i, idx)) return true;
        }
        return false;
    }

private:
    bool filter_accepts_row_self(int source_row,
                                 const QModelIndex& source_parent) const {
        return QSortFilterProxyModel::filterAcceptsRow(source_row, source_parent);
    }
};

}  // namespace

SceneBrowserWidget::SceneBrowserWidget(BifShellState* state, QWidget* parent)
    : QWidget(parent),
      m_state(state),
      m_model(nullptr),
      m_filter(nullptr),
      m_search(nullptr),
      m_view(nullptr) {
    setObjectName(QStringLiteral("scene_browser_widget"));

    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_search = new QLineEdit(this);
    m_search->setPlaceholderText(QStringLiteral("Filter prims..."));
    m_search->setClearButtonEnabled(true);
    m_search->setStyleSheet(QStringLiteral(
        "QLineEdit {"
        "  background-color: rgba(30, 34, 40, 255);"
        "  color: rgba(220, 222, 226, 255);"
        "  border: none;"
        "  border-bottom: 1px solid rgba(20, 22, 26, 255);"
        "  padding: 6px 10px;"
        "  font-size: 12px;"
        "}"));
    layout->addWidget(m_search);

    m_view = new QTreeView(this);
    m_view->setObjectName(QStringLiteral("scene_browser_view"));
    m_view->setHeaderHidden(true);
    m_view->setUniformRowHeights(true);
    m_view->setSelectionMode(QAbstractItemView::SingleSelection);
    m_view->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_view->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_view->setItemDelegate(new PrimRowDelegate(m_view));
    m_view->setStyleSheet(QStringLiteral(
        "QTreeView {"
        "  background-color: rgba(34, 38, 44, 255);"
        "  color: rgba(220, 222, 226, 255);"
        "  border: none; padding: 2px; outline: none;"
        "}"
        "QTreeView::item:selected { background-color: rgba(74, 144, 217, 80); }"));
    layout->addWidget(m_view, 1);

    m_model = new SceneBrowserModel(this);
    m_filter = new HierarchicalFilter(this);
    m_filter->setSourceModel(m_model);
    m_filter->setFilterCaseSensitivity(Qt::CaseInsensitive);
    m_filter->setRecursiveFilteringEnabled(false);  // We do our own.
    m_view->setModel(m_filter);
    m_view->expandAll();

    QObject::connect(m_search, &QLineEdit::textChanged,
        this, &SceneBrowserWidget::on_filter_changed);
    QObject::connect(m_view->selectionModel(), &QItemSelectionModel::currentChanged,
        this, &SceneBrowserWidget::on_selection_changed);
}

SceneBrowserWidget::~SceneBrowserWidget() = default;

void SceneBrowserWidget::on_filter_changed(const QString& text) {
    m_filter->setFilterFixedString(text);
    m_view->expandAll();  // re-expand so newly-included rows are visible
}

void SceneBrowserWidget::on_selection_changed(const QModelIndex& current,
                                              const QModelIndex& /*previous*/) {
    if (!m_state || !current.isValid()) return;
    const auto path = current.data(SceneBrowserModel::PathRole).toString();
    const auto type = current.data(SceneBrowserModel::TypeNameRole).toString();
    // Feed the selection through to BifShellState — property
    // inspector (Phase C.3) listens on selected_prim_pathChanged.
    // Phase E adds real AppEvent::PrimSelected dispatch for the
    // viewport highlight + downstream handlers.
    m_state->setSelected_prim_path(path);
    m_state->setSelected_prim_type(type);
    m_state->setStatus_message(
        QStringLiteral("Selected: %1  [%2]").arg(path).arg(type));
}
