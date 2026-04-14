// CommandPalette — Ctrl+P overlay for fuzzy-searching commands,
// prims, layers, nodes, and settings.
//
// Phase B.8 scope: command actions only (the QActions registered on
// the menu bar). Phase C+ adds prim paths from SceneLayerState,
// layer identifiers, node type names, and settings keys via a
// pluggable provider interface.
//
// Design: borderless QDialog with QLineEdit + QListView. Activation
// triggers the matched QAction and closes the dialog. Esc closes
// without action.

#pragma once

#include <QDialog>
#include <QHash>
#include <QString>

class QAction;
class QLineEdit;
class QListView;
class QSortFilterProxyModel;
class QStringListModel;

class CommandPalette : public QDialog {
    Q_OBJECT
public:
    /// `commands` maps human-readable name → action to trigger.
    /// Phase B.8 builds this from MenuActions; Phase C+ widens it.
    explicit CommandPalette(QWidget* parent, QHash<QString, QAction*> commands);
    ~CommandPalette() override;

protected:
    void keyPressEvent(QKeyEvent* event) override;
    void showEvent(QShowEvent* event) override;

private slots:
    void on_search_changed(const QString& text);
    void on_activate_index(const QModelIndex& index);

private:
    void activate_current();

    QLineEdit* m_search;
    QListView* m_results;
    QStringListModel* m_source_model;
    QSortFilterProxyModel* m_proxy;
    QHash<QString, QAction*> m_commands;
};
