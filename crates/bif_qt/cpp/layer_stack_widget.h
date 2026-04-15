// LayerStackWidget — Phase C.1 panel.
//
// Composition: toolbar (isolation toggle) + QListView (layer list).
// The list's QStyledItemDelegate paints a layer-color dot left of
// each row, then Qt's default delegate draws the checkbox (mute)
// and text (layer name, bold if working). Double-click a row to
// make it the working layer.

#pragma once

#include <QWidget>

class BifShellState;
class LayerStackModel;
class QAction;
class QListView;
class QToolBar;

class LayerStackWidget : public QWidget {
    Q_OBJECT
public:
    explicit LayerStackWidget(BifShellState* state, QWidget* parent = nullptr);
    ~LayerStackWidget() override;

private slots:
    void on_row_double_clicked(const QModelIndex& index);
    void on_state_revision_changed();

private:
    BifShellState* m_state;
    LayerStackModel* m_model;
    QListView* m_view;
    QToolBar* m_toolbar;
    QAction* m_isolation_action;
};
