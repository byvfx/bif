// PropertyInspectorWidget — Phase C.3 panel.
//
// Composition:
//   - Header: selected prim path + type name.
//   - Composition Arcs: collapsible QGroupBox with a list of layers
//     contributing opinions (Phase C.3 stub = layer stack in
//     strongest-first order).
//   - Attributes tab (QTableView): Name / Value / Type columns.
//     Name column has an opinion-color dot delegate.
//   - Relationships tab: placeholder for Phase C.4.
//
// Listens to `BifShellState::selected_prim_pathChanged` and rebuilds
// content. Fake attributes are derived from the prim type for Phase C.3;
// Phase E wires to `UsdPrim::GetAttributes()` via bif_core.

#pragma once

#include <QWidget>

class BifShellState;
class QCheckBox;
class QComboBox;
class QGroupBox;
class QLabel;
class QPushButton;
class QScrollArea;
class QStandardItemModel;
class QTabWidget;
class QTableView;
class QListWidget;
class QVBoxLayout;

class PropertyInspectorWidget : public QWidget {
    Q_OBJECT
public:
    explicit PropertyInspectorWidget(BifShellState* state, QWidget* parent = nullptr);
    ~PropertyInspectorWidget() override;

private slots:
    void on_selection_changed();
    void on_layer_state_changed();
    void on_visibility_toggled(bool checked);
    void on_bind_material_clicked();
    void on_shading_model_changed(const QString& model);

private:
    void rebuild();
    void populate_composition_arcs(const QString& prim_path);
    void populate_attributes(const QString& prim_path, const QString& prim_type);
    void populate_material_sheet();

    BifShellState* m_state;

    QLabel* m_header_path;
    QLabel* m_header_type;
    QCheckBox* m_visibility_box;
    QGroupBox* m_arcs_group;
    QListWidget* m_arcs_list;
    QTabWidget* m_tabs;
    QTableView* m_attrs_view;
    QStandardItemModel* m_attrs_model;
    QWidget* m_relationships_tab;
    // C4b-1 Material Sheet tab — scrollable container with grouped
    // OpenPBR / UsdPreviewSurface input editors per bound shader.
    QWidget* m_material_tab;
    QScrollArea* m_material_scroll;
    QWidget* m_material_content;
    QLabel* m_material_shader_label;
    QPushButton* m_bind_material_button;
    // C4b-3 shading-model swap dropdown.
    QComboBox* m_shading_model_combo;
    bool m_shading_model_confirm_pending;
};
