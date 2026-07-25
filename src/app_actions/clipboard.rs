use std::io::Result as IoResult;
use std::sync::mpsc::Receiver;

use crate::shell::clipboard::FileClipboardExportRequest;
use crate::shell::tasks::ShellTaskStatus;
use crate::shell::transfer::{ShellAsyncClipboardCompletion, ShellAsyncTaskResult};
use crate::{CopyLocationRequest, FikaWgpuApp, file_clipboard_role_as_str, paths_task_summary};

fn receive_clipboard_result<T>(reply_rx: Receiver<IoResult<T>>) -> Result<T, String> {
    reply_rx
        .recv()
        .map_err(|_| "clipboard worker stopped before replying".to_string())?
        .map_err(|error| error.to_string())
}

impl FikaWgpuApp {
    pub(crate) fn store_file_clipboard_request(&mut self, request: &FileClipboardExportRequest) {
        if let Some(clipboard) = self.clipboard.as_ref() {
            match clipboard.store_file_clipboard_async(
                request.role,
                request.paths.clone(),
                request.text.clone(),
            ) {
                Ok(reply_rx) => {
                    let request = request.clone();
                    self.spawn_clipboard_reply(reply_rx, move |result| {
                        ShellAsyncClipboardCompletion::StoreFile { request, result }
                    });
                }
                Err(error) => self.record_file_clipboard_store_error(request, error),
            }
        } else {
            fika_log!(
                "[fika-wgpu] clipboard-export-error role={} paths={} error=clipboard-unavailable",
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
            fika_log!(
                "[fika-wgpu] copy-location-error path={} error=clipboard-unavailable",
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
                self.spawn_clipboard_reply(reply_rx, move |result| {
                    ShellAsyncClipboardCompletion::CopyLocation { request, result }
                });
            }
            Err(error) => self.record_copy_location_error(&request, error),
        }
    }

    pub(crate) fn load_clipboard_text_for_paste(&mut self, use_context: bool, privileged: bool) {
        let Some(clipboard) = self.clipboard.as_ref() else {
            fika_log!("[fika-wgpu] paste-error error=clipboard-unavailable");
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Paste failed",
                "Clipboard is unavailable",
                false,
            ));
            return;
        };

        match clipboard.load_text_async() {
            Ok(reply_rx) => {
                self.spawn_clipboard_reply(reply_rx, move |result| {
                    ShellAsyncClipboardCompletion::LoadPaste {
                        use_context,
                        privileged,
                        result,
                    }
                });
            }
            Err(error) => {
                fika_log!("[fika-wgpu] paste-error load={error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Paste failed",
                    format!("Clipboard read failed: {error}"),
                    false,
                ));
            }
        }
    }

    pub(crate) fn queue_clipboard_clear(&mut self, reason: &'static str) {
        let Some(clipboard) = self.clipboard.as_ref() else {
            fika_log!(
                "[fika-wgpu] clipboard-clear-error reason={reason} error=clipboard-unavailable"
            );
            return;
        };

        match clipboard.store_text_async(String::new()) {
            Ok(reply_rx) => {
                self.spawn_clipboard_reply(reply_rx, move |result| {
                    ShellAsyncClipboardCompletion::Clear { reason, result }
                });
            }
            Err(error) => {
                fika_log!("[fika-wgpu] clipboard-clear-error reason={reason} error={error}");
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
                    fika_log!("[fika-wgpu] paste-error load={error}");
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
                    fika_log!("[fika-wgpu] clipboard-clear-error reason={reason} error={error}");
                }
                false
            }
        }
    }

    pub(crate) fn spawn_clipboard_reply<T, F>(&self, reply_rx: Receiver<IoResult<T>>, map: F)
    where
        T: Send + 'static,
        F: FnOnce(Result<T, String>) -> ShellAsyncClipboardCompletion + Send + 'static,
    {
        if let Err(error) = self.spawn_blocking_task_result(
            move || receive_clipboard_result(reply_rx),
            move |result| ShellAsyncTaskResult::Clipboard(map(result)),
        ) {
            fika_log!("[fika-wgpu] clipboard-reply-runtime-error error={error}");
        }
    }

    fn record_file_clipboard_store_error(
        &mut self,
        request: &FileClipboardExportRequest,
        error: String,
    ) {
        fika_log!(
            "[fika-wgpu] clipboard-export-error role={} paths={} error={error}",
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
        fika_log!(
            "[fika-wgpu] copy-location-error path={} error={error}",
            request.path.display()
        );
        self.scene.record_task_status(ShellTaskStatus::failed(
            "Copy Location failed",
            format!("{}: {error}", request.path.display()),
            false,
        ));
    }
}
