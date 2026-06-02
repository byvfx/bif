// KeybindingEditorDialog — modal Preferences-style dialog for
// remapping every shortcut routed through `bif_qt::shortcuts::lookup`.
//
// Source of truth is the static registry in shortcut_registry.cpp:
// every action that's been looked up at least once is enumerable
// via `registered_defaults()`. Apply persists overrides through
// `set_override()`; Restore Defaults clears them via `clear_override()`.
//
// Hot-reload: opener wires the QDialog::accepted signal to call
// `setShortcut(lookup(id, default))` for any QAction it owns. Actions
// outside that scope (camera, timeline, palette widget-owned actions)
// pick up the new binding on next app restart. This is a known
// v0.16.7 limitation — full hot-reload follows in a later pass.
//
// Conflict policy: a linear scan before Apply detects collisions.
// First conflict raises a QMessageBox and aborts the write; the user
// can retype before retrying.

#pragma once

#include <QDialog>
#include <QHash>
#include <QKeySequence>
#include <QString>

class QTreeWidget;
class QTreeWidgetItem;
class QKeySequenceEdit;

namespace bif_qt::shortcuts {

class KeybindingEditorDialog : public QDialog {
    Q_OBJECT
public:
    explicit KeybindingEditorDialog(QWidget* parent = nullptr);
    ~KeybindingEditorDialog() override;

protected:
    void accept() override;

private:
    struct Row {
        QString action_id;
        QKeySequence default_seq;
        QKeySequenceEdit* edit = nullptr;
        QTreeWidgetItem* item = nullptr;
    };

    void populate();
    void on_restore_defaults();

    // Returns true if no conflicts; otherwise shows QMessageBox and
    // returns false. `conflicting` receives the first offending pair
    // of action_ids for diagnostics.
    bool detect_conflicts(QString* conflict_msg) const;

    QTreeWidget* m_tree = nullptr;
    QHash<QString, Row> m_rows;  // keyed by action_id
};

}  // namespace bif_qt::shortcuts
