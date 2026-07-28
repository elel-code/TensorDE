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
            self.apply_action_outcome(
                event_loop,
                crate::app_actions::ShellActionOutcome::Present("directory-watch"),
            );
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

    /// Exit after zoom/scroll autosmoke queues drain (and pending redraws settle).
    ///
    /// Enable with `FIKA_WGPU_AUTOSMOKE_EXIT=1` for headless per-frame benches
    /// (e.g. `fika --view icons /bin`).
    fn maybe_autosmoke_exit(&mut self, event_loop: &ActiveEventLoop) {
        use std::sync::OnceLock;
        static ENABLED: OnceLock<bool> = OnceLock::new();
        let enabled = *ENABLED.get_or_init(|| {
            std::env::var_os("FIKA_WGPU_AUTOSMOKE_EXIT").is_some_and(|value| {
                let value = value.to_string_lossy();
                let value = value.trim().to_ascii_lowercase();
                !matches!(value.as_str(), "" | "0" | "false" | "no" | "off")
            })
        });
        if !enabled {
            return;
        }
        if !self.autosmoke_zoom_actions.is_empty() || !self.autosmoke_scroll_actions.is_empty() {
            return;
        }
        if self.pending_redraw_frames > 0 {
            return;
        }
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        // Wait for first present + any visible-priority work to settle a bit.
        if renderer.frame_count < 2 {
            return;
        }
        if renderer.render_work_pending {
            return;
        }
        fika_log!(
            "[fika-wgpu] autosmoke-exit frames={} icon_gpu_bytes={} thumb_ready_bytes={}",
            renderer.frame_count,
            renderer.icon_renderer.gpu_texture_bytes,
            renderer.icon_renderer.thumbnails.ready_bytes(),
        );
        self.exit_event_loop(event_loop, "autosmoke-complete");
    }
}
