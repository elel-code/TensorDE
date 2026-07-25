impl FikaWgpuApp {
    fn settings_snapshot(&self) -> ShellSettingsSnapshot {
        ShellSettingsSnapshot {
            show_hidden: self.scene.show_hidden,
            places_visible: self.scene.places_visible,
            dark_mode: self.scene.dark_mode,
            background_blur: self.scene.background_blur,
            background_opacity: self.scene.background_opacity,
        }
    }

    fn settings_dialog_spec(&self) -> ShellDialogWindowSpec {
        ShellDialogWindowSpec::fixed(
            "Fika Settings".to_string(),
            settings_dialog_window_size_scaled(self.scene.ui_scale()),
            self.open_with_window_theme(),
        )
    }

    fn ensure_settings_dialog_window(&mut self, event_loop: &ActiveEventLoop) -> bool {
        if let Some(dialog) = self.dialog_windows.get(ShellDialogWindowKind::Settings) {
            dialog.focus();
            return true;
        }
        self.settings_dialog.reset();
        let spec = self.settings_dialog_spec();
        self.ensure_dialog_window(event_loop, ShellDialogWindowKind::Settings, &spec)
    }

    fn sync_settings_dialog_window(&mut self) {
        if !self
            .dialog_windows
            .is_open(ShellDialogWindowKind::Settings)
        {
            return;
        }
        let spec = self.settings_dialog_spec();
        self.sync_dialog_window(ShellDialogWindowKind::Settings, &spec);
    }

    fn close_settings_dialog_window(&mut self) {
        self.settings_dialog.reset();
        self.close_dialog_window(ShellDialogWindowKind::Settings);
    }

    fn request_settings_dialog_redraw(&self) {
        self.request_dialog_redraw(ShellDialogWindowKind::Settings);
    }

    fn render_settings_dialog_now(&mut self, reason: &'static str) {
        let state = self.settings_dialog;
        let snapshot = self.settings_snapshot();
        let scale = self.scene.ui_scale();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog) = self.dialog_windows.get_mut(ShellDialogWindowKind::Settings) else {
            return;
        };
        let layout_size = dialog.layout_size();
        let (renderer, window) = dialog.renderer_and_window_mut();
        renderer.render_settings_dialog(
            window,
            state,
            snapshot,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            reason,
        );
    }

    fn apply_settings_action(
        &mut self,
        event_loop: &ActiveEventLoop,
        action: ShellSettingsAction,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let changed = match action {
            ShellSettingsAction::ToggleHiddenFiles => {
                let changed = self.toggle_user_hidden_visibility(size);
                if changed {
                    self.apply_action_outcome(
                        event_loop,
                        crate::app_actions::ShellActionOutcome::Present(
                            "settings-toggle-hidden",
                        ),
                    );
                }
                changed
            }
            ShellSettingsAction::TogglePlaces => {
                let changed = self.toggle_user_places_visibility(size);
                if changed {
                    self.apply_window_action_outcome(
                        crate::app_actions::ShellActionOutcome::Redraw,
                    );
                }
                changed
            }
            ShellSettingsAction::ToggleDarkMode => {
                let changed = self.toggle_user_dark_mode();
                if changed {
                    self.apply_action_outcome(
                        event_loop,
                        crate::app_actions::ShellActionOutcome::Present(
                            "settings-toggle-dark-mode",
                        ),
                    );
                }
                changed
            }
            ShellSettingsAction::ToggleBackgroundBlur => {
                let changed = self.toggle_user_background_blur();
                if changed {
                    self.apply_window_action_outcome(
                        crate::app_actions::ShellActionOutcome::Redraw,
                    );
                }
                changed
            }
            ShellSettingsAction::SetBackgroundOpacity(percent) => {
                let changed = self.set_user_background_opacity_percent(percent);
                if changed {
                    self.apply_window_action_outcome(
                        crate::app_actions::ShellActionOutcome::Redraw,
                    );
                }
                changed
            }
        };
        if changed {
            fika_log!(
                "[fika-wgpu] settings action={} value={:?}",
                action.as_str(),
                action
            );
            self.request_settings_dialog_redraw();
        }
    }

    fn settings_dialog_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        event: WindowEvent,
    ) {
        if self.handle_common_dialog_window_event(ShellDialogWindowKind::Settings, &event) {
            return;
        }
        match event {
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if event.state == ElementState::Pressed
                && escape_requested_for_key_event(&event) =>
            {
                self.close_settings_dialog_window();
            }
            WindowEvent::PointerMoved { position, .. } => {
                let Some(size) = self
                    .dialog_windows
                    .layout_size(ShellDialogWindowKind::Settings)
                else {
                    return;
                };
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                let hover_changed =
                    self.settings_dialog
                        .update_hover(point, size, self.scene.ui_scale());
                let dragging = self.settings_dialog.opacity_dragging;
                let drag_action = dragging
                    .then(|| opacity_percent_at_settings_point(point, size, self.scene.ui_scale()))
                    .flatten()
                    .map(ShellSettingsAction::SetBackgroundOpacity);
                let cursor = if settings_dialog_row_at_screen_point(
                    point,
                    size,
                    self.scene.ui_scale(),
                )
                .is_some()
                {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                };
                self.dialog_windows
                    .set_cursor(ShellDialogWindowKind::Settings, cursor);
                if hover_changed {
                    self.request_settings_dialog_redraw();
                }
                if let Some(action) = drag_action {
                    self.apply_settings_action(event_loop, action);
                }
            }
            WindowEvent::PointerLeft { .. } => {
                self.settings_dialog.opacity_dragging = false;
                self.dialog_windows
                    .set_cursor(ShellDialogWindowKind::Settings, CursorIcon::Default);
                if self.settings_dialog.clear_hover() {
                    self.request_settings_dialog_redraw();
                }
            }
            WindowEvent::PointerButton {
                state,
                position,
                button,
                ..
            } => {
                if button.mouse_button() != Some(MouseButton::Left) {
                    return;
                }
                let Some(size) = self
                    .dialog_windows
                    .layout_size(ShellDialogWindowKind::Settings)
                else {
                    return;
                };
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                if state == ElementState::Pressed {
                    let action = settings_action_at_screen_point(
                        point,
                        size,
                        self.scene.ui_scale(),
                    );
                    self.settings_dialog.opacity_dragging =
                        matches!(action, Some(ShellSettingsAction::SetBackgroundOpacity(_)));
                    if let Some(action) = action {
                        self.apply_settings_action(event_loop, action);
                    }
                } else {
                    self.settings_dialog.opacity_dragging = false;
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_settings_dialog_now("settings-dialog-redraw");
            }
            _ => {}
        }
    }
}
