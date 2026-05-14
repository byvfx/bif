#include "collection_editor_widget.h"

#include <QComboBox>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QInputDialog>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QPushButton>
#include <QVBoxLayout>

#include "bif_qt/src/main_window.cxxqt.h"

namespace {

// Allowed UsdCollectionAPI expansion-rule tokens, in display order.
const char* const kExpansionRules[] = {
    "expandPrims",
    "expandPrimsAndProperties",
    "explicitOnly",
};
constexpr int kNumExpansionRules = 3;

QListWidget* make_list() {
    auto* list = new QListWidget();
    list->setSelectionMode(QAbstractItemView::SingleSelection);
    list->setUniformItemSizes(true);
    return list;
}

} // namespace

CollectionEditorWidget::CollectionEditorWidget(BifShellState* state, QWidget* parent)
    : QWidget(parent), m_state(state) {
    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(8, 8, 8, 8);
    root->setSpacing(6);

    // ── prim path + follow toggle ─────────────────────────────────
    m_prim_path_edit = new QLineEdit();
    m_prim_path_edit->setPlaceholderText("/World/Hero");
    m_prim_path_edit->setReadOnly(true);  // v0.16.5: follow selection only

    auto* path_label = new QLabel("PRIM PATH");
    path_label->setObjectName(QStringLiteral("sectionHeader"));
    auto* path_form = new QFormLayout();
    path_form->addRow(path_label, m_prim_path_edit);
    root->addLayout(path_form);

    // ── collection picker + new ───────────────────────────────────
    auto* picker_row = new QHBoxLayout();
    m_collection_picker = new QComboBox();
    m_new_collection_btn = new QPushButton("New…");
    auto* coll_label = new QLabel("COLLECTION");
    coll_label->setObjectName(QStringLiteral("sectionHeader"));
    picker_row->addWidget(coll_label);
    picker_row->addWidget(m_collection_picker, /*stretch=*/1);
    picker_row->addWidget(m_new_collection_btn);
    root->addLayout(picker_row);

    // ── expansion rule ────────────────────────────────────────────
    auto* exp_row = new QHBoxLayout();
    m_expansion_combo = new QComboBox();
    for (int i = 0; i < kNumExpansionRules; ++i) {
        m_expansion_combo->addItem(QString::fromLatin1(kExpansionRules[i]));
    }
    auto* exp_label = new QLabel("EXPANSION");
    exp_label->setObjectName(QStringLiteral("sectionHeader"));
    exp_row->addWidget(exp_label);
    exp_row->addWidget(m_expansion_combo, /*stretch=*/1);
    root->addLayout(exp_row);

    // ── includes ──────────────────────────────────────────────────
    auto* includes_group = new QGroupBox("INCLUDES");
    auto* includes_layout = new QVBoxLayout(includes_group);
    m_includes_list = make_list();
    auto* inc_btns = new QHBoxLayout();
    m_add_include_btn = new QPushButton("Add…");
    m_remove_include_btn = new QPushButton("Remove");
    inc_btns->addWidget(m_add_include_btn);
    inc_btns->addWidget(m_remove_include_btn);
    inc_btns->addStretch();
    includes_layout->addWidget(m_includes_list);
    includes_layout->addLayout(inc_btns);
    root->addWidget(includes_group, /*stretch=*/1);

    // ── excludes ──────────────────────────────────────────────────
    auto* excludes_group = new QGroupBox("EXCLUDES");
    auto* excludes_layout = new QVBoxLayout(excludes_group);
    m_excludes_list = make_list();
    auto* exc_btns = new QHBoxLayout();
    m_add_exclude_btn = new QPushButton("Add…");
    m_remove_exclude_btn = new QPushButton("Remove");
    exc_btns->addWidget(m_add_exclude_btn);
    exc_btns->addWidget(m_remove_exclude_btn);
    exc_btns->addStretch();
    excludes_layout->addWidget(m_excludes_list);
    excludes_layout->addLayout(exc_btns);
    root->addWidget(excludes_group, /*stretch=*/1);

    // ── resolved members (read-only) ──────────────────────────────
    auto* members_group = new QGroupBox("RESOLVED MEMBERS");
    auto* members_layout = new QVBoxLayout(members_group);
    m_members_list = make_list();
    m_members_list->setSelectionMode(QAbstractItemView::NoSelection);
    members_layout->addWidget(m_members_list);
    root->addWidget(members_group, /*stretch=*/2);

    // ── signal wiring ─────────────────────────────────────────────
    if (m_state) {
        connect(m_state, &BifShellState::collection_revisionChanged,
                this, &CollectionEditorWidget::on_collection_revision_changed);
        connect(m_state, &BifShellState::selected_prim_pathChanged,
                this, &CollectionEditorWidget::on_selected_prim_changed);
    }
    connect(m_collection_picker, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &CollectionEditorWidget::on_collection_picked);
    connect(m_expansion_combo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &CollectionEditorWidget::on_expansion_rule_changed);
    connect(m_add_include_btn, &QPushButton::clicked,
            this, &CollectionEditorWidget::on_add_include);
    connect(m_remove_include_btn, &QPushButton::clicked,
            this, &CollectionEditorWidget::on_remove_include);
    connect(m_add_exclude_btn, &QPushButton::clicked,
            this, &CollectionEditorWidget::on_add_exclude);
    connect(m_remove_exclude_btn, &QPushButton::clicked,
            this, &CollectionEditorWidget::on_remove_exclude);
    connect(m_new_collection_btn, &QPushButton::clicked,
            this, &CollectionEditorWidget::on_new_collection);

    on_selected_prim_changed();
}

CollectionEditorWidget::~CollectionEditorWidget() = default;

QString CollectionEditorWidget::current_prim_path() const {
    return m_prim_path_edit->text();
}

