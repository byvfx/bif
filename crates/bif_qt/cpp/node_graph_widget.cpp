#include "node_graph_widget.h"

#include <QBrush>
#include <QColor>
#include <QFont>
#include <QGraphicsScene>
#include <QGraphicsView>
#include <QPainter>
#include <QPainterPath>
#include <QPen>
#include <QVBoxLayout>
#include <QWheelEvent>

namespace {

constexpr qreal kPinRadius = 5.0;
constexpr qreal kDefaultNodeWidth = 170.0;
constexpr qreal kHeaderHeight = 24.0;
constexpr qreal kRowHeight = 20.0;
constexpr qreal kBodyPadding = 6.0;

QColor header_color_for(NodeCategory cat) {
    switch (cat) {
        case NodeCategory::Composition: return QColor(74, 144, 217);   // blue
        case NodeCategory::Operation:   return QColor(230, 150, 70);   // orange
        case NodeCategory::Render:      return QColor(100, 180, 110);  // green
        case NodeCategory::Environment: return QColor(180, 120, 220);  // purple
    }
    return QColor(80, 85, 95);
}

QColor pin_color_for(const QString& pin_name) {
    // Phase D.2 heuristic — refine with real SceneNode pin types.
    const auto lower = pin_name.toLower();
    if (lower.contains(QLatin1String("scene")) || lower.contains(QLatin1String("usd"))
        || lower == QLatin1String("in") || lower == QLatin1String("out")) {
        return QColor(100, 200, 100);
    }
    if (lower.contains(QLatin1String("image")) || lower.contains(QLatin1String("texture"))) {
        return QColor(200, 150, 50);
    }
    if (lower.contains(QLatin1String("env")) || lower.contains(QLatin1String("hdri"))) {
        return QColor(100, 150, 255);
    }
    return QColor(180, 185, 195);
}

}  // namespace

// ---------------------------------------------------------------------------
// BifNodeGraphicsItem
// ---------------------------------------------------------------------------

BifNodeGraphicsItem::BifNodeGraphicsItem(const QString& title,
                                         const QString& type_name,
                                         NodeCategory category,
                                         QVector<Pin> inputs,
                                         QVector<Pin> outputs,
                                         QGraphicsItem* parent)
    : QGraphicsObject(parent),
      m_title(title),
      m_type_name(type_name),
      m_category(category),
      m_inputs(std::move(inputs)),
      m_outputs(std::move(outputs)),
      m_width(kDefaultNodeWidth),
      m_header_height(kHeaderHeight),
      m_row_height(kRowHeight),
      m_body_padding(kBodyPadding) {
    setFlag(QGraphicsItem::ItemIsMovable, true);
    setFlag(QGraphicsItem::ItemIsSelectable, true);
    setFlag(QGraphicsItem::ItemSendsGeometryChanges, true);
    setCacheMode(QGraphicsItem::DeviceCoordinateCache);
    setZValue(1.0);

    // Assign local pin positions — inputs on the left edge, outputs
    // on the right edge, spaced one row apart under the header.
    const int max_rows = qMax(m_inputs.size(), m_outputs.size());
    for (int i = 0; i < m_inputs.size(); ++i) {
        m_inputs[i].local_pos = QPointF(0, m_header_height + m_body_padding + i * m_row_height + m_row_height / 2);
        m_inputs[i].is_input = true;
    }
    for (int i = 0; i < m_outputs.size(); ++i) {
        m_outputs[i].local_pos = QPointF(m_width, m_header_height + m_body_padding + i * m_row_height + m_row_height / 2);
        m_outputs[i].is_input = false;
    }
    Q_UNUSED(max_rows);
}

QRectF BifNodeGraphicsItem::boundingRect() const {
    const int rows = qMax(m_inputs.size(), m_outputs.size());
    const qreal height = m_header_height + m_body_padding * 2 + rows * m_row_height;
    // Inflate a bit so the pin lollipops aren't clipped on the sides.
    return QRectF(-kPinRadius, 0, m_width + 2 * kPinRadius, height);
}

