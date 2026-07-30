impl FikaApp {
    fn queue_scene_change(&mut self, reason: &'static str, redraw_frames: u8) {
        if let Some(kind) = crate::ui::prewarm::visible_role_update_kind_for_reason(reason) {
            self.visible_role_updates.schedule(kind, Instant::now());
        }
        self.pending_redraw_frames = self.pending_redraw_frames.max(redraw_frames);
        self.pending_render_reason = Some(reason);
        if let Some(window) = self.window.as_ref() {
            window.set_title(&window_title(&self.scene));
            window.request_redraw();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::TaskDetail) {
            self.sync_task_detail_dialog_window();
        }
    }

    fn present_scene_change(&mut self, event_loop: &ActiveEventLoop, reason: &'static str) {
        self.pending_redraw_frames = VIEW_SWITCH_REDRAW_FRAMES;
        self.pending_render_reason = None;
        if let Some(window) = self.window.as_ref() {
            window.set_title(&window_title(&self.scene));
            window.request_redraw();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::TaskDetail) {
            self.sync_task_detail_dialog_window();
        }
        self.visible_role_sync_required = true;
        self.render_now(event_loop, reason, true);
    }

    fn settle_visible_icon_roles(&mut self) {
        let Some(kind) = self.visible_role_updates.take_due_update(Instant::now()) else {
            return;
        };
        let (reason, redraw_frames) = match kind {
            crate::ui::prewarm::VisibleRoleUpdateKind::VisibleRange => {
                ("scroll-settle", SCROLL_REDRAW_FRAMES)
            }
            crate::ui::prewarm::VisibleRoleUpdateKind::IconSize => {
                ("zoom-settle", ZOOM_REDRAW_FRAMES)
            }
        };
        self.visible_role_sync_required = true;
        self.queue_scene_change(reason, redraw_frames);
    }

    fn render_create_dialog_now(
        &mut self,
        event_loop: &ActiveEventLoop,
        reason: &'static str,
    ) {
        let Some(dialog_state) = self.scene.create_dialog.as_ref() else {
            self.close_create_dialog_window();
            return;
        };
        let scale = self.scene.ui_scale();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog_window) = self.dialog_windows.get_mut(ShellDialogWindowKind::Create) else {
            return;
        };
        let layout_size = dialog_window.layout_size();
        let (renderer, window) = dialog_window.renderer_and_window_mut();
        renderer.render_create_dialog(
            window,
            event_loop,
            dialog_state,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            reason,
        );
    }

    fn render_rename_dialog_now(
        &mut self,
        event_loop: &ActiveEventLoop,
        reason: &'static str,
    ) {
        let Some(dialog_state) = self.scene.rename_dialog.as_ref() else {
            self.close_rename_dialog_window();
            return;
        };
        let scale = self.scene.ui_scale();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog_window) = self.dialog_windows.get_mut(ShellDialogWindowKind::Rename) else {
            return;
        };
        let layout_size = dialog_window.layout_size();
        let (renderer, window) = dialog_window.renderer_and_window_mut();
        renderer.render_rename_dialog(
            window,
            event_loop,
            dialog_state,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            reason,
        );
    }

    fn render_open_with_dialog_now(
        &mut self,
        event_loop: &ActiveEventLoop,
        reason: &'static str,
    ) {
        let Some(chooser) = self.scene.open_with_chooser.as_ref() else {
            self.close_open_with_dialog_window();
            return;
        };
        let scale = self.scene.ui_scale();
        let caret_visible = self.scene.text_caret_visible();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog) = self.dialog_windows.get_mut(ShellDialogWindowKind::OpenWith) else {
            return;
        };
        let layout_size = dialog.layout_size();
        let (renderer, window) = dialog.renderer_and_window_mut();
        renderer.render_open_with_dialog(
            window,
            event_loop,
            chooser,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            caret_visible,
            reason,
        );
    }

    fn render_properties_dialog_now(
        &mut self,
        event_loop: &ActiveEventLoop,
        reason: &'static str,
    ) {
        let Some(overlay) = self.scene.properties_overlay.as_ref() else {
            self.close_properties_dialog_window();
            return;
        };
        let scale = self.scene.ui_scale();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog) = self.dialog_windows.get_mut(ShellDialogWindowKind::Properties) else {
            return;
        };
        let layout_size = dialog.layout_size();
        let (renderer, window) = dialog.renderer_and_window_mut();
        renderer.render_properties_dialog(
            window,
            event_loop,
            overlay,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            reason,
        );
    }

    fn render_task_detail_dialog_now(
        &mut self,
        event_loop: &ActiveEventLoop,
        reason: &'static str,
    ) {
        if self.scene.task_detail_dialog.is_none() {
            self.close_task_detail_dialog_window();
            return;
        }
        let scale = self.scene.ui_scale();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog) = self.dialog_windows.get_mut(ShellDialogWindowKind::TaskDetail) else {
            return;
        };
        let layout_size = dialog.layout_size();
        let (renderer, window) = dialog.renderer_and_window_mut();
        renderer.render_task_detail_dialog(
            window,
            event_loop,
            &self.scene.task_statuses,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            reason,
        );
    }

    fn render_trash_conflict_dialog_now(
        &mut self,
        event_loop: &ActiveEventLoop,
        reason: &'static str,
    ) {
        let Some(dialog_state) = self.scene.trash_conflict_dialog.as_ref() else {
            self.close_trash_conflict_dialog_window();
            return;
        };
        let scale = self.scene.ui_scale();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog) = self
            .dialog_windows
            .get_mut(ShellDialogWindowKind::TrashConflict)
        else {
            return;
        };
        let layout_size = dialog.layout_size();
        let (renderer, window) = dialog.renderer_and_window_mut();
        renderer.render_trash_conflict_dialog(
            window,
            event_loop,
            dialog_state,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            reason,
        );
    }

    fn render_now(
        &mut self,
        event_loop: &ActiveEventLoop,
        reason: &'static str,
        force_log: bool,
    ) {
        if !self.ensure_main_renderer(event_loop) {
            return;
        }
        if self.scene.is_properties_overlay_open()
            && !self.dialog_windows.is_open(ShellDialogWindowKind::Properties)
        {
            self.ensure_properties_dialog_window(event_loop);
        }
        if self.scene.is_task_detail_dialog_open()
            && !self.dialog_windows.is_open(ShellDialogWindowKind::TaskDetail)
        {
            self.ensure_task_detail_dialog_window(event_loop);
        }
        if self.scene.is_trash_conflict_dialog_open()
            && !self
                .dialog_windows
                .is_open(ShellDialogWindowKind::TrashConflict)
        {
            self.ensure_trash_conflict_dialog_window(event_loop);
        }
        self.reconcile_dialog_window_lifecycle();
        let rendered = {
            let role_updates_paused = self.visible_role_updates.role_updates_paused();
            let icon_size_update_pending =
                self.visible_role_updates.icon_size_update_pending();
            let resolve_visible_exact = self.visible_role_sync_required;
            let Some(window) = self.window.as_ref() else {
                return;
            };
            let Some(renderer) = self.renderer.as_mut() else {
                return;
            };

            renderer.render(
                window.as_ref(),
                event_loop,
                &mut self.scene,
                reason,
                crate::fika_renderer::VisibleRoleRenderPolicy {
                    paused: role_updates_paused,
                    icon_size_update_pending,
                    resolve_exact: resolve_visible_exact,
                },
                force_log,
            )
        };

        if rendered.consumed_redraw_request() && self.pending_redraw_frames > 0 {
            self.pending_redraw_frames -= 1;
        }
        if rendered.presented() {
            self.visible_role_sync_required = false;
            self.drive_autosmoke_after_render();
        }
    }
}
