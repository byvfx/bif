// RenderSettingsWidget — Phase D.3 panel.
//
// QFormLayout of render configuration controls. In C2, the selection-outline
// controls and Ivar preview surface are wired to `BifShellState`; the path-
// tracer and post-processing fields remain local until Phase E render-config
// plumbing lands.

#pragma once

#include <QPointer>
#include <QString>
#include <QWidget>

class BifShellState;
class QCheckBox;
class QColorDialog;
class QDoubleSpinBox;
class QLabel;
class QPushButton;
class QSpinBox;

class RenderSettingsWidget : public QWidget {
    Q_OBJECT
public:
    explicit RenderSettingsWidget(BifShellState* state, QWidget* parent = nullptr);
    ~RenderSettingsWidget() override;

private:
    void update_outline_color_button(const QString& hex);
    void update_ivar_status_label(const QString& status);

    BifShellState* m_state;
    QSpinBox* m_spp;
    QSpinBox* m_max_depth;
    QDoubleSpinBox* m_exposure;
    QDoubleSpinBox* m_gamma;
    QCheckBox* m_denoise;
    QCheckBox* m_sharc;
    QDoubleSpinBox* m_outline_width;
    QPushButton* m_outline_color;
    QPushButton* m_ivar_render;
    QLabel* m_ivar_status;
    QPointer<QColorDialog> m_outline_color_dialog;
    QString m_outline_color_before_dialog;
};
