# Qt Node Graph Basic Usability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a param panel sidebar + wire drag-connect to the Qt node graph, making UsdRead, HdriEnvironment, Xform, and IvarRender nodes functionally usable.

**Architecture:** 3-layer bridge — Rust backend (`bif_viewport/src/lib.rs`) exposes new query/setter fns; cxx-qt bridge (`bif_qt/src/main_window.rs`) wires them to Qt; C++ widget (`node_graph_widget.h/cpp`) adds `NodeParamPanel` sidebar and wire-drag interaction in `NodeGraphView`.

**Tech Stack:** Rust + cxx-qt 0.7, Qt 6 QGraphicsView/QGraphicsScene, egui_snarl as backend graph store.

## Global Constraints

- All Rust: `cargo clippy -- -D warnings` + `cargo fmt --check` must pass
- All C++ targets Qt 6 (no Qt 5 APIs); QJsonDocument is in Qt Core (no extra deps)
- No new crate dependencies
- cxx-qt bridge: follow the pattern of existing `on_node_graph_*` fns in `main_window.rs`
- TDD for all new Rust fns in `lib.rs` (test before impl)
- Commit after each task

---

## Confirmed Architecture (from codebase search)

- `GraphNodeId` is a newtype `GraphNodeId(u64)` — bridge converts: `bif_viewport::GraphNodeId(node_id as u64)`
- Renderer methods take `GraphNodeId`; bridge fns take `i32` and convert
- Dispatch method: `self.handle_node_graph_event(NodeGraphEvent::...)`
- `NodeGraphEvent::XformChanged { node_id: GraphNodeId }` **already exists** — no need to add it
- `on_start_ivar_render` takes **no args** — `fn on_start_ivar_render(self: Pin<&mut BifShellState>)`
- cxx-qt receiver: `Pin<&mut BifShellState>` (not MainWindow); bridge uses `with_viewport_mut(|vp| vp.renderer_mut().method(graph_id))`
- C++ calls bridge methods directly on `m_state`: `m_state->on_node_graph_*(...)` — no separate mainWindow() accessor

## File Map

| File | Change |
|------|--------|
| `crates/bif_viewport/src/lib.rs` | Add 5 pub fn (take `GraphNodeId`): `node_graph_get_node_info`, `node_graph_set_usd_read_path`, `node_graph_load_hdri`, `node_graph_set_xform_params`, `node_graph_connect_pins` |
| `crates/bif_qt/src/main_window.rs` | Expose all 5 fns in cxx-qt extern/impl block (take `i32`, convert to `GraphNodeId`) |
| `crates/bif_qt/cpp/node_graph_widget.h` | Add `NodeParamPanel` class; add wire-drag state + `pinsConnected` signal to `NodeGraphView`; add `node_by_backend_id` helper to `NodeGraphWidget` |
| `crates/bif_qt/cpp/node_graph_widget.cpp` | Implement `NodeParamPanel`; implement wire-drag mouse events |

---

## Task 1: Rust — node_graph_get_node_info

**Files:**
- Modify: `crates/bif_viewport/src/lib.rs` (near other `node_graph_*` fns)

**Interfaces:**
- Produces: `pub fn node_graph_get_node_info(&self, node_id: GraphNodeId) -> Option<String>`

JSON format (no external serializer — hand-built strings):
```
UsdRead:        {"type":"UsdRead","file_path":"...","is_loaded":true}
HdriEnvironment:{"type":"HdriEnvironment","file_path":"...","rotation":0.0,"intensity":1.0,"show_background":false}
Xform:          {"type":"Xform","tx":0.0,"ty":0.0,"tz":0.0,"rx":0.0,"ry":0.0,"rz":0.0,"sx":1.0,"sy":1.0,"sz":1.0}
IvarRender:     {"type":"IvarRender","spp":64}
other/missing:  None
```

- [ ] **Step 1: Write failing tests**

In the `#[cfg(test)]` mod at the bottom of `lib.rs`:

```rust
#[test]
fn node_graph_get_node_info_usd_read() {
    let mut state = make_test_state();
    let id = state.node_graph_add_node("UsdRead", 0.0, 0.0).unwrap();
    let info = state.node_graph_get_node_info(id).unwrap();
    assert!(info.contains("\"type\":\"UsdRead\""));
    assert!(info.contains("\"file_path\":\"\""));
    assert!(info.contains("\"is_loaded\":false"));
}

#[test]
fn node_graph_get_node_info_unknown_id_returns_none() {
    let state = make_test_state();
    assert!(state.node_graph_get_node_info(GraphNodeId(9999)).is_none());
}
```

- [ ] **Step 2: Run tests — expect compile failure (method missing)**

