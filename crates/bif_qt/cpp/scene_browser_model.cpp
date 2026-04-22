#include "scene_browser_model.h"

#include "bif_qt/src/main_window.cxxqt.h"

namespace {

struct PrimNodeData {
    QString name;
    QString type_name;
    QString path;
    QString kind;
    int color_index = -1;
    bool is_visible = true;
    bool is_active = true;
    int child_count = 0;
};

// Build a child node and parent it to `parent`. Returns the raw
// pointer for further chaining. Color index alternates by depth so
// the demo tree shows the layer-color-dot system across rows.
SceneBrowserModel::PrimNode* add_child(
    SceneBrowserModel::PrimNode* parent,
    const QString& name,
    const QString& type_name,
    int color_index,
    const QString& explicit_path = QString(),
    const QString& kind = QString(),
    bool is_visible = true,
    bool is_active = true) {
    auto node = std::make_unique<SceneBrowserModel::PrimNode>();
    node->name = name;
    node->type_name = type_name;
    node->kind = kind;
    node->color_index = color_index;
    node->is_visible = is_visible;
    node->is_active = is_active;
    node->child_count = 0;
    node->children_populated = true;
    node->parent = parent;
    if (!explicit_path.isEmpty()) {
        node->path = explicit_path;
    } else {
        node->path = parent->path == QLatin1String("/")
            ? QStringLiteral("/%1").arg(name)
            : QStringLiteral("%1/%2").arg(parent->path).arg(name);
    }
    auto* raw = node.get();
    parent->children.push_back(std::move(node));
    parent->child_count = static_cast<int>(parent->children.size());
    parent->children_populated = true;
    return raw;
}

PrimNodeData describe_prim(BifShellState* state, const QString& path) {
    PrimNodeData data;
    data.path = path;
    data.name = state->prim_display_name_at(path);
    data.type_name = state->prim_type_name_at(path);
    data.kind = state->prim_kind_at(path);
    data.is_visible = state->prim_is_visible_at(path);
    data.is_active = state->prim_is_active_at(path);
    data.color_index = state->prim_color_index_at(path);
    data.child_count = state->child_prim_count(path);
    if (data.name.isEmpty()) {
        data.name = path;
    }
    return data;
}

std::vector<PrimNodeData> describe_children(BifShellState* state, const QString& parent_path) {
    std::vector<PrimNodeData> nodes;
    if (!state) return nodes;
    const int n = state->child_prim_count(parent_path);
    nodes.reserve(static_cast<size_t>(qMax(0, n)));
    for (int i = 0; i < n; ++i) {
        const auto child_path = state->child_prim_path_at(parent_path, i);
        if (!child_path.isEmpty()) {
            nodes.push_back(describe_prim(state, child_path));
        }
    }
    return nodes;
}

std::vector<PrimNodeData> describe_root_prims(BifShellState* state) {
    std::vector<PrimNodeData> nodes;
    if (!state) return nodes;
    const int n = state->root_prim_count();
    nodes.reserve(static_cast<size_t>(qMax(0, n)));
    for (int i = 0; i < n; ++i) {
        const auto path = state->root_prim_path_at(i);
        if (!path.isEmpty()) {
            nodes.push_back(describe_prim(state, path));
        }
    }
    return nodes;
}

std::unique_ptr<SceneBrowserModel::PrimNode> make_node(
    SceneBrowserModel::PrimNode* parent,
    const PrimNodeData& data) {
    auto node = std::make_unique<SceneBrowserModel::PrimNode>();
    node->name = data.name;
    node->type_name = data.type_name;
    node->path = data.path;
    node->kind = data.kind;
    node->color_index = data.color_index;
    node->is_visible = data.is_visible;
    node->is_active = data.is_active;
    node->child_count = data.child_count;
    node->children_populated = (data.child_count == 0);
    node->parent = parent;
    return node;
}

}  // namespace

