impl TensorFilesApp {
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
            self.apply_action_outcome(
                event_loop,
                crate::app_actions::ShellActionOutcome::Present("async-task"),
            );
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

}
