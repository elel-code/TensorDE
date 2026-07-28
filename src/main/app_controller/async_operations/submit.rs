struct AsyncTransferRequest {
    source: ShellAsyncTransferSource,
    target_dir: PathBuf,
    mode: FileTransferMode,
    paths: Vec<PathBuf>,
    label: &'static str,
    clear_clipboard: bool,
    privileged: bool,
}

struct AsyncTransferTaskConfig<'a> {
    source: ShellAsyncTransferSource,
    target_dir: &'a Path,
    mode: FileTransferMode,
    item_count: usize,
    clear_clipboard: bool,
    privileged: bool,
}

struct AsyncTransferFailure {
    source: ShellAsyncTransferSource,
    target_dir: PathBuf,
    mode: FileTransferMode,
    label: &'static str,
    clear_clipboard: bool,
    privileged: bool,
}

impl FikaWgpuApp {
    /// Single entry point for typed operation requests from UI actions.
    fn submit_operation_request(&mut self, request: ShellOperationRequest) {
        match request {
            ShellOperationRequest::Transfer {
                source,
                target_dir,
                mode,
                paths,
                label,
                clear_clipboard,
                privileged,
            } => self.start_async_transfer_with_privilege(AsyncTransferRequest {
                source,
                target_dir,
                mode,
                paths,
                label,
                clear_clipboard,
                privileged,
            }),
            ShellOperationRequest::PasteText { target_dir, text } => {
                self.start_async_paste_text(target_dir, text);
            }
            ShellOperationRequest::MoveToTrash {
                paths,
                pane_to_reload,
                privileged,
                clear_selection_pane,
            } => self.start_async_move_to_trash_with_options(
                paths,
                pane_to_reload,
                privileged,
                clear_selection_pane,
            ),
            ShellOperationRequest::TrashView {
                action,
                operation,
                paths,
                pane_to_reload,
            } => self.start_async_trash_view_operation_with(
                action,
                operation,
                paths,
                pane_to_reload,
            ),
            ShellOperationRequest::Create { request } => self.start_async_create(request),
            ShellOperationRequest::Rename { request } => self.start_async_rename(request),
            ShellOperationRequest::Device { request } => self.start_async_device(request),
            ShellOperationRequest::Launch {
                kind,
                running_label,
                running_detail,
                work,
            } => self.start_async_launch_work(kind, running_label, running_detail, work),
            ShellOperationRequest::Navigation {
                generation,
                pane,
                source_path,
                target_path,
                history,
                reason,
            } => self.start_async_navigation(
                generation,
                pane,
                source_path,
                target_path,
                history,
                reason,
            ),
            ShellOperationRequest::Clipboard { work } => self.start_async_clipboard(work),
        }
    }

    fn start_async_navigation(
        &mut self,
        generation: u64,
        pane: ShellPaneId,
        source_path: PathBuf,
        target_path: PathBuf,
        history: ShellNavigationHistoryUpdate,
        reason: &'static str,
    ) {
        let listing_target = target_path.clone();
        let _ = self.spawn_blocking_task_result(
            move || read_shell_entries_sync(&listing_target),
            move |result| {
                ShellAsyncTaskResult::Navigation(ShellAsyncNavigationCompletion {
                    generation,
                    pane,
                    source_path,
                    target_path,
                    history,
                    reason,
                    result,
                })
            },
        );
    }