void BifNodeGraphicsItem::paint(QPainter* painter,
                                const QStyleOptionGraphicsItem* /*option*/,
                                QWidget* /*widget*/) {
    painter->setRenderHint(QPainter::Antialiasing, true);

    const int rows = qMax(m_inputs.size(), m_outputs.size());
    const qreal body_height = m_header_height + m_body_padding * 2 + rows * m_row_height;
    const QRectF body_rect(0, 0, m_width, body_height);

    // Body
    const bool selected = isSelected();
    painter->setPen(QPen(selected ? QColor(74, 144, 217) : QColor(20, 22, 26), selected ? 2.0 : 1.0));
    painter->setBrush(QColor(42, 47, 54));
    painter->drawRoundedRect(body_rect, 4, 4);

    // Header
    const QRectF header_rect(0, 0, m_width, m_header_height);
    QPainterPath header_path;
    header_path.addRoundedRect(header_rect, 4, 4);
    // Bottom-fill to avoid rounded bottom on header (only top rounded).
    painter->setClipPath(header_path);
    painter->setPen(Qt::NoPen);
    painter->setBrush(header_color_for(m_category));
    painter->drawRect(header_rect);
    painter->setClipping(false);

    // Header text — node title (bold) + type label in lighter tone
    QFont title_font = painter->font();
    title_font.setBold(true);
    title_font.setPointSize(qMax(8, title_font.pointSize() - 1));
    painter->setFont(title_font);
    painter->setPen(QColor(255, 255, 255));
    painter->drawText(QRectF(6, 2, m_width - 12, m_header_height - 4),
                      Qt::AlignLeft | Qt::AlignVCenter, m_title);
    painter->drawText(QRectF(6, 2, m_width - 6, m_header_height - 4),
                      Qt::AlignRight | Qt::AlignVCenter,
                      QStringLiteral("[%1]").arg(m_type_name));

    // Pins + pin labels
    QFont pin_font = painter->font();
    pin_font.setBold(false);
    pin_font.setPointSize(qMax(7, pin_font.pointSize() - 1));
    painter->setFont(pin_font);

    for (int i = 0; i < m_inputs.size(); ++i) {
        const auto& pin = m_inputs[i];
        painter->setPen(QPen(QColor(20, 22, 26), 1));
        painter->setBrush(pin_color_for(pin.name));
        painter->drawEllipse(pin.local_pos, kPinRadius, kPinRadius);
        painter->setPen(QColor(200, 205, 215));
        painter->drawText(QRectF(kPinRadius + 6, pin.local_pos.y() - 8, m_width / 2, 16),
                          Qt::AlignLeft | Qt::AlignVCenter, pin.name);
    }
    for (int i = 0; i < m_outputs.size(); ++i) {
        const auto& pin = m_outputs[i];
        painter->setPen(QPen(QColor(20, 22, 26), 1));
        painter->setBrush(pin_color_for(pin.name));
        painter->drawEllipse(pin.local_pos, kPinRadius, kPinRadius);
        painter->setPen(QColor(200, 205, 215));
        painter->drawText(QRectF(m_width / 2, pin.local_pos.y() - 8,
                                  m_width / 2 - kPinRadius - 6, 16),
                          Qt::AlignRight | Qt::AlignVCenter, pin.name);
    }
}

QPointF BifNodeGraphicsItem::scene_pin_pos(int pin_index, bool is_input) const {
    const auto& pins = is_input ? m_inputs : m_outputs;
    if (pin_index < 0 || pin_index >= pins.size()) return scenePos();
    return mapToScene(pins[pin_index].local_pos);
}

QVariant BifNodeGraphicsItem::itemChange(GraphicsItemChange change,
                                         const QVariant& value) {
    if (change == ItemPositionHasChanged) {
        emit moved();
    }
    return QGraphicsObject::itemChange(change, value);
}

