#include "property_inspector_widget.h"

#include <QCheckBox>
#include <QColor>
#include <QColorDialog>
#include <QComboBox>
#include <QDoubleSpinBox>
#include <QGroupBox>
#include <QHeaderView>
#include <QHBoxLayout>
#include <QInputDialog>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMessageBox>
#include <QPainter>
#include <QPointer>
#include <QPushButton>
#include <QScrollArea>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QStandardItemModel>
#include <QStyledItemDelegate>
#include <QTabWidget>
#include <QTableView>
#include <QTimer>
#include <QVBoxLayout>

#include <array>
#include <utility>
#include <vector>

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

// OpenPBR + UsdPreviewSurface section ordering for the Material Sheet.
// Each entry maps a section header to the input names that should
// surface under it. Inputs not in any list go to "Other".
const std::vector<std::pair<QString, std::vector<QString>>>& openpbr_sections() {
    static const std::vector<std::pair<QString, std::vector<QString>>> sections = {
        {QStringLiteral("Base"),
         {QStringLiteral("base_weight"), QStringLiteral("base_color"),
          QStringLiteral("base_metalness"), QStringLiteral("base_diffuse_roughness"),
          QStringLiteral("diffuseColor"), QStringLiteral("useSpecularWorkflow"),
          QStringLiteral("metallic")}},
        {QStringLiteral("Specular"),
         {QStringLiteral("specular_weight"), QStringLiteral("specular_color"),
          QStringLiteral("specular_roughness"), QStringLiteral("specular_ior"),
          QStringLiteral("specular_anisotropy"), QStringLiteral("specular_rotation"),
          QStringLiteral("specularColor"), QStringLiteral("roughness"),
          QStringLiteral("ior")}},
        {QStringLiteral("Transmission"),
         {QStringLiteral("transmission_weight"), QStringLiteral("transmission_color"),
          QStringLiteral("transmission_depth"), QStringLiteral("transmission_scatter"),
          QStringLiteral("transmission_dispersion_scale"),
          QStringLiteral("transmission_dispersion_abbe_number"),
          QStringLiteral("opacity"), QStringLiteral("opacityThreshold")}},
        {QStringLiteral("Subsurface"),
         {QStringLiteral("subsurface_weight"), QStringLiteral("subsurface_color"),
          QStringLiteral("subsurface_radius"), QStringLiteral("subsurface_radius_scale"),
          QStringLiteral("subsurface_scatter_anisotropy")}},
        {QStringLiteral("Coat"),
         {QStringLiteral("coat_weight"), QStringLiteral("coat_color"),
          QStringLiteral("coat_roughness"), QStringLiteral("coat_anisotropy"),
          QStringLiteral("coat_rotation"), QStringLiteral("coat_ior"),
          QStringLiteral("coat_darkening"), QStringLiteral("clearcoat"),
          QStringLiteral("clearcoatRoughness")}},
        {QStringLiteral("Emission"),
         {QStringLiteral("emission_luminance"), QStringLiteral("emission_color"),
          QStringLiteral("emissiveColor")}},
        {QStringLiteral("Geometry"),
         {QStringLiteral("geometry_opacity"), QStringLiteral("geometry_thin_walled"),
          QStringLiteral("geometry_normal"), QStringLiteral("geometry_coat_normal"),
          QStringLiteral("geometry_tangent"), QStringLiteral("normal"),
          QStringLiteral("displacement"), QStringLiteral("occlusion")}},
    };
    return sections;
}

// Parse a comma-separated triple from "0.1, 0.2, 0.3" into 3 floats.
bool parse_color3(const QString& value, float* r, float* g, float* b) {
    const auto parts = value.split(QLatin1Char(','));
    if (parts.size() != 3) return false;
    bool ok1 = false, ok2 = false, ok3 = false;
    *r = parts[0].trimmed().toFloat(&ok1);
    *g = parts[1].trimmed().toFloat(&ok2);
    *b = parts[2].trimmed().toFloat(&ok3);
    return ok1 && ok2 && ok3;
}

}  // namespace

