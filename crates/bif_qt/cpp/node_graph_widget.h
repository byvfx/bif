// NodeGraphWidget — Phase D.2 panel.
//
// QGraphicsView + QGraphicsScene replacing egui-snarl. Each node is a
// BifNodeGraphicsItem (QGraphicsObject subclass) with a header bar,
// input/output pin lollipops, and bezier wires connecting pins
// through BifNodeWire (QGraphicsPathItem). Pan via middle-mouse
// drag, zoom via mouse wheel.
//
// Phase D.2 shipped as a visual graph surface independent of a real
// graph backend. The current bridge creates matching SceneNode entries
// in bif_viewport for Qt-created nodes; wires/properties follow later.
//
// Node color coding (ADR-003 blue/orange composition/operations) is
// a Phase D.2 polish item — baseline here uses uniform neutral grey
// node bodies with accent headers per node category.

#pragma once

#include <QDoubleSpinBox>
#include <QGraphicsObject>
#include <QGraphicsPathItem>
#include <QLineEdit>
#include <QSpinBox>
#include <QStackedWidget>
#include <QVector>
#include <QWidget>

#include <QGraphicsView>

class QGraphicsScene;
class BifShellState;

/// Sidebar param panel — one QStackedWidget page per node type.
class NodeParamPanel : public QWidget {
    Q_OBJECT
public:
    explicit NodeParamPanel(BifShellState* state, QWidget* parent = nullptr);
    void show_params_for(int backend_id);
    void clear();

private slots:
    void on_usd_browse();
    void on_usd_path_changed();
    void on_hdri_browse();
    void on_hdri_apply();
    /// Live rotation/intensity push — no file reload. Fires on every spinbox step.
    void on_hdri_params_changed();
    void on_xform_apply();
    void on_ivar_render_clicked();

private:
    BifShellState* m_state;
    int m_current_id{-1};
    QStackedWidget* m_stack;
    // UsdRead page widgets
    QLineEdit* m_usd_path;
    // HdriEnvironment page widgets
    QLineEdit* m_hdri_path;
    QDoubleSpinBox* m_hdri_rotation;
    QDoubleSpinBox* m_hdri_intensity;
    // Xform page: [row][col] where row 0=T,1=R,2=S and col 0=X,1=Y,2=Z
    QDoubleSpinBox* m_xform[3][3];
    // IvarRender page widgets
    QSpinBox* m_spp;
};

/// Category colors — composition nodes (USD ingest/export) get a
/// blue accent, operations (scatter/xform/instance) get orange.
/// The `header_color_index` field on BifNodeGraphicsItem selects.
enum class NodeCategory {
    Composition,   // blue accent
    Operation,     // orange accent
    Render,        // green accent
    Environment,   // purple accent
};

class BifNodeWire;

/// Visual node in the graph — header + body + pin rows.
class BifNodeGraphicsItem : public QGraphicsObject {
    Q_OBJECT
public:
    struct Pin {
        QString name;
        // Local pin-center position relative to the node's top-left.
        QPointF local_pos;
        bool is_input;
    };

    BifNodeGraphicsItem(const QString& title,
                        const QString& type_name,
                        NodeCategory category,
                        QVector<Pin> inputs,
                        QVector<Pin> outputs,
                        QGraphicsItem* parent = nullptr);

    QRectF boundingRect() const override;
    void paint(QPainter* painter,
               const QStyleOptionGraphicsItem* option,
               QWidget* widget = nullptr) override;

    int input_count() const { return m_inputs.size(); }
    int output_count() const { return m_outputs.size(); }

    /// Scene-coords of pin `pin_index`. If `is_input` is true, resolves
    /// against `m_inputs`; otherwise `m_outputs`.
    QPointF scene_pin_pos(int pin_index, bool is_input) const;

    void set_backend_id(int backend_id);
    int backend_id() const;

signals:
    /// Fired whenever the node's scene position changes so wires
    /// attached to its pins can repath themselves.
    void moved();
    void selected(int backend_id);

protected:
    QVariant itemChange(GraphicsItemChange change,
                        const QVariant& value) override;

private:
    QString m_title;
    QString m_type_name;
    NodeCategory m_category;
    QVector<Pin> m_inputs;
    QVector<Pin> m_outputs;
    int m_backend_id;
    qreal m_width;
    qreal m_header_height;
    qreal m_row_height;
    qreal m_body_padding;
};

/// Bezier wire between two pins on two nodes.
class BifNodeWire : public QGraphicsPathItem {
public:
    BifNodeWire(BifNodeGraphicsItem* from_node, int from_pin_index,
                BifNodeGraphicsItem* to_node, int to_pin_index,
                QGraphicsItem* parent = nullptr);

    /// Recompute path from current pin positions. Called by the
    /// widget when either endpoint node emits `moved()`.
    void refresh();

    BifNodeGraphicsItem* to_node() const { return m_to_node; }
    int to_pin_index() const { return m_to_pin_index; }

private:
    BifNodeGraphicsItem* m_from_node;
    int m_from_pin_index;
    BifNodeGraphicsItem* m_to_node;
    int m_to_pin_index;
};

/// Zooming + middle-mouse panning QGraphicsView subclass. Wheel
/// events must be handled here (not on the outer widget) because
/// QGraphicsView consumes them for its built-in scroll behavior.
class NodeGraphView : public QGraphicsView {
    Q_OBJECT
public:
    explicit NodeGraphView(QGraphicsScene* scene, QWidget* parent = nullptr);
    void set_nodes(QVector<BifNodeGraphicsItem*>* nodes);

signals:
    void addNodeRequested(const QString& type_name, QPointF scene_pos);
    void deleteSelectedNodesRequested();
    void pinsConnected(int from_backend_id, int from_pin, int to_backend_id, int to_pin);

protected:
    void wheelEvent(QWheelEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void keyPressEvent(QKeyEvent* event) override;
    void contextMenuEvent(QContextMenuEvent* event) override;

private:
    struct PinRef {
        BifNodeGraphicsItem* node{nullptr};
        int pin_index{-1};
        bool is_input{false};
        bool valid() const { return node != nullptr; }
    };
    PinRef pin_at(QPointF scene_pos) const;
    static constexpr qreal kPinHitRadius = 8.0;

    QVector<BifNodeGraphicsItem*>* m_nodes{nullptr};
    bool m_dragging{false};
    PinRef m_drag_from;
    QGraphicsPathItem* m_drag_wire{nullptr};
};

class NodeGraphWidget : public QWidget {
    Q_OBJECT
public:
    explicit NodeGraphWidget(BifShellState* state, QWidget* parent = nullptr);
    ~NodeGraphWidget() override;

private:
    int create_backend_node(const QString& type_name, QPointF scene_pos);
    void add_node_for_type(const QString& type_name, QPointF scene_pos);
    void delete_selected_nodes();
    void on_node_selected(int backend_id);
    BifNodeGraphicsItem* add_node(const QString& title,
                                  const QString& type_name,
                                  NodeCategory category,
                                  int num_inputs,
                                  int num_outputs,
                                  QPointF scene_pos);
    BifNodeWire* connect_pins(BifNodeGraphicsItem* from, int from_pin,
                              BifNodeGraphicsItem* to, int to_pin);
    BifNodeGraphicsItem* node_by_backend_id(int id) const;

    BifShellState* m_state;
    QGraphicsScene* m_scene;
    NodeGraphView* m_view;
    NodeParamPanel* m_param_panel;
    QVector<BifNodeGraphicsItem*> m_nodes;
    QVector<BifNodeWire*> m_wires;
};
