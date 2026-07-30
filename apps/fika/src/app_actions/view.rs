use crate::windowing::ActiveEventLoop;
use crate::windowing::PhysicalSize;

use super::outcome::ShellActionOutcome;
use crate::ui::file_item_view::file_manager_icon_size_for_zoom_level;
use crate::ui::options::ShellViewMode;
use crate::ui::pane::ShellPaneId;
use crate::ui::shortcuts::ZoomAction;
use crate::{
    FikaApp, save_places_visible_setting, save_preview_size_settings, save_show_hidden_setting,
    save_view_mode_setting,
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

    pub(crate) fn set_user_zoom(&mut self, action: ZoomAction, size: PhysicalSize<u32>) -> bool {
        if !self.scene.zoom(action, size) {
            return false;
        }
        self.record_zoom_setting_for_pane(self.scene.active_pane());
        true
    }

    pub(crate) fn zoom_levels_snapshot(&self) -> [Option<i32>; 2] {
        ShellPaneId::ALL.map(|pane| self.scene.pane_zoom_level(pane))
    }

    pub(crate) fn record_changed_zoom_settings(&mut self, before: [Option<i32>; 2]) {
        for (pane, old_level) in ShellPaneId::ALL.into_iter().zip(before) {
            if self.scene.pane_zoom_level(pane) != old_level {
                self.record_zoom_setting_for_pane(pane);
            }
        }
    }

    pub(crate) fn record_zoom_setting_for_pane(&mut self, pane: ShellPaneId) {
        let Some(pane) = self.scene.pane_state(pane) else {
            return;
        };
        let preview_size = file_manager_icon_size_for_zoom_level(pane.zoom_level()) as u16;
        let index = match pane.view_mode {
            ShellViewMode::Icons => 0,
            ShellViewMode::Compact => 1,
            ShellViewMode::Details => 2,
        };
        self.pending_preview_sizes[index] = Some(preview_size);
    }

    pub(crate) fn flush_preview_size_settings(&mut self) {
        if self.pending_preview_sizes.iter().all(Option::is_none) {
            return;
        }
        let pending = std::mem::take(&mut self.pending_preview_sizes);
        if let Err(error) = save_preview_size_settings(&self.settings_path, pending) {
            fika_log!("[fika] settings-save-error {error}");
            for (slot, value) in self.pending_preview_sizes.iter_mut().zip(pending) {
                if slot.is_none() {
                    *slot = value;
                }
            }
        }
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
