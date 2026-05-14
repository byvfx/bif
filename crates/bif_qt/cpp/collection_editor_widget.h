// CollectionEditorWidget — v0.16.5 panel.
//
// Composition: prim-path field + collection picker + expansion-rule
// combo + three QListWidgets (includes / excludes / resolved members)
// + add/remove buttons. Reads/writes through BifShellState's
// `collection_*` invokables (see crates/bif_qt/src/main_window.rs).
//
// Refreshes on `collection_revisionChanged` and on prim-selection
// changes from the scene browser (`selected_prim_pathChanged`).

#pragma once

#include <QString>
#include <QWidget>

class BifShellState;
class QComboBox;
class QLineEdit;
class QListWidget;
class QPushButton;

class CollectionEditorWidget : public QWidget {
    Q_OBJECT
public:
    explicit CollectionEditorWidget(BifShellState* state, QWidget* parent = nullptr);
    ~CollectionEditorWidget() override;

private slots:
    void on_collection_revision_changed();
    void on_selected_prim_changed();
    void on_collection_picked(int index);
    void on_expansion_rule_changed(int index);
    void on_add_include();
    void on_remove_include();
    void on_add_exclude();
    void on_remove_exclude();
    void on_new_collection();

private:
    void refresh_collection_list();
    void refresh_details();
    QString current_prim_path() const;
    QString current_collection_name() const;

    BifShellState* m_state;
    QLineEdit* m_prim_path_edit;
    QComboBox* m_collection_picker;
    QPushButton* m_new_collection_btn;
    QComboBox* m_expansion_combo;
    QListWidget* m_includes_list;
    QListWidget* m_excludes_list;
    QListWidget* m_members_list;
    QPushButton* m_add_include_btn;
    QPushButton* m_remove_include_btn;
    QPushButton* m_add_exclude_btn;
    QPushButton* m_remove_exclude_btn;
    bool m_suppress_signals = false;
};
