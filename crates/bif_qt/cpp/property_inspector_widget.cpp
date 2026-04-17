#include "property_inspector_widget.h"

#include <QColor>
#include <QGroupBox>
#include <QHeaderView>
#include <QLabel>
#include <QListWidget>
#include <QPainter>
#include <QStandardItemModel>
#include <QStyledItemDelegate>
#include <QTabWidget>
#include <QTableView>
#include <QVBoxLayout>

#include "bif_qt/src/main_window.cxxqt.h"

namespace {

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

// Custom role carrying the winning-layer color index for the Name
// column. The delegate paints a dot before the text using this.
constexpr int ColorIndexRole = Qt::UserRole + 1;

// Painter that draws a color dot at the left of the cell, then the
// default text at a shifted rect. Only active on column 0.
class OpinionDotDelegate : public QStyledItemDelegate {
public:
    using QStyledItemDelegate::QStyledItemDelegate;

    static constexpr int kDotDiameter = 6;
    static constexpr int kDotPad = 6;

    void paint(QPainter* painter,
               const QStyleOptionViewItem& option,
               const QModelIndex& index) const override {
        if (index.column() != 0) {
            QStyledItemDelegate::paint(painter, option, index);
            return;
        }

        const int color_index = index.data(ColorIndexRole).toInt();

        QStyleOptionViewItem adjusted = option;
        adjusted.rect.setLeft(option.rect.left() + kDotDiameter + kDotPad);
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
};

QLabel* make_dim_label(const QString& text, QWidget* parent) {
    auto* l = new QLabel(text, parent);
    l->setStyleSheet(QStringLiteral(
        "color: rgba(140, 145, 155, 255); font-size: 11px;"
        "letter-spacing: 1px; text-transform: uppercase;"));
    return l;
}

}  // namespace

PropertyInspectorWidget::PropertyInspectorWidget(BifShellState* state, QWidget* parent)
    : QWidget(parent),
      m_state(state),
      m_header_path(nullptr),
      m_header_type(nullptr),
      m_arcs_group(nullptr),
      m_arcs_list(nullptr),
      m_tabs(nullptr),
      m_attrs_view(nullptr),
      m_attrs_model(nullptr),
      m_relationships_tab(nullptr) {
    setObjectName(QStringLiteral("property_inspector_widget"));

    auto* outer = new QVBoxLayout(this);
    outer->setContentsMargins(8, 8, 8, 8);
    outer->setSpacing(6);

    // Header
    outer->addWidget(make_dim_label(QStringLiteral("Selected Prim"), this));
    m_header_path = new QLabel(QStringLiteral("(none)"), this);
    m_header_path->setStyleSheet(QStringLiteral(
        "color: rgba(220, 222, 226, 255); font-size: 13px; font-weight: 500;"));
    m_header_path->setWordWrap(true);
    outer->addWidget(m_header_path);
    m_header_type = new QLabel(QStringLiteral(""), this);
    m_header_type->setStyleSheet(QStringLiteral(
        "color: rgba(74, 144, 217, 255); font-size: 11px; font-weight: 400;"));
    outer->addWidget(m_header_type);

    // Composition Arcs (collapsible via checkable group box).
    m_arcs_group = new QGroupBox(QStringLiteral("Composition Arcs"), this);
    m_arcs_group->setCheckable(true);
    m_arcs_group->setChecked(true);
    m_arcs_group->setStyleSheet(QStringLiteral(
        "QGroupBox {"
        "  color: rgba(220, 222, 226, 255);"
        "  border: 1px solid rgba(60, 65, 75, 255);"
        "  border-radius: 4px;"
        "  margin-top: 12px; padding: 8px 6px 6px 6px;"
        "}"
        "QGroupBox::title {"
        "  subcontrol-origin: margin; left: 8px; padding: 0 4px;"
        "  color: rgba(140, 145, 155, 255); font-size: 11px;"
        "  text-transform: uppercase; letter-spacing: 1px;"
        "}"));
    auto* arcs_layout = new QVBoxLayout(m_arcs_group);
    arcs_layout->setContentsMargins(4, 6, 4, 4);
    m_arcs_list = new QListWidget(m_arcs_group);
    m_arcs_list->setMaximumHeight(110);
    m_arcs_list->setStyleSheet(QStringLiteral(
        "QListWidget {"
        "  background-color: rgba(30, 34, 40, 255);"
        "  color: rgba(220, 222, 226, 255);"
        "  border: none; padding: 2px; outline: none;"
        "}"
        "QListWidget::item { padding: 3px 6px; font-size: 11px; }"));
    arcs_layout->addWidget(m_arcs_list);
    QObject::connect(m_arcs_group, &QGroupBox::toggled, m_arcs_list, &QWidget::setVisible);
    outer->addWidget(m_arcs_group);

    // Tabs
    m_tabs = new QTabWidget(this);
    m_tabs->setDocumentMode(true);
    outer->addWidget(m_tabs, 1);

    // Attributes tab
    m_attrs_model = new QStandardItemModel(this);
    m_attrs_model->setHorizontalHeaderLabels(
        {QStringLiteral("Name"), QStringLiteral("Value"), QStringLiteral("Type")});
    m_attrs_view = new QTableView(m_tabs);
    m_attrs_view->setModel(m_attrs_model);
    m_attrs_view->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_attrs_view->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_attrs_view->setSelectionMode(QAbstractItemView::SingleSelection);
    m_attrs_view->setAlternatingRowColors(false);
    m_attrs_view->verticalHeader()->setVisible(false);
    m_attrs_view->horizontalHeader()->setStretchLastSection(false);
    m_attrs_view->horizontalHeader()->setSectionResizeMode(0, QHeaderView::ResizeToContents);
    m_attrs_view->horizontalHeader()->setSectionResizeMode(1, QHeaderView::Stretch);
    m_attrs_view->horizontalHeader()->setSectionResizeMode(2, QHeaderView::ResizeToContents);
    m_attrs_view->setItemDelegate(new OpinionDotDelegate(m_attrs_view));
    m_attrs_view->setStyleSheet(QStringLiteral(
        "QTableView {"
        "  background-color: rgba(34, 38, 44, 255);"
        "  color: rgba(220, 222, 226, 255);"
        "  gridline-color: rgba(50, 55, 62, 255);"
        "  border: none; outline: none;"
        "}"
        "QTableView::item { padding: 2px 4px; }"
        "QHeaderView::section {"
        "  background-color: rgba(42, 47, 54, 255);"
        "  color: rgba(140, 145, 155, 255);"
        "  border: none; padding: 4px 6px; font-size: 11px;"
        "}"));
    m_tabs->addTab(m_attrs_view, QStringLiteral("Attributes"));

    // Relationships tab — placeholder
    m_relationships_tab = new QWidget(m_tabs);
    auto* rel_layout = new QVBoxLayout(m_relationships_tab);
    auto* rel_note = new QLabel(
        QStringLiteral("(Relationships panel — Phase C.4)"),
        m_relationships_tab);
    rel_note->setAlignment(Qt::AlignCenter);
    rel_note->setStyleSheet(QStringLiteral(
        "color: rgba(140, 145, 155, 255); padding: 24px;"));
    rel_layout->addWidget(rel_note);
    rel_layout->addStretch();
    m_tabs->addTab(m_relationships_tab, QStringLiteral("Relationships"));

    if (m_state) {
        QObject::connect(m_state, &BifShellState::selected_prim_pathChanged,
            this, &PropertyInspectorWidget::on_selection_changed);
        QObject::connect(m_state, &BifShellState::layer_state_revisionChanged,
            this, &PropertyInspectorWidget::on_layer_state_changed);
    }

    rebuild();
}

PropertyInspectorWidget::~PropertyInspectorWidget() = default;

void PropertyInspectorWidget::on_selection_changed() {
    rebuild();
}

void PropertyInspectorWidget::on_layer_state_changed() {
    // Composition arcs depend on layer stack; refresh them.
    if (!m_state) return;
    populate_composition_arcs(m_state->getSelected_prim_path());
}

void PropertyInspectorWidget::rebuild() {
    if (!m_state) return;
    const auto path = m_state->getSelected_prim_path();
    const auto type = m_state->getSelected_prim_type();

    if (path.isEmpty()) {
        m_header_path->setText(QStringLiteral("(none)"));
        m_header_type->setText(QString());
        m_arcs_list->clear();
        m_attrs_model->removeRows(0, m_attrs_model->rowCount());
        return;
    }

    m_header_path->setText(path);
    // Friendly prim-type label ("Xform" → "Transform") with the raw
    // USD name in the tooltip so pipeline folks can still grep.
    if (type.isEmpty()) {
        m_header_type->setText(QStringLiteral(""));
        m_header_type->setToolTip(QString());
    } else {
        const auto friendly = m_state->friendly_prim_type(type);
        m_header_type->setText(QStringLiteral("[%1]").arg(friendly));
        m_header_type->setToolTip(QStringLiteral("USD schema: %1").arg(type));
    }

    populate_composition_arcs(path);
    populate_attributes(path, type);
}

void PropertyInspectorWidget::populate_composition_arcs(const QString& prim_path) {
    if (!m_state) return;
    m_arcs_list->clear();
    if (prim_path.isEmpty()) return;

    // Phase E.2 move 8: pull the real prim stack via
    // `UsdStage::get_prim_stack` behind BifShellState invokables.
    const int n = m_state->selected_prim_stack_count();
    if (n == 0) {
        auto* item = new QListWidgetItem(
            QStringLiteral("(no authored opinions)"),
            m_arcs_list);
        item->setFlags(Qt::NoItemFlags);
        return;
    }

    for (int i = 0; i < n; ++i) {
        const auto layer = m_state->selected_prim_stack_layer_at(i);
        const auto spec = m_state->selected_prim_stack_specifier_at(i);
        const bool has_opinion = m_state->selected_prim_stack_has_opinion_at(i);
        const int color_index = m_state->selected_prim_stack_color_index_at(i);

        QString label = QStringLiteral("  ▸ %1   %2")
            .arg(spec.isEmpty() ? QStringLiteral("?") : spec, -6)
            .arg(layer);
        if (!has_opinion) {
            label += QStringLiteral("   (inherited)");
        }

        auto* item = new QListWidgetItem(label, m_arcs_list);
        if (i == 0) {
            QFont f = item->font();
            f.setBold(true);
            item->setFont(f);
        }
        if (color_index >= 0 && color_index < 8) {
            item->setForeground(LAYER_PALETTE[color_index]);
        }
    }
}

void PropertyInspectorWidget::populate_attributes(const QString& prim_path,
                                                  const QString& /*prim_type*/) {
    m_attrs_model->removeRows(0, m_attrs_model->rowCount());
    if (!m_state || prim_path.isEmpty()) return;

    // Phase E.2 move 8: pull real authored attributes via
    // `UsdStage::get_prim_attributes` behind BifShellState invokables.
    const int n = m_state->selected_prim_attribute_count();
    for (int i = 0; i < n; ++i) {
        const auto name = m_state->selected_prim_attribute_name_at(i);
        const auto value = m_state->selected_prim_attribute_value_at(i);
        const auto type_name = m_state->selected_prim_attribute_type_at(i);

        // Tier 1 item #3: friendly attribute label, raw USD name in tooltip.
        const auto friendly = m_state->friendly_attribute_name(name);
        auto* name_item = new QStandardItem(friendly);
        if (friendly != name) {
            name_item->setToolTip(QStringLiteral("USD: %1").arg(name));
        }
        // Opinion dot color: derive from the winning layer in the
        // prim stack (strongest authored layer, index 0 when present).
        int winning_color = -1;
        if (m_state->selected_prim_stack_count() > 0) {
            winning_color = m_state->selected_prim_stack_color_index_at(0);
        }
        name_item->setData(winning_color, ColorIndexRole);
        auto* value_item = new QStandardItem(value);
        auto* type_item = new QStandardItem(type_name);
        type_item->setForeground(QColor(140, 145, 155));
        m_attrs_model->appendRow({name_item, value_item, type_item});
    }
}
