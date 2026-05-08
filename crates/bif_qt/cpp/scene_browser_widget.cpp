#include "scene_browser_widget.h"
#include "scene_browser_model.h"

#include <QColor>
#include <QHeaderView>
#include <QItemSelectionModel>
#include <QLineEdit>
#include <QMouseEvent>
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

// Tree-row delegate that paints visibility in its own column and a
// small layer-color dot before the prim name.
// Inactive prims (`is_active == false`) are dimmed across all columns.
class PrimRowDelegate : public QStyledItemDelegate {
public:
    using QStyledItemDelegate::QStyledItemDelegate;

    static constexpr int kDotDiameter = 6;
    static constexpr int kDotPad = 6;
    static constexpr int kEyeWidth = 12;
    static constexpr int kVisibilityColumnWidth = 26;
    static constexpr int kNameReserved = kDotDiameter + kDotPad;

    void paint(QPainter* painter,
               const QStyleOptionViewItem& option,
               const QModelIndex& index) const override {
        const bool is_active = index.data(SceneBrowserModel::IsActiveRole).toBool();
        const bool is_visible = index.data(SceneBrowserModel::IsVisibleRole).toBool();
        const int color_index = index.data(SceneBrowserModel::ColorIndexRole).toInt();
        const bool is_visibility_col = index.column() == SceneBrowserModel::ColVisibility;
        const bool is_name_col = index.column() == SceneBrowserModel::ColName;

        QStyleOptionViewItem adjusted = option;
        if (is_name_col) {
            adjusted.rect.setLeft(option.rect.left() + kNameReserved);
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

        if (!is_visibility_col && !is_name_col) return;

        painter->save();
        painter->setRenderHint(QPainter::Antialiasing, true);
        const int cy = option.rect.center().y();

        if (is_visibility_col) {
            const int ex = option.rect.center().x() - kEyeWidth / 2;
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
        }

        if (is_name_col && color_index >= 0 && color_index < 8) {
            auto color = LAYER_PALETTE[color_index];
            if (!is_active) color.setAlpha(130);
            const int cx = option.rect.left() + 2 + kDotDiameter / 2;
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
        if (index.column() == SceneBrowserModel::ColVisibility) {
            base.setWidth(kVisibilityColumnWidth);
        }
        if (index.column() == SceneBrowserModel::ColName) {
            base.setWidth(base.width() + kNameReserved);
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
    m_view->viewport()->installEventFilter(this);
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
    m_view->setTreePosition(SceneBrowserModel::ColName);
    auto* header = m_view->header();
    header->setSectionResizeMode(SceneBrowserModel::ColVisibility, QHeaderView::Fixed);
    header->resizeSection(SceneBrowserModel::ColVisibility, PrimRowDelegate::kVisibilityColumnWidth);
    header->setSectionResizeMode(SceneBrowserModel::ColName, QHeaderView::Stretch);
    header->setSectionResizeMode(SceneBrowserModel::ColType, QHeaderView::ResizeToContents);
    header->setSectionResizeMode(SceneBrowserModel::ColChildren, QHeaderView::ResizeToContents);
    header->setSectionResizeMode(SceneBrowserModel::ColKind, QHeaderView::ResizeToContents);
    header->setStretchLastSection(false);

    QObject::connect(m_search, &QLineEdit::textChanged,
        this, &SceneBrowserWidget::on_filter_changed);
    QObject::connect(m_view->selectionModel(), &QItemSelectionModel::currentChanged,
        this, &SceneBrowserWidget::on_selection_changed);
    if (m_state) {
        QObject::connect(m_state, &BifShellState::selected_prim_pathChanged,
            this, &SceneBrowserWidget::on_external_selection_changed);
    }
    QObject::connect(m_view, &QTreeView::expanded, this,
        [this](const QModelIndex& proxy_index) {
            if (!m_model || !m_filter) return;
            const auto source_index = m_filter->mapToSource(proxy_index);
            if (source_index.isValid() && m_model->canFetchMore(source_index)) {
                m_model->fetchMore(source_index);
            }
        });

    // Save/restore expanded state across model resets (visibility toggle, reload).
    QObject::connect(m_model, &QAbstractItemModel::modelAboutToBeReset, this,
        [this]() {
            m_expanded_paths.clear();
            save_expanded_state(m_view->rootIndex());
        });
    QObject::connect(m_model, &QAbstractItemModel::modelReset, this,
        [this]() {
            restore_expanded_state(m_view->rootIndex());
        });
}

SceneBrowserWidget::~SceneBrowserWidget() = default;

QModelIndex SceneBrowserWidget::find_source_index_for_path(const QString& path,
                                                           const QModelIndex& parent) {
    if (!m_model) return {};
    const int rows = m_model->rowCount(parent);
    for (int row = 0; row < rows; ++row) {
        const QModelIndex idx = m_model->index(row, SceneBrowserModel::ColName, parent);
        if (!idx.isValid()) continue;
        if (idx.data(SceneBrowserModel::PathRole).toString() == path) {
            return idx;
        }
        if (m_model->canFetchMore(idx)) {
            m_model->fetchMore(idx);
        }
        const QModelIndex child = find_source_index_for_path(path, idx);
        if (child.isValid()) return child;
    }
    return {};
}

void SceneBrowserWidget::select_path(const QString& path) {
    if (!m_view || !m_model || !m_filter || path.isEmpty()) return;
    const QModelIndex source_idx = find_source_index_for_path(path);
    if (!source_idx.isValid()) return;
    QModelIndex proxy_idx = m_filter->mapFromSource(source_idx);
    if (!proxy_idx.isValid()) return;
    if (m_view->currentIndex() == proxy_idx) return;

    QModelIndex parent = proxy_idx.parent();
    while (parent.isValid()) {
        m_view->expand(parent);
        parent = parent.parent();
    }
    m_syncing_external_selection = true;
    m_view->selectionModel()->setCurrentIndex(
        proxy_idx,
        QItemSelectionModel::ClearAndSelect | QItemSelectionModel::Rows);
    m_syncing_external_selection = false;
    m_view->scrollTo(proxy_idx, QAbstractItemView::PositionAtCenter);
}

bool SceneBrowserWidget::eventFilter(QObject* watched, QEvent* event) {
    if (watched != m_view->viewport()
        || event->type() != QEvent::MouseButtonRelease
        || !m_state) {
        return QWidget::eventFilter(watched, event);
    }

    auto* mouse_event = static_cast<QMouseEvent*>(event);
    if (mouse_event->button() != Qt::LeftButton) {
        return QWidget::eventFilter(watched, event);
    }

    const QModelIndex index = m_view->indexAt(mouse_event->pos());
    if (!index.isValid() || index.column() != SceneBrowserModel::ColVisibility) {
        return QWidget::eventFilter(watched, event);
    }

    const QRect rect = m_view->visualRect(index);
    const int cy = rect.center().y();
    const int ex = rect.center().x() - PrimRowDelegate::kEyeWidth / 2;
    const QRectF eye_rect(ex, cy - 4, PrimRowDelegate::kEyeWidth, 8);
    if (!eye_rect.contains(mouse_event->pos())) {
        return QWidget::eventFilter(watched, event);
    }

    const QString path = index.data(SceneBrowserModel::PathRole).toString();
    if (!path.isEmpty()) {
        const bool is_visible = index.data(SceneBrowserModel::IsVisibleRole).toBool();
        m_state->on_set_visibility(path, !is_visible);
    }
    return true;
}

void SceneBrowserWidget::on_filter_changed(const QString& text) {
    m_filter->setFilterFixedString(text);
    if (!text.isEmpty()) {
        m_view->expandAll();  // expose newly-included rows while filtering
    }
}

void SceneBrowserWidget::on_external_selection_changed() {
    if (!m_state) return;
    select_path(m_state->getSelected_prim_path());
}

void SceneBrowserWidget::on_selection_changed(const QModelIndex& current,
                                              const QModelIndex& /*previous*/) {
    if (m_syncing_external_selection) return;
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

void SceneBrowserWidget::save_expanded_state(const QModelIndex& parent) {
    if (!m_filter || !m_model) return;
    const int count = m_filter->rowCount(parent);
    for (int r = 0; r < count; ++r) {
        const QModelIndex idx = m_filter->index(r, 0, parent);
        if (m_view->isExpanded(idx)) {
            const auto src = m_filter->mapToSource(idx);
            const QString path = m_model->data(src, SceneBrowserModel::PathRole).toString();
            if (!path.isEmpty()) {
                m_expanded_paths.insert(path);
            }
            save_expanded_state(idx);
        }
    }
}

void SceneBrowserWidget::restore_expanded_state(const QModelIndex& parent) {
    if (!m_filter || !m_model) return;
    const int count = m_filter->rowCount(parent);
    for (int r = 0; r < count; ++r) {
        const QModelIndex idx = m_filter->index(r, 0, parent);
        const auto src = m_filter->mapToSource(idx);
        const QString path = m_model->data(src, SceneBrowserModel::PathRole).toString();
        if (m_expanded_paths.contains(path)) {
            m_view->expand(idx);
            // Fetch children if needed
            if (m_model->canFetchMore(src)) {
                m_model->fetchMore(src);
            }
            restore_expanded_state(idx);
        }
    }
}