    fn start_async_clipboard(&mut self, work: ShellClipboardWork) {
        match work {
            ShellClipboardWork::StoreFile { request, reply_rx } => {
                if let Err(error) = self.spawn_blocking_task_result(
                    move || receive_clipboard_reply(reply_rx),
                    move |result| {
                        ShellAsyncTaskResult::Clipboard(ShellAsyncClipboardCompletion::StoreFile {
                            request,
                            result,
                        })
                    },
                ) {
                    fika_log!("[fika-wgpu] clipboard-reply-runtime-error error={error}");
                }
            }
            ShellClipboardWork::CopyLocation { request, reply_rx } => {
                if let Err(error) = self.spawn_blocking_task_result(
                    move || receive_clipboard_reply(reply_rx),
                    move |result| {
                        ShellAsyncTaskResult::Clipboard(
                            ShellAsyncClipboardCompletion::CopyLocation { request, result },
                        )
                    },
                ) {
                    fika_log!("[fika-wgpu] clipboard-reply-runtime-error error={error}");
                }
            }
            ShellClipboardWork::LoadPaste {
                use_context,
                privileged,
                reply_rx,
            } => {
                if let Err(error) = self.spawn_blocking_task_result(
                    move || receive_clipboard_reply(reply_rx),
                    move |result| {
                        ShellAsyncTaskResult::Clipboard(ShellAsyncClipboardCompletion::LoadPaste {
                            use_context,
                            privileged,
                            result,
                        })
                    },
                ) {
                    fika_log!("[fika-wgpu] clipboard-reply-runtime-error error={error}");
                }
            }
            ShellClipboardWork::Clear { reason, reply_rx } => {
                if let Err(error) = self.spawn_blocking_task_result(
                    move || receive_clipboard_reply(reply_rx),
                    move |result| {
                        ShellAsyncTaskResult::Clipboard(ShellAsyncClipboardCompletion::Clear {
                            reason,
                            result,
                        })
                    },
                ) {
                    fika_log!("[fika-wgpu] clipboard-reply-runtime-error error={error}");
                }
            }
        }
    }