```
cargo test -p bif_viewport node_graph_get_node_info 2>&1 | tail -5
```

- [ ] **Step 3: Implement**

The snarl node access pattern (from `node_graph_delete_node` in lib.rs):
- Convert: `let snarl_id: egui_snarl::NodeId = node_id.into();`
- Existence check: `self.nodes.node_graph_state.snarl.node_ids().any(|id| id == snarl_id)`
- Indexed access: `&self.nodes.node_graph_state.snarl[snarl_id]`

```rust
pub fn node_graph_get_node_info(&self, node_id: GraphNodeId) -> Option<String> {
    let snarl_id: egui_snarl::NodeId = node_id.into();
    let snarl = &self.nodes.node_graph_state.snarl;
    if !snarl.node_ids().any(|id| id == snarl_id) {
        return None;
    }
    let json = match &snarl[snarl_id] {
        SceneNode::UsdRead { file_path, is_loaded, .. } => format!(
            r#"{{"type":"UsdRead","file_path":{},"is_loaded":{}}}"#,
            json_str(file_path), is_loaded
        ),
        SceneNode::HdriEnvironment { file_path, rotation, intensity, show_background, .. } => format!(
            r#"{{"type":"HdriEnvironment","file_path":{},"rotation":{},"intensity":{},"show_background":{}}}"#,
            json_str(file_path), rotation, intensity, show_background
        ),
        SceneNode::Xform { translate: [tx,ty,tz], rotate: [rx,ry,rz], scale: [sx,sy,sz], .. } => format!(
            r#"{{"type":"Xform","tx":{},"ty":{},"tz":{},"rx":{},"ry":{},"rz":{},"sx":{},"sy":{},"sz":{}}}"#,
            tx, ty, tz, rx, ry, rz, sx, sy, sz
        ),
        SceneNode::IvarRender { spp, .. } => format!(r#"{{"type":"IvarRender","spp":{}}}"#, spp),
        _ => return None,
    };
    Some(json)
}

// Private helper — in the same impl block:
fn json_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
```

- [ ] **Step 4: Run tests — expect green**

```
cargo test -p bif_viewport node_graph_get_node_info
```

- [ ] **Step 5: Clippy + fmt**

```
cargo clippy -p bif_viewport -- -D warnings && cargo fmt
```

- [ ] **Step 6: Commit**

```
git add crates/bif_viewport/src/lib.rs
git commit -m "feat(node-graph): node_graph_get_node_info for UsdRead/Hdri/Xform/IvarRender"
```

---

## Task 2: Rust — setter + connect fns

**Files:**
- Modify: `crates/bif_viewport/src/lib.rs`

**Interfaces:**
- Produces (all on the same `impl Renderer` block as Task 1, all take `GraphNodeId`):
  - `pub fn node_graph_set_usd_read_path(&mut self, node_id: GraphNodeId, path: String) -> bool`
  - `pub fn node_graph_load_hdri(&mut self, node_id: GraphNodeId, path: String, rotation: f32, intensity: f32) -> bool`
  - `pub fn node_graph_set_xform_params(&mut self, node_id: GraphNodeId, tx: f32, ty: f32, tz: f32, rx: f32, ry: f32, rz: f32, sx: f32, sy: f32, sz: f32) -> bool`
  - `pub fn node_graph_connect_pins(&mut self, from_id: GraphNodeId, from_pin: i32, to_id: GraphNodeId, to_pin: i32) -> bool`

SceneNode field reference (verified from source):
- `UsdRead`: `file_path: String`, `is_loaded: bool`, `error: Option<String>`
- `HdriEnvironment`: `file_path: String`, `rotation: f32`, `intensity: f32`, `show_background: bool`
- `Xform`: `translate: [f32; 3]`, `rotate: [f32; 3]`, `scale: [f32; 3]`, `is_applied: bool`
- `IvarRender`: `spp: u32`
- PinType: `Scene`, `Image`, `Environment` — checked via `SceneNode::output_pin(index)` / `input_pin(index)`
- Dispatch: `self.handle_node_graph_event(NodeGraphEvent::...)` (confirmed from existing code)
- `NodeGraphEvent::XformChanged { node_id: GraphNodeId }` — **already exists** in mod.rs

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn node_graph_set_usd_read_path_updates_node() {
    let mut state = make_test_state();
    let id = state.node_graph_add_node("UsdRead", 0.0, 0.0).unwrap();
    let ok = state.node_graph_set_usd_read_path(id, "/tmp/test.usda".to_string());
    assert!(ok);
    let info = state.node_graph_get_node_info(id).unwrap();
    assert!(info.contains("/tmp/test.usda"));
}

