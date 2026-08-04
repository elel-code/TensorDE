use crate::ui::clipboard::FileClipboardExportRequest;
use crate::ui::operation_request::{ShellClipboardWork, ShellOperationRequest};
use crate::ui::tasks::ShellTaskStatus;
use crate::ui::transfer::ShellAsyncClipboardCompletion;
use crate::{CopyLocationRequest, TensorFilesApp, file_clipboard_role_as_str, paths_task_summary};

impl TensorFilesApp {
    pub(crate) fn store_file_clipboard_request(&mut self, request: &FileClipboardExportRequest) {
        if let Some(clipboard) = self.clipboard.as_ref() {
            match clipboard.store_file_clipboard_async(
                request.role,
                request.paths.clone(),
                request.text.clone(),
            ) {
                Ok(reply_rx) => {
                    self.submit_operation_request(ShellOperationRequest::clipboard(
                        ShellClipboardWork::StoreFile {
                            request: request.clone(),
                            reply_rx,
                        },
                    ));
                }
                Err(error) => self.record_file_clipboard_store_error(request, error),
            }
        } else {
            tensor_files_log!(
                "[tensor-files] clipboard-export-error role={} paths={} error=clipboard-unavailable",
                file_clipboard_role_as_str(request.role),
                request.paths.len()
            );
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Clipboard failed",
                format!(
                    "Clipboard is unavailable for {}",
                    paths_task_summary(&request.paths)
                ),
                false,
            ));
        }
    }

    pub(crate) fn store_copy_location_request(&mut self, request: CopyLocationRequest) {
        let Some(clipboard) = self.clipboard.as_ref() else {
            tensor_files_log!(
                "[tensor-files] copy-location-error path={} error=clipboard-unavailable",
                request.path.display()
            );
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Copy Location failed",
                format!("Clipboard is unavailable for {}", request.path.display()),
                false,
            ));
            return;
        };

        match clipboard.store_text_async(request.text.clone()) {
            Ok(reply_rx) => {
                self.submit_operation_request(ShellOperationRequest::clipboard(
                    ShellClipboardWork::CopyLocation { request, reply_rx },
                ));
            }
            Err(error) => self.record_copy_location_error(&request, error),
        }
    }

    pub(crate) fn load_clipboard_text_for_paste(&mut self, use_context: bool, privileged: bool) {
        let Some(clipboard) = self.clipboard.as_ref() else {
            tensor_files_log!("[tensor-files] paste-error error=clipboard-unavailable");
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Paste failed",
                "Clipboard is unavailable",
                false,
            ));
            return;
        };

        match clipboard.load_text_async() {
            Ok(reply_rx) => {
                self.submit_operation_request(ShellOperationRequest::clipboard(
                    ShellClipboardWork::LoadPaste {
                        use_context,
                        privileged,
                        reply_rx,
                    },
                ));
            }
            Err(error) => {
                tensor_files_log!("[tensor-files] paste-error load={error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Paste failed",
                    format!("Clipboard read failed: {error}"),
                    false,
                ));
            }
        }
    }

    /// Middle-click / primary selection paste (async; does not block the UI thread).
    pub(crate) fn load_primary_text_for_paste(&mut self) {
        let Some(clipboard) = self.clipboard.as_ref() else {
            tensor_files_log!("[tensor-files] primary-paste-error error=clipboard-unavailable");
            return;
        };

        match clipboard.load_primary_text_async() {
            Ok(reply_rx) => {
                // Same completion path as Ctrl+V into the active pane.
                self.submit_operation_request(ShellOperationRequest::clipboard(
                    ShellClipboardWork::LoadPaste {
                        use_context: false,
                        privileged: false,
                        reply_rx,
                    },
                ));
            }
            Err(error) => {
                // Missing primary global or empty offer is common; keep quiet unless noisy.
                tensor_files_log!("[tensor-files] primary-paste-error load={error}");
            }
        }
    }

    pub(crate) fn queue_clipboard_clear(&mut self, reason: &'static str) {
        let Some(clipboard) = self.clipboard.as_ref() else {
            tensor_files_log!(
                "[tensor-files] clipboard-clear-error reason={reason} error=clipboard-unavailable"
            );
            return;
        };

        match clipboard.store_text_async(String::new()) {
            Ok(reply_rx) => {
                self.submit_operation_request(ShellOperationRequest::clipboard(
                    ShellClipboardWork::Clear { reason, reply_rx },
                ));
            }
            Err(error) => {
                tensor_files_log!(
                    "[tensor-files] clipboard-clear-error reason={reason} error={error}"
                );
            }
        }
    }

    pub(crate) fn apply_async_clipboard_completion(
        &mut self,
        completion: ShellAsyncClipboardCompletion,
    ) -> bool {
        match completion {
            ShellAsyncClipboardCompletion::StoreFile { request, result } => match result {
                Ok(()) => {
                    self.scene.record_file_clipboard_export(&request);
                    true
                }
                Err(error) => {
                    self.record_file_clipboard_store_error(&request, error);
                    true
                }
            },
            ShellAsyncClipboardCompletion::CopyLocation { request, result } => match result {
                Ok(()) => {
                    self.scene.record_copy_location(&request);
                    true
                }
                Err(error) => {
                    self.record_copy_location_error(&request, error);
                    true
                }
            },
            ShellAsyncClipboardCompletion::LoadPaste {
                use_context,
                privileged,
                result,
            } => match result {
                Ok(text) => self.finish_paste_from_clipboard_text(use_context, privileged, text),
                Err(error) => {
                    tensor_files_log!("[tensor-files] paste-error load={error}");
                    self.scene.record_task_status(ShellTaskStatus::failed(
                        "Paste failed",
                        format!("Clipboard read failed: {error}"),
                        false,
                    ));
                    true
                }
            },
            ShellAsyncClipboardCompletion::Clear { reason, result } => {
                if let Err(error) = result {
                    tensor_files_log!(
                        "[tensor-files] clipboard-clear-error reason={reason} error={error}"
                    );
                }
                false
            }
        }
    }

    fn record_file_clipboard_store_error(
        &mut self,
        request: &FileClipboardExportRequest,
        error: String,
    ) {
        tensor_files_log!(
            "[tensor-files] clipboard-export-error role={} paths={} error={error}",
            file_clipboard_role_as_str(request.role),
            request.paths.len()
        );
        self.scene.record_task_status(ShellTaskStatus::failed(
            "Clipboard failed",
            format!(
                "Could not store {}: {error}",
                paths_task_summary(&request.paths)
            ),
            false,
        ));
    }

    fn record_copy_location_error(&mut self, request: &CopyLocationRequest, error: String) {
        tensor_files_log!(
            "[tensor-files] copy-location-error path={} error={error}",
            request.path.display()
        );
        self.scene.record_task_status(ShellTaskStatus::failed(
            "Copy Location failed",
            format!("{}: {error}", request.path.display()),
            false,
        ));
    }
}
