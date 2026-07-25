use std::path::Path;

use crate::platform::{ActiveEventLoop, PhysicalSize};

use super::outcome::ShellActionOutcome;
use crate::shell::create_rename::disk::{
    create_entry_on_disk_explicit_async, rename_entry_on_disk_explicit_async,
};
use crate::shell::privilege::should_attempt_privileged_operation;
use crate::shell::tasks::ShellTaskStatus;
use crate::shell::transfer::{
    ShellAsyncCreateCompletion, ShellAsyncRenameCompletion, ShellAsyncTaskResult,
};
use crate::{FikaWgpuApp, path_display_label, task_error_detail};
use fika_core::{
    MimeApplicationCache, set_default_mime_application, spawn_operation_task_with_completion,
};

impl FikaWgpuApp {
    pub(crate) fn commit_rename_dialog(&mut self, _event_loop: &ActiveEventLoop) {
        let request = match self.scene.rename_entry_request() {
            Ok(request) => request,
            Err(error) => {
                if self.scene.set_rename_dialog_error(error) {
                    self.finish_rename_dialog_state_change();
                }
                return;
            }
        };

        // Keep the dialog open in a busy state so failures can re-surface errors.
        let _ = self.scene.set_rename_dialog_busy(true);
        self.finish_rename_dialog_state_change();
        let privileged = request.privileged;
        self.scene.record_task_status(ShellTaskStatus::completed(
            if privileged {
                "Administrator rename"
            } else {
                "Renaming"
            },
            format!(
                "{} to {}",
                request.original_name,
                path_display_label(&request.target)
            ),
            privileged,
        ));
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);