#[test]
fn node_graph_connect_pins_compatible() {
    let mut state = make_test_state();
    let usd = state.node_graph_add_node("UsdRead", 0.0, 0.0).unwrap();
    let xform = state.node_graph_add_node("Xform", 200.0, 0.0).unwrap();
    // UsdRead output[0] → Xform input[0]: both PinType::Scene
    assert!(state.node_graph_connect_pins(usd, 0, xform, 0));
}

#[test]
fn node_graph_connect_pins_bad_id_returns_false() {
    let mut state = make_test_state();
    assert!(!state.node_graph_connect_pins(GraphNodeId(9999), 0, GraphNodeId(9998), 0));
}
```

- [ ] **Step 2: Run — expect failure**

```
cargo test -p bif_viewport node_graph_set_usd node_graph_connect 2>&1 | tail -5
```

- [ ] **Step 3: Implement node_graph_set_usd_read_path**

Dispatch method confirmed: `self.handle_node_graph_event(NodeGraphEvent::...)`.
Snarl access pattern: `let snarl_id: egui_snarl::NodeId = node_id.into();` then `&mut snarl[snarl_id]`.

```rust
pub fn node_graph_set_usd_read_path(&mut self, node_id: GraphNodeId, path: String) -> bool {
    let snarl_id: egui_snarl::NodeId = node_id.into();
    let snarl = &mut self.nodes.node_graph_state.snarl;
    if !snarl.node_ids().any(|id| id == snarl_id) { return false; }
    let SceneNode::UsdRead { file_path, is_loaded, error } = &mut snarl[snarl_id] else { return false; };
    *file_path = path.clone();
    *is_loaded = false;
    *error = None;
    self.handle_node_graph_event(NodeGraphEvent::LoadUsdFile { node_id, path });
    true
}
```

- [ ] **Step 4: Implement node_graph_load_hdri**

```rust
pub fn node_graph_load_hdri(
    &mut self, node_id: GraphNodeId, path: String, rotation: f32, intensity: f32,
) -> bool {
    let snarl_id: egui_snarl::NodeId = node_id.into();
    let snarl = &mut self.nodes.node_graph_state.snarl;
    if !snarl.node_ids().any(|id| id == snarl_id) { return false; }
    let SceneNode::HdriEnvironment { file_path, rotation: r, intensity: i, .. } = &mut snarl[snarl_id] else { return false; };
    *file_path = path.clone();
    *r = rotation;
    *i = intensity;
    self.handle_node_graph_event(NodeGraphEvent::LoadHdri { node_id, path, rotation, intensity });
    true
}
```

- [ ] **Step 5: Implement node_graph_set_xform_params**

`NodeGraphEvent::XformChanged { node_id: GraphNodeId }` **already exists** in mod.rs — no changes needed there.

```rust
pub fn node_graph_set_xform_params(
    &mut self, node_id: GraphNodeId,
    tx: f32, ty: f32, tz: f32,
    rx: f32, ry: f32, rz: f32,
    sx: f32, sy: f32, sz: f32,
) -> bool {
    let snarl_id: egui_snarl::NodeId = node_id.into();
    let snarl = &mut self.nodes.node_graph_state.snarl;
    if !snarl.node_ids().any(|id| id == snarl_id) { return false; }
    let SceneNode::Xform { translate, rotate, scale, is_applied, .. } = &mut snarl[snarl_id] else { return false; };
    *translate = [tx, ty, tz];
    *rotate = [rx, ry, rz];
    *scale = [sx, sy, sz];
    *is_applied = false;
    self.handle_node_graph_event(NodeGraphEvent::XformChanged { node_id });
    true
}
```

- [ ] **Step 6: Implement node_graph_connect_pins**

```rust
pub fn node_graph_connect_pins(
    &mut self, from_id: GraphNodeId, from_pin: i32, to_id: GraphNodeId, to_pin: i32,
) -> bool {
    use egui_snarl::{InPinId, OutPinId};
    let from_snarl: egui_snarl::NodeId = from_id.into();
    let to_snarl: egui_snarl::NodeId = to_id.into();
    let snarl = &mut self.nodes.node_graph_state.snarl;
    let ids: Vec<_> = snarl.node_ids().collect();
    if !ids.contains(&from_snarl) || !ids.contains(&to_snarl) { return false; }
    let from_type = snarl[from_snarl].output_pin(from_pin as usize).map(|(_, t)| t);
    let to_type = snarl[to_snarl].input_pin(to_pin as usize).map(|(_, t)| t);
    if from_type.is_none() || to_type.is_none() || from_type != to_type {
        return false;
    }
    snarl.connect(
        OutPinId { node: from_snarl, output: from_pin as usize },
        InPinId { node: to_snarl, input: to_pin as usize },
    );
    true
}
```

- [ ] **Step 7: Run all new tests — expect green**

```
cargo test -p bif_viewport node_graph_set node_graph_load node_graph_connect
```

- [ ] **Step 8: Clippy + fmt + commit**

```
cargo clippy -p bif_viewport -- -D warnings && cargo fmt
git add crates/bif_viewport/src/lib.rs crates/bif_viewport/src/node_graph/mod.rs crates/bif_viewport/src/node_graph/node_dispatch.rs
git commit -m "feat(node-graph): setter + connect fns for 4 node types"
```

---

## Task 3: cxx-qt bridge — expose 5 fns in main_window.rs

**Files:**
- Modify: `crates/bif_qt/src/main_window.rs`

**Interfaces:**
- Consumes: all 5 fns from Tasks 1–2
- Produces: Qt-callable `on_node_graph_*` bridge methods

- [ ] **Step 1: Locate the extern + impl blocks**

Grep for `fn on_node_graph_delete_node` in `main_window.rs`. This shows the exact pattern:
- Declaration (in `extern "RustQt"` block): `fn on_node_graph_delete_node(self: Pin<&mut BifShellState>, node_id: i32) -> bool;`
- Impl: `fn on_node_graph_delete_node(mut self: Pin<&mut Self>, node_id: i32) -> bool { let graph_id = bif_viewport::GraphNodeId(node_id as u64); with_viewport_mut(|vp| vp.renderer_mut().node_graph_delete_node(graph_id)) ... }`

Follow this exact pattern for all 5 new fns.

- [ ] **Step 2: Add declarations to the `extern "RustQt"` block** (where existing `on_node_graph_*` declarations live)

```rust
fn on_node_graph_get_node_info(self: Pin<&mut BifShellState>, node_id: i32) -> QString;
fn on_node_graph_set_usd_read_path(self: Pin<&mut BifShellState>, node_id: i32, path: QString) -> bool;
fn on_node_graph_load_hdri(self: Pin<&mut BifShellState>, node_id: i32, path: QString, rotation: f32, intensity: f32) -> bool;
fn on_node_graph_set_xform_params(self: Pin<&mut BifShellState>, node_id: i32, tx: f32, ty: f32, tz: f32, rx: f32, ry: f32, rz: f32, sx: f32, sy: f32, sz: f32) -> bool;
fn on_node_graph_connect_pins(self: Pin<&mut BifShellState>, from_id: i32, from_pin: i32, to_id: i32, to_pin: i32) -> bool;
```

- [ ] **Step 3: Add implementations** (near `on_node_graph_delete_node` impl)

```rust
fn on_node_graph_get_node_info(mut self: Pin<&mut Self>, node_id: i32) -> cxx_qt_lib::QString {
    if node_id < 0 { return cxx_qt_lib::QString::default(); }
    let graph_id = bif_viewport::GraphNodeId(node_id as u64);
    let result = with_viewport_mut(|vp| vp.renderer_mut().node_graph_get_node_info(graph_id))
        .flatten()
        .unwrap_or_default();
    cxx_qt_lib::QString::from(&result)
}

