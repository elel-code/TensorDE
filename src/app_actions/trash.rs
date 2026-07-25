use crate::platform::ActiveEventLoop;

use super::outcome::ShellActionOutcome;
use crate::FikaWgpuApp;
use crate::shell::context_menu::ShellContextMenuAction;
use crate::shell::tasks::ShellTaskStatus;
use fika_core::is_network_path;

impl FikaWgpuApp {
    pub(crate) fn perform_trash_view_context_action(
        &mut self,
        event_loop: &ActiveEventLoop,
        action: ShellContextMenuAction,
    ) {
        if action == ShellContextMenuAction::EmptyTrash {
            match self.start_async_trash_view_operation(action) {
                Ok(()) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
                Err(error) => {
                    fika_log!(
                        "[fika-wgpu] trash-view-error action={} {error}",
                        action.as_str()
                    );
                    self.scene.record_task_status(ShellTaskStatus::failed(
                        "Empty Trash failed",
                        error,
                        false,
                    ));
                    self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                }
            }
            return;
        }

        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.perform_trash_view_context_action(action, size) {
            Ok(result) if result.success_count > 0 => {
                self.apply_action_outcome(event_loop, ShellActionOutcome::Present(action.as_str()));
            }
            Ok(_) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] trash-view-error action={} {error}",
                    action.as_str()
                );
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }

    pub(crate) fn move_context_target_to_trash(
        &mut self,
        _event_loop: &ActiveEventLoop,
        privileged: bool,
    ) {
        let pane_to_reload = self
            .scene
            .context_target_pane()
            .unwrap_or_else(|| self.scene.active_pane());
        let paths = match self.scene.context_target_trash_paths() {
            Ok(paths) => paths,
            Err(error) => {
                fika_log!("[fika-wgpu] trash-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    if privileged {
                        "Administrator move to Trash failed"
                    } else {
                        "Move to Trash failed"
                    },
                    error,
                    privileged,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
        };
        if paths.iter().any(|path| is_network_path(path)) {
            self.scene.record_task_status(ShellTaskStatus::failed(
                if privileged {
                    "Administrator move to Trash failed"
                } else {
                    "Move to Trash failed"
                },
                "remote trash is not available yet",
                privileged,
            ));
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            return;
        }

        self.start_async_move_to_trash(paths, pane_to_reload, privileged);
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }

    pub(crate) fn replace_trash_restore_conflicts(&mut self, event_loop: &ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.replace_trash_restore_conflicts(size) {
            Ok(result) if result.success_count > 0 => {
                self.apply_action_outcome(
                    event_loop,
                    ShellActionOutcome::Present("replace-trash-conflicts"),
                );
            }
            Ok(_) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
            Err(error) => {
                fika_log!("[fika-wgpu] trash-conflict-error {error}");
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }
}
