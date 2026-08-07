use crate::windowing::ActiveEventLoop;

use super::outcome::ShellActionOutcome;
use crate::TensorFilesApp;
use crate::ui::tasks::ShellTaskStatus;
use tensor_files_core::default_user_places_path;

impl TensorFilesApp {
    pub(crate) fn open_add_network_folder_input(&mut self) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let changed = self.scene.open_add_network_folder_location_draft(size);
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
    }

    pub(crate) fn add_context_target_to_places(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self
            .scene
            .add_context_target_to_places(&default_user_places_path(), size)
        {
            Ok(true) => self.apply_window_action_outcome(ShellActionOutcome::Present("add-place")),
            Ok(false) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
            Err(error) => {
                tensor_files_log!("[tensor-files] add-place-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Add to Places failed",
                    error,
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }

    pub(crate) fn remove_context_place(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self
            .scene
            .remove_context_place(&default_user_places_path(), size)
        {
            Ok(true) => {
                self.apply_window_action_outcome(ShellActionOutcome::Present("remove-place"))
            }
            Ok(false) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
            Err(error) => {
                tensor_files_log!("[tensor-files] remove-place-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Remove Place failed",
                    error,
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }
}