fn on_node_graph_set_usd_read_path(mut self: Pin<&mut Self>, node_id: i32, path: cxx_qt_lib::QString) -> bool {
    if node_id < 0 { return false; }
    let graph_id = bif_viewport::GraphNodeId(node_id as u64);
    with_viewport_mut(|vp| vp.renderer_mut().node_graph_set_usd_read_path(graph_id, path.to_string()))
        .unwrap_or(false)
}

fn on_node_graph_load_hdri(mut self: Pin<&mut Self>, node_id: i32, path: cxx_qt_lib::QString, rotation: f32, intensity: f32) -> bool {
    if node_id < 0 { return false; }
    let graph_id = bif_viewport::GraphNodeId(node_id as u64);
    with_viewport_mut(|vp| vp.renderer_mut().node_graph_load_hdri(graph_id, path.to_string(), rotation, intensity))
        .unwrap_or(false)
}

fn on_node_graph_set_xform_params(mut self: Pin<&mut Self>, node_id: i32, tx: f32, ty: f32, tz: f32, rx: f32, ry: f32, rz: f32, sx: f32, sy: f32, sz: f32) -> bool {
    if node_id < 0 { return false; }
    let graph_id = bif_viewport::GraphNodeId(node_id as u64);
    with_viewport_mut(|vp| vp.renderer_mut().node_graph_set_xform_params(graph_id, tx, ty, tz, rx, ry, rz, sx, sy, sz))
        .unwrap_or(false)
}

