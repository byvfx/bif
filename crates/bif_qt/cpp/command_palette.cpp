#include "command_palette.h"

#include <QAction>
#include <QKeyEvent>
#include <QLineEdit>
#include <QListView>
#include <QSortFilterProxyModel>
#include <QStringListModel>
#include <QVBoxLayout>

CommandPalette::CommandPalette(QWidget* parent, QHash<QString, QAction*> commands)
    : QDialog(parent),
      m_search(nullptr),
      m_results(nullptr),
      m_source_model(nullptr),
      m_proxy(nullptr),
      m_commands(std::move(commands)) {
    setObjectName(QStringLiteral("command_palette"));
    setWindowFlags(Qt::Dialog | Qt::FramelessWindowHint);
    setAttribute(Qt::WA_TranslucentBackground);
    setModal(true);
    setFixedWidth(480);

    setStyleSheet(QStringLiteral(
        "QDialog#command_palette {"
        "  background-color: rgba(34, 38, 44, 245);"
        "  border: 1px solid rgba(74, 144, 217, 200);"
        "  border-radius: 8px;"
        "}"
        "QLineEdit {"
        "  background-color: rgba(30, 34, 40, 255);"
        "  color: rgba(220, 222, 226, 255);"
        "  border: 1px solid rgba(60, 65, 75, 255);"
        "  border-radius: 4px;"
        "  padding: 8px 12px;"
        "  font-size: 14px;"
        "}"
        "QListView {"
        "  background-color: transparent;"
        "  color: rgba(220, 222, 226, 255);"
        "  border: none;"
        "  outline: none;"
        "  padding: 4px;"
        "}"
        "QListView::item { padding: 6px 10px; border-radius: 4px; }"
        "QListView::item:hover { background-color: rgba(51, 56, 64, 255); }"
        "QListView::item:selected { background-color: rgba(74, 144, 217, 100); }"));

    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(8, 8, 8, 8);
    layout->setSpacing(6);

    m_search = new QLineEdit(this);
    m_search->setPlaceholderText(
        QStringLiteral("Type a command  (Phase C: + prims / layers / nodes / settings)"));
    m_search->setClearButtonEnabled(true);
    layout->addWidget(m_search);

    m_results = new QListView(this);
    m_results->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_results->setSelectionMode(QAbstractItemView::SingleSelection);
    m_results->setUniformItemSizes(true);
    m_results->setMaximumHeight(280);
    layout->addWidget(m_results);

    QStringList names;
    names.reserve(m_commands.size());
    for (auto it = m_commands.constBegin(); it != m_commands.constEnd(); ++it) {
        names.append(it.key());
    }
    names.sort(Qt::CaseInsensitive);

    m_source_model = new QStringListModel(names, this);
    m_proxy = new QSortFilterProxyModel(this);
    m_proxy->setSourceModel(m_source_model);
    m_proxy->setFilterCaseSensitivity(Qt::CaseInsensitive);
    m_proxy->setFilterKeyColumn(0);
    m_results->setModel(m_proxy);

    QObject::connect(m_search, &QLineEdit::textChanged,
        this, &CommandPalette::on_search_changed);
    QObject::connect(m_search, &QLineEdit::returnPressed,
        this, &CommandPalette::activate_current);
    QObject::connect(m_results, &QListView::activated,
        this, &CommandPalette::on_activate_index);
}

CommandPalette::~CommandPalette() = default;

void CommandPalette::on_search_changed(const QString& text) {
    m_proxy->setFilterFixedString(text);
    // Auto-select the first row so Enter is meaningful.
    if (m_proxy->rowCount() > 0) {
        m_results->setCurrentIndex(m_proxy->index(0, 0));
    }
}

void CommandPalette::on_activate_index(const QModelIndex& index) {
    if (!index.isValid()) return;
    const auto name = index.data().toString();
    auto* action = m_commands.value(name, nullptr);
    if (action != nullptr) {
        action->trigger();
    }
    accept();
}

void CommandPalette::activate_current() {
    auto idx = m_results->currentIndex();
    if (!idx.isValid() && m_proxy->rowCount() > 0) {
        idx = m_proxy->index(0, 0);
    }
    if (idx.isValid()) {
        on_activate_index(idx);
    }
}

void CommandPalette::keyPressEvent(QKeyEvent* event) {
    // Up/Down route through the QListView's selection model so the
    // user can navigate without leaving the QLineEdit focus.
    if (event->key() == Qt::Key_Down || event->key() == Qt::Key_Up) {
        const int dir = event->key() == Qt::Key_Down ? 1 : -1;
        const int rows = m_proxy->rowCount();
        if (rows == 0) return;
        auto idx = m_results->currentIndex();
        const int new_row = idx.isValid()
            ? qBound(0, idx.row() + dir, rows - 1)
            : 0;
        m_results->setCurrentIndex(m_proxy->index(new_row, 0));
        event->accept();
        return;
    }
    QDialog::keyPressEvent(event);
}

void CommandPalette::showEvent(QShowEvent* event) {
    QDialog::showEvent(event);
    m_search->setFocus();
    m_search->selectAll();
    if (m_proxy->rowCount() > 0) {
        m_results->setCurrentIndex(m_proxy->index(0, 0));
    }
}