// ---------------------------------------------------------------------------
// BifNodeWire
// ---------------------------------------------------------------------------

BifNodeWire::BifNodeWire(BifNodeGraphicsItem* from_node, int from_pin_index,
                         BifNodeGraphicsItem* to_node, int to_pin_index,
                         QGraphicsItem* parent)
    : QGraphicsPathItem(parent),
      m_from_node(from_node),
      m_from_pin_index(from_pin_index),
      m_to_node(to_node),
      m_to_pin_index(to_pin_index) {
    setPen(QPen(QColor(120, 135, 155), 2.0, Qt::SolidLine, Qt::RoundCap));
    setZValue(0.5);  // behind nodes
    refresh();
}

void BifNodeWire::refresh() {
    if (!m_from_node || !m_to_node) return;
    const QPointF p0 = m_from_node->scene_pin_pos(m_from_pin_index, /*is_input=*/false);
    const QPointF p1 = m_to_node->scene_pin_pos(m_to_pin_index, /*is_input=*/true);
    const qreal dx = qMax<qreal>(60.0, qAbs(p1.x() - p0.x()) * 0.5);
    QPainterPath path;
    path.moveTo(p0);
    path.cubicTo(p0 + QPointF(dx, 0), p1 - QPointF(dx, 0), p1);
    setPath(path);

    // TODO (Phase D follow-up): add an orthogonal / manhattan routing
    // option (toggle on the node graph toolbar). Sketch: horizontal
    // halfway lead from p0, vertical span, horizontal lead into p1,
    // with rounded corners. Useful for dense graphs à la Nuke/Houdini.
}

// ---------------------------------------------------------------------------
// NodeGraphView
// ---------------------------------------------------------------------------

NodeGraphView::NodeGraphView(QGraphicsScene* scene, QWidget* parent)
    : QGraphicsView(scene, parent) {
    setRenderHint(QPainter::Antialiasing, true);
    setRenderHint(QPainter::SmoothPixmapTransform, true);
    setDragMode(QGraphicsView::RubberBandDrag);
    setTransformationAnchor(QGraphicsView::AnchorUnderMouse);
    setResizeAnchor(QGraphicsView::AnchorViewCenter);
    setViewportUpdateMode(QGraphicsView::BoundingRectViewportUpdate);
    setStyleSheet(QStringLiteral(
        "QGraphicsView { background-color: rgba(26, 29, 33, 255); border: none; }"));
}

void NodeGraphView::wheelEvent(QWheelEvent* event) {
    const double factor = event->angleDelta().y() > 0 ? 1.15 : 1.0 / 1.15;
    scale(factor, factor);
    event->accept();
}

void NodeGraphView::mousePressEvent(QMouseEvent* event) {
    if (event->button() == Qt::MiddleButton) {
        // Swap to hand drag for middle-button pan. Cache the previous
        // mode so mouseReleaseEvent can restore it.
        setDragMode(QGraphicsView::ScrollHandDrag);
        // Translate the middle-click into a left-click so the built-in
        // ScrollHandDrag handler picks it up.
        QMouseEvent fake(event->type(), event->position(), event->scenePosition(),
                         event->globalPosition(), Qt::LeftButton,
                         Qt::LeftButton, event->modifiers());
        QGraphicsView::mousePressEvent(&fake);
        event->accept();
        return;
    }
    QGraphicsView::mousePressEvent(event);
}

void NodeGraphView::mouseReleaseEvent(QMouseEvent* event) {
    if (event->button() == Qt::MiddleButton) {
        QMouseEvent fake(event->type(), event->position(), event->scenePosition(),
                         event->globalPosition(), Qt::LeftButton,
                         Qt::NoButton, event->modifiers());
        QGraphicsView::mouseReleaseEvent(&fake);
        setDragMode(QGraphicsView::RubberBandDrag);
        event->accept();
        return;
    }
    QGraphicsView::mouseReleaseEvent(event);
}

// ---------------------------------------------------------------------------
// NodeGraphWidget
// ---------------------------------------------------------------------------