PropertyInspectorWidget::PropertyInspectorWidget(BifShellState* state, QWidget* parent)
    : QWidget(parent),
      m_state(state),
      m_header_path(nullptr),
      m_header_type(nullptr),
      m_visibility_box(nullptr),
      m_arcs_group(nullptr),
      m_arcs_list(nullptr),
      m_tabs(nullptr),
      m_attrs_view(nullptr),
      m_attrs_model(nullptr),
      m_relationships_tab(nullptr),
      m_material_tab(nullptr),
      m_material_scroll(nullptr),
      m_material_content(nullptr),
      m_material_shader_label(nullptr),
      m_bind_material_button(nullptr),
      m_shading_model_combo(nullptr),
      m_shading_model_confirm_pending(false) {
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

    // Visibility checkbox (C4b-Carry-1). Authors a working-layer
    // `visibility = invisible/inherited` opinion via the Renderer
    // edit-history dispatcher so toggles save through Ctrl+S and
    // collapse to one undo step.
    m_visibility_box = new QCheckBox(QStringLiteral("Visible"), this);
    m_visibility_box->setStyleSheet(QStringLiteral(
        "QCheckBox { color: rgba(220, 222, 226, 255); font-size: 11px; }"));
    QObject::connect(m_visibility_box, &QCheckBox::toggled,
        this, &PropertyInspectorWidget::on_visibility_toggled);
    outer->addWidget(m_visibility_box);

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

    // Material Sheet tab (C4b-1) — bound material's surface shader
    // inputs grouped by OpenPBR / UsdPreviewSurface section.
    m_material_tab = new QWidget(m_tabs);
    auto* mat_layout = new QVBoxLayout(m_material_tab);
    mat_layout->setContentsMargins(4, 4, 4, 4);
    mat_layout->setSpacing(4);

    // Header: bound shader path + shading model dropdown + Bind action.
    auto* mat_header = new QHBoxLayout();
    m_material_shader_label = new QLabel(QStringLiteral("(no material bound)"), m_material_tab);
    m_material_shader_label->setStyleSheet(QStringLiteral(
        "color: rgba(140, 145, 155, 255); font-size: 11px;"));
    m_material_shader_label->setWordWrap(true);

    m_shading_model_combo = new QComboBox(m_material_tab);
    m_shading_model_combo->addItem(QStringLiteral("OpenPBR"));
    m_shading_model_combo->addItem(QStringLiteral("UsdPreviewSurface"));
    m_shading_model_combo->setStyleSheet(QStringLiteral(
        "QComboBox { padding: 2px 6px; font-size: 11px; }"));
    QObject::connect(m_shading_model_combo, &QComboBox::currentTextChanged,
        this, &PropertyInspectorWidget::on_shading_model_changed);

    m_bind_material_button = new QPushButton(QStringLiteral("Bind…"), m_material_tab);
    m_bind_material_button->setStyleSheet(QStringLiteral(
        "QPushButton { padding: 2px 10px; font-size: 11px; }"));
    QObject::connect(m_bind_material_button, &QPushButton::clicked,
        this, &PropertyInspectorWidget::on_bind_material_clicked);

    mat_header->addWidget(m_material_shader_label, 1);
    mat_header->addWidget(m_shading_model_combo, 0);
    mat_header->addWidget(m_bind_material_button, 0);
    mat_layout->addLayout(mat_header);

    m_material_scroll = new QScrollArea(m_material_tab);
    m_material_scroll->setWidgetResizable(true);
    m_material_scroll->setFrameShape(QFrame::NoFrame);
    m_material_content = new QWidget(m_material_scroll);
    auto* content_layout = new QVBoxLayout(m_material_content);
    content_layout->setContentsMargins(0, 0, 0, 0);
    content_layout->setSpacing(6);
    content_layout->addStretch();
    m_material_scroll->setWidget(m_material_content);
    mat_layout->addWidget(m_material_scroll, 1);

    m_tabs->addTab(m_material_tab, QStringLiteral("Material Sheet"));

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
        if (m_visibility_box) {
            m_visibility_box->setEnabled(false);
            const QSignalBlocker blocker(m_visibility_box);
            m_visibility_box->setChecked(true);
        }
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

    if (m_visibility_box) {
        const bool visible = m_state->prim_is_visible_at(path);
        m_visibility_box->setEnabled(true);
        const QSignalBlocker blocker(m_visibility_box);
        m_visibility_box->setChecked(visible);
    }

    populate_composition_arcs(path);
    populate_attributes(path, type);
    populate_material_sheet();
}

void PropertyInspectorWidget::on_visibility_toggled(bool checked) {
    if (!m_state) return;
    const auto path = m_state->getSelected_prim_path();
    if (path.isEmpty()) return;
    m_state->on_set_visibility(path, checked);
}

