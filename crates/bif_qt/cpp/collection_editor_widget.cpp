#include "collection_editor_widget.h"

#include <QComboBox>
#include <QEvent>
#include <QFormLayout>
#include <QFrame>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QListWidgetItem>
#include <QMouseEvent>
#include <QToolButton>
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

constexpr const char* kChevronExpanded = u8"▾";   // ▾
constexpr const char* kChevronCollapsed = u8"▸";  // ▸

QListWidget* make_role_list(const char* object_name) {
    auto* list = new QListWidget();
    list->setObjectName(QString::fromLatin1(object_name));
    list->setSelectionMode(QAbstractItemView::SingleSelection);
    list->setUniformItemSizes(true);
    list->setFrameShape(QFrame::NoFrame);
    list->setSizeAdjustPolicy(QAbstractScrollArea::AdjustToContents);
    return list;
}

QLabel* make_empty_hint(const QString& text) {
    auto* label = new QLabel(text);
    label->setObjectName(QStringLiteral("sectionEmptyHint"));
    label->setAlignment(Qt::AlignLeft | Qt::AlignVCenter);
    label->hide();
    return label;
}

}  // namespace

// ---------------------------------------------------------------------------
// SectionBlock — clickable header + body. Click anywhere on the header row
// (except the trailing icon button) toggles the body. Icon button propagates
// its own click for "add" actions.
// ---------------------------------------------------------------------------

SectionBlock::SectionBlock(const QString& title, QWidget* parent)
    : QWidget(parent) {
    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(2);

    m_header = new QFrame(this);
    m_header->setObjectName(QStringLiteral("sectionHeaderRow"));
    m_header->setCursor(Qt::PointingHandCursor);
    auto* hl = new QHBoxLayout(m_header);
    hl->setContentsMargins(4, 2, 4, 2);
    hl->setSpacing(4);

    m_chevron = new QLabel(QString::fromUtf8(kChevronExpanded));
    m_chevron->setObjectName(QStringLiteral("sectionChevron"));
    m_chevron->setFixedWidth(14);

    m_title = new QLabel(title.toUpper());
    m_title->setObjectName(QStringLiteral("sectionHeader"));

    m_icon_btn = new QToolButton();
    m_icon_btn->setObjectName(QStringLiteral("sectionIcon"));
    m_icon_btn->setText(QStringLiteral("+"));
    m_icon_btn->setCursor(Qt::PointingHandCursor);
    m_icon_btn->setToolTip(QStringLiteral("Add"));
    m_icon_btn->setAutoRaise(true);
    m_icon_btn->setFocusPolicy(Qt::NoFocus);

    hl->addWidget(m_chevron);
    hl->addWidget(m_title);
    hl->addStretch();
    hl->addWidget(m_icon_btn);

    // Click anywhere on the header (except the icon button) toggles.
    m_header->installEventFilter(this);

    m_body = new QWidget(this);
    auto* bl = new QVBoxLayout(m_body);
    bl->setContentsMargins(8, 4, 4, 4);
    bl->setSpacing(2);

    root->addWidget(m_header);
    root->addWidget(m_body);
}

void SectionBlock::set_expanded(bool expanded) {
    if (m_expanded == expanded) return;
    m_expanded = expanded;
    m_body->setVisible(expanded);
    m_chevron->setText(QString::fromUtf8(expanded ? kChevronExpanded : kChevronCollapsed));
}

void SectionBlock::toggle() {
    set_expanded(!m_expanded);
}

bool SectionBlock::eventFilter(QObject* obj, QEvent* ev) {
    if (obj == m_header && ev->type() == QEvent::MouseButtonRelease) {
        auto* me = static_cast<QMouseEvent*>(ev);
        if (me->button() == Qt::LeftButton) {
            toggle();
            return true;
        }
    }
    return QWidget::eventFilter(obj, ev);
}

// ---------------------------------------------------------------------------
// CollectionEditorWidget
// ---------------------------------------------------------------------------

