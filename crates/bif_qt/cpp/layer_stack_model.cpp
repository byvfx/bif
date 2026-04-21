#include "layer_stack_model.h"

#include <QFont>

#include "bif_qt/src/main_window.cxxqt.h"

LayerStackModel::LayerStackModel(BifShellState* state, QObject* parent)
    : QAbstractListModel(parent), m_state(state) {
    if (m_state) {
        // Auto-generated from #[qproperty(i32, layer_state_revision)].
        QObject::connect(
            m_state, &BifShellState::layer_state_revisionChanged,
            this, &LayerStackModel::on_state_revision_changed);
    }
}

LayerStackModel::~LayerStackModel() = default;

int LayerStackModel::rowCount(const QModelIndex& parent) const {
    if (parent.isValid() || !m_state) return 0;
    return m_state->layer_count();
}

QVariant LayerStackModel::data(const QModelIndex& index, int role) const {
    if (!index.isValid() || !m_state) return {};
    const int row = index.row();
    if (row < 0 || row >= m_state->layer_count()) return {};

    switch (role) {
        case Qt::DisplayRole:
            return m_state->layer_name_at(row);
        case Qt::CheckStateRole:
            return m_state->layer_muted_at(row) ? Qt::Checked : Qt::Unchecked;
        case Qt::FontRole: {
            QFont f;
            f.setBold(m_state->layer_is_working(row));
            return f;
        }
        case Qt::ToolTipRole:
            return m_state->layer_identifier_at(row);
        case DepthRole:
            return m_state->layer_depth_at(row);
        case ColorIndexRole:
            return m_state->layer_color_index_at(row);
        case IsWorkingRole:
            return m_state->layer_is_working(row);
        case IdentifierRole:
            return m_state->layer_identifier_at(row);
    }
    return {};
}

bool LayerStackModel::setData(const QModelIndex& index, const QVariant& value, int role) {
    if (!index.isValid() || !m_state) return false;
    if (role == Qt::CheckStateRole) {
        const bool muted = (value.toInt() == Qt::Checked);
        m_state->set_layer_muted(index.row(), muted);
        // layer_state_revision bump will invalidate the cell via the
        // connected dataChanged emission from on_state_revision_changed.
        return true;
    }
    return false;
}

Qt::ItemFlags LayerStackModel::flags(const QModelIndex& index) const {
    auto base = QAbstractListModel::flags(index);
    if (index.isValid()) {
        base |= Qt::ItemIsUserCheckable | Qt::ItemIsSelectable | Qt::ItemIsEnabled;
    }
    return base;
}

QHash<int, QByteArray> LayerStackModel::roleNames() const {
    auto roles = QAbstractListModel::roleNames();
    roles.insert(DepthRole, "depth");
    roles.insert(ColorIndexRole, "colorIndex");
    roles.insert(IsWorkingRole, "isWorking");
    roles.insert(IdentifierRole, "identifier");
    return roles;
}

void LayerStackModel::activate_as_working(const QModelIndex& index) {
    if (!index.isValid() || !m_state) return;
    m_state->set_working_layer(index.row());
}

void LayerStackModel::on_state_revision_changed() {
    // Row count may have changed (layer add/remove is Phase E but the
    // pattern lives here). Simplest correct thing is beginResetModel.
    beginResetModel();
    endResetModel();
}
