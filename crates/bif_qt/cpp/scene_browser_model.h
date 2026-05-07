// SceneBrowserModel — QAbstractItemModel for the prim tree.
//
// Phase E.2 move 7 (2026-04-16): reads the live UsdStage via cxx-qt
// invokables on `BifShellState`. Listens for
// `scene_browser_revisionChanged` and rebuilds the tree on stage
// load / close. Empty state stays empty; no demo USD hierarchy is
// seeded into normal app sessions.
//
// Tree storage: owned `PrimNode`s with parent/children pointers,
// `QModelIndex::internalPointer()` carries the node*. Fast O(1)
// row/parent lookup at the cost of a flat memory layout — fine
// for root.usda-scale stages, will need lazy fetch
// (`canFetchMore` / `fetchMore`) for 100K+ prim scenes.

#pragma once

#include <QAbstractItemModel>
#include <QPointer>
#include <QString>
#include <memory>
#include <vector>

class BifShellState;

class SceneBrowserModel : public QAbstractItemModel {
    Q_OBJECT
public:
    enum PrimRoles {
        PathRole = Qt::UserRole + 1,
        TypeNameRole,
        ColorIndexRole,
        KindRole,
        IsVisibleRole,
        IsActiveRole,
        ChildrenCountRole,
    };

    enum Columns {
        ColVisibility = 0,
        ColName,
        ColType,
        ColChildren,
        ColKind,
        ColumnCount_,
    };

    /// `state` is the source of prim tree data once a stage is loaded.
    /// Pass `nullptr` for demo-tree-only mode (tests).
    explicit SceneBrowserModel(BifShellState* state, QObject* parent = nullptr);
    ~SceneBrowserModel() override;

    QModelIndex index(int row, int column,
                      const QModelIndex& parent = QModelIndex()) const override;
    QModelIndex parent(const QModelIndex& index) const override;
    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    int columnCount(const QModelIndex& parent = QModelIndex()) const override;
    bool hasChildren(const QModelIndex& parent = QModelIndex()) const override;
    bool canFetchMore(const QModelIndex& parent) const override;
    void fetchMore(const QModelIndex& parent) override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QVariant headerData(int section, Qt::Orientation orientation,
                        int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    /// Rebuild the tree from the live UsdStage via BifShellState's
    /// prim-tree invokables. Called on `scene_browser_revisionChanged`.
    void rebuild_from_state();

    // PrimNode is public so the demo-tree builder in the .cpp file
    // (anonymous namespace) can construct one. Phase E swaps the
    // builder for a Rust-driven path; this stays public for the
    // shared-helper escape hatch.
    struct PrimNode {
        QString name;
        QString type_name;
        QString path;
        QString kind;       // "component", "assembly", "group", … or empty
        int color_index;
        bool is_visible{true};
        bool is_active{true};
        int child_count{0};
        bool children_populated{true};
        PrimNode* parent;
        // std::vector — Qt's QList/QVector require copy-constructible
        // T but unique_ptr is move-only. std handles it cleanly.
        std::vector<std::unique_ptr<PrimNode>> children;

        PrimNode* child_at(int row) const;
        int row_in_parent() const;
    };

public slots:
    void on_state_revision_changed();

private:
    PrimNode* node_for_index(const QModelIndex& index) const;

    QPointer<BifShellState> m_state;
    std::unique_ptr<PrimNode> m_root;
};
