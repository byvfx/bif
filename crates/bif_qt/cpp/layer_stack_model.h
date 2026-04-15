// LayerStackModel — QAbstractListModel adapter over BifShellState's
// SceneLayerState surface (Phase C.1).
//
// Reads layer data through BifShellState's cxx-qt invokables:
//   layer_count / layer_name_at / layer_depth_at / layer_color_index_at
//   / layer_muted_at / layer_is_working
// Writes via:
//   set_layer_muted (Qt::CheckStateRole)
//   set_working_layer (custom MakeWorkingRole or double-click)
//
// Refreshes on `BifShellState::layer_state_revisionChanged` —
// cxx-qt auto-generates this from the #[qproperty(i32,
// layer_state_revision)] declaration. On change we beginResetModel
// / endResetModel because row count may change (layer add/remove
// is a Phase E concern, but the pattern is in place).

#pragma once

#include <QAbstractListModel>
#include <QPointer>

class BifShellState;

class LayerStackModel : public QAbstractListModel {
    Q_OBJECT
public:
    enum LayerRoles {
        DepthRole = Qt::UserRole + 1,
        ColorIndexRole,
        IsWorkingRole,
        IdentifierRole,
    };

    explicit LayerStackModel(BifShellState* state, QObject* parent = nullptr);
    ~LayerStackModel() override;

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    bool setData(const QModelIndex& index, const QVariant& value, int role) override;
    Qt::ItemFlags flags(const QModelIndex& index) const override;
    QHash<int, QByteArray> roleNames() const override;

    /// Convert a list-row click into a set_working_layer invocation.
    void activate_as_working(const QModelIndex& index);

public slots:
    void on_state_revision_changed();

private:
    QPointer<BifShellState> m_state;
};
