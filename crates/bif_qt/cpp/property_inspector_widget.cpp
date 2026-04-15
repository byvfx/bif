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

// Fake attributes per prim type. Phase E replaces with UsdPrim::GetAttributes.
struct FakeAttr {
    const char* name;
    const char* value;
    const char* type_name;
    int winning_layer;  // color index
};

const std::vector<FakeAttr>& attrs_for_type(const QString& prim_type) {
    // -1 = no dot (attribute unauthored anywhere). Variable per type.
    static const std::vector<FakeAttr> mesh = {
        {"points", "(432 float3 values)", "point3f[]", 2},
        {"faceVertexCounts", "(240 int values)", "int[]", 2},
        {"faceVertexIndices", "(720 int values)", "int[]", 2},
        {"normals", "(432 float3 values)", "normal3f[]", 2},
        {"extent", "[(-1, -1, -1), (1, 1, 1)]", "float3[2]", 2},
        {"primvars:displayColor", "[(0.8, 0.7, 0.6)]", "color3f[]", 1},
        {"visibility", "\"inherited\"", "token", -1},
    };
    static const std::vector<FakeAttr> xform = {
        {"xformOp:translate", "(0, 0, 0)", "double3", 1},
        {"xformOp:rotateXYZ", "(0, 0, 0)", "float3", 1},
        {"xformOp:scale", "(1, 1, 1)", "float3", 1},
        {"xformOpOrder", "[\"xformOp:translate\", \"xformOp:rotateXYZ\", \"xformOp:scale\"]", "token[]", 0},
        {"visibility", "\"inherited\"", "token", -1},
    };
    static const std::vector<FakeAttr> light = {
        {"inputs:intensity", "1.0", "float", 1},
        {"inputs:exposure", "0.0", "float", -1},
        {"inputs:color", "(1, 1, 1)", "color3f", 2},
        {"inputs:angle", "0.53", "float", 2},
        {"visibility", "\"inherited\"", "token", -1},
    };
    static const std::vector<FakeAttr> scope = {
        {"visibility", "\"inherited\"", "token", -1},
        {"purpose", "\"default\"", "token", -1},
    };
    static const std::vector<FakeAttr> skeleton = {
        {"joints", "[\"Root\", \"Hip\", \"Spine\", \"Head\"]", "token[]", 1},
        {"bindTransforms", "(4 matrix4d values)", "matrix4d[]", 1},
        {"restTransforms", "(4 matrix4d values)", "matrix4d[]", 1},
    };
    static const std::vector<FakeAttr> empty;

    if (prim_type == QLatin1String("Mesh")) return mesh;
    if (prim_type == QLatin1String("Xform")) return xform;
    if (prim_type == QLatin1String("DistantLight") ||
        prim_type == QLatin1String("DomeLight")) return light;
    if (prim_type == QLatin1String("Scope") ||
        prim_type == QLatin1String("SkelRoot")) return scope;
    if (prim_type == QLatin1String("Skeleton")) return skeleton;
    return empty;
}

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
    m_header_type->setText(type.isEmpty() ? QStringLiteral("") : QStringLiteral("[%1]").arg(type));

    populate_composition_arcs(path);
    populate_attributes(path, type);
}

void PropertyInspectorWidget::populate_composition_arcs(const QString& prim_path) {
    if (!m_state) return;
    m_arcs_list->clear();
    if (prim_path.isEmpty()) return;

    const int n = m_state->layer_count();
    if (n == 0) {
        auto* item = new QListWidgetItem(
            QStringLiteral("(no layer stack — seed demo data)"),
            m_arcs_list);
        item->setFlags(Qt::NoItemFlags);
        return;
    }

    // Phase C.3 stub: list every layer in the stack ordered
    // strongest-first, with a specifier tag derived from index
    // (first = def, rest = over). Phase E replaces with the real
    // `UsdStage::get_prim_stack(prim_path)` result.
    for (int i = 0; i < n; ++i) {
        const auto name = m_state->layer_name_at(i);
        const auto spec = (i == 0) ? QStringLiteral("def") : QStringLiteral("over");
        auto* item = new QListWidgetItem(
            QStringLiteral("  ▸ %1   %2")
                .arg(spec, -6)
                .arg(name),
            m_arcs_list);
        if (i == 0) {
            QFont f = item->font();
            f.setBold(true);
            item->setFont(f);
        }
    }
}

void PropertyInspectorWidget::populate_attributes(const QString& /*prim_path*/,
                                                  const QString& prim_type) {
    m_attrs_model->removeRows(0, m_attrs_model->rowCount());
    const auto& attrs = attrs_for_type(prim_type);
    for (const auto& attr : attrs) {
        auto* name_item = new QStandardItem(QString::fromLatin1(attr.name));
        name_item->setData(attr.winning_layer, ColorIndexRole);
        auto* value_item = new QStandardItem(QString::fromLatin1(attr.value));
        auto* type_item = new QStandardItem(QString::fromLatin1(attr.type_name));
        type_item->setForeground(QColor(140, 145, 155));
        m_attrs_model->appendRow({name_item, value_item, type_item});
    }
}