CollectionEditorWidget::CollectionEditorWidget(BifShellState* state, QWidget* parent)
    : QWidget(parent), m_state(state) {
    setObjectName(QStringLiteral("collection_editor_widget"));
    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(8, 8, 8, 8);
    root->setSpacing(8);

    // ── prim path (read-only, follows scene selection) ────────────
    auto* path_label = new QLabel(QStringLiteral("PRIM PATH"));
    path_label->setObjectName(QStringLiteral("sectionHeader"));
    m_prim_path_edit = new QLineEdit();
    m_prim_path_edit->setPlaceholderText(QStringLiteral("(no prim selected)"));
    m_prim_path_edit->setReadOnly(true);
    root->addWidget(path_label);
    root->addWidget(m_prim_path_edit);

    // ── collection picker + new ──────────────────────────────────
    auto* coll_label = new QLabel(QStringLiteral("COLLECTION"));
    coll_label->setObjectName(QStringLiteral("sectionHeader"));
    auto* picker_row = new QHBoxLayout();
    picker_row->setSpacing(4);
    m_collection_picker = new QComboBox();
    m_new_collection_btn = new QToolButton();
    m_new_collection_btn->setObjectName(QStringLiteral("sectionIcon"));
    m_new_collection_btn->setText(QStringLiteral("+"));
    m_new_collection_btn->setToolTip(QStringLiteral("Apply new CollectionAPI"));
    m_new_collection_btn->setCursor(Qt::PointingHandCursor);
    m_new_collection_btn->setAutoRaise(true);
    picker_row->addWidget(m_collection_picker, /*stretch=*/1);
    picker_row->addWidget(m_new_collection_btn);
    root->addWidget(coll_label);
    root->addLayout(picker_row);

    // ── expansion rule ────────────────────────────────────────────
    auto* exp_label = new QLabel(QStringLiteral("EXPANSION"));
    exp_label->setObjectName(QStringLiteral("sectionHeader"));
    m_expansion_combo = new QComboBox();
    for (int i = 0; i < kNumExpansionRules; ++i) {
        m_expansion_combo->addItem(QString::fromLatin1(kExpansionRules[i]));
    }
    root->addWidget(exp_label);
    root->addWidget(m_expansion_combo);

    // ── includes section ──────────────────────────────────────────
    m_includes_section = new SectionBlock(QStringLiteral("Includes"), this);
    m_includes_section->setObjectName(QStringLiteral("sectionBlock"));
    m_includes_list = make_role_list("includesList");
    m_includes_empty = make_empty_hint(QStringLiteral("Click + to include a prim"));
    auto* inc_layout = qobject_cast<QVBoxLayout*>(m_includes_section->body()->layout());
    inc_layout->addWidget(m_includes_empty);
    inc_layout->addWidget(m_includes_list);
    root->addWidget(m_includes_section);

    // ── excludes section ──────────────────────────────────────────
    m_excludes_section = new SectionBlock(QStringLiteral("Excludes"), this);
    m_excludes_section->setObjectName(QStringLiteral("sectionBlock"));
    m_excludes_list = make_role_list("excludesList");
    m_excludes_empty = make_empty_hint(QStringLiteral("Click + to exclude a prim"));
    auto* exc_layout = qobject_cast<QVBoxLayout*>(m_excludes_section->body()->layout());
    exc_layout->addWidget(m_excludes_empty);
    exc_layout->addWidget(m_excludes_list);
    root->addWidget(m_excludes_section);

    // ── resolved members (read-only) ─────────────────────────────
    m_members_section = new SectionBlock(QStringLiteral("Resolved Members"), this);
    m_members_section->setObjectName(QStringLiteral("sectionBlock"));
    m_members_section->icon_button()->hide();  // read-only section
    m_members_list = make_role_list("membersList");
    m_members_list->setSelectionMode(QAbstractItemView::NoSelection);
    m_members_empty = make_empty_hint(QStringLiteral("(no resolved members)"));
    auto* mem_layout = qobject_cast<QVBoxLayout*>(m_members_section->body()->layout());
    mem_layout->addWidget(m_members_empty);
    mem_layout->addWidget(m_members_list);
    root->addWidget(m_members_section, /*stretch=*/1);

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
    connect(m_new_collection_btn, &QToolButton::clicked,
            this, &CollectionEditorWidget::on_new_collection);
    connect(m_includes_section->icon_button(), &QToolButton::clicked,
            this, &CollectionEditorWidget::on_add_include);
    connect(m_excludes_section->icon_button(), &QToolButton::clicked,
            this, &CollectionEditorWidget::on_add_exclude);

    // Backspace/Delete on a list item removes it.
    m_includes_list->installEventFilter(this);
    m_excludes_list->installEventFilter(this);

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
    cancel_inline_add();
    refresh_collection_list();
}

