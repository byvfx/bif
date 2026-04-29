#include "usda_panel_widget.h"

#include <QFontDatabase>
#include <QHBoxLayout>
#include <QLabel>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QVBoxLayout>

#include "bif_qt/src/main_window.cxxqt.h"

UsdaPanelWidget::UsdaPanelWidget(BifShellState* state, QWidget* parent)
    : QWidget(parent),
      m_state(state),
      m_header(nullptr),
      m_editor(nullptr),
      m_apply_button(nullptr),
      m_error_label(nullptr),
      m_dirty(false) {
    setObjectName(QStringLiteral("usda_panel_widget"));

    auto* outer = new QVBoxLayout(this);
    outer->setContentsMargins(8, 8, 8, 8);
    outer->setSpacing(6);

    // Header row: layer label + Apply button.
    auto* header_row = new QHBoxLayout();
    m_header = new QLabel(QStringLiteral("(no edit target)"), this);
    m_header->setStyleSheet(QStringLiteral(
        "color: rgba(140, 145, 155, 255); font-size: 11px;"
        "letter-spacing: 1px; text-transform: uppercase;"));
    header_row->addWidget(m_header, 1);

    m_apply_button = new QPushButton(QStringLiteral("Apply"), this);
    m_apply_button->setStyleSheet(QStringLiteral(
        "QPushButton { padding: 2px 14px; font-size: 11px; }"));
    QObject::connect(m_apply_button, &QPushButton::clicked,
        this, &UsdaPanelWidget::on_apply_clicked);
    header_row->addWidget(m_apply_button, 0);
    outer->addLayout(header_row);

    // QPlainTextEdit — monospace, expandable.
    m_editor = new QPlainTextEdit(this);
    m_editor->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));
    m_editor->setLineWrapMode(QPlainTextEdit::NoWrap);
    m_editor->setStyleSheet(QStringLiteral(
        "QPlainTextEdit {"
        "  background-color: rgba(28, 30, 34, 255);"
        "  color: rgba(220, 222, 226, 255);"
        "  border: 1px solid rgba(60, 65, 75, 255);"
        "  selection-background-color: rgba(74, 144, 217, 100);"
        "}"));
    QObject::connect(m_editor, &QPlainTextEdit::textChanged, this,
        [this]() { m_dirty = true; });
    outer->addWidget(m_editor, 1);

    // Error indicator — empty unless Apply rejects.
    m_error_label = new QLabel(QString(), this);
    m_error_label->setStyleSheet(QStringLiteral(
        "color: rgba(220, 100, 100, 255); font-size: 11px; padding: 2px 4px;"));
    m_error_label->setWordWrap(true);
    m_error_label->hide();
    outer->addWidget(m_error_label, 0);

    if (m_state) {
        QObject::connect(m_state, &BifShellState::layer_state_revisionChanged,
            this, &UsdaPanelWidget::on_layer_state_changed);
    }

    reload_layer_text();
}

UsdaPanelWidget::~UsdaPanelWidget() = default;

void UsdaPanelWidget::on_layer_state_changed() {
    // External undo/redo or other panels may have rewritten the
    // edit-target. Only pull fresh text when the user isn't actively
    // editing in the panel.
    if (m_editor && m_editor->hasFocus()) {
        return;
    }
    reload_layer_text();
}

void UsdaPanelWidget::reload_layer_text() {
    if (!m_state || !m_editor) return;
    const QString text = m_state->active_edit_target_layer_text();
    {
        const QSignalBlocker blocker(m_editor);
        if (m_editor->toPlainText() != text) {
            m_editor->setPlainText(text);
        }
    }
    m_dirty = false;

    // Header reflects the current edit-target identifier (may be a
    // long absolute path; users can hover for the full string).
    if (m_state->active_edit_target_is_set()) {
        const auto identifier = m_state->active_edit_target_identifier();
        const auto name = m_state->active_edit_target_name();
        m_header->setText(QStringLiteral("Edit target: %1").arg(name));
        m_header->setToolTip(identifier);
    } else {
        m_header->setText(QStringLiteral("(no edit target)"));
        m_header->setToolTip(QString());
    }
}

void UsdaPanelWidget::on_apply_clicked() {
    if (!m_state || !m_editor) return;
    const QString text = m_editor->toPlainText();
    const QString error = m_state->on_apply_usda(text);
    if (error.isEmpty()) {
        m_error_label->hide();
        m_error_label->clear();
        m_dirty = false;
    } else {
        m_error_label->setText(error);
        m_error_label->show();
    }
}