fn on_node_graph_connect_pins(mut self: Pin<&mut Self>, from_id: i32, from_pin: i32, to_id: i32, to_pin: i32) -> bool {
    if from_id < 0 || to_id < 0 { return false; }
    let from_gid = bif_viewport::GraphNodeId(from_id as u64);
    let to_gid   = bif_viewport::GraphNodeId(to_id as u64);
    with_viewport_mut(|vp| vp.renderer_mut().node_graph_connect_pins(from_gid, from_pin, to_gid, to_pin))
        .unwrap_or(false)
}
```

> Check the exact return type of `with_viewport_mut` — if it returns `Option<T>`, use `.unwrap_or(false)`. If it returns `T` directly, call the inner fn without `.unwrap_or`. Match whatever pattern `on_node_graph_delete_node` uses.

- [ ] **Step 3: Build check**

```
cargo build -p bif_qt 2>&1 | head -40
```
Expected: zero warnings, zero errors.

- [ ] **Step 4: Commit**

```
git add crates/bif_qt/src/main_window.rs
git commit -m "feat(node-graph): expose 5 bridge fns via cxx-qt"
```

---

## Task 4: C++ — NodeParamPanel widget

**Files:**
- Modify: `crates/bif_qt/cpp/node_graph_widget.h`
- Modify: `crates/bif_qt/cpp/node_graph_widget.cpp`

**Interfaces:**
- Consumes: `on_node_graph_get_node_info`, `on_node_graph_set_usd_read_path`, `on_node_graph_load_hdri`, `on_node_graph_set_xform_params`, `on_start_ivar_render` from Task 3
- Produces: `NodeParamPanel` class; `NodeGraphWidget::show_params_for(int)` slot

Design: `QStackedWidget` with 5 pages (index 0 = empty, 1 = UsdRead, 2 = Hdri, 3 = Xform, 4 = IvarRender). `show_params_for` parses the JSON returned by `on_node_graph_get_node_info` and switches to the right page.

- [ ] **Step 1: Add NodeParamPanel to node_graph_widget.h**

Insert before the `NodeGraphView` class declaration:

```cpp
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
```

Also add `NodeParamPanel* m_param_panel;` to `NodeGraphWidget`'s private members.

- [ ] **Step 2: Add includes to node_graph_widget.cpp**

At the top of the .cpp file (after existing includes), add any missing:

```cpp
#include <QDoubleSpinBox>
#include <QFileDialog>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QSpinBox>
#include <QStackedWidget>
#include <QVBoxLayout>
```

- [ ] **Step 3: Implement NodeParamPanel constructor**

```cpp
NodeParamPanel::NodeParamPanel(BifShellState* state, QWidget* parent)
    : QWidget(parent), m_state(state)
{
    setFixedWidth(220);
    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(4, 4, 4, 4);
    m_stack = new QStackedWidget;
    root->addWidget(m_stack);
    root->addStretch();

    // Page 0: nothing selected
    m_stack->addWidget(new QLabel("No node selected"));

    // Page 1: UsdRead
    {
        auto* page = new QWidget;
        auto* lay = new QFormLayout(page);
        m_usd_path = new QLineEdit;
        auto* browse = new QPushButton("...");
        browse->setFixedWidth(28);
        auto* row = new QHBoxLayout;
        row->addWidget(m_usd_path);
        row->addWidget(browse);
        lay->addRow("USD File:", row);
        m_stack->addWidget(page);
        connect(browse, &QPushButton::clicked, this, &NodeParamPanel::on_usd_browse);
        connect(m_usd_path, &QLineEdit::editingFinished, this, &NodeParamPanel::on_usd_path_changed);
    }

    // Page 2: HdriEnvironment
    {
        auto* page = new QWidget;
        auto* lay = new QFormLayout(page);
        m_hdri_path = new QLineEdit;
        auto* browse = new QPushButton("...");
        browse->setFixedWidth(28);
        auto* row = new QHBoxLayout;
        row->addWidget(m_hdri_path);
        row->addWidget(browse);
        lay->addRow("HDR File:", row);
        m_hdri_rotation = new QDoubleSpinBox;
        m_hdri_rotation->setRange(-360.0, 360.0);
        m_hdri_rotation->setSingleStep(1.0);
        lay->addRow("Rotation:", m_hdri_rotation);
        m_hdri_intensity = new QDoubleSpinBox;
        m_hdri_intensity->setRange(0.0, 100.0);
        m_hdri_intensity->setSingleStep(0.1);
        m_hdri_intensity->setValue(1.0);
        lay->addRow("Intensity:", m_hdri_intensity);
        auto* apply = new QPushButton("Apply");
        lay->addRow(apply);
        m_stack->addWidget(page);
        connect(browse, &QPushButton::clicked, this, &NodeParamPanel::on_hdri_browse);
        connect(apply, &QPushButton::clicked, this, &NodeParamPanel::on_hdri_apply);
    }

    // Page 3: Xform
    {
        auto* page = new QWidget;
        auto* lay = new QFormLayout(page);
        const char* row_labels[] = {"T", "R", "S"};
        const char* axes[] = {"X", "Y", "Z"};
        for (int r = 0; r < 3; ++r) {
            auto* hlay = new QHBoxLayout;
            for (int c = 0; c < 3; ++c) {
                m_xform[r][c] = new QDoubleSpinBox;
                m_xform[r][c]->setRange(-9999.0, 9999.0);
                m_xform[r][c]->setSingleStep(0.1);
                m_xform[r][c]->setDecimals(3);
                if (r == 2) m_xform[r][c]->setValue(1.0); // scale default
                m_xform[r][c]->setPrefix(QString(axes[c]) + ":");
                hlay->addWidget(m_xform[r][c]);
            }
            auto* rowWidget = new QWidget;
            rowWidget->setLayout(hlay);
            lay->addRow(QString(row_labels[r]) + ":", rowWidget);
        }
        auto* apply = new QPushButton("Apply");
        lay->addRow(apply);
        m_stack->addWidget(page);
        connect(apply, &QPushButton::clicked, this, &NodeParamPanel::on_xform_apply);
    }

    // Page 4: IvarRender
    {
        auto* page = new QWidget;
        auto* lay = new QFormLayout(page);
        m_spp = new QSpinBox;
        m_spp->setRange(1, 65536);
        m_spp->setValue(64);
        lay->addRow("SPP:", m_spp);
        auto* btn = new QPushButton("Render");
        lay->addRow(btn);
        m_stack->addWidget(page);
        connect(btn, &QPushButton::clicked, this, &NodeParamPanel::on_ivar_render_clicked);
    }
}
```

- [ ] **Step 4: Implement show_params_for, clear, and slots**

```cpp
void NodeParamPanel::clear() {
    m_current_id = -1;
    m_stack->setCurrentIndex(0);
}