    fn start_async_create(&mut self, request: CreateEntryRequest) {
        let privileged = request.privileged;
        let request_for_task = request.clone();
        if let Err(error) = self.spawn_async_task_result(
            move || async move {
                crate::shell::create_rename::disk::create_entry_on_disk_explicit_async(
                    request_for_task,
                )
                .await
            },
            move |outcome| {
                ShellAsyncTaskResult::Create(ShellAsyncCreateCompletion { request, outcome })
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

    fn start_async_rename(&mut self, request: RenameEntryRequest) {
        let privileged = request.privileged;
        let request_for_task = request.clone();
        if let Err(error) = self.spawn_async_task_result(
            move || async move {
                crate::shell::create_rename::disk::rename_entry_on_disk_explicit_async(
                    request_for_task,
                )
                .await
            },
            move |outcome| {
                ShellAsyncTaskResult::Rename(ShellAsyncRenameCompletion { request, outcome })
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

    fn start_async_device(&mut self, request: DeviceActionRequest) {
        let action_label = request.action.label();
        let action_name = request.action.as_str();
        let request_for_task = request.clone();
        if let Err(error) = self.spawn_async_task_result(
            move || async move {
                perform_device_place_operation(
                    WGPU_SHELL_PANE_ID,
                    request_for_task.id.clone(),
                    request_for_task.label.clone(),
                    request_for_task.operation,
                )
                .await
            },
            move |result| {
                ShellAsyncTaskResult::Device(ShellAsyncDeviceCompletion { request, result })
            },
        ) {
            fika_log!("[fika-wgpu] device-action-runtime-error action={action_name} error={error}");
            self.scene.record_task_status(ShellTaskStatus::failed(
                format!("{action_label} failed"),
                error.to_string(),
                false,
            ));
        }
    }

    fn start_async_launch_work(
        &mut self,
        kind: ShellAsyncLaunchKind,
        running_label: String,
        running_detail: String,
        work: ShellLaunchWork,
    ) {
        self.start_async_launch_task(kind, running_label, running_detail, move || async move {
            match work {
                ShellLaunchWork::Systemd {
                    plan,
                    path,
                    app_name,
                    target_label,
                } => match kind {
                    ShellAsyncLaunchKind::OpenFile => {
                        match launch_with_systemd_user(plan).await {
                            Ok(result) => {
                                let message = format!(
                                    "Opened {} with {} via {} systemd unit(s)",
                                    path.display(),
                                    app_name,
                                    result.units.len()
                                );
                                fika_log!(
                                    "[fika-wgpu] open-finished path={} app={:?} units={}",
                                    path.display(),
                                    app_name,
                                    result.units.join(",")
                                );
                                (true, message)
                            }
                            Err(error) => {
                                let message = format!(
                                    "Cannot open {} with {}: {error}",
                                    path.display(),
                                    app_name
                                );
                                fika_log!(
                                    "[fika-wgpu] open-finished path={} app={:?} error={error}",
                                    path.display(),
                                    app_name
                                );
                                (false, message)
                            }
                        }
                    }
                    ShellAsyncLaunchKind::OpenWith => {
                        let result = launch_with_systemd_user(plan).await;
                        let success = result.is_ok();
                        let status = OpenWithLaunchResult {
                            pane_id: WGPU_SHELL_PANE_ID,
                            path,
                            app_name,
                            result,
                        }
                        .status_message();
                        fika_log!("[fika-wgpu] open-with-finished {status}");
                        (success, status)
                    }
                    ShellAsyncLaunchKind::ServiceMenu => {
                        let result = launch_with_systemd_user(plan).await;
                        let success = result.is_ok();
                        let status = ServiceMenuLaunchResult {
                            pane_id: WGPU_SHELL_PANE_ID,
                            target_label: target_label.unwrap_or_else(|| path.display().to_string()),
                            app_name,
                            result,
                        }
                        .status_message();
                        fika_log!("[fika-wgpu] service-menu-finished {status}");
                        (success, status)
                    }
                    ShellAsyncLaunchKind::ArkExtractAndTrash => {
                        unreachable!("ark extract work uses ShellLaunchWork::ArkExtractAndTrash")
                    }
                },
                ShellLaunchWork::ArkExtractAndTrash { request } => {
                    let paths = request.paths.clone();
                    let app_name = request.app_name.clone();
                    let target_label = service_menu_target_label(&paths);
                    match crate::shell::ark::extract::execute_ark_extract_and_trash(request).await {
                        Ok(message) => {
                            let status = format!("Ran {app_name} for {target_label}: {message}");
                            fika_log!("[fika-wgpu] service-menu-finished {status}");
                            (true, status)
                        }
                        Err(err) => {
                            let status =
                                format!("Cannot run {app_name} for {target_label}: {err}");
                            fika_log!("[fika-wgpu] service-menu-finished {status}");
                            (false, status)
                        }
                    }
                }
            }
        });
    }

    fn start_async_transfer_with_privilege(&mut self, request: AsyncTransferRequest) {
        let AsyncTransferRequest {
            source,
            target_dir,
            mode,
            paths,
            label,
            clear_clipboard,
            privileged,
        } = request;
        let controller = OperationController::new();
        let task_id = self.begin_async_transfer_task(
            AsyncTransferTaskConfig {
                source,
                target_dir: &target_dir,
                mode,
                item_count: paths.len(),
                clear_clipboard,
                privileged,
            },
            controller.clone(),
        );
        let failure = AsyncTransferFailure {
            source,
            target_dir: target_dir.clone(),
            mode,
            label,
            clear_clipboard,
            privileged,
        };
        let work_target = target_dir.clone();
        if let Err(error) = self.spawn_async_transfer_completion(
            task_id,
            source,
            target_dir.clone(),
            move || async move {
                transfer_paths_async_with_controller_and_privilege(
                    work_target,
                    mode,
                    paths,
                    label,
                    clear_clipboard,
                    controller,
                    privileged,
                )
                .await
            },
        ) {
            self.fail_async_transfer_spawn(task_id, failure, error);
        }
    }

    fn start_async_paste_text(&mut self, target_dir: PathBuf, text: String) {
        let controller = OperationController::new();
        let task_id = self.begin_async_transfer_task(
            AsyncTransferTaskConfig {
                source: ShellAsyncTransferSource::Paste,
                target_dir: &target_dir,
                mode: FileTransferMode::Copy,
                item_count: 1,
                clear_clipboard: false,
                privileged: false,
            },
            controller,
        );
        let work_target = target_dir.clone();
        if let Err(error) = self.spawn_async_transfer_completion(
            task_id,
            ShellAsyncTransferSource::Paste,
            target_dir.clone(),
            move || async move { paste_text_async(work_target, text).await },
        ) {
            self.fail_async_transfer_spawn(
                task_id,
                AsyncTransferFailure {
                    source: ShellAsyncTransferSource::Paste,
                    target_dir,
                    mode: FileTransferMode::Copy,
                    label: "Paste",
                    clear_clipboard: false,
                    privileged: false,
                },
                error,
            );
        }
    }

    fn begin_async_transfer_task(
        &mut self,
        config: AsyncTransferTaskConfig<'_>,
        controller: OperationController,
    ) -> ShellTaskId {
        let AsyncTransferTaskConfig {
            source,
            target_dir,
            mode,
            item_count,
            clear_clipboard,
            privileged,
        } = config;
        let task_id = self.register_active_task(controller);
        let base_detail = async_transfer_task_detail(target_dir, item_count, clear_clipboard);
        self.active_task_base_details
            .insert(task_id, base_detail.clone());
        self.scene.record_async_transfer_started(
            task_id,
            source,
            mode,
            item_count,
            base_detail,
            privileged,
        );
        task_id
    }

    fn spawn_async_transfer_completion<F, Fut>(
        &self,
        task_id: ShellTaskId,
        source: ShellAsyncTransferSource,
        target_dir: PathBuf,
        task: F,
    ) -> Result<(), OperationRuntimeError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ShellTransferExecution> + 'static,
    {
        self.spawn_async_task_result(task, move |transfer| {
            ShellAsyncTaskResult::Transfer(ShellAsyncTransferCompletion {
                task_id,
                source,
                target_dir,
                transfer,
            })
        })
    }

    fn fail_async_transfer_spawn(
        &mut self,
        task_id: ShellTaskId,
        failure: AsyncTransferFailure,
        error: impl std::fmt::Display,
    ) {
        let AsyncTransferFailure {
            source,
            target_dir,
            mode,
            label,
            clear_clipboard,
            privileged,
        } = failure;
        let mut transfer =
            transfer_runtime_failure(target_dir.clone(), mode, label, clear_clipboard, error);
        transfer.privileged = privileged;
        self.post_async_task_result(ShellAsyncTaskResult::Transfer(
            ShellAsyncTransferCompletion {
                task_id,
                source,
                target_dir,
                transfer,
            },
        ));
    }

    fn start_async_move_to_trash_with_options(
        &mut self,
        paths: Vec<PathBuf>,
        pane_to_reload: ShellPaneId,
        privileged: bool,
        clear_selection_pane: Option<ShellPaneId>,
    ) {
        let task_id = self.register_active_task(OperationController::new());
        self.scene
            .record_async_move_to_trash_started(task_id, paths.len(), privileged);
        let paths_for_task = paths.clone();
        if let Err(error) = self.spawn_async_task_result(
            move || async move {
                trash_paths_async_with_privilege(paths_for_task, privileged).await
            },
            move |result| {
                ShellAsyncTaskResult::MoveToTrash(ShellAsyncMoveToTrashCompletion {
                    task_id,
                    pane_to_reload,
                    clear_selection_pane,
                    paths,
                    privileged,
                    result,
                })
            },
        ) {
            fika_log!("[fika-wgpu] move-to-trash-runtime-error {error}");
            self.forget_active_task(task_id);
            self.scene.record_task_status(ShellTaskStatus::failed(
                if privileged {
                    "Administrator move to Trash failed"
                } else {
                    "Move to Trash failed"
                },
                error.to_string(),
                privileged,
            ));
        }
    }

    fn start_async_trash_view_operation_with(
        &mut self,
        action: ShellContextMenuAction,
        operation: TrashViewOperation,
        paths: Vec<PathBuf>,
        pane_to_reload: ShellPaneId,
    ) {
        let task_id = self.register_active_task(OperationController::new());
        self.scene
            .record_async_trash_view_started(task_id, operation, paths.len());

        if let Err(error) = self.spawn_async_task_result(
            move || async move {
                trash_view_operation_result_async(WGPU_SHELL_PANE_ID, operation, paths).await
            },
            move |result| {
                ShellAsyncTaskResult::TrashView(ShellAsyncTrashViewCompletion {
                    task_id,
                    action,
                    pane_to_reload,
                    result,
                })
            },
        ) {
            fika_log!(
                "[fika-wgpu] trash-view-runtime-error action={} {error}",
                action.as_str()
            );
            self.post_async_task_result(ShellAsyncTaskResult::TrashView(
                ShellAsyncTrashViewCompletion {
                    task_id,
                    action,
                    pane_to_reload,
                    result: trash_view_operation_runtime_failure(operation),
                },
            ));
        }
    }
}
