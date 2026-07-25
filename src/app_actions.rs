mod appearance;
mod clipboard;
mod device;
mod dialog_commit;
mod drag;
mod keyboard;
mod launch;
mod navigation;
mod outcome;
mod places;
mod pointer;
mod pointer_route;
mod scroll;
mod transfer;
mod trash;
mod view;

use crate::platform::ActiveEventLoop;

use self::outcome::ShellActionOutcome;
use crate::shell::action::{
    ContextMenuActionDispatch, ContextMenuCommandDispatch, FileKeyboardCommandDispatch,
    context_menu_command_dispatch, file_keyboard_command_dispatch,
};
use crate::shell::context_menu::ShellContextMenuCommand;
use crate::shell::shortcuts::{FileKeyboardCommand, SelectionCommand};
use crate::shell::tasks::ShellTaskStatus;
use crate::{FikaWgpuApp, file_clipboard_role_as_str};

impl FikaWgpuApp {
    pub(crate) fn perform_context_menu_action(
        &mut self,
        event_loop: &ActiveEventLoop,
        command: ShellContextMenuCommand,
    ) {
        let (action, dispatch) = match context_menu_command_dispatch(command) {
            ContextMenuCommandDispatch::SetViewMode(view_mode) => {
                let outcome = self
                    .renderer
                    .as_ref()
                    .map(|renderer| renderer.size)
                    .map(|size| {
                        ShellActionOutcome::redraw_if(self.set_user_view_mode(view_mode, size))
                    })
                    .unwrap_or(ShellActionOutcome::None);
                self.apply_window_action_outcome(outcome);
                return;
            }
            ContextMenuCommandDispatch::CreateEntry { kind, privileged } => {
                if self
                    .scene
                    .open_create_dialog_from_context_with_kind(kind, privileged)
                {
                    self.ensure_create_dialog_window(event_loop);
                    self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                }
                return;
            }
            ContextMenuCommandDispatch::RunServiceMenuAction { action_id } => {
                self.run_context_service_menu_action(action_id);
                return;
            }
            ContextMenuCommandDispatch::OpenWithApplication { desktop_id } => {
                self.open_context_target_with_application(desktop_id);
                return;
            }
            ContextMenuCommandDispatch::RedrawOnly => {
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
            ContextMenuCommandDispatch::Action { action, dispatch } => (action, dispatch),
        };
        match dispatch {
            ContextMenuActionDispatch::OpenWith => {
                if self
                    .scene
                    .open_open_with_chooser_from_context(&self.mime_applications)
                {
                    self.ensure_open_with_dialog_window(event_loop);
                    self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                }
            }
            ContextMenuActionDispatch::Refresh => self.reload_scene_path(event_loop),
            ContextMenuActionDispatch::ToggleHiddenFiles => {
                let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
                    return;
                };
                if self.toggle_user_hidden_visibility(size) {
                    self.scene.record_task_status(ShellTaskStatus::completed(
                        if self.scene.show_hidden {
                            "Hidden Files Shown"
                        } else {
                            "Hidden Files Hidden"
                        },
                        "Current view updated",
                        false,
                    ));
                    self.apply_action_outcome(
                        event_loop,
                        ShellActionOutcome::Present("context-toggle-hidden"),
                    );
                }
            }
            ContextMenuActionDispatch::OpenContextTargetInSplitPane => {
                self.open_context_target_in_split_pane(event_loop, action.as_str());
            }
            ContextMenuActionDispatch::SelectAll => {
                let _ = self
                    .scene
                    .apply_selection_command(SelectionCommand::SelectAll);
                self.scene.record_task_status(ShellTaskStatus::completed(
                    "Selected All",
                    self.scene.active_pane_status_summary(),
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
            ContextMenuActionDispatch::Properties => {
                let changed = self.scene.open_properties_overlay_from_context();
                if changed {
                    self.ensure_properties_dialog_window(event_loop);
                }
                self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
            }
            ContextMenuActionDispatch::CreateNew => {
                if self.scene.open_create_dialog_from_context() {
                    self.ensure_create_dialog_window(event_loop);
                    self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                }
            }
            ContextMenuActionDispatch::Rename { privileged } => {
                if self.scene.open_rename_dialog_from_context(privileged) {
                    self.ensure_rename_dialog_window(event_loop);
                    self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                }
            }
            ContextMenuActionDispatch::AddToPlaces => self.add_context_target_to_places(event_loop),
            ContextMenuActionDispatch::AddNetworkFolder => self.open_add_network_folder_input(),
            ContextMenuActionDispatch::RemovePlace => self.remove_context_place(event_loop),
            ContextMenuActionDispatch::TrashView => {
                self.perform_trash_view_context_action(event_loop, action)
            }
            ContextMenuActionDispatch::MoveToTrash { privileged } => {
                self.move_context_target_to_trash(event_loop, privileged)
            }
            ContextMenuActionDispatch::FileClipboard(role) => {
                match self.scene.context_target_file_clipboard_request(action) {
                    Ok(Some(request)) => self.store_file_clipboard_request(&request),
                    Ok(None) => {
                        fika_log!(
                            "[fika-wgpu] clipboard-export-error role={} target=none",
                            file_clipboard_role_as_str(role)
                        );
                        self.scene.record_task_status(ShellTaskStatus::failed(
                            "Clipboard failed",
                            "No file target",
                            false,
                        ));
                    }
                    Err(error) => {
                        fika_log!(
                            "[fika-wgpu] clipboard-export-error role={} {error}",
                            file_clipboard_role_as_str(role)
                        );
                        self.scene.record_task_status(ShellTaskStatus::failed(
                            "Clipboard failed",
                            error,
                            false,
                        ));
                    }
                }
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
            ContextMenuActionDispatch::CopyLocation => {
                match self.scene.context_target_copy_location_request() {
                    Some(request) => self.store_copy_location_request(request),
                    None => {
                        fika_log!("[fika-wgpu] copy-location-error target=none");
                        self.scene.record_task_status(ShellTaskStatus::failed(
                            "Copy Location failed",
                            "No target",
                            false,
                        ));
                    }
                }
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
            ContextMenuActionDispatch::Device => {
                self.perform_device_context_action(event_loop, action)
            }
            ContextMenuActionDispatch::Paste { privileged } => {
                self.paste_from_clipboard(event_loop, privileged)
            }
            ContextMenuActionDispatch::Noop => {}
        }
    }

    pub(crate) fn perform_file_keyboard_command(
        &mut self,
        event_loop: &ActiveEventLoop,
        command: FileKeyboardCommand,
    ) {
        let Some(_size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match file_keyboard_command_dispatch(command) {
            FileKeyboardCommandDispatch::Clipboard(role) => {
                match self.scene.active_file_clipboard_request(role) {
                    Ok(Some(request)) => self.store_file_clipboard_request(&request),
                    Ok(None) => fika_log!(
                        "[fika-wgpu] clipboard-export-error role={} target=none",
                        file_clipboard_role_as_str(role)
                    ),
                    Err(error) => fika_log!(
                        "[fika-wgpu] clipboard-export-error role={} {error}",
                        file_clipboard_role_as_str(role)
                    ),
                }
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
            FileKeyboardCommandDispatch::Paste => {
                self.paste_from_clipboard_into_active_pane(event_loop)
            }
            FileKeyboardCommandDispatch::Rename => {
                if self.scene.open_rename_dialog_from_active_selection(false) {
                    self.ensure_rename_dialog_window(event_loop);
                    self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                }
            }
            FileKeyboardCommandDispatch::Delete => self.delete_active_selection(event_loop),
        }
    }
}
