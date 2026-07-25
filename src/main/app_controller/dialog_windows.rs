impl FikaWgpuApp {

    fn ensure_open_with_dialog_window(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(spec) = self.open_with_dialog_spec() else {
            self.close_open_with_dialog_window();
            return false;
        };
        if !self.ensure_dialog_window(event_loop, ShellDialogWindowKind::OpenWith, &spec) {
            if self.scene.close_open_with_chooser() {
                self.request_main_redraw();
            }
            return false;
        }
        true
    }

    fn sync_open_with_dialog_window(&mut self) {
        let Some(spec) = self.open_with_dialog_spec() else {
            self.close_open_with_dialog_window();
            return;
        };
        self.sync_dialog_window(ShellDialogWindowKind::OpenWith, &spec);
    }

    fn close_open_with_dialog_window(&mut self) {
        self.close_dialog_window(ShellDialogWindowKind::OpenWith);
    }

    fn finish_open_with_dialog_state_change(&mut self) {
        if self.scene.is_open_with_chooser_open() {
            if self.dialog_windows.is_open(ShellDialogWindowKind::OpenWith) {
                self.sync_open_with_dialog_window();
            } else {
                if self.scene.close_open_with_chooser() {
                    self.request_main_redraw();
                }
            }
        } else {
            self.close_open_with_dialog_window();
            self.request_main_redraw();
        }
    }

    fn reconcile_open_with_dialog_lifecycle(&mut self) {
        if !self.dialog_windows.is_open(ShellDialogWindowKind::OpenWith) {
            return;
        }
        if !self.scene.is_open_with_chooser_open() {
            self.close_open_with_dialog_window();
        }
    }

    fn reconcile_dialog_window_lifecycle(&mut self) {
        if self.dialog_windows.is_open(ShellDialogWindowKind::Create)
            && !self.scene.is_create_dialog_open()
        {
            self.close_create_dialog_window();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::Rename)
            && !self.scene.is_rename_dialog_open()
        {
            self.close_rename_dialog_window();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::Properties)
            && !self.scene.is_properties_overlay_open()
        {
            self.close_properties_dialog_window();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::TaskDetail)
            && !self.scene.is_task_detail_dialog_open()
        {
            self.close_task_detail_dialog_window();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::TrashConflict)
            && !self.scene.is_trash_conflict_dialog_open()
        {
            self.close_trash_conflict_dialog_window();
        }
        self.reconcile_open_with_dialog_lifecycle();
    }

    fn drive_directory_watchers(&mut self, event_loop: &ActiveEventLoop) {
        self.directory_watchers.sync_with_scene(&self.scene);
        self.directory_watchers.drain_events(&self.scene);

        let now = Instant::now();
        let reload_paths = self.directory_watchers.take_due_reload_paths(now);
        if reload_paths.is_empty() {
            return;
        }

        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            self.directory_watchers.defer_reload_paths(reload_paths);
            return;
        };

        let mut changed = false;
        for path in reload_paths {
            match self.scene.reload_panes_showing_path(&path, size) {
                Ok(reloaded) => changed |= reloaded,
                Err(error) => {
                    fika_log!(
                        "[fika-wgpu] directory-watch-reload-error path={} error={error}",
                        path.display()
                    );
                }
            }
        }
        self.directory_watchers.sync_with_scene(&self.scene);
        if changed {
            self.present_scene_change(event_loop, "directory-watch");
        }
    }

    fn toggle_user_dark_mode(&mut self) -> bool {
        self.scene.toggle_dark_mode();
        if let Err(error) = save_dark_mode_setting(&self.settings_path, self.scene.dark_mode) {
            fika_log!("[fika-wgpu] settings-save-error {error}");
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::Create) {
            self.sync_create_dialog_window();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::Rename) {
            self.sync_rename_dialog_window();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::OpenWith) {
            self.sync_open_with_dialog_window();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::Properties) {
            self.sync_properties_dialog_window();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::Settings) {
            self.sync_settings_dialog_window();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::TaskDetail) {
            self.sync_task_detail_dialog_window();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::TrashConflict) {
            self.sync_trash_conflict_dialog_window();
        }
        true
    }

    fn next_task_id(&mut self) -> ShellTaskId {
        let task_id = self.next_task_id;
        self.next_task_id = self.next_task_id.saturating_add(1).max(1);
        task_id
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
                    self.active_task_controllers.remove(&completion.task_id);
                    self.active_task_base_details.remove(&completion.task_id);
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
                    self.active_task_controllers.remove(&completion.task_id);
                    self.active_task_base_details.remove(&completion.task_id);
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
                    self.active_task_controllers.remove(&completion.task_id);
                    self.active_task_base_details.remove(&completion.task_id);
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
            }
        }
        if changed {
            self.present_scene_change(event_loop, "async-task");
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

    fn autosmoke_work_pending(&self) -> bool {
        !self.autosmoke_zoom_actions.is_empty()
            || !self.autosmoke_scroll_actions.is_empty()
            || self
                .dialog_lifecycle_smoke
                .is_some_and(DialogLifecycleSmoke::pending)
    }

    fn drive_autosmoke_after_render(&mut self) {
        let Some((size, frame_count)) = self
            .renderer
            .as_ref()
            .map(|renderer| (renderer.size, renderer.frame_count))
        else {
            return;
        };
        if frame_count == 0 {
            return;
        }
        self.drive_autosmoke_zoom(size);
        self.drive_autosmoke_scroll(size);
        if self.autosmoke_work_pending()
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }
    }

    fn drive_dialog_lifecycle_autosmoke(&mut self, event_loop: &ActiveEventLoop) {
        let Some(smoke_state) = self.dialog_lifecycle_smoke else {
            return;
        };
        let kind = smoke_state.kind;
        match smoke_state.step {
            DialogLifecycleSmokeStep::WaitMainFrame => {
                let Some(frame_count) = self.renderer.as_ref().map(|renderer| renderer.frame_count)
                else {
                    return;
                };
                if frame_count == 0 {
                    return;
                }
                if !self.open_dialog_for_autosmoke(kind) {
                    self.finish_dialog_lifecycle_autosmoke(false, event_loop);
                    return;
                }
                if !self.ensure_dialog_window_for_autosmoke_kind(kind, event_loop) {
                    self.finish_dialog_lifecycle_autosmoke(false, event_loop);
                    return;
                }
                fika_log!(
                    "[fika-wgpu] dialog-smoke open kind={} main_frame={}",
                    kind.as_str(),
                    frame_count
                );
                if let Some(smoke) = self.dialog_lifecycle_smoke.as_mut() {
                    smoke.step = DialogLifecycleSmokeStep::WaitDialogFrame;
                }
                self.request_dialog_redraw(kind);
            }
            DialogLifecycleSmokeStep::WaitDialogFrame => {
                let frame_count = self.dialog_windows.frame_count(kind).unwrap_or(0);
                if frame_count == 0 {
                    self.request_dialog_redraw(kind);
                    return;
                }
                let close_frame = self
                    .renderer
                    .as_ref()
                    .map(|renderer| renderer.frame_count)
                    .unwrap_or(0);
                fika_log!(
                    "[fika-wgpu] dialog-smoke close kind={} dialog_frame={} main_frame={}",
                    kind.as_str(),
                    frame_count,
                    close_frame
                );
                self.handle_common_dialog_window_event(kind, &WindowEvent::CloseRequested);
                if let Some(smoke) = self.dialog_lifecycle_smoke.as_mut() {
                    smoke.step = DialogLifecycleSmokeStep::WaitMainFrameAfterClose;
                    smoke.close_frame = close_frame;
                }
                self.request_main_redraw();
            }
            DialogLifecycleSmokeStep::WaitMainFrameAfterClose => {
                let Some(smoke) = self.dialog_lifecycle_smoke else {
                    return;
                };
                let Some(frame_count) = self.renderer.as_ref().map(|renderer| renderer.frame_count)
                else {
                    self.finish_dialog_lifecycle_autosmoke(false, event_loop);
                    return;
                };
                if self.dialog_windows.has_open_window() || frame_count < smoke.close_frame {
                    self.request_main_redraw();
                    return;
                }
                if smoke.cycles_remaining > 1 {
                    if let Some(smoke) = self.dialog_lifecycle_smoke.as_mut() {
                        smoke.cycles_remaining -= 1;
                        smoke.step = DialogLifecycleSmokeStep::WaitMainFrame;
                    }
                    self.request_main_redraw();
                    return;
                }
                self.finish_dialog_lifecycle_autosmoke(true, event_loop);
            }
            DialogLifecycleSmokeStep::Complete | DialogLifecycleSmokeStep::Failed => {}
        }
    }

    fn open_dialog_for_autosmoke(&mut self, kind: ShellDialogWindowKind) -> bool {
        match kind {
            ShellDialogWindowKind::Create => self.scene.open_create_dialog_for_autosmoke(),
            ShellDialogWindowKind::OpenWith => self
                .scene
                .open_open_with_chooser_for_autosmoke(&self.mime_applications),
            ShellDialogWindowKind::Rename => self.scene.open_rename_dialog_for_autosmoke(),
            ShellDialogWindowKind::Settings => true,
            ShellDialogWindowKind::Properties
            | ShellDialogWindowKind::TaskDetail
            | ShellDialogWindowKind::TrashConflict => false,
        }
    }

    fn ensure_dialog_window_for_autosmoke_kind(
        &mut self,
        kind: ShellDialogWindowKind,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        match kind {
            ShellDialogWindowKind::Create => self.ensure_create_dialog_window(event_loop),
            ShellDialogWindowKind::OpenWith => self.ensure_open_with_dialog_window(event_loop),
            ShellDialogWindowKind::Rename => self.ensure_rename_dialog_window(event_loop),
            ShellDialogWindowKind::Properties => {
                self.ensure_properties_dialog_window(event_loop)
            }
            ShellDialogWindowKind::Settings => self.ensure_settings_dialog_window(event_loop),
            ShellDialogWindowKind::TaskDetail => {
                self.ensure_task_detail_dialog_window(event_loop)
            }
            ShellDialogWindowKind::TrashConflict => {
                self.ensure_trash_conflict_dialog_window(event_loop)
            }
        }
    }

    fn finish_dialog_lifecycle_autosmoke(
        &mut self,
        success: bool,
        event_loop: &ActiveEventLoop,
    ) {
        let Some(()) = self.dialog_lifecycle_smoke.as_mut().map(|smoke| {
            smoke.step = if success {
                DialogLifecycleSmokeStep::Complete
            } else {
                DialogLifecycleSmokeStep::Failed
            };
            event_loop.set_control_flow(ControlFlow::Wait);
        }) else {
            return;
        };
        fika_log!(
            "[fika-wgpu] dialog-smoke {} main_open={} dialogs_open={}",
            if success { "complete" } else { "failed" },
            self.window.is_some() as u8,
            self.dialog_windows.has_open_window() as u8,
        );
    }

    fn drive_autosmoke_zoom(&mut self, size: PhysicalSize<u32>) {
        if !(self.autosmoke_zoom_allow_pending_redraw || self.pending_redraw_frames == 0)
            || Instant::now() < self.next_autosmoke_zoom
        {
            return;
        }
        let Some(action) = self.autosmoke_zoom_actions.pop_front() else {
            return;
        };
        if self.scene.zoom(action, size) {
            fika_log!("[fika-wgpu] autosmoke-zoom action={}", action.as_str());
            self.next_autosmoke_zoom = Instant::now() + self.autosmoke_zoom_interval;
            self.queue_scene_change("autosmoke-zoom", ZOOM_REDRAW_FRAMES);
        } else {
            self.next_autosmoke_zoom = Instant::now() + self.autosmoke_zoom_interval;
        }
    }

    fn drive_autosmoke_scroll(&mut self, size: PhysicalSize<u32>) {
        if !(self.autosmoke_scroll_allow_pending_redraw || self.pending_redraw_frames == 0)
            || Instant::now() < self.next_autosmoke_scroll
        {
            return;
        }
        while let Some(action) = self.autosmoke_scroll_actions.pop_front() {
            let old_x = self.scene.panes[ShellPaneId::SLOT_0].scroll_x;
            let old_y = self.scene.panes[ShellPaneId::SLOT_0].scroll_y;
            let changed = self.scene.scroll_by(action.delta, size);
            let new_x = self.scene.panes[ShellPaneId::SLOT_0].scroll_x;
            let new_y = self.scene.panes[ShellPaneId::SLOT_0].scroll_y;
            fika_log!(
                "[fika-wgpu] autosmoke-scroll action={} delta={:.1} changed={} old_scroll_x={:.1} new_scroll_x={:.1} old_scroll_y={:.1} new_scroll_y={:.1}",
                action.label,
                action.delta,
                changed as u8,
                old_x,
                new_x,
                old_y,
                new_y
            );
            self.next_autosmoke_scroll = Instant::now() + self.autosmoke_scroll_interval;
            if changed {
                self.queue_scene_change("autosmoke-scroll", SCROLL_REDRAW_FRAMES);
                break;
            }
        }
    }

    fn cancel_task_if_running(&mut self, task_id: ShellTaskId) {
        if let Some(controller) = self.active_task_controllers.get(&task_id) {
            controller.cancel();
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
        let task_id = self.next_task_id();
        let controller = OperationController::new();
        self.active_task_controllers
            .insert(task_id, controller.clone());
        let base_detail = async_transfer_task_detail(&target_dir, paths.len(), clear_clipboard);
        self.active_task_base_details
            .insert(task_id, base_detail.clone());
        self.scene.record_async_transfer_started(
            task_id,
            source,
            mode,
            paths.len(),
            base_detail,
            privileged,
        );
        let tx = self.async_task_tx.clone();
        let proxy = self.event_loop_proxy.clone();
        let target_dir_for_error = target_dir.clone();
        if let Err(error) = spawn_operation_task_with_completion(
            {
                let controller = controller.clone();
                let target_dir = target_dir.clone();
                let paths = paths.clone();
                move || async move {
                    transfer_paths_async_with_controller_and_privilege(
                        target_dir,
                        mode,
                        paths,
                        label,
                        clear_clipboard,
                        controller,
                        privileged,
                    )
                    .await
                }
            },
            move |transfer| {
                if tx
                    .send(ShellAsyncTaskResult::Transfer(
                        ShellAsyncTransferCompletion {
                            task_id,
                            source,
                            target_dir,
                            transfer,
                        },
                    ))
                    .is_ok()
                {
                    proxy.wake_up();
                }
            },
        ) {
            let mut transfer = transfer_runtime_failure(
                target_dir_for_error.clone(),
                mode,
                label,
                clear_clipboard,
                error,
            );
            transfer.privileged = privileged;
            let _ = self.async_task_tx.send(ShellAsyncTaskResult::Transfer(
                ShellAsyncTransferCompletion {
                    task_id,
                    source,
                    target_dir: target_dir_for_error,
                    transfer,
                },
            ));
            self.event_loop_proxy.wake_up();
        }
    }

    fn start_async_move_to_trash(
        &mut self,
        paths: Vec<PathBuf>,
        pane_to_reload: ShellPaneId,
        privileged: bool,
    ) {
        self.start_async_move_to_trash_with_options(paths, pane_to_reload, privileged, None);
    }

    fn start_async_move_to_trash_with_options(
        &mut self,
        paths: Vec<PathBuf>,
        pane_to_reload: ShellPaneId,
        privileged: bool,
        clear_selection_pane: Option<ShellPaneId>,
    ) {
        let task_id = self.next_task_id();
        self.active_task_controllers
            .insert(task_id, OperationController::new());
        self.scene
            .record_async_move_to_trash_started(task_id, paths.len(), privileged);
        let tx = self.async_task_tx.clone();
        let proxy = self.event_loop_proxy.clone();
        let paths_for_task = paths.clone();
        if let Err(error) = spawn_operation_task_with_completion(
            move || async move {
                trash_paths_async_with_privilege(paths_for_task, privileged).await
            },
            move |result| {
                if tx
                    .send(ShellAsyncTaskResult::MoveToTrash(
                        ShellAsyncMoveToTrashCompletion {
                            task_id,
                            pane_to_reload,
                            clear_selection_pane,
                            paths,
                            privileged,
                            result,
                        },
                    ))
                    .is_ok()
                {
                    proxy.wake_up();
                }
            },
        ) {
            fika_log!("[fika-wgpu] move-to-trash-runtime-error {error}");
            self.active_task_controllers.remove(&task_id);
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
}
