#include "render_settings_widget.h"

#include <QCheckBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QGroupBox>
#include <QLabel>
#include <QSpinBox>
#include <QVBoxLayout>

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
        "QCheckBox { color: rgba(220, 222, 226, 255); }");
}

}  // namespace

RenderSettingsWidget::RenderSettingsWidget(QWidget* parent)
    : QWidget(parent),
      m_spp(nullptr),
      m_max_depth(nullptr),
      m_exposure(nullptr),
      m_gamma(nullptr),
      m_denoise(nullptr),
      m_sharc(nullptr) {
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

    // Footnote
    auto* note = new QLabel(
        QStringLiteral("(values stored locally — Phase E wires bif_renderer::RenderConfig)"),
        this);
    note->setStyleSheet(QStringLiteral(
        "color: rgba(100, 105, 115, 255); font-size: 10px;"));
    note->setAlignment(Qt::AlignCenter);
    outer->addWidget(note);

    outer->addStretch();
}

RenderSettingsWidget::~RenderSettingsWidget() = default;