        let tx = self.async_task_tx.clone();
        let proxy = self.event_loop_proxy.clone();
        let request_for_task = request.clone();
        if let Err(error) = spawn_operation_task_with_completion(
            move || async move { rename_entry_on_disk_explicit_async(request_for_task).await },
            move |outcome| {
                if tx
                    .send(ShellAsyncTaskResult::Rename(ShellAsyncRenameCompletion {
                        request,
                        outcome,
                    }))
                    .is_ok()
                {
                    proxy.wake_up();
                }
            },
        ) {
            fika_log!("[fika-wgpu] rename-runtime-error {error}");
            if self.scene.set_rename_dialog_error(error.to_string()) {
                self.finish_rename_dialog_state_change();
            }
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Rename failed",
                error.to_string(),
                privileged,
            ));
        }
    }

    pub(crate) fn apply_async_rename_completion(
        &mut self,
        completion: ShellAsyncRenameCompletion,
        size: PhysicalSize<u32>,
    ) -> bool {
        let request = completion.request;
        let outcome = match completion.outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                let administrator_available =
                    !request.privileged && should_attempt_privileged_operation(&error);
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Rename failed",
                    task_error_detail(&error, administrator_available),
                    request.privileged,
                ));
                if self.scene.set_rename_dialog_error(error) {
                    self.finish_rename_dialog_state_change();
                }
                return true;
            }
        };

        if outcome.privileged {
            self.scene.record_task_status(ShellTaskStatus::completed(
                "Administrator rename",
                format!(
                    "{} to {}",
                    path_display_label(&request.source),
                    outcome
                        .message
                        .clone()
                        .unwrap_or_else(|| request.name.clone())
                ),
                true,
            ));
        } else {
            self.scene.record_task_status(ShellTaskStatus::completed(
                "Renamed",
                format!(
                    "{} to {}",
                    request.original_name,
                    path_display_label(&request.target)
                ),
                false,
            ));
        }

        self.scene.close_rename_dialog_after_success(&request);
        self.close_rename_dialog_window();
        let affected_dir = request.target.parent().map(Path::to_path_buf);
        let reload_result = affected_dir
            .as_deref()
            .ok_or_else(|| format!("rename target has no parent: {}", request.target.display()))
            .and_then(|dir| self.scene.reload_panes_showing_path(dir, size));
        match reload_result {
            Ok(_) => {
                self.scene
                    .select_entry_by_name_in_pane(request.pane, &request.name, size);
                true
            }
            Err(error) => {
                fika_log!("[fika-wgpu] rename-reload-error {error}");
                true
            }
        }
    }

    pub(crate) fn commit_create_dialog(&mut self, _event_loop: &ActiveEventLoop) {
        let request = match self.scene.create_entry_request() {
            Ok(request) => request,
            Err(error) => {
                if self.scene.set_create_dialog_error(error) {
                    self.finish_create_dialog_state_change();
                }
                return;
            }
        };

        let _ = self.scene.set_create_dialog_busy(true);
        self.finish_create_dialog_state_change();
        let privileged = request.privileged;
        let kind = request.kind;
        self.scene.record_task_status(ShellTaskStatus::completed(
            if privileged {
                format!("Administrator create {}", kind.as_str())
            } else {
                format!("Creating {}", kind.as_str())
            },
            format!("{} in {}", request.name, request.parent.display()),
            privileged,
        ));
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);

        let tx = self.async_task_tx.clone();
        let proxy = self.event_loop_proxy.clone();
        let request_for_task = request.clone();
        if let Err(error) = spawn_operation_task_with_completion(
            move || async move { create_entry_on_disk_explicit_async(request_for_task).await },
            move |outcome| {
                if tx
                    .send(ShellAsyncTaskResult::Create(ShellAsyncCreateCompletion {
                        request,
                        outcome,
                    }))
                    .is_ok()
                {
                    proxy.wake_up();
                }
            },
        ) {
            fika_log!("[fika-wgpu] create-runtime-error {error}");
            if self.scene.set_create_dialog_error(error.to_string()) {
                self.finish_create_dialog_state_change();
            }
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Create failed",
                error.to_string(),
                privileged,
            ));
        }
    }

    pub(crate) fn apply_async_create_completion(
        &mut self,
        completion: ShellAsyncCreateCompletion,
        size: PhysicalSize<u32>,
    ) -> bool {
        let request = completion.request;
        let outcome = match completion.outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                let administrator_available =
                    !request.privileged && should_attempt_privileged_operation(&error);
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Create failed",
                    task_error_detail(&error, administrator_available),
                    request.privileged,
                ));
                if self.scene.set_create_dialog_error(error) {
                    self.finish_create_dialog_state_change();
                }
                return true;
            }
        };

        if outcome.privileged {
            self.scene.record_task_status(ShellTaskStatus::completed(
                format!("Administrator create {}", request.kind.as_str()),
                format!(
                    "{} in {}",
                    outcome
                        .message
                        .clone()
                        .unwrap_or_else(|| request.name.clone()),
                    request.parent.display()
                ),
                true,
            ));
        } else {
            self.scene.record_task_status(ShellTaskStatus::completed(
                format!("Created {}", request.kind.as_str()),
                format!("{} in {}", request.name, request.parent.display()),
                false,
            ));
        }

        self.scene.close_create_dialog_after_success(&request);
        self.close_create_dialog_window();
        match self.scene.reload_panes_showing_path(&request.parent, size) {
            Ok(_) => {
                self.scene
                    .select_entry_by_name_in_pane(request.pane, &request.name, size);
                true
            }
            Err(error) => {
                fika_log!("[fika-wgpu] create-new-reload-error {error}");
                true
            }
        }
    }

    pub(crate) fn commit_open_with_chooser(&mut self) {
        let request = match self.scene.open_with_launch_request(&self.mime_applications) {
            Ok(request) => request,
            Err(error) => {
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Open With failed",
                    error.clone(),
                    false,
                ));
                if self.scene.set_open_with_chooser_error(error) {
                    self.finish_open_with_dialog_state_change();
                }
                return;
            }
        };

        if let Some(default_update) = request.default_update.as_ref() {
            match set_default_mime_application(
                &default_update.mime_type,
                &default_update.desktop_id,
            ) {
                Ok(path) => {
                    fika_log!(
                        "[fika-wgpu] open-with-default mime={} desktop={} path={}",
                        default_update.mime_type,
                        default_update.desktop_id,
                        path.display()
                    );
                    self.mime_applications = MimeApplicationCache::load();
                }
                Err(error) => {
                    self.scene.record_task_status(ShellTaskStatus::failed(
                        "Set Default Application failed",
                        error.clone(),
                        false,
                    ));
                    if self.scene.set_open_with_chooser_error(error) {
                        self.finish_open_with_dialog_state_change();
                    }
                    return;
                }
            }
        }

        self.scene.close_open_with_chooser_after_success(&request);
        self.close_open_with_dialog_window();
        self.launch_open_with_request(request);
    }
}
