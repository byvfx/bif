// FirstLaunchWidget — shown as the central viewport area when no
// stage is loaded. Two big "New Stage" / "Open Stage" cards plus
// a Recent Stages list (Phase B.6 stub — list is empty for now,
// Phase C populates it from QSettings).
//
// Emits `newStageClicked` and `openStageClicked` signals; window
// builder wires them to BifShellState invokables and flips the
// central QStackedWidget to the viewport.

#pragma once

#include <QWidget>

class QListWidget;

class FirstLaunchWidget : public QWidget {
    Q_OBJECT
public:
    explicit FirstLaunchWidget(QWidget* parent = nullptr);
    ~FirstLaunchWidget() override;

    /// Refresh the Recent Stages list from QSettings. Phase B.6
    /// implementation always renders empty; Phase C populates.
    void refresh_recents();

signals:
    void newStageClicked();
    void openStageClicked();
    void recentStageActivated(const QString& path);

private:
    QListWidget* m_recent_list;
};
