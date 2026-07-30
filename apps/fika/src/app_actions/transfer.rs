use crate::windowing::ActiveEventLoop;

use super::outcome::ShellActionOutcome;
use crate::FikaApp;
use crate::ui::drop_menu::ShellDropOperationRequest;
use crate::ui::operation_request::ShellOperationRequest;
use crate::ui::tasks::ShellTaskStatus;
use crate::ui::transfer::ShellAsyncTransferSource;
use fika_core::{FileClipboardRole, FileTransferMode, decode_file_clipboard_text, is_network_path};

impl FikaApp {
    pub(crate) fn paste_from_clipboard(&mut self, event_loop: &ActiveEventLoop, privileged: bool) {
        self.paste_from_clipboard_with_target(event_loop, true, privileged);
    }

    pub(crate) fn paste_from_clipboard_into_active_pane(&mut self, event_loop: &ActiveEventLoop) {
        self.paste_from_clipboard_with_target(event_loop, false, false);
    }

    pub(crate) fn paste_from_clipboard_with_target(
        &mut self,
        _event_loop: &ActiveEventLoop,
        use_context: bool,
        privileged: bool,
    ) {
        if self.renderer.is_none() {
            return;
        }
        self.load_clipboard_text_for_paste(use_context, privileged);
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }

    pub(crate) fn finish_paste_from_clipboard_text(
        &mut self,
        use_context: bool,
        privileged: bool,
        text: String,
    ) -> bool {
        if self.renderer.is_none() {
            return false;
        }
        let target_dir = if use_context {
            self.scene
                .context_target_paste_directory()
                .or_else(|| self.scene.active_pane_paste_directory())
        } else {
            self.scene.active_pane_paste_directory()
        };
        let Some((_target_pane, target_dir)) = target_dir else {
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Paste failed",
                "No paste target pane",
                privileged,
            ));
            return true;
        };
        if is_network_path(&target_dir) {
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Paste failed",
                "Remote paste target is not available yet",
                privileged,
            ));
            return true;
        }

        if let Some(payload) = decode_file_clipboard_text(&text) {
            if payload.paths.iter().any(|path| is_network_path(path)) {
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Paste failed",
                    "Remote paste source is not available yet",
                    privileged,
                ));
                return true;
            }
            let mode = match payload.role {
                FileClipboardRole::Copy => FileTransferMode::Copy,
                FileClipboardRole::Cut => FileTransferMode::Move,
            };
            self.submit_operation_request(ShellOperationRequest::transfer(
                ShellAsyncTransferSource::Paste,
                target_dir,
                mode,
                payload.paths,
                "Paste",
                payload.role == FileClipboardRole::Cut,
                privileged,
            ));
            return true;
        }
        if privileged {
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Administrator paste failed",
                "administrator paste is only available for file clipboard items",
                true,
            ));
            return true;
        }
        if text.trim().is_empty() {
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Paste failed",
                "clipboard is empty",
                false,
            ));
            return true;
        }
        self.submit_operation_request(ShellOperationRequest::paste_text(target_dir, text));
        true
    }

    pub(crate) fn perform_drop_operation_request(
        &mut self,
        request: ShellDropOperationRequest,
    ) -> ShellActionOutcome {
        if let Err(error) = self.scene.validate_drop_operation_request(&request) {
            self.scene.record_task_status(ShellTaskStatus::failed(
                if request.privileged {
                    "Administrator drop failed"
                } else {
                    "Drop failed"
                },
                error,
                request.privileged,
            ));
            return ShellActionOutcome::Redraw;
        }
        self.submit_operation_request(ShellOperationRequest::transfer(
            ShellAsyncTransferSource::Drop,
            request.target_dir,
            request.mode,
            request.sources,
            request.mode.label(),
            false,
            request.privileged,
        ));
        ShellActionOutcome::Redraw
    }
}