void NodeParamPanel::show_params_for(int backend_id) {
    m_current_id = backend_id;
    // BifShellState IS the cxx-qt QObject — call bridge methods directly on m_state.
    // Confirmed from existing code: m_state->on_node_graph_select_node(backend_id), etc.
    QString json = m_state->on_node_graph_get_node_info(backend_id);
    if (json.isEmpty()) { clear(); return; }
    QJsonObject obj = QJsonDocument::fromJson(json.toUtf8()).object();
    QString type = obj.value("type").toString();
    if (type == "UsdRead") {
        m_usd_path->setText(obj.value("file_path").toString());
        m_stack->setCurrentIndex(1);
    } else if (type == "HdriEnvironment") {
        m_hdri_path->setText(obj.value("file_path").toString());
        m_hdri_rotation->setValue(obj.value("rotation").toDouble());
        m_hdri_intensity->setValue(obj.value("intensity").toDouble());
        m_stack->setCurrentIndex(2);
    } else if (type == "Xform") {
        m_xform[0][0]->setValue(obj.value("tx").toDouble());
        m_xform[0][1]->setValue(obj.value("ty").toDouble());
        m_xform[0][2]->setValue(obj.value("tz").toDouble());
        m_xform[1][0]->setValue(obj.value("rx").toDouble());
        m_xform[1][1]->setValue(obj.value("ry").toDouble());
        m_xform[1][2]->setValue(obj.value("rz").toDouble());
        m_xform[2][0]->setValue(obj.value("sx").toDouble());
        m_xform[2][1]->setValue(obj.value("sy").toDouble());
        m_xform[2][2]->setValue(obj.value("sz").toDouble());
        m_stack->setCurrentIndex(3);
    } else if (type == "IvarRender") {
        m_spp->setValue(obj.value("spp").toInt());
        m_stack->setCurrentIndex(4);
    } else {
        clear();
    }
}

void NodeParamPanel::on_usd_browse() {
    QString path = QFileDialog::getOpenFileName(this, "Open USD", QString(),
        "USD Files (*.usda *.usdc *.usd)");
    if (path.isEmpty()) return;
    m_usd_path->setText(path);
    on_usd_path_changed();
}

void NodeParamPanel::on_usd_path_changed() {
    if (m_current_id < 0) return;
    m_state->on_node_graph_set_usd_read_path(m_current_id, m_usd_path->text());
}

void NodeParamPanel::on_hdri_browse() {
    QString path = QFileDialog::getOpenFileName(this, "Open HDRI", QString(),
        "HDR Images (*.hdr *.exr)");
    if (path.isEmpty()) return;
    m_hdri_path->setText(path);
}

