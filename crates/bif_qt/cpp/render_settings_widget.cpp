#include "render_settings_widget.h"

#include <QCheckBox>
#include <QColorDialog>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QGroupBox>
#include <QLabel>
#include <QPushButton>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QVBoxLayout>

#include "bif_qt/src/main_window.cxxqt.h"

namespace {

QString kGroupQss() {
    return QStringLiteral(
        "QGroupBox {"
        "  color: rgba(220, 222, 226, 255);"
        "  border: 1px solid rgba(60, 65, 75, 255);"
        "  border-radius: 4px;"
        "  margin-top: 14px; padding: 10px 8px 8px 8px;"
        "}"
        "QGroupBox::title {"
        "  subcontrol-origin: margin; left: 8px; padding: 0 4px;"
        "  color: rgba(140, 145, 155, 255); font-size: 11px;"
        "  text-transform: uppercase; letter-spacing: 1px;"
        "}"
        "QLabel { color: rgba(180, 185, 195, 255); font-size: 12px; }"
        "QSpinBox, QDoubleSpinBox {"
        "  background-color: rgba(30, 34, 40, 255);"
        "  color: rgba(220, 222, 226, 255);"
        "  border: 1px solid rgba(60, 65, 75, 180);"
        "  border-radius: 3px; padding: 2px 4px; min-width: 64px;"
        "}"
        "QPushButton {"
        "  background-color: rgba(30, 34, 40, 255);"
        "  color: rgba(220, 222, 226, 255);"
        "  border: 1px solid rgba(60, 65, 75, 180);"
        "  border-radius: 3px; padding: 4px 8px;"
        "}"
        "QPushButton:hover { border-color: rgba(90, 100, 120, 255); }"
        "QCheckBox { color: rgba(220, 222, 226, 255); }");
}

QString outline_button_style(const QString& hex) {
    const QColor color(hex);
    const bool light_text = color.isValid() && color.lightness() < 140;
    const QString text = light_text
        ? QStringLiteral("rgba(220, 222, 226, 255)")
        : QStringLiteral("rgba(20, 22, 26, 255)");
    return QStringLiteral(
        "QPushButton {"
        "  background-color: %1;"
        "  color: %2;"
        "  border: 1px solid rgba(60, 65, 75, 220);"
        "  border-radius: 3px; padding: 4px 8px;"
        "}"
        "QPushButton:hover { border-color: rgba(120, 130, 150, 255); }")
        .arg(hex, text);
}

}  // namespace

