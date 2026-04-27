// UsdaPanelWidget — C4b-2 USDA dock panel.
//
// Composition:
//   - Header label showing the active edit-target layer identifier.
//   - QPlainTextEdit with the layer's serialized USDA text.
//   - Apply button: validates via `parse_usda` then dispatches a
//     `ReplaceLayerContents` edit through the C4a edit history.
//   - Status label shows the parse / dispatch error message in red
//     when Apply rejects the text; cleared on success.
//
// Apply-only validation per the C4b handoff — no real-time keystroke
// parse. Edits sync back into the editor on layer-state revision
// bumps so undo/redo from the rest of the app reflects in the panel.

#pragma once

#include <QWidget>

class BifShellState;
class QLabel;
class QPlainTextEdit;
class QPushButton;

class UsdaPanelWidget : public QWidget {
    Q_OBJECT
public:
    explicit UsdaPanelWidget(BifShellState* state, QWidget* parent = nullptr);
    ~UsdaPanelWidget() override;

private slots:
    void on_apply_clicked();
    void on_layer_state_changed();

private:
    void reload_layer_text();

    BifShellState* m_state;
    QLabel* m_header;
    QPlainTextEdit* m_editor;
    QPushButton* m_apply_button;
    QLabel* m_error_label;
    // Suppress reload while user is in the middle of editing — only
    // pull fresh text from the layer on layer_state revision bumps
    // when the editor is not focused. Otherwise external undo/redo
    // would clobber unsaved keystrokes.
    bool m_dirty;
};
