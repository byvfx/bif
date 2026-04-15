// SceneBrowserModel — QAbstractItemModel for the prim tree (Phase C.2).
//
// Phase C.2 ships with a hardcoded demo tree (~10 prims) so the
// panel UX can be validated independent of USD load. Phase E
// replaces the data source with `bif_core::CompositeProvider`
// reads via cxx-qt invokables on `BifShellState`.
//
// Tree storage: owned `PrimNode`s with parent/children pointers,
// `QModelIndex::internalPointer()` carries the node*. Fast O(1)
// row/parent lookup at the cost of a flat memory layout — fine
// for the demo, will need streaming/lazy-fetch in Phase E for
// 100K+ prim acceptance criterion (`canFetchMore` / `fetchMore`).

#pragma once

#include <QAbstractItemModel>
#include <QString>
#include <memory>
#include <vector>

class SceneBrowserModel : public QAbstractItemModel {
    Q_OBJECT
public:
    enum PrimRoles {
        PathRole = Qt::UserRole + 1,
        TypeNameRole,
        ColorIndexRole,
    };

    explicit SceneBrowserModel(QObject* parent = nullptr);
    ~SceneBrowserModel() override;

    QModelIndex index(int row, int column,
                      const QModelIndex& parent = QModelIndex()) const override;
    QModelIndex parent(const QModelIndex& index) const override;
    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    int columnCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QVariant headerData(int section, Qt::Orientation orientation,
                        int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    /// Replace the tree with a hardcoded demo (Phase C.2). Phase E
    /// replaces this with `populate_from_provider(...)`.
    void seed_demo_tree();

    // PrimNode is public so the demo-tree builder in the .cpp file
    // (anonymous namespace) can construct one. Phase E swaps the
    // builder for a Rust-driven path; this stays public for the
    // shared-helper escape hatch.
    struct PrimNode {
        QString name;
        QString type_name;
        QString path;
        int color_index;
        PrimNode* parent;
        // std::vector — Qt's QList/QVector require copy-constructible
        // T but unique_ptr is move-only. std handles it cleanly.
        std::vector<std::unique_ptr<PrimNode>> children;

        PrimNode* child_at(int row) const;
        int row_in_parent() const;
    };

private:
    PrimNode* node_for_index(const QModelIndex& index) const;

    std::unique_ptr<PrimNode> m_root;
};
