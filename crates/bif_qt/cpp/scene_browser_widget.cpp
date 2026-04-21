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
// branch decoration, plus (col 0) an eye glyph reflecting visibility.
// Inactive prims (`is_active == false`) are dimmed across all columns.
// Indent is handled natively by QTreeView; we only reserve the chrome
// strip on column 0's left.
class PrimRowDelegate : public QStyledItemDelegate {
public:
    using QStyledItemDelegate::QStyledItemDelegate;

    static constexpr int kDotDiameter = 6;
    static constexpr int kDotPad = 6;
    static constexpr int kEyeWidth = 12;
    static constexpr int kEyePad = 4;
    static constexpr int kReserved = kEyeWidth + kEyePad + kDotDiameter + kDotPad;

    void paint(QPainter* painter,
               const QStyleOptionViewItem& option,
               const QModelIndex& index) const override {
        const bool is_active = index.data(SceneBrowserModel::IsActiveRole).toBool();
        const bool is_visible = index.data(SceneBrowserModel::IsVisibleRole).toBool();
        const int color_index = index.data(SceneBrowserModel::ColorIndexRole).toInt();
        const bool is_name_col = index.column() == SceneBrowserModel::ColName;

        QStyleOptionViewItem adjusted = option;
        if (is_name_col) {
            adjusted.rect.setLeft(option.rect.left() + kReserved);
        }

        // Inactive dimming — fade text across all columns.
        if (!is_active) {
            QPalette pal = adjusted.palette;
            const auto fg = pal.color(QPalette::Text);
            pal.setColor(QPalette::Text,        QColor(fg.red(), fg.green(), fg.blue(), 110));
            pal.setColor(QPalette::WindowText,  QColor(fg.red(), fg.green(), fg.blue(), 110));
            pal.setColor(QPalette::HighlightedText,
                         QColor(fg.red(), fg.green(), fg.blue(), 160));
            adjusted.palette = pal;
        }

        QStyledItemDelegate::paint(painter, adjusted, index);

        if (!is_name_col) return;

        painter->save();
        painter->setRenderHint(QPainter::Antialiasing, true);
        const int cy = option.rect.center().y();

        // Eye glyph (visibility) — flush left, before the color dot.
        const int ex = option.rect.left() + 2;
        const QRectF eye_rect(ex, cy - 4, kEyeWidth, 8);
        const QColor eye_fg = is_visible
            ? QColor(200, 204, 212, is_active ? 255 : 130)
            : QColor(110, 114, 122, is_active ? 200 : 110);
        painter->setPen(QPen(eye_fg, 1.0));
        painter->setBrush(Qt::NoBrush);
        painter->drawEllipse(eye_rect);
        painter->setBrush(eye_fg);
        painter->setPen(Qt::NoPen);
        painter->drawEllipse(QPointF(ex + kEyeWidth / 2.0, cy), 1.6, 1.6);
        if (!is_visible) {
            painter->setPen(QPen(eye_fg, 1.0));
            painter->drawLine(QPointF(ex - 1, cy + 5),
                              QPointF(ex + kEyeWidth + 1, cy - 5));
        }

        // Layer color dot (existing) — between eye and the row text.
        if (color_index >= 0 && color_index < 8) {
            auto color = LAYER_PALETTE[color_index];
            if (!is_active) color.setAlpha(130);
            const int cx = ex + kEyeWidth + kEyePad + kDotDiameter / 2;
            painter->setPen(Qt::NoPen);
            painter->setBrush(color);
            painter->drawEllipse(QPoint(cx, cy), kDotDiameter / 2, kDotDiameter / 2);
        }

        painter->restore();
    }

    QSize sizeHint(const QStyleOptionViewItem& option,
                   const QModelIndex& index) const override {
        QSize base = QStyledItemDelegate::sizeHint(option, index);
        base.setHeight(qMax(26, base.height()));
        if (index.column() == SceneBrowserModel::ColName) {
            base.setWidth(base.width() + kReserved);
        }
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
    m_view->setHeaderHidden(false);
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
        "QTreeView::item:selected { background-color: rgba(74, 144, 217, 80); }"
        "QHeaderView::section {"
        "  background-color: rgba(28, 32, 38, 255);"
        "  color: rgba(180, 184, 192, 255);"
        "  border: none;"
        "  border-right: 1px solid rgba(20, 22, 26, 255);"
        "  padding: 4px 8px;"
        "  font-size: 11px;"
        "}"));
    layout->addWidget(m_view, 1);

    m_model = new SceneBrowserModel(m_state, this);
    m_filter = new HierarchicalFilter(this);
    m_filter->setSourceModel(m_model);
    m_filter->setFilterCaseSensitivity(Qt::CaseInsensitive);
    m_filter->setRecursiveFilteringEnabled(false);  // We do our own.
    m_view->setModel(m_filter);
    auto* header = m_view->header();
    header->setSectionResizeMode(SceneBrowserModel::ColName, QHeaderView::Stretch);
    header->setSectionResizeMode(SceneBrowserModel::ColType, QHeaderView::ResizeToContents);
    header->setSectionResizeMode(SceneBrowserModel::ColChildren, QHeaderView::ResizeToContents);
    header->setSectionResizeMode(SceneBrowserModel::ColKind, QHeaderView::ResizeToContents);
    header->setStretchLastSection(false);
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
    // Route through Rust so the renderer-side selection path updates
    // the viewport gizmo + outline highlight. `on_tree_prim_selected`
    // also mirrors path/type into the shell qprops the property
    // inspector listens on (selected_prim_pathChanged).
    m_state->on_tree_prim_selected(path, type);
    m_state->setStatus_message(
        QStringLiteral("Selected: %1  [%2]").arg(path).arg(type));
}
