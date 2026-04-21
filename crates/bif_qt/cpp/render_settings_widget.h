// RenderSettingsWidget — Phase D.3 panel.
//
// QFormLayout of render configuration controls. Phase D.3 values
// are local state (not yet wired to bif_renderer::RenderConfig —
// that's Phase E). Two groups: Path Tracer and Post-Processing.

#pragma once

#include <QWidget>

class QCheckBox;
class QDoubleSpinBox;
class QSpinBox;

class RenderSettingsWidget : public QWidget {
    Q_OBJECT
public:
    explicit RenderSettingsWidget(QWidget* parent = nullptr);
    ~RenderSettingsWidget() override;

private:
    QSpinBox* m_spp;
    QSpinBox* m_max_depth;
    QDoubleSpinBox* m_exposure;
    QDoubleSpinBox* m_gamma;
    QCheckBox* m_denoise;
    QCheckBox* m_sharc;
};
