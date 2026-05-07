// SceneBrowserWidget — Phase C.2 panel.
//
// Composition: search QLineEdit + QTreeView (over SceneBrowserModel
// filtered by QSortFilterProxyModel). Selection emits a status-bar
// message via BifShellState (Phase E wires real AppEvent::PrimSelected).
//
// Phase C.2 acceptance: visible prim hierarchy + filterable + selectable.
// 100K-prim virtualization criterion (per plan §C-2) carries to Phase E
// when the data source switches from the hardcoded demo tree to the
// real CompositeProvider with lazy fetchMore.

#pragma once

#include <QModelIndex>
#include <QWidget>

class BifShellState;
class SceneBrowserModel;
class QLineEdit;
class QSortFilterProxyModel;
class QTreeView;

class SceneBrowserWidget : public QWidget {
    Q_OBJECT
public:
    explicit SceneBrowserWidget(BifShellState* state, QWidget* parent = nullptr);
    ~SceneBrowserWidget() override;

private slots:
    void on_filter_changed(const QString& text);
    void on_selection_changed(const QModelIndex& current, const QModelIndex& previous);
    void on_external_selection_changed();

protected:
    bool eventFilter(QObject* watched, QEvent* event) override;

private:
    QModelIndex find_source_index_for_path(const QString& path,
                                           const QModelIndex& parent = QModelIndex());
    void select_path(const QString& path);

    BifShellState* m_state;
    SceneBrowserModel* m_model;
    QSortFilterProxyModel* m_filter;
    QLineEdit* m_search;
    QTreeView* m_view;
    bool m_syncing_external_selection{false};
};