void CollectionEditorWidget::on_collection_revision_changed() {
    cancel_inline_add();
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

    // Empty-state hints
    m_includes_empty->setVisible(have_coll && m_includes_list->count() == 0);
    m_excludes_empty->setVisible(have_coll && m_excludes_list->count() == 0);
    m_members_empty->setVisible(have_coll && m_members_list->count() == 0);

    const bool enable = have_coll;
    m_expansion_combo->setEnabled(enable);
    m_includes_section->icon_button()->setEnabled(enable);
    m_excludes_section->icon_button()->setEnabled(enable);

    m_suppress_signals = false;
}

void CollectionEditorWidget::on_collection_picked(int /*index*/) {
    if (m_suppress_signals) return;
    cancel_inline_add();
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

// ── inline add row ─────────────────────────────────────────────────────────
//
// Clicking the section's `+` (or the picker's `+` for New Collection) spawns
// a transient QListWidgetItem with an embedded QLineEdit. Return commits, Esc
// or focus-out with empty text cancels, focus-out with text commits.

void CollectionEditorWidget::begin_inline_add(PendingTarget which) {
    cancel_inline_add();  // only one pending edit at a time
    m_pending_target = which;

    QString placeholder;
    QListWidget* list = nullptr;
    switch (which) {
        case PendingTarget::Includes:
            list = m_includes_list;
            placeholder = QStringLiteral("/World/Hero/Body");
            break;
        case PendingTarget::Excludes:
            list = m_excludes_list;
            placeholder = QStringLiteral("/World/Hero/Eyes");
            break;
        case PendingTarget::NewCollection:
            placeholder = QStringLiteral("collection_name");
            break;
        default:
            return;
    }

    m_pending_edit = new QLineEdit();
    m_pending_edit->setPlaceholderText(placeholder);
    m_pending_edit->setFrame(false);
    m_pending_edit->installEventFilter(this);

    if (list) {
        // List-bound: embed as a transient row at the bottom.
        m_pending_list = list;
        if (which == PendingTarget::Includes && m_includes_empty->isVisible()) {
            m_includes_empty->hide();
        }
        if (which == PendingTarget::Excludes && m_excludes_empty->isVisible()) {
            m_excludes_empty->hide();
        }
        auto* item = new QListWidgetItem(list);
        item->setSizeHint(QSize(0, 28));
        list->setItemWidget(item, m_pending_edit);
        m_pending_edit->setFocus(Qt::ShortcutFocusReason);
    } else {
        // New-collection: float the edit beneath the picker row by replacing
        // the picker's text temporarily.
        m_collection_picker->setEditable(true);
        m_collection_picker->setEditText(QString());
        m_collection_picker->lineEdit()->setPlaceholderText(placeholder);
        m_collection_picker->lineEdit()->installEventFilter(this);
        m_collection_picker->lineEdit()->setFocus(Qt::ShortcutFocusReason);
        // For new-collection mode we reroute commit through the combo's lineEdit.
        delete m_pending_edit;
        m_pending_edit = m_collection_picker->lineEdit();
    }
}

void CollectionEditorWidget::commit_inline_add() {
    if (m_pending_target == PendingTarget::None || !m_pending_edit || !m_state) {
        cancel_inline_add();
        return;
    }
    const QString text = m_pending_edit->text().trimmed();
    const QString path = current_prim_path();

    if (text.isEmpty() || path.isEmpty()) {
        cancel_inline_add();
        return;
    }

    const PendingTarget which = m_pending_target;
    const QString coll = current_collection_name();

    // Tear down the inline row BEFORE the revision bump fires refresh_details
    // (which clears the list and would dangle the embedded QLineEdit).
    cancel_inline_add();

    switch (which) {
        case PendingTarget::Includes:
            if (!coll.isEmpty()) {
                m_state->collection_add_target(path, coll, text, /*is_include=*/true);
            }
            break;
        case PendingTarget::Excludes:
            if (!coll.isEmpty()) {
                m_state->collection_add_target(path, coll, text, /*is_include=*/false);
            }
            break;
        case PendingTarget::NewCollection:
            if (m_state->collection_apply(path, text)) {
                // Picker repopulates via revision bump; re-select on next tick.
                const int idx = m_collection_picker->findText(text);
                if (idx >= 0) m_collection_picker->setCurrentIndex(idx);
            }
            break;
        default:
            break;
    }
}

void CollectionEditorWidget::cancel_inline_add() {
    if (m_pending_target == PendingTarget::NewCollection) {
        if (m_pending_edit) m_pending_edit->removeEventFilter(this);
        m_collection_picker->setEditable(false);
        m_pending_edit = nullptr;
    } else if (m_pending_list && m_pending_edit) {
        m_pending_edit->removeEventFilter(this);
        // The QLineEdit is owned by the list-item widget map; clearing the
        // setItemWidget association deletes it. We delete the trailing item
        // (which is always the inline-add row when active).
        const int last = m_pending_list->count() - 1;
        if (last >= 0) {
            delete m_pending_list->takeItem(last);
        }
        m_pending_edit = nullptr;
        m_pending_list = nullptr;
    }
    m_pending_target = PendingTarget::None;
}

void CollectionEditorWidget::on_add_include() {
    if (current_prim_path().isEmpty() || current_collection_name().isEmpty()) return;
    if (!m_includes_section->body()->isVisible()) {
        m_includes_section->set_expanded(true);
    }
    begin_inline_add(PendingTarget::Includes);
}

void CollectionEditorWidget::on_add_exclude() {
    if (current_prim_path().isEmpty() || current_collection_name().isEmpty()) return;
    if (!m_excludes_section->body()->isVisible()) {
        m_excludes_section->set_expanded(true);
    }
    begin_inline_add(PendingTarget::Excludes);
}

void CollectionEditorWidget::on_new_collection() {
    if (current_prim_path().isEmpty()) return;
    begin_inline_add(PendingTarget::NewCollection);
}

void CollectionEditorWidget::remove_selected_from(QListWidget* list, bool is_include) {
    if (!m_state || !list) return;
    auto* item = list->currentItem();
    if (!item) return;
    const QString path = current_prim_path();
    const QString coll = current_collection_name();
    if (path.isEmpty() || coll.isEmpty()) return;
    m_state->collection_remove_target(path, coll, item->text(), is_include);
}

bool CollectionEditorWidget::eventFilter(QObject* obj, QEvent* ev) {
    // Inline-add commit/cancel routing for both the QLineEdit and the picker's
    // editable lineEdit.
    if (obj == m_pending_edit && m_pending_target != PendingTarget::None) {
        if (ev->type() == QEvent::KeyPress) {
            auto* key = static_cast<QKeyEvent*>(ev);
            if (key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter) {
                commit_inline_add();
                return true;
            }
            if (key->key() == Qt::Key_Escape) {
                cancel_inline_add();
                return true;
            }
        } else if (ev->type() == QEvent::FocusOut) {
            // Focus loss → commit if non-empty, else cancel.
            const QString text = m_pending_edit ? m_pending_edit->text().trimmed() : QString();
            if (text.isEmpty()) {
                cancel_inline_add();
            } else {
                commit_inline_add();
            }
            return false;
        }
    }

    // Delete / Backspace on includes/excludes lists removes the selection.
    if (ev->type() == QEvent::KeyPress) {
        auto* key = static_cast<QKeyEvent*>(ev);
        if (key->key() == Qt::Key_Delete || key->key() == Qt::Key_Backspace) {
            if (obj == m_includes_list) {
                remove_selected_from(m_includes_list, /*is_include=*/true);
                return true;
            }
            if (obj == m_excludes_list) {
                remove_selected_from(m_excludes_list, /*is_include=*/false);
                return true;
            }
        }
    }

    return QWidget::eventFilter(obj, ev);
}