NodeGraphWidget::NodeGraphWidget(QWidget* parent)
    : QWidget(parent), m_scene(nullptr), m_view(nullptr) {
    setObjectName(QStringLiteral("node_graph_widget"));

    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_scene = new QGraphicsScene(this);
    m_scene->setSceneRect(-2000, -1200, 4000, 2400);
    m_scene->setBackgroundBrush(QColor(26, 29, 33));

    // NodeGraphView subclass handles wheel-zoom + middle-button pan.
    // Can't use a plain QGraphicsView here — wheel events need to be
    // overridden on the view itself (wheel doesn't propagate to the
    // wrapping QWidget's wheelEvent).
    m_view = new NodeGraphView(m_scene, this);
    layout->addWidget(m_view, 1);

    seed_demo_graph();
}

NodeGraphWidget::~NodeGraphWidget() = default;

BifNodeGraphicsItem* NodeGraphWidget::add_node(const QString& title,
                                               const QString& type_name,
                                               NodeCategory category,
                                               int num_inputs,
                                               int num_outputs,
                                               QPointF scene_pos) {
    QVector<BifNodeGraphicsItem::Pin> inputs;
    QVector<BifNodeGraphicsItem::Pin> outputs;
    for (int i = 0; i < num_inputs; ++i) {
        inputs.append({QStringLiteral("in%1").arg(i + 1), {}, true});
    }
    for (int i = 0; i < num_outputs; ++i) {
        outputs.append({QStringLiteral("out%1").arg(i + 1), {}, false});
    }
    auto* node = new BifNodeGraphicsItem(
        title, type_name, category, inputs, outputs);
    node->setPos(scene_pos);
    m_scene->addItem(node);
    m_nodes.append(node);
    return node;
}

BifNodeWire* NodeGraphWidget::connect_pins(BifNodeGraphicsItem* from, int from_pin,
                                           BifNodeGraphicsItem* to, int to_pin) {
    auto* wire = new BifNodeWire(from, from_pin, to, to_pin);
    m_scene->addItem(wire);
    m_wires.append(wire);
    // BifNodeWire isn't a QObject (QGraphicsPathItem has no QObject
    // base), so it can't be a connect context. Tie connection lifetime
    // to the sender node — when `from`/`to` is destroyed the scene is
    // tearing down so `wire` will be gone shortly after.
    QObject::connect(from, &BifNodeGraphicsItem::moved, from, [wire]() {
        wire->refresh();
    });
    QObject::connect(to, &BifNodeGraphicsItem::moved, to, [wire]() {
        wire->refresh();
    });
    return wire;
}

void NodeGraphWidget::seed_demo_graph() {
    // A small chain mimicking a typical BIF assembly graph:
    //   UsdRead → Scatter → Xform → IvarRender
    // + a branching UsdPrim feed into GraftBranches.
    auto* read = add_node(
        QStringLiteral("Hero"), QStringLiteral("UsdRead"),
        NodeCategory::Composition, 0, 1, QPointF(-540, -80));

    auto* scatter = add_node(
        QStringLiteral("Scatter100"), QStringLiteral("Scatter"),
        NodeCategory::Operation, 1, 1, QPointF(-280, -80));

    auto* xform = add_node(
        QStringLiteral("Offset"), QStringLiteral("Xform"),
        NodeCategory::Operation, 1, 1, QPointF(-20, -80));

    auto* render = add_node(
        QStringLiteral("Beauty"), QStringLiteral("IvarRender"),
        NodeCategory::Render, 2, 0, QPointF(240, -80));

    auto* hdri = add_node(
        QStringLiteral("Studio"), QStringLiteral("HdriEnvironment"),
        NodeCategory::Environment, 0, 1, QPointF(-20, 120));

    connect_pins(read, 0, scatter, 0);
    connect_pins(scatter, 0, xform, 0);
    connect_pins(xform, 0, render, 0);
    connect_pins(hdri, 0, render, 1);
}