void PropertyInspectorWidget::on_shading_model_changed(const QString& model) {
    if (!m_state || model.isEmpty()) return;
    if (model == m_state->selected_prim_material_shader_id()) {
        return;
    }
    if (m_shading_model_confirm_pending) {
        const QSignalBlocker blocker(m_shading_model_combo);
        const auto current_id = m_state->selected_prim_material_shader_id();
        const int idx = m_shading_model_combo->findText(current_id);
        if (idx >= 0) m_shading_model_combo->setCurrentIndex(idx);
        return;
    }
    m_shading_model_confirm_pending = true;

    // Confirm the lossy swap with the user before authoring. The
    // dispatcher returns the dropped param list as `\n`-separated;
    // we surface them in the QMessageBox before kicking off the
    // begin_group/end_group sequence.
    QPointer<PropertyInspectorWidget> self(this);
    QPointer<BifShellState> state(m_state);
    QTimer::singleShot(0, this, [self, state, model]() {
        if (!self) return;
        self->m_shading_model_confirm_pending = false;
        if (!state || !self->m_shading_model_combo) return;

        const auto dropped_preview = QMessageBox::question(
            self,
            QStringLiteral("Swap shading model"),
            QStringLiteral("Switching to <b>%1</b> may drop parameters that the "
                           "target shader can't represent (Subsurface, Transmission, "
                           "Coat for UsdPreviewSurface). The whole swap will land "
                           "as a single Ctrl+Z step. Continue?")
                .arg(model.toHtmlEscaped()),
            QMessageBox::Yes | QMessageBox::No,
            QMessageBox::No);
        if (dropped_preview != QMessageBox::Yes) {
            // Revert the dropdown without retriggering the slot.
            const QSignalBlocker blocker(self->m_shading_model_combo);
            const auto current_id = state->selected_prim_material_shader_id();
            const int idx = self->m_shading_model_combo->findText(current_id);
            if (idx >= 0) self->m_shading_model_combo->setCurrentIndex(idx);
            return;
        }

        const QString result = state->on_set_shading_model(model);
        if (result.startsWith(QStringLiteral("error:"))) {
            QMessageBox::warning(self,
                QStringLiteral("Shading swap failed"),
                result.mid(6));
            return;
        }
        if (!result.isEmpty()) {
            QMessageBox::information(self,
                QStringLiteral("Shading model swapped"),
                QStringLiteral("Dropped parameters that don't round-trip into %1:\n\n%2")
                    .arg(model, result));
        }
    });
}

void PropertyInspectorWidget::on_bind_material_clicked() {
    if (!m_state) return;
    const auto prim_path = m_state->getSelected_prim_path();
    if (prim_path.isEmpty()) return;
    bool ok = false;
    const QString material_path = QInputDialog::getText(
        this,
        QStringLiteral("Bind Material"),
        QStringLiteral("Material prim path (e.g. /Materials/Red):"),
        QLineEdit::Normal,
        QString(),
        &ok);
    if (!ok || material_path.trimmed().isEmpty()) {
        return;
    }
    m_state->on_bind_material(material_path.trimmed());
}

