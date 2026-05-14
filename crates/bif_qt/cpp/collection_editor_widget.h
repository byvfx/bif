// CollectionEditorWidget — v0.16.6 Graphite-aesthetic redesign.
//
// Three collapsible section blocks (Includes / Excludes / Resolved Members)
// each headered by an uppercase QLabel#sectionHeader + a QToolButton#sectionIcon
// "+" trigger. Adding a target spawns an inline QLineEdit row in the list rather
// than opening a modal QInputDialog. List rows render with a left accent stripe
// (primary_container for includes, outline for excludes, transparent for
// resolved). Styling lives entirely in theme.rs — no inline setStyleSheet here.
//
// Reads/writes through BifShellState's `collection_*` invokables (see
// crates/bif_qt/src/main_window.rs). Refreshes on `collection_revisionChanged`
// and on prim-selection changes from the scene browser
// (`selected_prim_pathChanged`).

#pragma once

#include <QString>
#include <QWidget>

class BifShellState;
class QComboBox;
class QEvent;
class QFrame;
class QLabel;
class QLineEdit;
class QListWidget;
class QToolButton;
class QVBoxLayout;

// A collapsible section: clickable header row + body. Body visibility toggles
// on header click. Header chevron glyph reflects current state.
class SectionBlock : public QWidget {
    Q_OBJECT
public:
    SectionBlock(const QString& title, QWidget* parent = nullptr);
    QWidget* body() { return m_body; }
    QToolButton* icon_button() { return m_icon_btn; }
    void set_expanded(bool expanded);

protected:
    bool eventFilter(QObject* obj, QEvent* ev) override;

private:
    void toggle();

    QLabel* m_chevron;
    QLabel* m_title;
    QToolButton* m_icon_btn;
    QFrame* m_header;
    QWidget* m_body;
    bool m_expanded = true;
};


class CollectionEditorWidget : public QWidget {
    Q_OBJECT
public:
    explicit CollectionEditorWidget(BifShellState* state, QWidget* parent = nullptr);
    ~CollectionEditorWidget() override;

protected:
    bool eventFilter(QObject* obj, QEvent* ev) override;

private slots:
    void on_collection_revision_changed();
    void on_selected_prim_changed();
    void on_collection_picked(int index);
    void on_expansion_rule_changed(int index);
    void on_add_include();
    void on_add_exclude();
    void on_new_collection();

private:
    enum class PendingTarget { None, Includes, Excludes, NewCollection };

    void refresh_collection_list();
    void refresh_details();
    QString current_prim_path() const;
    QString current_collection_name() const;
    void begin_inline_add(PendingTarget which);
    void commit_inline_add();
    void cancel_inline_add();
    void remove_selected_from(QListWidget* list, bool is_include);

    BifShellState* m_state;
    QLineEdit* m_prim_path_edit;
    QComboBox* m_collection_picker;
    QToolButton* m_new_collection_btn;
    QComboBox* m_expansion_combo;

    SectionBlock* m_includes_section;
    SectionBlock* m_excludes_section;
    SectionBlock* m_members_section;

    QListWidget* m_includes_list;
    QListWidget* m_excludes_list;
    QListWidget* m_members_list;

    QLabel* m_includes_empty;
    QLabel* m_excludes_empty;
    QLabel* m_members_empty;

    // Inline-add row state. Lives on m_includes_list / m_excludes_list / outside.
    QListWidget* m_pending_list = nullptr;
    QLineEdit* m_pending_edit = nullptr;
    PendingTarget m_pending_target = PendingTarget::None;

    bool m_suppress_signals = false;
};
