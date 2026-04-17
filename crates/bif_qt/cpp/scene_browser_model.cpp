#include "scene_browser_model.h"

#include "bif_qt/src/main_window.cxxqt.h"

namespace {

// Build a child node and parent it to `parent`. Returns the raw
// pointer for further chaining. Color index alternates by depth so
// the demo tree shows the layer-color-dot system across rows.
SceneBrowserModel::PrimNode* add_child(
    SceneBrowserModel::PrimNode* parent,
    const QString& name,
    const QString& type_name,
    int color_index,
    const QString& explicit_path = QString()) {
    auto node = std::make_unique<SceneBrowserModel::PrimNode>();
    node->name = name;
    node->type_name = type_name;
    node->color_index = color_index;
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
    return raw;
}

// Recursively pull children of `parent_path` from BifShellState and
// attach them under `parent_node`. Depth-limited to keep the initial
// tree walk bounded — USD stages with pathological nesting don't
// stall the UI thread.
constexpr int kMaxTreeDepth = 64;

void populate_subtree(
    BifShellState* state,
    SceneBrowserModel::PrimNode* parent_node,
    const QString& parent_path,
    int depth) {
    if (!state || depth >= kMaxTreeDepth) return;
    const int n = state->child_prim_count(parent_path);
    for (int i = 0; i < n; ++i) {
        const auto child_path = state->child_prim_path_at(parent_path, i);
        if (child_path.isEmpty()) continue;
        const auto name = state->prim_display_name_at(child_path);
        const auto type_name = state->prim_type_name_at(child_path);
        auto* child_node = add_child(
            parent_node,
            name.isEmpty() ? child_path : name,
            type_name,
            /*color_index=*/-1,
            /*explicit_path=*/child_path);
        populate_subtree(state, child_node, child_path, depth + 1);
    }
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
    m_root->color_index = -1;
    m_root->parent = nullptr;

    // Initial content — if a stage is already loaded, pull real data;
    // otherwise seed demo tree so first-launch still looks alive.
    if (m_state && m_state->root_prim_count() > 0) {
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

    if (m_state) {
        const int n = m_state->root_prim_count();
        for (int i = 0; i < n; ++i) {
            const auto path = m_state->root_prim_path_at(i);
            if (path.isEmpty()) continue;
            const auto name = m_state->prim_display_name_at(path);
            const auto type_name = m_state->prim_type_name_at(path);
            auto* root = add_child(
                m_root.get(),
                name.isEmpty() ? path : name,
                type_name,
                /*color_index=*/-1,
                /*explicit_path=*/path);
            populate_subtree(m_state, root, path, /*depth=*/1);
        }
    }

    // Fallback — if nothing loaded, keep first-launch looking alive.
    if (m_root->children.empty()) {
        endResetModel();
        seed_demo_tree();
        return;
    }

    endResetModel();
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
    // Name only. A Type column could be added later.
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