void NodeParamPanel::on_hdri_apply() {
    if (m_current_id < 0) return;
    m_state->on_node_graph_load_hdri(
        m_current_id, m_hdri_path->text(),
        static_cast<float>(m_hdri_rotation->value()),
        static_cast<float>(m_hdri_intensity->value()));
}

void NodeParamPanel::on_xform_apply() {
    if (m_current_id < 0) return;
    m_state->on_node_graph_set_xform_params(
        m_current_id,
        static_cast<float>(m_xform[0][0]->value()),
        static_cast<float>(m_xform[0][1]->value()),
        static_cast<float>(m_xform[0][2]->value()),
        static_cast<float>(m_xform[1][0]->value()),
        static_cast<float>(m_xform[1][1]->value()),
        static_cast<float>(m_xform[1][2]->value()),
        static_cast<float>(m_xform[2][0]->value()),
        static_cast<float>(m_xform[2][1]->value()),
        static_cast<float>(m_xform[2][2]->value()));
}

void NodeParamPanel::on_ivar_render_clicked() {
    if (m_current_id < 0) return;
    // on_start_ivar_render takes NO args (confirmed from main_window.rs line 439).
    // SPP is stored on the IvarRender node — wiring it to the renderer's ivar_state is a follow-up.
    m_state->on_start_ivar_render();
}
```

> **Note:** Check the exact call pattern for `m_state->mainWindow()` by searching for any existing call in `node_graph_widget.cpp`. If `mainWindow()` isn't a method on `BifShellState`, find the correct accessor.

- [ ] **Step 5: Wire NodeParamPanel into NodeGraphWidget**

In `NodeGraphWidget` constructor (where the layout is set up), replace the current plain `QVBoxLayout` with a horizontal split:

```cpp
m_param_panel = new NodeParamPanel(m_state, this);
auto* hlay = new QHBoxLayout(this);
hlay->setContentsMargins(0, 0, 0, 0);
hlay->setSpacing(0);
hlay->addWidget(m_view, 1);
hlay->addWidget(m_param_panel, 0);
```

In `NodeGraphWidget::on_node_selected(int backend_id)`, add:

```cpp
m_param_panel->show_params_for(backend_id);
```

- [ ] **Step 6: Build check**

```
cargo build -p bif_qt 2>&1 | head -60
```

- [ ] **Step 7: Smoke test**

Launch app (`cargo run -p bif_viewer`). Add a UsdRead node, click it — confirm the param panel shows a file path field + browse button on the right side.

- [ ] **Step 8: Commit**

```
git add crates/bif_qt/cpp/node_graph_widget.h crates/bif_qt/cpp/node_graph_widget.cpp
git commit -m "feat(node-graph): NodeParamPanel sidebar for 4 node types"
```

---

## Task 5: C++ — Wire drag-connect

**Files:**
- Modify: `crates/bif_qt/cpp/node_graph_widget.h`
- Modify: `crates/bif_qt/cpp/node_graph_widget.cpp`

**Interfaces:**
- Consumes: `on_node_graph_connect_pins` from Task 3; `BifNodeGraphicsItem::scene_pin_pos`, `backend_id()`, `m_nodes` from NodeGraphWidget
- Produces: interactive drag-from-output-pin-to-input-pin connection; visual wire on success

Design: drag state lives in `NodeGraphView`. On left-click within `kPinHitRadius` of an output pin — start drag. On release over an input pin on a different node — emit `pinsConnected` signal. `NodeGraphWidget` handles the signal by calling the Rust bridge and drawing the visual wire.

- [ ] **Step 1: Add pin count accessors to BifNodeGraphicsItem in the header**

In the `BifNodeGraphicsItem` class public section, add:

```cpp
int input_count() const { return m_inputs.size(); }
int output_count() const { return m_outputs.size(); }
```

- [ ] **Step 2: Add wire-drag state + signal to NodeGraphView in the header**

Replace the existing `NodeGraphView` declaration with:

```cpp
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
```

- [ ] **Step 3: Implement set_nodes and pin_at in .cpp**

```cpp
void NodeGraphView::set_nodes(QVector<BifNodeGraphicsItem*>* nodes) {
    m_nodes = nodes;
}

