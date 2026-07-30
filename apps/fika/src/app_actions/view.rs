use crate::windowing::ActiveEventLoop;
use crate::windowing::PhysicalSize;

use super::outcome::ShellActionOutcome;
use crate::ui::options::ShellViewMode;
use crate::{
    FikaApp, save_places_visible_setting, save_show_hidden_setting, save_view_mode_setting,
};

impl FikaApp {
    pub(crate) fn set_user_view_mode(
        &mut self,
        view_mode: ShellViewMode,
        size: PhysicalSize<u32>,
    ) -> bool {
        if !self.scene.set_view_mode(view_mode, size) {
            return false;
        }
        if let Err(error) = save_view_mode_setting(&self.settings_path, view_mode) {
            fika_log!("[fika] settings-save-error {error}");
        }
        true
    }

    pub(crate) fn toggle_user_hidden_visibility(&mut self, size: PhysicalSize<u32>) -> bool {
        if !self.scene.toggle_hidden_visibility(size) {
            return false;
        }
        if let Err(error) = save_show_hidden_setting(&self.settings_path, self.scene.show_hidden) {
            fika_log!("[fika] settings-save-error {error}");
        }
        self.request_settings_dialog_redraw();
        true
    }

    pub(crate) fn toggle_user_places_visibility(&mut self, size: PhysicalSize<u32>) -> bool {
        if !self.scene.toggle_places_visibility(size) {
            return false;
        }
        if let Err(error) =
            save_places_visible_setting(&self.settings_path, self.scene.places_visible)
        {
            fika_log!("[fika] settings-save-error {error}");
        }
        self.request_settings_dialog_redraw();
        true
    }

    pub(crate) fn open_context_target_in_split_pane(
        &mut self,
        event_loop: &ActiveEventLoop,
        reason: &'static str,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.open_split_pane_from_context(size) {
            Ok(true) => self.apply_action_outcome(event_loop, ShellActionOutcome::Present(reason)),
            Ok(false) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
            Err(error) => {
                fika_log!("[fika] split-pane-error {error}");
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }

    pub(crate) fn toggle_split_view_from_toolbar(&mut self, event_loop: &ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.toggle_split_view_from_toolbar(size) {
            Ok(true) => self.apply_action_outcome(
                event_loop,
                ShellActionOutcome::Present("toolbar-split-view"),
            ),
            Ok(false) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
            Err(error) => {
                fika_log!("[fika] split-pane-error {error}");
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }
}
