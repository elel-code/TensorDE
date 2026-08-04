use crate::windowing::PhysicalSize;
use crate::windowing::{ActiveEventLoop, ElementState, KeyEvent};

use super::outcome::{ShellActionEffect, ShellActionOutcome};
use crate::TensorFilesApp;
use crate::ui::animation::ShellAnimationKind;
use crate::ui::shortcuts::{
    LocationCommand, dark_mode_toggle_requested_for_key_event, escape_requested_for_key_event,
    file_keyboard_command_for_key_event, filter_command_for_key_event,
    hidden_toggle_requested_for_key_event, is_activation_key, location_command_for_key_event,
    navigation_action_for_key, path_navigation_action_for_key, reload_requested_for_key_event,
    selection_command_for_key_event, view_mode_for_key_event, zoom_action_for_key_event,
};

impl TensorFilesApp {
    pub(crate) fn handle_main_keyboard_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        event: &KeyEvent,
    ) {
        if event.state != ElementState::Pressed {
            return;
        }
        if self
            .window
            .as_ref()
            .is_some_and(|window| self.text_input_consumes_key(window.id(), event))
        {
            return;
        }
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let shortcut = self.modifiers.state().control_key() || self.modifiers.state().meta_key();
        let effect = self.dispatch_main_keyboard_input(event_loop, event, size, shortcut);
        self.apply_action_effect(event_loop, effect);
    }

    fn dispatch_main_keyboard_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        event: &KeyEvent,
        size: PhysicalSize<u32>,
        shortcut: bool,
    ) -> ShellActionEffect {
        if self.scene.is_drop_menu_open() && escape_requested_for_key_event(event) {
            let changed = self.scene.close_drop_menu();
            return ShellActionOutcome::redraw_if(changed).into();
        }
        if self.scene.is_context_menu_open() && escape_requested_for_key_event(event) {
            let changed = self.scene.close_context_menu();
            return ShellActionOutcome::redraw_if(changed).into();
        }
        if dark_mode_toggle_requested_for_key_event(
            event,
            shortcut,
            self.modifiers.state().shift_key(),
        ) {
            let changed = self.toggle_user_dark_mode();
            return ShellActionOutcome::present_if(changed, "toggle-dark-mode").into();
        }
        if let Some(command) =
            location_command_for_key_event(event, shortcut, self.scene.is_location_editing())
        {
            if command == LocationCommand::Commit {
                self.commit_location_draft(event_loop);
                return ShellActionOutcome::None.into();
            } else {
                let shine = matches!(command, LocationCommand::Activate);
                let changed = self.scene.apply_location_command(command, size);
                // Activate starts location focus shine; keep presentation queued.
                return ShellActionOutcome::redraw_if(changed)
                    .with_animation_if(shine && changed, ShellAnimationKind::LocationFocusShine)
                    .into();
            }
        }
        if let Some(command) =
            filter_command_for_key_event(event, shortcut, self.scene.filter_active)
        {
            let changed = self.scene.apply_filter_command(command, size);
            return ShellActionOutcome::redraw_if(changed).into();
        }
        if let Some(command) = file_keyboard_command_for_key_event(event, shortcut) {
            self.perform_file_keyboard_command(event_loop, command);
            return ShellActionOutcome::None.into();
        }
        if let Some(view_mode) = view_mode_for_key_event(event, shortcut) {
            let changed = self.set_user_view_mode(view_mode, size);
            return ShellActionOutcome::present_if(changed, "switch-immediate").into();
        }
        if shortcut && let Some(zoom_action) = zoom_action_for_key_event(event) {
            if self.set_user_zoom(zoom_action, size) {
                return ShellActionOutcome::queue_animation(ShellAnimationKind::ZoomSettle).into();
            }
            return ShellActionOutcome::None.into();
        }
        if let Some(command) = selection_command_for_key_event(event, shortcut) {
            let changed = self.scene.apply_selection_command(command);
            return ShellActionOutcome::redraw_if(changed).into();
        }
        if reload_requested_for_key_event(event, shortcut) {
            self.reload_scene_path(event_loop);
            return ShellActionOutcome::None.into();
        }
        if hidden_toggle_requested_for_key_event(event, shortcut) {
            let changed = self.toggle_user_hidden_visibility(size);
            return ShellActionOutcome::present_if(changed, "toggle-hidden").into();
        }
        if is_activation_key(&event.logical_key) {
            if let Some((pane, path)) = self.scene.selected_directory_path() {
                return ShellActionEffect::load_path(pane, path, "activate-directory");
            } else if let Some(request) = self.scene.selected_file_open_request() {
                self.launch_open_file_request(&request);
            }
            return ShellActionOutcome::None.into();
        }
        if let Some(action) =
            path_navigation_action_for_key(&event.logical_key, self.modifiers.state().alt_key())
        {
            self.perform_path_navigation(event_loop, action);
            return ShellActionOutcome::None.into();
        }
        let Some(action) = navigation_action_for_key(&event.logical_key) else {
            return ShellActionOutcome::None.into();
        };
        let extend = self.modifiers.state().shift_key();
        let changed = self.scene.navigate(action, extend, size);
        ShellActionOutcome::redraw_if(changed).into()
    }
}
