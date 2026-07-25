impl FikaWgpuApp {
    fn next_task_id(&mut self) -> ShellTaskId {
        let task_id = self.next_task_id;
        self.next_task_id = self.next_task_id.saturating_add(1).max(1);
        task_id
    }

    /// Deliver a completion on the UI event-loop thread via the shared channel.
    fn post_async_task_result(&self, result: ShellAsyncTaskResult) {
        if self.async_task_tx.send(result).is_ok() {
            self.event_loop_proxy.wake_up();
        }
    }

    /// Spawn a Compio task and post its completion through the shared channel.
    fn spawn_async_task_result<F, Fut>(
        &self,
        task: F,
        map: impl FnOnce(Fut::Output) -> ShellAsyncTaskResult + Send + 'static,
    ) -> Result<(), OperationRuntimeError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future + 'static,
        Fut::Output: Send + 'static,
    {
        let tx = self.async_task_tx.clone();
        let proxy = self.event_loop_proxy.clone();
        spawn_operation_task_with_completion(task, move |output| {
            if tx.send(map(output)).is_ok() {
                proxy.wake_up();
            }
        })
    }

    /// Spawn blocking work and post its completion through the shared channel.
    fn spawn_blocking_task_result<F, T>(
        &self,
        task: F,
        map: impl FnOnce(T) -> ShellAsyncTaskResult + Send + 'static,
    ) -> Result<(), OperationRuntimeError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let tx = self.async_task_tx.clone();
        let proxy = self.event_loop_proxy.clone();
        spawn_blocking_operation_with_completion(task, move |output| {
            if tx.send(map(output)).is_ok() {
                proxy.wake_up();
            }
        })
    }

    fn drain_async_task_results(&mut self, event_loop: &ActiveEventLoop) {
        let mut changed = false;
        while let Ok(result) = self.async_task_rx.try_recv() {
            match result {
                ShellAsyncTaskResult::Navigation(completion) => {
                    let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
                        continue;
                    };
                    changed |= self.apply_async_navigation_completion(completion, size);
                }
                ShellAsyncTaskResult::Transfer(completion) => {
                    self.forget_active_task(completion.task_id);
                    let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
                        continue;
                    };
                    let clear_clipboard = completion.transfer.result.clear_clipboard
                        && completion.transfer.result.failure_count == 0;
                    let apply_result = self
                        .scene
                        .apply_async_transfer_completion(&completion, size);
                    match apply_result {
                        Ok(result) => {
                            changed = true;
                            if clear_clipboard && result.changed() {
                                self.queue_clipboard_clear("paste-transfer");
                            }
                        }
                        Err(error) => {
                            self.scene.record_task_status(ShellTaskStatus::failed(
                                "Task update failed",
                                error,
                                completion.transfer.privileged,
                            ));
                            changed = true;
                        }
                    }
                }
                ShellAsyncTaskResult::TrashView(completion) => {
                    self.forget_active_task(completion.task_id);
                    let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
                        continue;
                    };
                    match self
                        .scene
                        .apply_async_trash_view_completion(&completion, size)
                    {
                        Ok(()) => {
                            changed = true;
                        }
                        Err(error) => {
                            self.scene.finish_task_status(
                                completion.task_id,
                                ShellTaskStatus::failed("Task update failed", error, false),
                            );
                            changed = true;
                        }
                    }
                }
                ShellAsyncTaskResult::MoveToTrash(completion) => {
                    self.forget_active_task(completion.task_id);
                    let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
                        continue;
                    };
                    match self
                        .scene
                        .apply_async_move_to_trash_completion(&completion, size)
                    {
                        Ok(_) => {
                            changed = true;
                        }
                        Err(error) => {
                            self.scene.finish_task_status(
                                completion.task_id,
                                ShellTaskStatus::failed(
                                    "Task update failed",
                                    error,
                                    completion.privileged,
                                ),
                            );
                            changed = true;
                        }
                    }
                }
                ShellAsyncTaskResult::Clipboard(completion) => {
                    changed |= self.apply_async_clipboard_completion(completion);
                }
                ShellAsyncTaskResult::Create(completion) => {
                    let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
                        continue;
                    };
                    changed |= self.apply_async_create_completion(completion, size);
                }
                ShellAsyncTaskResult::Rename(completion) => {
                    let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
                        continue;
                    };
                    changed |= self.apply_async_rename_completion(completion, size);
                }
                ShellAsyncTaskResult::Device(completion) => {
                    changed |= self.apply_async_device_completion(event_loop, completion);
                }
                ShellAsyncTaskResult::Launch(completion) => {
                    self.forget_active_task(completion.task_id);
                    changed |= self.apply_async_launch_completion(completion);
                }
            }
        }
        if changed {
            self.present_scene_change(event_loop, "async-task");
        }
    }

    fn apply_async_launch_completion(&mut self, completion: ShellAsyncLaunchCompletion) -> bool {
        let status = if completion.success {
            ShellTaskStatus::completed(
                completion.kind.success_label(),
                completion.status_message,
                false,
            )
        } else {
            ShellTaskStatus::failed(
                completion.kind.failure_label(),
                completion.status_message,
                false,
            )
        };
        self.scene.finish_task_status(completion.task_id, status);
        true
    }

    fn start_async_launch_task<F, Fut>(
        &mut self,
        kind: ShellAsyncLaunchKind,
        running_label: impl Into<String>,
        running_detail: impl Into<String>,
        task: F,
    ) where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = (bool, String)> + 'static,
    {
        let task_id = self.register_active_task(OperationController::new());
        self.scene.record_task_status(ShellTaskStatus::running(
            task_id,
            running_label,
            running_detail,
            false,
        ));
        if let Err(error) = self.spawn_async_task_result(task, move |(success, status_message)| {
            ShellAsyncTaskResult::Launch(ShellAsyncLaunchCompletion {
                task_id,
                kind,
                status_message,
                success,
            })
        }) {
            self.forget_active_task(task_id);
            self.scene.record_task_status(ShellTaskStatus::failed(
                kind.failure_label(),
                error.to_string(),
                false,
            ));
        }
    }

    fn refresh_active_task_progress(&mut self) -> bool {
        let mut changed = false;
        for (task_id, controller) in &self.active_task_controllers {
            let progress = controller.progress();
            if progress.bytes_total == 0 {
                continue;
            }
            let Some(base_detail) = self.active_task_base_details.get(task_id) else {
                continue;
            };
            let percentage = progress
                .bytes_done
                .saturating_mul(100)
                .checked_div(progress.bytes_total)
                .unwrap_or_default()
                .min(100);
            let detail = format!(
                "{} | {} / {} ({}%)",
                base_detail,
                format_size(progress.bytes_done.min(progress.bytes_total)),
                format_size(progress.bytes_total),
                percentage
            );
            changed |= self.scene.update_running_task_detail(*task_id, detail);
        }
        changed
    }

    fn cancel_task_if_running(&mut self, task_id: ShellTaskId) {
        if let Some(controller) = self.active_task_controllers.get(&task_id) {
            controller.cancel();
        }
    }

    fn forget_active_task(&mut self, task_id: ShellTaskId) {
        self.active_task_controllers.remove(&task_id);
        self.active_task_base_details.remove(&task_id);
    }

    fn register_active_task(
        &mut self,
        controller: OperationController,
    ) -> ShellTaskId {
        let task_id = self.next_task_id();
        self.active_task_controllers.insert(task_id, controller);
        task_id
    }

    /// Single entry point for typed file-operation requests from UI actions.
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
            } => self.start_async_transfer_with_privilege(
                source,
                target_dir,
                mode,
                paths,
                label,
                clear_clipboard,
                privileged,
            ),
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
        }
    }

    fn start_async_transfer_with_privilege(
        &mut self,
        source: ShellAsyncTransferSource,
        target_dir: PathBuf,
        mode: FileTransferMode,
        paths: Vec<PathBuf>,
        label: &'static str,
        clear_clipboard: bool,
        privileged: bool,
    ) {
        let controller = OperationController::new();
        let task_id = self.begin_async_transfer_task(
            source,
            &target_dir,
            mode,
            paths.len(),
            clear_clipboard,
            privileged,
            controller.clone(),
        );
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
            self.fail_async_transfer_spawn(
                task_id,
                source,
                target_dir,
                mode,
                label,
                clear_clipboard,
                privileged,
                error,
            );
        }
    }

    fn start_async_paste_text(&mut self, target_dir: PathBuf, text: String) {
        let controller = OperationController::new();
        let task_id = self.begin_async_transfer_task(
            ShellAsyncTransferSource::Paste,
            &target_dir,
            FileTransferMode::Copy,
            1,
            false,
            false,
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
                ShellAsyncTransferSource::Paste,
                target_dir,
                FileTransferMode::Copy,
                "Paste",
                false,
                false,
                error,
            );
        }
    }

    fn begin_async_transfer_task(
        &mut self,
        source: ShellAsyncTransferSource,
        target_dir: &Path,
        mode: FileTransferMode,
        item_count: usize,
        clear_clipboard: bool,
        privileged: bool,
        controller: OperationController,
    ) -> ShellTaskId {
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
        source: ShellAsyncTransferSource,
        target_dir: PathBuf,
        mode: FileTransferMode,
        label: &'static str,
        clear_clipboard: bool,
        privileged: bool,
        error: impl std::fmt::Display,
    ) {
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

    fn start_async_trash_view_operation(
        &mut self,
        action: ShellContextMenuAction,
    ) -> Result<(), String> {
        let pane_to_reload = self
            .scene
            .context_target_pane()
            .unwrap_or_else(|| self.scene.active_pane());
        let (operation, paths) = self.scene.context_target_trash_view_operation(action)?;
        self.submit_operation_request(ShellOperationRequest::trash_view(
            action,
            operation,
            paths,
            pane_to_reload,
        ));
        Ok(())
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
            {
                let paths = paths;
                move || async move {
                    trash_view_operation_result_async(WGPU_SHELL_PANE_ID, operation, paths).await
                }
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