void PropertyInspectorWidget::populate_material_sheet() {
    if (!m_state || !m_material_content) return;

    // Drop the existing children so a fresh layout doesn't duplicate
    // section group boxes between selection changes.
    QLayout* old_layout = m_material_content->layout();
    if (old_layout) {
        QLayoutItem* item = nullptr;
        while ((item = old_layout->takeAt(0)) != nullptr) {
            if (auto* w = item->widget()) {
                w->deleteLater();
            }
            delete item;
        }
        delete old_layout;
    }
    auto* content_layout = new QVBoxLayout(m_material_content);
    content_layout->setContentsMargins(0, 0, 0, 0);
    content_layout->setSpacing(6);

    const auto shader_path = m_state->selected_prim_material_shader_path();
    if (shader_path.isEmpty()) {
        m_material_shader_label->setText(QStringLiteral("(no material bound)"));
        if (m_shading_model_combo) {
            m_shading_model_combo->setEnabled(false);
        }
        auto* hint = new QLabel(
            QStringLiteral("Click <b>Bind…</b> to assign a material to this prim."),
            m_material_content);
        hint->setStyleSheet(QStringLiteral("color: rgba(140, 145, 155, 255); padding: 8px;"));
        hint->setAlignment(Qt::AlignCenter);
        hint->setWordWrap(true);
        content_layout->addWidget(hint);
        content_layout->addStretch();
        return;
    }
    m_material_shader_label->setText(
        QStringLiteral("Shader: <code>%1</code>").arg(shader_path.toHtmlEscaped()));

    // Reflect current shader id on the dropdown without retriggering
    // the swap path. C4b-3.
    if (m_shading_model_combo) {
        const auto shader_id = m_state->selected_prim_material_shader_id();
        const QSignalBlocker blocker(m_shading_model_combo);
        m_shading_model_combo->setEnabled(true);
        const int idx = m_shading_model_combo->findText(shader_id);
        if (idx >= 0) {
            m_shading_model_combo->setCurrentIndex(idx);
        } else {
            // Unknown id: surface as a non-selectable placeholder
            // so the user sees the actual stored value.
            int existing = m_shading_model_combo->findText(shader_id);
            if (existing < 0 && !shader_id.isEmpty()) {
                m_shading_model_combo->insertItem(0, shader_id);
                m_shading_model_combo->setCurrentIndex(0);
            }
        }
    }

    // Bucket inputs by OpenPBR section.
    const int n = m_state->selected_prim_material_input_count();
    QHash<QString, QList<int>> buckets;
    for (const auto& sec : openpbr_sections()) {
        buckets.insert(sec.first, {});
    }
    QList<int> other_indices;
    auto find_section = [](const QString& name) -> QString {
        for (const auto& sec : openpbr_sections()) {
            for (const auto& known : sec.second) {
                if (known == name) return sec.first;
            }
        }
        return QStringLiteral("Other");
    };
    for (int i = 0; i < n; ++i) {
        const QString name = m_state->selected_prim_material_input_name_at(i);
        const QString section = find_section(name);
        if (section == QStringLiteral("Other")) {
            other_indices.push_back(i);
        } else {
            buckets[section].push_back(i);
        }
    }

    auto build_input_row = [this](QWidget* parent, int input_index) -> QWidget* {
        const QString name = m_state->selected_prim_material_input_name_at(input_index);
        const QString type_name = m_state->selected_prim_material_input_type_at(input_index);
        const QString value_str = m_state->selected_prim_material_input_value_at(input_index);

        auto* row = new QWidget(parent);
        auto* h = new QHBoxLayout(row);
        h->setContentsMargins(4, 2, 4, 2);
        h->setSpacing(6);

        auto* label = new QLabel(name, row);
        label->setMinimumWidth(140);
        label->setStyleSheet(QStringLiteral("color: rgba(220, 222, 226, 255); font-size: 11px;"));
        label->setToolTip(QStringLiteral("USD type: %1").arg(type_name));
        h->addWidget(label, 0);

        QPointer<BifShellState> state(m_state);
        const QString cap_name = name;
        const QString cap_type = type_name;

        if (type_name == QStringLiteral("float") || type_name == QStringLiteral("double")) {
            auto* spin = new QDoubleSpinBox(row);
            spin->setRange(-1.0e6, 1.0e6);
            spin->setDecimals(4);
            spin->setSingleStep(0.01);
            spin->setValue(value_str.toDouble());
            QObject::connect(spin, &QDoubleSpinBox::editingFinished, [state, cap_name, cap_type, spin]() {
                if (!state) return;
                state->on_set_material_param(cap_name, cap_type,
                    QString::number(spin->value(), 'g', 6));
            });
            h->addWidget(spin, 1);
        } else if (type_name == QStringLiteral("int")) {
            auto* spin = new QSpinBox(row);
            spin->setRange(INT_MIN / 2, INT_MAX / 2);
            spin->setValue(value_str.toInt());
            QObject::connect(spin, &QSpinBox::editingFinished, [state, cap_name, cap_type, spin]() {
                if (!state) return;
                state->on_set_material_param(cap_name, cap_type, QString::number(spin->value()));
            });
            h->addWidget(spin, 1);
        } else if (type_name == QStringLiteral("bool")) {
            auto* box = new QCheckBox(row);
            box->setChecked(value_str.compare(QStringLiteral("true"), Qt::CaseInsensitive) == 0
                || value_str == QStringLiteral("1"));
            QObject::connect(box, &QCheckBox::toggled, [state, cap_name, cap_type](bool v) {
                if (!state) return;
                state->on_set_material_param(cap_name, cap_type,
                    v ? QStringLiteral("true") : QStringLiteral("false"));
            });
            h->addWidget(box, 0);
            h->addStretch();
        } else if (type_name == QStringLiteral("color3f") || type_name == QStringLiteral("float3")) {
            float r = 0, g = 0, b = 0;
            parse_color3(value_str, &r, &g, &b);
            auto* swatch = new QPushButton(row);
            swatch->setMinimumSize(28, 18);
            swatch->setMaximumSize(80, 22);
            const QColor initial = QColor::fromRgbF(qBound(0.0f, r, 1.0f),
                qBound(0.0f, g, 1.0f), qBound(0.0f, b, 1.0f));
            swatch->setStyleSheet(QStringLiteral("QPushButton { background-color: %1; border: 1px solid rgba(60,65,75,255); }")
                .arg(initial.name()));
            auto* edit = new QLineEdit(value_str, row);
            edit->setStyleSheet(QStringLiteral("font-size: 11px;"));
            QObject::connect(swatch, &QPushButton::clicked, [state, cap_name, cap_type, swatch, edit]() {
                if (!state) return;
                float cr = 0, cg = 0, cb = 0;
                parse_color3(edit->text(), &cr, &cg, &cb);
                const QColor seed = QColor::fromRgbF(qBound(0.0f, cr, 1.0f),
                    qBound(0.0f, cg, 1.0f), qBound(0.0f, cb, 1.0f));
                const QColor picked = QColorDialog::getColor(seed, swatch,
                    QStringLiteral("Pick color"));
                if (!picked.isValid()) return;
                // Note: QColorDialog returns sRGB; USD color3f wants
                // linear. Convert at the boundary.
                auto srgb_to_linear = [](double c) {
                    return (c <= 0.04045) ? (c / 12.92)
                                          : std::pow((c + 0.055) / 1.055, 2.4);
                };
                const double lr = srgb_to_linear(picked.redF());
                const double lg = srgb_to_linear(picked.greenF());
                const double lb = srgb_to_linear(picked.blueF());
                const QString triple = QStringLiteral("%1,%2,%3")
                    .arg(lr, 0, 'g', 6).arg(lg, 0, 'g', 6).arg(lb, 0, 'g', 6);
                edit->setText(triple);
                swatch->setStyleSheet(QStringLiteral("QPushButton { background-color: %1; border: 1px solid rgba(60,65,75,255); }")
                    .arg(picked.name()));
                state->on_set_material_param(cap_name, cap_type, triple);
            });
            QObject::connect(edit, &QLineEdit::editingFinished, [state, cap_name, cap_type, edit]() {
                if (!state) return;
                state->on_set_material_param(cap_name, cap_type, edit->text().trimmed());
            });
            h->addWidget(swatch, 0);
            h->addWidget(edit, 1);
        } else {
            // Fallback: free-form text edit.
            auto* edit = new QLineEdit(value_str, row);
            edit->setStyleSheet(QStringLiteral("font-size: 11px;"));
            QObject::connect(edit, &QLineEdit::editingFinished, [state, cap_name, cap_type, edit]() {
                if (!state) return;
                state->on_set_material_param(cap_name, cap_type, edit->text());
            });
            h->addWidget(edit, 1);
        }
        return row;
    };

    auto add_section = [&](const QString& title, const QList<int>& indices) {
        if (indices.isEmpty()) return;
        auto* group = new QGroupBox(title, m_material_content);
        group->setStyleSheet(QStringLiteral(
            "QGroupBox {"
            "  color: rgba(220, 222, 226, 255);"
            "  border: 1px solid rgba(60, 65, 75, 255);"
            "  border-radius: 4px;"
            "  margin-top: 10px; padding: 6px 4px 4px 4px;"
            "}"
            "QGroupBox::title {"
            "  subcontrol-origin: margin; left: 8px; padding: 0 4px;"
            "  color: rgba(140, 145, 155, 255); font-size: 11px;"
            "  text-transform: uppercase; letter-spacing: 1px;"
            "}"));
        auto* gv = new QVBoxLayout(group);
        gv->setContentsMargins(2, 4, 2, 2);
        gv->setSpacing(2);
        for (int idx : indices) {
            gv->addWidget(build_input_row(group, idx));
        }
        content_layout->addWidget(group);
    };

    for (const auto& sec : openpbr_sections()) {
        add_section(sec.first, buckets.value(sec.first));
    }
    add_section(QStringLiteral("Other"), other_indices);
    content_layout->addStretch();
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
        const auto opinions = m_state->selected_prim_attr_tooltip_at(i);

        // Tier 1 item #3: friendly attribute label, raw USD name in tooltip.
        const auto friendly = m_state->friendly_attribute_name(name);
        auto* name_item = new QStandardItem(friendly);
        QString tooltip = QStringLiteral("<b>USD name:</b> <code>%1</code>")
            .arg(name.toHtmlEscaped());
        if (!opinions.isEmpty()) {
            tooltip += QStringLiteral("<br/><br/>") + opinions;
        }
        name_item->setToolTip(tooltip);
        // Per-attribute winning opinion color (Tier 1.5).
        name_item->setData(m_state->selected_prim_attr_color_index_at(i), ColorIndexRole);
        auto* value_item = new QStandardItem(value);
        auto* type_item = new QStandardItem(type_name);
        value_item->setToolTip(tooltip);
        type_item->setToolTip(tooltip);
        type_item->setForeground(QColor(140, 145, 155));
        m_attrs_model->appendRow({name_item, value_item, type_item});
    }
}
