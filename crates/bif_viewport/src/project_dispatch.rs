//! Project event dispatch — new, open, save, save-as, open-recent.

use std::path::PathBuf;

use crate::Renderer;

impl Renderer {
    pub(crate) fn handle_project_new(&mut self) {
        if self.save_if_needed_then_proceed("New Project") {
            self.reset_project();
        }
    }

    pub(crate) fn handle_project_open(&mut self) {
        if self.save_if_needed_then_proceed("Open Project") {
            let path = self.with_dialog_focus(|| {
                rfd::FileDialog::new()
                    .add_filter("BIF Project", &["bif", "bifa"])
                    .pick_file()
            });
            if let Some(path) = path {
                self.open_project(&path);
            }
        }
    }

    pub(crate) fn handle_project_save(&mut self) {
        if let Some(path) = self.project.file_path.clone() {
            self.save_project_to(&path);
        } else {
            // No path yet — trigger Save As
            self.save_project_as();
        }
    }

    pub(crate) fn handle_project_save_as(&mut self) {
        self.save_project_as();
    }

    pub(crate) fn handle_project_open_recent(&mut self, path: PathBuf) {
        if self.save_if_needed_then_proceed("Open Recent") {
            self.open_project(&path);
        }
    }
}