RenderSettingsWidget::RenderSettingsWidget(BifShellState* state, QWidget* parent)
    : QWidget(parent),
      m_state(state),
      m_spp(nullptr),
      m_max_depth(nullptr),
      m_exposure(nullptr),
      m_gamma(nullptr),
      m_denoise(nullptr),
      m_sharc(nullptr),
      m_outline_width(nullptr),
      m_outline_color(nullptr),
      m_ivar_render(nullptr),
      m_ivar_status(nullptr) {
    setObjectName(QStringLiteral("render_settings_widget"));

    auto* outer = new QVBoxLayout(this);
    outer->setContentsMargins(10, 10, 10, 10);
    outer->setSpacing(8);

    // Path Tracer group
    auto* pt_group = new QGroupBox(QStringLiteral("Path Tracer"), this);
    pt_group->setStyleSheet(kGroupQss());
    auto* pt_form = new QFormLayout(pt_group);
    pt_form->setLabelAlignment(Qt::AlignRight | Qt::AlignVCenter);
    pt_form->setHorizontalSpacing(10);
    pt_form->setVerticalSpacing(6);

    m_spp = new QSpinBox(pt_group);
    m_spp->setRange(1, 16384);
    m_spp->setValue(64);
    m_spp->setSuffix(QStringLiteral(" spp"));
    pt_form->addRow(QStringLiteral("Samples per pixel:"), m_spp);

    m_max_depth = new QSpinBox(pt_group);
    m_max_depth->setRange(1, 32);
    m_max_depth->setValue(8);
    pt_form->addRow(QStringLiteral("Max ray depth:"), m_max_depth);

    m_sharc = new QCheckBox(QStringLiteral("SHARC radiance cache"), pt_group);
    m_sharc->setChecked(true);
    pt_form->addRow(QString(), m_sharc);

    outer->addWidget(pt_group);

    // Selection Outline group
    auto* outline_group = new QGroupBox(QStringLiteral("Selection Outline"), this);
    outline_group->setStyleSheet(kGroupQss());
    auto* outline_form = new QFormLayout(outline_group);
    outline_form->setLabelAlignment(Qt::AlignRight | Qt::AlignVCenter);
    outline_form->setHorizontalSpacing(10);
    outline_form->setVerticalSpacing(6);

    m_outline_width = new QDoubleSpinBox(outline_group);
    m_outline_width->setRange(0.001, 0.05);
    m_outline_width->setDecimals(3);
    m_outline_width->setSingleStep(0.001);
    m_outline_width->setValue(m_state ? m_state->getOutline_width() : 0.004);
    outline_form->addRow(QStringLiteral("Width:"), m_outline_width);

    m_outline_color = new QPushButton(outline_group);
    const auto outline_hex = m_state
        ? m_state->getOutline_color_hex()
        : QStringLiteral("#FFA600");
    update_outline_color_button(outline_hex);
    outline_form->addRow(QStringLiteral("Color:"), m_outline_color);

    outer->addWidget(outline_group);

    // Post-Processing group
    auto* pp_group = new QGroupBox(QStringLiteral("Post-Processing"), this);
    pp_group->setStyleSheet(kGroupQss());
    auto* pp_form = new QFormLayout(pp_group);
    pp_form->setLabelAlignment(Qt::AlignRight | Qt::AlignVCenter);
    pp_form->setHorizontalSpacing(10);
    pp_form->setVerticalSpacing(6);

    m_exposure = new QDoubleSpinBox(pp_group);
    m_exposure->setRange(-10.0, 10.0);
    m_exposure->setSingleStep(0.1);
    m_exposure->setValue(0.0);
    m_exposure->setSuffix(QStringLiteral(" EV"));
    pp_form->addRow(QStringLiteral("Exposure:"), m_exposure);

    m_gamma = new QDoubleSpinBox(pp_group);
    m_gamma->setRange(0.1, 4.0);
    m_gamma->setSingleStep(0.05);
    m_gamma->setValue(2.2);
    pp_form->addRow(QStringLiteral("Gamma:"), m_gamma);

    m_denoise = new QCheckBox(QStringLiteral("OIDN denoising"), pp_group);
    m_denoise->setChecked(true);
    pp_form->addRow(QString(), m_denoise);

    outer->addWidget(pp_group);

    // Ivar Preview group
    auto* ivar_group = new QGroupBox(QStringLiteral("Ivar Preview"), this);
    ivar_group->setStyleSheet(kGroupQss());
    auto* ivar_form = new QFormLayout(ivar_group);
    ivar_form->setLabelAlignment(Qt::AlignRight | Qt::AlignTop);
    ivar_form->setHorizontalSpacing(10);
    ivar_form->setVerticalSpacing(6);

    m_ivar_render = new QPushButton(QStringLiteral("Ivar Render"), ivar_group);
    ivar_form->addRow(QStringLiteral("Render:"), m_ivar_render);

    m_ivar_status = new QLabel(ivar_group);
    m_ivar_status->setWordWrap(true);
    update_ivar_status_label(m_state ? m_state->getIvar_status() : QString());
    ivar_form->addRow(QStringLiteral("Status:"), m_ivar_status);

    outer->addWidget(ivar_group);

    // Footnote
    auto* note = new QLabel(
        QStringLiteral(
            "(Selection outline + Ivar preview are live. Path-tracer and post "
            "controls remain local until Phase E wires bif_renderer::RenderConfig.)"),
        this);
    note->setStyleSheet(QStringLiteral(
        "color: rgba(100, 105, 115, 255); font-size: 10px;"));
    note->setWordWrap(true);
    note->setAlignment(Qt::AlignCenter);
    outer->addWidget(note);

    outer->addStretch();

    if (m_state) {
        QObject::connect(
            m_outline_width,
            QOverload<double>::of(&QDoubleSpinBox::valueChanged),
            this,
            [this](double width) {
                if (m_state) m_state->on_set_outline_width(width);
            });
        QObject::connect(
            m_state,
            &BifShellState::outline_widthChanged,
            this,
            [this]() {
                if (!m_state) return;
                const QSignalBlocker blocker(m_outline_width);
                m_outline_width->setValue(m_state->getOutline_width());
            });
        QObject::connect(
            m_state,
            &BifShellState::outline_color_hexChanged,
            this,
            [this]() {
                if (!m_state) return;
                update_outline_color_button(m_state->getOutline_color_hex());
            });
        QObject::connect(
            m_outline_color,
            &QPushButton::clicked,
            this,
            [this]() {
                if (!m_state) return;
                if (m_outline_color_dialog) {
                    m_outline_color_dialog->raise();
                    m_outline_color_dialog->activateWindow();
                    return;
                }

                m_outline_color_before_dialog = m_state->getOutline_color_hex();
                QColor initial(m_outline_color_before_dialog);
                if (!initial.isValid()) {
                    initial = QColor(QStringLiteral("#FFA600"));
                }

                auto* dialog = new QColorDialog(initial, this);
                dialog->setAttribute(Qt::WA_DeleteOnClose);
                dialog->setOption(QColorDialog::DontUseNativeDialog, true);
                dialog->setOption(QColorDialog::ShowAlphaChannel, false);
                dialog->setWindowTitle(QStringLiteral("Selection Outline Color"));
                m_outline_color_dialog = dialog;

                QObject::connect(
                    dialog,
                    &QObject::destroyed,
                    this,
                    [this]() { m_outline_color_dialog = nullptr; });
                QObject::connect(
                    dialog,
                    &QColorDialog::currentColorChanged,
                    this,
                    [this](const QColor& color) {
                        if (!m_state || !color.isValid()) return;
                        m_state->on_set_outline_color(color.name(QColor::HexRgb).toUpper());
                    });
                QObject::connect(
                    dialog,
                    &QColorDialog::rejected,
                    this,
                    [this]() {
                        if (!m_state || m_outline_color_before_dialog.isEmpty()) return;
                        m_state->on_set_outline_color(m_outline_color_before_dialog);
                    });
                dialog->open();
            });
        QObject::connect(
            m_ivar_render,
            &QPushButton::clicked,
            this,
            [this]() {
                if (m_state) m_state->on_start_ivar_render();
            });
        QObject::connect(
            m_state,
            &BifShellState::ivar_statusChanged,
            this,
            [this]() {
                if (!m_state) return;
                update_ivar_status_label(m_state->getIvar_status());
            });
    }
}

RenderSettingsWidget::~RenderSettingsWidget() = default;

void RenderSettingsWidget::update_outline_color_button(const QString& hex) {
    m_outline_color->setText(hex.toUpper());
    m_outline_color->setStyleSheet(outline_button_style(hex));
}

void RenderSettingsWidget::update_ivar_status_label(const QString& status) {
    if (status.isEmpty()) {
        m_ivar_status->setText(QStringLiteral("Idle"));
        m_ivar_status->setStyleSheet(QStringLiteral(
            "color: rgba(140, 145, 155, 255); font-size: 12px;"));
        return;
    }
    m_ivar_status->setText(status);
    m_ivar_status->setStyleSheet(QStringLiteral(
        "color: rgba(220, 222, 226, 255); font-size: 12px;"));
}