SceneBrowserModel::PrimNode* SceneBrowserModel::PrimNode::child_at(int row) const {
    if (row < 0 || static_cast<size_t>(row) >= children.size()) return nullptr;
    return children[row].get();
}

int SceneBrowserModel::PrimNode::row_in_parent() const {
    if (!parent) return 0;
    for (size_t i = 0; i < parent->children.size(); ++i) {
        if (parent->children[i].get() == this) return static_cast<int>(i);
    }
    return 0;
}

SceneBrowserModel::SceneBrowserModel(BifShellState* state, QObject* parent)
    : QAbstractItemModel(parent),
      m_state(state),
      m_root(std::make_unique<PrimNode>()) {
    m_root->name = QStringLiteral("(root)");
    m_root->path = QStringLiteral("/");
    m_root->child_count = 0;
    m_root->children_populated = true;
    m_root->color_index = -1;
    m_root->parent = nullptr;

    // Initial content — if a stage is already loaded, pull real data;
    // otherwise seed demo tree so first-launch still looks alive.
    if (m_state && m_state->root_prim_count() > 0) {
        m_has_ever_loaded_stage = true;
        rebuild_from_state();
    } else {
        seed_demo_tree();
    }

    if (m_state) {
        QObject::connect(
            m_state.data(), &BifShellState::scene_browser_revisionChanged,
            this, &SceneBrowserModel::on_state_revision_changed);
    }
}

SceneBrowserModel::~SceneBrowserModel() = default;

void SceneBrowserModel::on_state_revision_changed() {
    rebuild_from_state();
}

void SceneBrowserModel::rebuild_from_state() {
    beginResetModel();
    m_root->children.clear();
    m_root->child_count = 0;
    m_root->children_populated = true;

    if (m_state) {
        const auto roots = describe_root_prims(m_state);
        if (!roots.empty()) {
            m_has_ever_loaded_stage = true;
        }
        for (const auto& root_data : roots) {
            m_root->children.push_back(make_node(m_root.get(), root_data));
        }
        m_root->child_count = static_cast<int>(m_root->children.size());
    }

    endResetModel();

    // Keep first-launch lively until a real stage has loaded once.
    // After that, a close-stage should leave the tree empty.
    if (m_root->children.empty() && !m_has_ever_loaded_stage) {
        seed_demo_tree();
    }
}

void SceneBrowserModel::seed_demo_tree() {
    beginResetModel();
    m_root->children.clear();

    auto* world = add_child(m_root.get(), QStringLiteral("World"), QStringLiteral("Xform"), 0);

    auto* hero = add_child(world, QStringLiteral("Hero"), QStringLiteral("Xform"), 1);
    auto* geom = add_child(hero, QStringLiteral("Geom"), QStringLiteral("Scope"), 2);
    add_child(geom, QStringLiteral("Body"), QStringLiteral("Mesh"), 2);
    add_child(geom, QStringLiteral("Head"), QStringLiteral("Mesh"), 2);
    auto* skel = add_child(hero, QStringLiteral("Skel"), QStringLiteral("SkelRoot"), 1);
    add_child(skel, QStringLiteral("Skeleton"), QStringLiteral("Skeleton"), 1);

    auto* sky = add_child(world, QStringLiteral("Sky"), QStringLiteral("Xform"), 0);
    add_child(sky, QStringLiteral("Sun"), QStringLiteral("DistantLight"), 0);
    add_child(sky, QStringLiteral("Dome"), QStringLiteral("DomeLight"), 0);

    add_child(world, QStringLiteral("Ground"), QStringLiteral("Mesh"), 0);

    endResetModel();
}

SceneBrowserModel::PrimNode* SceneBrowserModel::node_for_index(const QModelIndex& index) const {
    if (!index.isValid()) return m_root.get();
    return static_cast<PrimNode*>(index.internalPointer());
}

