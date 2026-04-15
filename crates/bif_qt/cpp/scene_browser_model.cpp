#include "scene_browser_model.h"

namespace {

// Build a child node and parent it to `parent`. Returns the raw
// pointer for further chaining. Color index alternates by depth so
// the demo tree shows the layer-color-dot system across rows.
SceneBrowserModel::PrimNode* add_child(
    SceneBrowserModel::PrimNode* parent,
    const QString& name,
    const QString& type_name,
    int color_index) {
    auto node = std::make_unique<SceneBrowserModel::PrimNode>();
    node->name = name;
    node->type_name = type_name;
    node->color_index = color_index;
    node->parent = parent;
    node->path = parent->path == QLatin1String("/")
        ? QStringLiteral("/%1").arg(name)
        : QStringLiteral("%1/%2").arg(parent->path).arg(name);
    auto* raw = node.get();
    parent->children.push_back(std::move(node));
    return raw;
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

SceneBrowserModel::SceneBrowserModel(QObject* parent)
    : QAbstractItemModel(parent), m_root(std::make_unique<PrimNode>()) {
    m_root->name = QStringLiteral("(root)");
    m_root->path = QStringLiteral("/");
    m_root->color_index = -1;
    m_root->parent = nullptr;
    seed_demo_tree();
}

SceneBrowserModel::~SceneBrowserModel() = default;

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
    // Phase C.2: name only. Phase D might add Type column.
    return 1;
}

QVariant SceneBrowserModel::data(const QModelIndex& index, int role) const {
    if (!index.isValid()) return {};
    auto* node = static_cast<PrimNode*>(index.internalPointer());
    switch (role) {
        case Qt::DisplayRole:
            return node->name;
        case Qt::ToolTipRole:
            return QStringLiteral("%1\n%2").arg(node->path).arg(node->type_name);
        case PathRole:
            return node->path;
        case TypeNameRole:
            return node->type_name;
        case ColorIndexRole:
            return node->color_index;
    }
    return {};
}

QVariant SceneBrowserModel::headerData(int section, Qt::Orientation orientation,
                                       int role) const {
    if (orientation != Qt::Horizontal || role != Qt::DisplayRole) return {};
    if (section == 0) return QStringLiteral("Prim");
    return {};
}

QHash<int, QByteArray> SceneBrowserModel::roleNames() const {
    auto roles = QAbstractItemModel::roleNames();
    roles.insert(PathRole, "path");
    roles.insert(TypeNameRole, "typeName");
    roles.insert(ColorIndexRole, "colorIndex");
    return roles;
}
