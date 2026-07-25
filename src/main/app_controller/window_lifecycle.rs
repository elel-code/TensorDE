impl FikaWgpuApp {
    fn new(
        scene: ShellScene,
        auto_cycle_views: bool,
        settings_path: PathBuf,
        event_loop_proxy: EventLoopProxy,
    ) -> Self {
        let (async_task_tx, async_task_rx) = mpsc::channel();
        let autosmoke_zoom = autosmoke_zoom_config();
        let autosmoke_scroll = autosmoke_scroll_config(SCROLL_LINE_PX * 2.0);
        let mut directory_watchers = ShellDirectoryWatcherRuntime::new(event_loop_proxy.clone());
        directory_watchers.sync_with_scene(&scene);
        Self {
            scene,
            mime_applications: MimeApplicationCache::load(),
            settings_path,
            event_loop_proxy,
            directory_watchers,
            async_task_tx,
            async_task_rx,
            navigation_generations: [0; 2],
            active_task_controllers: HashMap::new(),
            active_task_base_details: HashMap::new(),
            next_task_id: 1,
            modifiers: Modifiers::default(),
            pinch_zoom: None,
            incoming_dnd_transfer: None,
            outgoing_dnd_transfer: None,
            outgoing_dnd_start_failed: false,
            renderer: None,
            dialog_windows: ShellDialogWindows::default(),
            text_input: FikaTextInputRuntime::default(),
            settings_dialog: ShellSettingsDialogState::default(),
            clipboard: None,
            window: None,
            cursor_icon: CursorIcon::Default,
            pending_redraw_frames: 0,
            pending_render_reason: None,
            last_location_text_caret_dirty_value: 0,
            last_open_with_text_caret_dirty_value: 0,
            auto_cycle_views,
            next_auto_cycle: Instant::now() + AUTO_CYCLE_INTERVAL,
            autosmoke_zoom_actions: autosmoke_zoom.actions,
            next_autosmoke_zoom: Instant::now() + autosmoke_zoom.interval,
            autosmoke_zoom_interval: autosmoke_zoom.interval,
            autosmoke_zoom_allow_pending_redraw: autosmoke_zoom.allow_pending_redraw,
            autosmoke_scroll_actions: autosmoke_scroll.actions,
            next_autosmoke_scroll: Instant::now() + autosmoke_scroll.interval,
            autosmoke_scroll_interval: autosmoke_scroll.interval,
            autosmoke_scroll_allow_pending_redraw: autosmoke_scroll.allow_pending_redraw,
            dialog_lifecycle_smoke: DialogLifecycleSmoke::from_env(),
        }
    }

    fn set_window_cursor(&mut self, cursor_icon: CursorIcon) {
        if self.cursor_icon == cursor_icon {
            return;
        }
        self.cursor_icon = cursor_icon;
        if let Some(window) = self.window.as_ref() {
            window.set_cursor(cursor_icon);
        }
    }

    fn set_open_with_dialog_cursor(&mut self, cursor_icon: CursorIcon) {
        self.dialog_windows
            .set_cursor(ShellDialogWindowKind::OpenWith, cursor_icon);
    }

    fn update_window_cursor_for_scene(&mut self, size: PhysicalSize<u32>) {
        self.set_window_cursor(self.scene.cursor_icon(size));
    }

    fn update_open_with_dialog_cursor_for_scene(&mut self, size: PhysicalSize<u32>) {
        self.set_open_with_dialog_cursor(self.scene.open_with_chooser_cursor_icon(size));
    }

    fn request_main_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn request_open_with_dialog_redraw(&self) -> bool {
        self.request_dialog_redraw(ShellDialogWindowKind::OpenWith)
    }

    fn request_dialog_redraw(&self, kind: ShellDialogWindowKind) -> bool {
        self.dialog_windows.request_redraw(kind)
    }

    fn trace_window_event(&self, window_id: WindowId, event: &WindowEvent) {
        if !fika_dialog_trace_enabled() {
            return;
        }
        if window_event_trace_is_high_volume(event) && !fika_dialog_trace_verbose_enabled() {
            return;
        }
        let main_id = self.window.as_ref().map(|window| window.id());
        let dialog_kind = self.dialog_windows.window_kind_for_id(window_id);
        let role = if main_id == Some(window_id) {
            "main"
        } else if dialog_kind.is_some() {
            "dialog"
        } else {
            "unknown"
        };
        fika_dialog_trace!(
            "[fika-wgpu] window-event event={} window={:?} role={} main={:?} dialog={} dialogs_open={}",
            window_event_label(event),
            window_id,
            role,
            main_id,
            dialog_kind
                .map(ShellDialogWindowKind::as_str)
                .unwrap_or("none"),
            self.dialog_windows.has_open_window() as u8,
        );
    }

    fn exit_event_loop(&self, event_loop: &ActiveEventLoop, reason: &'static str) {
        fika_log!(
            "[fika-wgpu] event-loop-exit reason={} main_open={} dialogs_open={}",
            reason,
            self.window.is_some() as u8,
            self.dialog_windows.has_open_window() as u8,
        );
        event_loop.exit();
    }

    fn drop_windows_for_exit(&mut self) {
        self.renderer = None;
        self.dialog_windows.close_all();
        self.clipboard = None;
        self.window = None;
    }

    fn drain_dialog_window_deferred_closes(&mut self) {
        if self.dialog_windows.drain_ready_deferred_closes() {
            self.request_main_redraw();
        }
    }

    fn open_with_dialog_title(&self) -> String {
        self.scene
            .open_with_chooser
            .as_ref()
            .map(|chooser| format!("Open With - {}", path_name_or_display(&chooser.path)))
            .unwrap_or_else(|| "Open With".to_string())
    }

    fn open_with_dialog_surface_size(&self) -> Option<PhysicalSize<u32>> {
        self.scene
            .open_with_chooser
            .as_ref()
            .map(|chooser| open_with_chooser_window_size_scaled(chooser, self.scene.ui_scale()))
    }

    fn open_with_window_theme(&self) -> Theme {
        if self.scene.theme().is_dark() {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    fn ensure_dialog_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        kind: ShellDialogWindowKind,
        spec: &ShellDialogWindowSpec,
    ) -> bool {
        if let Some(dialog) = self.dialog_windows.get_mut(kind) {
            dialog.sync(spec);
            return true;
        }
        let dialog = match ShellDetachedDialogWindow::create(
            event_loop,
            self.renderer.as_ref(),
            kind,
            spec,
        ) {
            Ok(dialog) => dialog,
            Err(error) => {
                fika_log!("[fika-wgpu] {error}");
                return false;
            }
        };
        self.dialog_windows.set(kind, dialog);
        self.sync_dialog_window(kind, spec);
        true
    }

    fn sync_dialog_window(&mut self, kind: ShellDialogWindowKind, spec: &ShellDialogWindowSpec) {
        if let Some(dialog) = self.dialog_windows.get_mut(kind) {
            dialog.sync(spec);
        }
    }

    fn close_dialog_window(&mut self, kind: ShellDialogWindowKind) -> bool {
        let closed = self.dialog_windows.close(kind);
        fika_dialog_trace!(
            "[fika-wgpu] dialog-close-dispatch kind={} closed={}",
            kind.as_str(),
            closed as u8
        );
        closed
    }

    fn close_dialog_state_and_window(&mut self, kind: ShellDialogWindowKind) -> bool {
        let changed = match kind {
            ShellDialogWindowKind::Create => self.scene.close_create_dialog(),
            ShellDialogWindowKind::OpenWith => self.scene.close_open_with_chooser(),
            ShellDialogWindowKind::Properties => self.scene.close_properties_overlay(),
            ShellDialogWindowKind::Rename => self.scene.close_rename_dialog(),
            ShellDialogWindowKind::Settings => {
                self.settings_dialog.reset();
                true
            }
            ShellDialogWindowKind::TaskDetail => self.scene.close_task_detail_dialog(),
            ShellDialogWindowKind::TrashConflict => self.scene.close_trash_conflict_dialog(),
        };
        let closed = self.close_dialog_window(kind);
        fika_dialog_trace!(
            "[fika-wgpu] dialog-state-close kind={} changed={} closed={}",
            kind.as_str(),
            changed as u8,
            closed as u8
        );
        changed
    }

    fn close_main_window_from_window_manager_request(
        &mut self,
        event_loop: &ActiveEventLoop,
        reason: &'static str,
    ) {
        fika_dialog_trace!(
            "[fika-wgpu] main-close accept=1 reason={} dialogs_open={}",
            reason,
            self.dialog_windows.has_open_window() as u8,
        );
        self.drop_windows_for_exit();
        self.exit_event_loop(event_loop, reason);
    }

    fn sync_dialog_window_for_kind(&mut self, kind: ShellDialogWindowKind) {
        match kind {
            ShellDialogWindowKind::Create => self.sync_create_dialog_window(),
            ShellDialogWindowKind::OpenWith => self.sync_open_with_dialog_window(),
            ShellDialogWindowKind::Properties => self.sync_properties_dialog_window(),
            ShellDialogWindowKind::Rename => self.sync_rename_dialog_window(),
            ShellDialogWindowKind::Settings => self.sync_settings_dialog_window(),
            ShellDialogWindowKind::TaskDetail => self.sync_task_detail_dialog_window(),
            ShellDialogWindowKind::TrashConflict => self.sync_trash_conflict_dialog_window(),
        }
    }

    fn handle_common_dialog_window_event(
        &mut self,
        kind: ShellDialogWindowKind,
        event: &WindowEvent,
    ) -> bool {
        let Some(event) = self.dialog_windows.handle_window_event(kind, event) else {
            return false;
        };
        fika_dialog_trace!(
            "[fika-wgpu] dialog-host-event kind={} event={:?}",
            kind.as_str(),
            event
        );
        match event {
            ShellDialogWindowHostEvent::CloseRequested => {
                if self.close_dialog_state_and_window(kind) {
                    self.request_main_redraw();
                }
                true
            }
            ShellDialogWindowHostEvent::SurfaceResized => true,
            ShellDialogWindowHostEvent::ScaleFactorChanged {
                scale_factor,
                renderer_size,
            } => {
                let size = self
                    .renderer
                    .as_ref()
                    .map(|renderer| renderer.size)
                    .unwrap_or(renderer_size);
                if self.scene.set_scale_factor(scale_factor, size) {
                    self.request_main_redraw();
                }
                self.sync_dialog_window_for_kind(kind);
                true
            }
            ShellDialogWindowHostEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                true
            }
        }
    }

    fn create_dialog_title(&self) -> String {
        let Some(dialog) = self.scene.create_dialog.as_ref() else {
            return "Create New".to_string();
        };
        if dialog.privileged {
            "Create New as Administrator".to_string()
        } else {
            "Create New".to_string()
        }
    }

    fn create_dialog_spec(&self) -> Option<ShellDialogWindowSpec> {
        self.scene.create_dialog.as_ref()?;
        Some(ShellDialogWindowSpec::fixed(
            self.create_dialog_title(),
            create_dialog_window_size_scaled(self.scene.ui_scale()),
            self.open_with_window_theme(),
        ))
    }

    fn ensure_create_dialog_window(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(spec) = self.create_dialog_spec() else {
            self.close_create_dialog_window();
            return false;
        };
        if !self.ensure_dialog_window(event_loop, ShellDialogWindowKind::Create, &spec) {
            if self.scene.close_create_dialog() {
                self.request_main_redraw();
            }
            return false;
        }
        self.close_rename_dialog_window();
        true
    }

    fn sync_create_dialog_window(&mut self) {
        let Some(spec) = self.create_dialog_spec() else {
            self.close_create_dialog_window();
            return;
        };
        self.sync_dialog_window(ShellDialogWindowKind::Create, &spec);
    }

    fn close_create_dialog_window(&mut self) {
        self.close_dialog_window(ShellDialogWindowKind::Create);
    }

    fn finish_create_dialog_state_change(&mut self) {
        if self.scene.is_create_dialog_open() {
            if self.dialog_windows.is_open(ShellDialogWindowKind::Create) {
                self.sync_create_dialog_window();
            } else {
                if self.scene.close_create_dialog() {
                    self.request_main_redraw();
                }
            }
        } else {
            self.close_create_dialog_window();
            self.request_main_redraw();
        }
    }

    fn rename_dialog_title(&self) -> String {
        let Some(dialog) = self.scene.rename_dialog.as_ref() else {
            return "Rename".to_string();
        };
        match (dialog.is_dir, dialog.privileged) {
            (true, true) => "Rename Folder as Administrator",
            (false, true) => "Rename File as Administrator",
            (true, false) => "Rename Folder",
            (false, false) => "Rename File",
        }
        .to_string()
    }

    fn rename_dialog_spec(&self) -> Option<ShellDialogWindowSpec> {
        self.scene.rename_dialog.as_ref()?;
        Some(ShellDialogWindowSpec::fixed(
            self.rename_dialog_title(),
            rename_dialog_window_size_scaled(self.scene.ui_scale()),
            self.open_with_window_theme(),
        ))
    }

    fn ensure_rename_dialog_window(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(spec) = self.rename_dialog_spec() else {
            self.close_rename_dialog_window();
            return false;
        };
        if !self.ensure_dialog_window(event_loop, ShellDialogWindowKind::Rename, &spec) {
            if self.scene.close_rename_dialog() {
                self.request_main_redraw();
            }
            return false;
        }
        self.close_create_dialog_window();
        true
    }

    fn sync_rename_dialog_window(&mut self) {
        let Some(spec) = self.rename_dialog_spec() else {
            self.close_rename_dialog_window();
            return;
        };
        self.sync_dialog_window(ShellDialogWindowKind::Rename, &spec);
    }

    fn close_rename_dialog_window(&mut self) {
        self.close_dialog_window(ShellDialogWindowKind::Rename);
    }

    fn finish_rename_dialog_state_change(&mut self) {
        if self.scene.is_rename_dialog_open() {
            if self.dialog_windows.is_open(ShellDialogWindowKind::Rename) {
                self.sync_rename_dialog_window();
            } else {
                if self.scene.close_rename_dialog() {
                    self.request_main_redraw();
                }
            }
        } else {
            self.close_rename_dialog_window();
            self.request_main_redraw();
        }
    }

    fn open_with_dialog_spec(&self) -> Option<ShellDialogWindowSpec> {
        Some(ShellDialogWindowSpec::fixed(
            self.open_with_dialog_title(),
            self.open_with_dialog_surface_size()?,
            self.open_with_window_theme(),
        ))
    }

    fn properties_dialog_spec(&self) -> Option<ShellDialogWindowSpec> {
        let overlay = self.scene.properties_overlay.as_ref()?;
        Some(ShellDialogWindowSpec::fixed(
            overlay.title.clone(),
            properties_dialog_window_size_scaled(overlay, self.scene.ui_scale()),
            self.open_with_window_theme(),
        ))
    }

    fn ensure_properties_dialog_window(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(spec) = self.properties_dialog_spec() else {
            self.close_properties_dialog_window();
            return false;
        };
        if !self.ensure_dialog_window(event_loop, ShellDialogWindowKind::Properties, &spec) {
            self.scene.close_properties_overlay();
            self.request_main_redraw();
            return false;
        }
        true
    }

    fn sync_properties_dialog_window(&mut self) {
        let Some(spec) = self.properties_dialog_spec() else {
            self.close_properties_dialog_window();
            return;
        };
        self.sync_dialog_window(ShellDialogWindowKind::Properties, &spec);
    }

    fn close_properties_dialog_window(&mut self) {
        self.close_dialog_window(ShellDialogWindowKind::Properties);
    }

    fn task_detail_dialog_spec(&self) -> Option<ShellDialogWindowSpec> {
        self.scene.task_detail_dialog.as_ref()?;
        Some(ShellDialogWindowSpec::fixed(
            "Task Details".to_string(),
            task_detail_dialog_window_size_scaled(
                self.scene.task_statuses.len(),
                self.scene.ui_scale(),
            ),
            self.open_with_window_theme(),
        ))
    }

    fn ensure_task_detail_dialog_window(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(spec) = self.task_detail_dialog_spec() else {
            self.close_task_detail_dialog_window();
            return false;
        };
        if !self.ensure_dialog_window(event_loop, ShellDialogWindowKind::TaskDetail, &spec) {
            self.scene.close_task_detail_dialog();
            self.request_main_redraw();
            return false;
        }
        true
    }

    fn sync_task_detail_dialog_window(&mut self) {
        let Some(spec) = self.task_detail_dialog_spec() else {
            self.close_task_detail_dialog_window();
            return;
        };
        self.sync_dialog_window(ShellDialogWindowKind::TaskDetail, &spec);
    }

    fn close_task_detail_dialog_window(&mut self) {
        self.close_dialog_window(ShellDialogWindowKind::TaskDetail);
    }

    fn finish_task_detail_dialog_state_change(&mut self) {
        if self.scene.is_task_detail_dialog_open() {
            self.sync_task_detail_dialog_window();
        } else {
            self.close_task_detail_dialog_window();
        }
        self.request_main_redraw();
    }

    fn trash_conflict_dialog_spec(&self) -> Option<ShellDialogWindowSpec> {
        self.scene.trash_conflict_dialog.as_ref()?;
        Some(ShellDialogWindowSpec::fixed(
            "Restore Conflict".to_string(),
            trash_conflict_dialog_window_size_scaled(self.scene.ui_scale()),
            self.open_with_window_theme(),
        ))
    }

    fn ensure_trash_conflict_dialog_window(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(spec) = self.trash_conflict_dialog_spec() else {
            self.close_trash_conflict_dialog_window();
            return false;
        };
        if !self.ensure_dialog_window(event_loop, ShellDialogWindowKind::TrashConflict, &spec) {
            self.scene.close_trash_conflict_dialog();
            self.request_main_redraw();
            return false;
        }
        true
    }

    fn sync_trash_conflict_dialog_window(&mut self) {
        let Some(spec) = self.trash_conflict_dialog_spec() else {
            self.close_trash_conflict_dialog_window();
            return;
        };
        self.sync_dialog_window(ShellDialogWindowKind::TrashConflict, &spec);
    }

    fn close_trash_conflict_dialog_window(&mut self) {
        self.close_dialog_window(ShellDialogWindowKind::TrashConflict);
    }
}