QModelIndex SceneBrowserModel::index(int row, int column, const QModelIndex& parent) const {
    if (!hasIndex(row, column, parent)) return {};
    auto* parent_node = node_for_index(parent);
    auto* child = parent_node->child_at(row);
    return child ? createIndex(row, column, child) : QModelIndex();
}

QModelIndex SceneBrowserModel::parent(const QModelIndex& index) const {
    if (!index.isValid()) return {};
    auto* node = static_cast<PrimNode*>(index.internalPointer());
    auto* parent_node = node->parent;
    if (!parent_node || parent_node == m_root.get()) return {};
    return createIndex(parent_node->row_in_parent(), 0, parent_node);
}

int SceneBrowserModel::rowCount(const QModelIndex& parent) const {
    if (parent.column() > 0) return 0;
    return static_cast<int>(node_for_index(parent)->children.size());
}

int SceneBrowserModel::columnCount(const QModelIndex& /*parent*/) const {
    return ColumnCount_;
}

bool SceneBrowserModel::hasChildren(const QModelIndex& parent) const {
    if (!parent.isValid()) {
        return m_root->child_count > 0;
    }
    return node_for_index(parent)->child_count > 0;
}

bool SceneBrowserModel::canFetchMore(const QModelIndex& parent) const {
    if (!m_state || !parent.isValid()) return false;
    auto* node = node_for_index(parent);
    return node->child_count > 0 && !node->children_populated;
}

void SceneBrowserModel::fetchMore(const QModelIndex& parent) {
    if (!canFetchMore(parent)) return;
    auto* parent_node = node_for_index(parent);
    const auto children = describe_children(m_state, parent_node->path);
    parent_node->child_count = static_cast<int>(children.size());
    parent_node->children_populated = true;
    if (children.empty()) return;

    beginInsertRows(parent, 0, static_cast<int>(children.size()) - 1);
    for (const auto& child_data : children) {
        parent_node->children.push_back(make_node(parent_node, child_data));
    }
    endInsertRows();
}

QVariant SceneBrowserModel::data(const QModelIndex& index, int role) const {
    if (!index.isValid()) return {};
    auto* node = static_cast<PrimNode*>(index.internalPointer());
    switch (role) {
        case Qt::DisplayRole:
            switch (index.column()) {
                case ColName: return node->name;
                case ColType: return node->type_name;
                case ColChildren:
                    return node->child_count > 0 ? QVariant(node->child_count) : QVariant();
                case ColKind: return node->kind;
            }
            return {};
        case Qt::ToolTipRole:
            return QStringLiteral("%1\n%2").arg(node->path).arg(node->type_name);
        case PathRole:
            return node->path;
        case TypeNameRole:
            return node->type_name;
        case ColorIndexRole:
            return node->color_index;
        case KindRole:
            return node->kind;
        case IsVisibleRole:
            return node->is_visible;
        case IsActiveRole:
            return node->is_active;
        case ChildrenCountRole:
            return node->child_count;
    }
    return {};
}

QVariant SceneBrowserModel::headerData(int section, Qt::Orientation orientation,
                                       int role) const {
    if (orientation != Qt::Horizontal || role != Qt::DisplayRole) return {};
    switch (section) {
        case ColName: return QStringLiteral("Prim");
        case ColType: return QStringLiteral("Type");
        case ColChildren: return QStringLiteral("Children");
        case ColKind: return QStringLiteral("Kind");
    }
    return {};
}

QHash<int, QByteArray> SceneBrowserModel::roleNames() const {
    auto roles = QAbstractItemModel::roleNames();
    roles.insert(PathRole, "path");
    roles.insert(TypeNameRole, "typeName");
    roles.insert(ColorIndexRole, "colorIndex");
    roles.insert(KindRole, "kind");
    roles.insert(IsVisibleRole, "isVisible");
    roles.insert(IsActiveRole, "isActive");
    roles.insert(ChildrenCountRole, "childrenCount");
    return roles;
}
