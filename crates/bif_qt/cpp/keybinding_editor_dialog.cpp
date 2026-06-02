#include "keybinding_editor_dialog.h"

#include "shortcut_registry.h"

#include <QDialogButtonBox>
#include <QHeaderView>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QMap>
#include <QMessageBox>
#include <QPushButton>
#include <QTreeWidget>
#include <QTreeWidgetItem>
#include <QVBoxLayout>

namespace bif_qt::shortcuts {

namespace {

// Split `category.action` → ("Category", "Action"). Falls back to
// ("General", id) if no dot is present. Title-cases the category so
// "panels" → "Panels" / "timeline" → "Timeline".
QPair<QString, QString> split_id(const QString& id) {
    const int dot = id.indexOf(QLatin1Char('.'));
    if (dot < 0) {
        return {QStringLiteral("General"), id};
    }
    QString category = id.left(dot);
    if (!category.isEmpty()) {
        category[0] = category[0].toUpper();
    }
    return {category, id.mid(dot + 1)};
}

// Pretty-print the action portion: "scene_browser" → "Scene Browser".
QString pretty_action(const QString& tail) {
    QString out;
    out.reserve(tail.size());
    bool capitalize_next = true;
    for (QChar c : tail) {
        if (c == QLatin1Char('_')) {
            out.append(QLatin1Char(' '));
            capitalize_next = true;
            continue;
        }
        out.append(capitalize_next ? c.toUpper() : c);
        capitalize_next = false;
    }
    return out;
}

}  // namespace

KeybindingEditorDialog::KeybindingEditorDialog(QWidget* parent)
    : QDialog(parent) {
    setWindowTitle(QStringLiteral("Preferences — Keyboard Shortcuts"));
    setModal(true);
    resize(560, 520);

    auto* layout = new QVBoxLayout(this);

    auto* hint = new QLabel(
        QStringLiteral("Click a shortcut to record a new key sequence. "
                       "Some shortcuts only take effect after restart."),
        this);
    hint->setWordWrap(true);
    hint->setObjectName(QStringLiteral("keybindingHint"));
    layout->addWidget(hint);

    m_tree = new QTreeWidget(this);
    m_tree->setColumnCount(2);
    m_tree->setHeaderLabels({QStringLiteral("Action"), QStringLiteral("Shortcut")});
    m_tree->header()->setStretchLastSection(false);
    m_tree->header()->setSectionResizeMode(0, QHeaderView::Stretch);
    m_tree->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
    m_tree->setRootIsDecorated(true);
    m_tree->setUniformRowHeights(false);
    layout->addWidget(m_tree, 1);

    auto* buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel
            | QDialogButtonBox::RestoreDefaults,
        this);
    layout->addWidget(buttons);

    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    if (auto* restore = buttons->button(QDialogButtonBox::RestoreDefaults)) {
        connect(restore, &QPushButton::clicked, this,
                &KeybindingEditorDialog::on_restore_defaults);
    }

    populate();
}

KeybindingEditorDialog::~KeybindingEditorDialog() = default;

void KeybindingEditorDialog::populate() {
    m_tree->clear();
    m_rows.clear();

    // Snapshot the registry. Group by category so a sorted, predictable
    // layout regardless of insertion order from various widget ctors.
    const auto defaults = registered_defaults();
    QMap<QString, QMap<QString, QString>> grouped;  // category -> action -> action_id
    for (auto it = defaults.constBegin(); it != defaults.constEnd(); ++it) {
        const auto [category, tail] = split_id(it.key());
        grouped[category][pretty_action(tail)] = it.key();
    }

    for (auto cat_it = grouped.constBegin(); cat_it != grouped.constEnd(); ++cat_it) {
        auto* header = new QTreeWidgetItem(m_tree);
        header->setText(0, cat_it.key());
        header->setFirstColumnSpanned(true);
        QFont bold = header->font(0);
        bold.setBold(true);
        header->setFont(0, bold);
        header->setExpanded(true);

        const auto& actions = cat_it.value();
        for (auto a_it = actions.constBegin(); a_it != actions.constEnd(); ++a_it) {
            const QString& action_id = a_it.value();
            const QKeySequence default_seq = defaults.value(action_id);

            // The current effective sequence reflects any saved override.
            const QKeySequence current = lookup(action_id.toLatin1().constData(), default_seq);

            auto* item = new QTreeWidgetItem(header);
            item->setText(0, a_it.key());
            item->setToolTip(0, action_id);

            auto* edit = new QKeySequenceEdit(current, m_tree);
            edit->setClearButtonEnabled(true);
            m_tree->setItemWidget(item, 1, edit);

            Row row;
            row.action_id = action_id;
            row.default_seq = default_seq;
            row.edit = edit;
            row.item = item;
            m_rows.insert(action_id, row);
        }
    }
}

bool KeybindingEditorDialog::detect_conflicts(QString* conflict_msg) const {
    QHash<QString, QString> seen;  // sequence string -> action_id that holds it
    for (auto it = m_rows.constBegin(); it != m_rows.constEnd(); ++it) {
        const QKeySequence seq = it.value().edit->keySequence();
        if (seq.isEmpty()) {
            continue;
        }
        const QString key = seq.toString(QKeySequence::PortableText);
        if (seen.contains(key)) {
            if (conflict_msg) {
                *conflict_msg = QStringLiteral(
                    "\"%1\" is already bound to:\n  • %2\n  • %3\n\n"
                    "Pick a different sequence or clear one of them.")
                    .arg(seq.toString(QKeySequence::NativeText),
                         seen.value(key),
                         it.key());
            }
            return false;
        }
        seen.insert(key, it.key());
    }
    return true;
}

void KeybindingEditorDialog::accept() {
    QString conflict_msg;
    if (!detect_conflicts(&conflict_msg)) {
        QMessageBox::warning(this, QStringLiteral("Shortcut Conflict"), conflict_msg);
        return;
    }

    // Persist any row whose current sequence differs from its default
    // as an override; rows that match the default clear their override
    // so future default changes propagate. Empty sequences clear too.
    for (auto it = m_rows.constBegin(); it != m_rows.constEnd(); ++it) {
        const Row& row = it.value();
        const QKeySequence seq = row.edit->keySequence();
        const QByteArray id_bytes = row.action_id.toLatin1();
        if (seq.isEmpty() || seq == row.default_seq) {
            clear_override(id_bytes.constData());
        } else {
            set_override(id_bytes.constData(), seq);
        }
    }

    QDialog::accept();
}

void KeybindingEditorDialog::on_restore_defaults() {
    const auto reply = QMessageBox::question(
        this,
        QStringLiteral("Restore Default Shortcuts"),
        QStringLiteral("Reset every keyboard shortcut to its default? "
                       "This clears all your customizations."),
        QMessageBox::Yes | QMessageBox::No,
        QMessageBox::No);
    if (reply != QMessageBox::Yes) {
        return;
    }
    for (auto it = m_rows.begin(); it != m_rows.end(); ++it) {
        Row& row = it.value();
        row.edit->setKeySequence(row.default_seq);
    }
}

}  // namespace bif_qt::shortcuts