QString CollectionEditorWidget::current_collection_name() const {
    return m_collection_picker->currentText();
}

void CollectionEditorWidget::on_selected_prim_changed() {
    if (!m_state) return;
    const QString sel = m_state->getSelected_prim_path();
    m_prim_path_edit->setText(sel);
    refresh_collection_list();
}

void CollectionEditorWidget::on_collection_revision_changed() {
    refresh_collection_list();
}

void CollectionEditorWidget::refresh_collection_list() {
    if (!m_state) return;
    m_suppress_signals = true;
    const QString prev = m_collection_picker->currentText();
    m_collection_picker->clear();
    const QString path = current_prim_path();
    if (!path.isEmpty()) {
        const int n = m_state->collection_count(path);
        for (int i = 0; i < n; ++i) {
            m_collection_picker->addItem(m_state->collection_name_at(path, i));
        }
        // Restore previous selection if it still exists.
        const int idx = m_collection_picker->findText(prev);
        if (idx >= 0) m_collection_picker->setCurrentIndex(idx);
    }
    m_suppress_signals = false;
    refresh_details();
}

void CollectionEditorWidget::refresh_details() {
    if (!m_state) return;
    m_suppress_signals = true;
    m_includes_list->clear();
    m_excludes_list->clear();
    m_members_list->clear();

    const QString path = current_prim_path();
    const QString coll = current_collection_name();
    const bool have_coll = !path.isEmpty() && !coll.isEmpty();

    if (have_coll) {
        const int inc_n = m_state->collection_includes_count(path, coll);
        for (int i = 0; i < inc_n; ++i) {
            m_includes_list->addItem(m_state->collection_include_at(path, coll, i));
        }
        const int exc_n = m_state->collection_excludes_count(path, coll);
        for (int i = 0; i < exc_n; ++i) {
            m_excludes_list->addItem(m_state->collection_exclude_at(path, coll, i));
        }
        const int mem_n = m_state->collection_members_count(path, coll);
        for (int i = 0; i < mem_n; ++i) {
            m_members_list->addItem(m_state->collection_member_at(path, coll, i));
        }
        const QString rule = m_state->collection_expansion_rule(path, coll);
        const int rule_idx = m_expansion_combo->findText(rule);
        if (rule_idx >= 0) m_expansion_combo->setCurrentIndex(rule_idx);
    }

    const bool enable = have_coll;
    m_expansion_combo->setEnabled(enable);
    m_add_include_btn->setEnabled(enable);
    m_remove_include_btn->setEnabled(enable);
    m_add_exclude_btn->setEnabled(enable);
    m_remove_exclude_btn->setEnabled(enable);

    m_suppress_signals = false;
}

void CollectionEditorWidget::on_collection_picked(int /*index*/) {
    if (m_suppress_signals) return;
    refresh_details();
}

void CollectionEditorWidget::on_expansion_rule_changed(int index) {
    if (m_suppress_signals || !m_state) return;
    if (index < 0 || index >= kNumExpansionRules) return;
    const QString path = current_prim_path();
    const QString coll = current_collection_name();
    if (path.isEmpty() || coll.isEmpty()) return;
    m_state->collection_set_expansion_rule(
        path, coll, QString::fromLatin1(kExpansionRules[index]));
}

void CollectionEditorWidget::on_add_include() {
    if (!m_state) return;
    const QString path = current_prim_path();
    const QString coll = current_collection_name();
    if (path.isEmpty() || coll.isEmpty()) return;
    bool ok = false;
    const QString target = QInputDialog::getText(
        this, "Add Include Target",
        "Prim path to include:", QLineEdit::Normal, "/", &ok);
    if (!ok || target.isEmpty()) return;
    m_state->collection_add_target(path, coll, target, /*is_include=*/true);
}

void CollectionEditorWidget::on_remove_include() {
    if (!m_state) return;
    auto* item = m_includes_list->currentItem();
    if (!item) return;
    const QString path = current_prim_path();
    const QString coll = current_collection_name();
    if (path.isEmpty() || coll.isEmpty()) return;
    m_state->collection_remove_target(path, coll, item->text(), /*is_include=*/true);
}

void CollectionEditorWidget::on_add_exclude() {
    if (!m_state) return;
    const QString path = current_prim_path();
    const QString coll = current_collection_name();
    if (path.isEmpty() || coll.isEmpty()) return;
    bool ok = false;
    const QString target = QInputDialog::getText(
        this, "Add Exclude Target",
        "Prim path to exclude:", QLineEdit::Normal, "/", &ok);
    if (!ok || target.isEmpty()) return;
    m_state->collection_add_target(path, coll, target, /*is_include=*/false);
}

void CollectionEditorWidget::on_remove_exclude() {
    if (!m_state) return;
    auto* item = m_excludes_list->currentItem();
    if (!item) return;
    const QString path = current_prim_path();
    const QString coll = current_collection_name();
    if (path.isEmpty() || coll.isEmpty()) return;
    m_state->collection_remove_target(path, coll, item->text(), /*is_include=*/false);
}

void CollectionEditorWidget::on_new_collection() {
    if (!m_state) return;
    const QString path = current_prim_path();
    if (path.isEmpty()) return;
    bool ok = false;
    const QString name = QInputDialog::getText(
        this, "New Collection",
        "Collection name:", QLineEdit::Normal, "", &ok);
    if (!ok || name.isEmpty()) return;
    if (m_state->collection_apply(path, name)) {
        // Pick the newly-created collection after the model refreshes.
        // refresh_collection_list runs in response to revision bump; we
        // re-find the new name once the picker has been repopulated.
        const int idx = m_collection_picker->findText(name);
        if (idx >= 0) m_collection_picker->setCurrentIndex(idx);
    }
}