NodeGraphView::PinRef NodeGraphView::pin_at(QPointF scene_pos) const {
    if (!m_nodes) return {};
    for (auto* node : *m_nodes) {
        for (int i = 0; i < node->output_count(); ++i) {
            if (QLineF(node->scene_pin_pos(i, false), scene_pos).length() <= kPinHitRadius)
                return {node, i, false};
        }
        for (int i = 0; i < node->input_count(); ++i) {
            if (QLineF(node->scene_pin_pos(i, true), scene_pos).length() <= kPinHitRadius)
                return {node, i, true};
        }
    }
    return {};
}
```

- [ ] **Step 4: Implement drag mouse events in .cpp**

```cpp
void NodeGraphView::mousePressEvent(QMouseEvent* event) {
    if (event->button() == Qt::LeftButton) {
        PinRef hit = pin_at(mapToScene(event->pos()));
        if (hit.valid() && !hit.is_input) {
            m_dragging = true;
            m_drag_from = hit;
            m_drag_wire = new QGraphicsPathItem;
            m_drag_wire->setPen(QPen(Qt::white, 1.5, Qt::DashLine));
            scene()->addItem(m_drag_wire);
            event->accept();
            return;
        }
    }
    QGraphicsView::mousePressEvent(event);
}

void NodeGraphView::mouseMoveEvent(QMouseEvent* event) {
    if (m_dragging && m_drag_wire) {
        QPointF from = m_drag_from.node->scene_pin_pos(m_drag_from.pin_index, false);
        QPointF to = mapToScene(event->pos());
        qreal dx = (to.x() - from.x()) * 0.5;
        QPainterPath path;
        path.moveTo(from);
        path.cubicTo(from + QPointF(dx, 0), to - QPointF(dx, 0), to);
        m_drag_wire->setPath(path);
        event->accept();
        return;
    }
    QGraphicsView::mouseMoveEvent(event);
}

void NodeGraphView::mouseReleaseEvent(QMouseEvent* event) {
    if (m_dragging) {
        m_dragging = false;
        if (m_drag_wire) {
            scene()->removeItem(m_drag_wire);
            delete m_drag_wire;
            m_drag_wire = nullptr;
        }
        if (event->button() == Qt::LeftButton) {
            PinRef hit = pin_at(mapToScene(event->pos()));
            if (hit.valid() && hit.is_input && hit.node != m_drag_from.node) {
                emit pinsConnected(
                    m_drag_from.node->backend_id(), m_drag_from.pin_index,
                    hit.node->backend_id(), hit.pin_index);
            }
        }
        event->accept();
        return;
    }
    QGraphicsView::mouseReleaseEvent(event);
}
```

- [ ] **Step 5: Wire NodeGraphWidget to handle pinsConnected**

In `NodeGraphWidget` constructor (after creating `m_view`), add:

```cpp
m_view->set_nodes(&m_nodes);
connect(m_view, &NodeGraphView::pinsConnected,
    this, [this](int from_id, int from_pin, int to_id, int to_pin) {
        bool ok = m_state->on_node_graph_connect_pins(
            from_id, from_pin, to_id, to_pin);
        if (ok) {
            BifNodeGraphicsItem* from_node = node_by_backend_id(from_id);
            BifNodeGraphicsItem* to_node   = node_by_backend_id(to_id);
            if (from_node && to_node)
                connect_pins(from_node, from_pin, to_node, to_pin);
        }
    });
```

Add the `node_by_backend_id` helper to `NodeGraphWidget` private section in the header:
```cpp
BifNodeGraphicsItem* node_by_backend_id(int id) const;
```

And implement in .cpp:
```cpp
BifNodeGraphicsItem* NodeGraphWidget::node_by_backend_id(int id) const {
    for (auto* n : m_nodes)
        if (n->backend_id() == id) return n;
    return nullptr;
}
```

- [ ] **Step 6: Build check**

```
cargo build -p bif_qt 2>&1 | head -60
```

- [ ] **Step 7: Smoke test**

Launch app. Add UsdRead + Xform nodes. Drag from the UsdRead output pin (right side) to the Xform input pin (left side). Confirm a bezier wire appears and the connection is created (check Rust logs or node state).

- [ ] **Step 8: Commit**

```
git add crates/bif_qt/cpp/node_graph_widget.h crates/bif_qt/cpp/node_graph_widget.cpp
git commit -m "feat(node-graph): wire drag-connect between compatible pins"
```

---

## Resolved Pre-Flight Questions

All 5 questions answered via codebase search before execution:

1. **GraphNodeId pattern** — `bif_viewport::GraphNodeId(node_id as u64)` for i32→GraphNodeId; `let snarl_id: egui_snarl::NodeId = node_id.into()` for snarl access.
2. **dispatch method** — `self.handle_node_graph_event(NodeGraphEvent::...)` ✓
3. **XformChanged** — already exists in mod.rs line 155 ✓
4. **on_start_ivar_render** — takes no args; SPP sync to renderer is a follow-up ✓
5. **C++ accessor** — `m_state` IS the BifShellState cxx-qt object; call `m_state->on_node_graph_*()` directly ✓
