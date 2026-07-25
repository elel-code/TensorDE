impl FikaWgpuApp {

    fn start_async_trash_view_operation(
        &mut self,
        action: ShellContextMenuAction,
    ) -> Result<(), String> {
        let pane_to_reload = self
            .scene
            .context_target_pane()
            .unwrap_or_else(|| self.scene.active_pane());
        let (operation, paths) = self.scene.context_target_trash_view_operation(action)?;
        let task_id = self.next_task_id();
        self.active_task_controllers
            .insert(task_id, OperationController::new());
        self.scene
            .record_async_trash_view_started(task_id, operation, paths.len());

        let tx = self.async_task_tx.clone();
        let proxy = self.event_loop_proxy.clone();
        if let Err(error) = spawn_operation_task_with_completion(
            {
                let paths = paths;
                move || async move {
                    trash_view_operation_result_async(WGPU_SHELL_PANE_ID, operation, paths).await
                }
            },
            move |result| {
                if tx
                    .send(ShellAsyncTaskResult::TrashView(
                        ShellAsyncTrashViewCompletion {
                            task_id,
                            action,
                            pane_to_reload,
                            result,
                        },
                    ))
                    .is_ok()
                {
                    proxy.wake_up();
                }
            },
        ) {
            fika_log!(
                "[fika-wgpu] trash-view-runtime-error action={} {error}",
                action.as_str()
            );
            let _ = self.async_task_tx.send(ShellAsyncTaskResult::TrashView(
                ShellAsyncTrashViewCompletion {
                    task_id,
                    action,
                    pane_to_reload,
                    result: trash_view_operation_runtime_failure(operation),
                },
            ));
            self.event_loop_proxy.wake_up();
        }
        Ok(())
    }

    fn create_dialog_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        if self.handle_common_dialog_window_event(ShellDialogWindowKind::Create, &event) {
            return;
        }
        match event {
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                if self
                    .dialog_windows
                    .get(ShellDialogWindowKind::Create)
                    .is_some_and(|window| self.text_input_consumes_key(window.window_id(), &event))
                {
                    return;
                }
                let size = self
                    .dialog_windows
                    .layout_size(ShellDialogWindowKind::Create)
                    .unwrap_or_else(|| PhysicalSize::new(1, 1));
                let shortcut =
                    self.modifiers.state().control_key() || self.modifiers.state().meta_key();
                if self.scene.create_dialog.as_ref().is_some_and(|d| d.busy) {
                    return;
                }
                match create_command_for_key_event(&event, shortcut) {
                    CreateCommand::Commit => self.commit_create_dialog(event_loop),
                    CreateCommand::Ignore => {}
                    command => {
                        if self.scene.apply_create_command(command, size) {
                            self.finish_create_dialog_state_change();
                        }
                    }
                }
            }
            WindowEvent::PointerMoved { .. } | WindowEvent::PointerLeft { .. } => {
                self.dialog_windows
                    .set_cursor(ShellDialogWindowKind::Create, CursorIcon::Default);
            }
            WindowEvent::PointerButton {
                state,
                position,
                button,
                ..
            } => {
                if state != ElementState::Pressed {
                    return;
                }
                let Some(mouse_button) = button.mouse_button() else {
                    return;
                };
                if mouse_button != MouseButton::Left {
                    return;
                }
                let Some(size) = self
                    .dialog_windows
                    .layout_size(ShellDialogWindowKind::Create)
                else {
                    return;
                };
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                if self.scene.create_dialog.as_ref().is_some_and(|d| d.busy) {
                    return;
                }
                match self.scene.create_dialog_click_at_screen_point(point, size) {
                    CreateDialogClick::Outside | CreateDialogClick::Cancel => {
                        if self.close_dialog_state_and_window(ShellDialogWindowKind::Create) {
                            self.request_main_redraw();
                        }
                    }
                    CreateDialogClick::Commit => self.commit_create_dialog(event_loop),
                    CreateDialogClick::Kind(kind) => {
                        if self
                            .scene
                            .apply_create_command(CreateCommand::SetKind(kind), size)
                        {
                            self.finish_create_dialog_state_change();
                        }
                    }
                    CreateDialogClick::Inside => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_create_dialog_now("create-dialog-redraw");
            }
            _ => {}
        }
    }

    fn rename_dialog_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        if self.handle_common_dialog_window_event(ShellDialogWindowKind::Rename, &event) {
            return;
        }
        match event {
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                if self
                    .dialog_windows
                    .get(ShellDialogWindowKind::Rename)
                    .is_some_and(|window| self.text_input_consumes_key(window.window_id(), &event))
                {
                    return;
                }
                if self.scene.rename_dialog.as_ref().is_some_and(|d| d.busy) {
                    return;
                }
                let shortcut =
                    self.modifiers.state().control_key() || self.modifiers.state().meta_key();
                match rename_command_for_key_event(&event, shortcut) {
                    RenameCommand::Commit => self.commit_rename_dialog(event_loop),
                    RenameCommand::Ignore => {}
                    command => {
                        if self.scene.apply_rename_command(command) {
                            self.finish_rename_dialog_state_change();
                        }
                    }
                }
            }
            WindowEvent::PointerMoved { .. } | WindowEvent::PointerLeft { .. } => {
                self.dialog_windows
                    .set_cursor(ShellDialogWindowKind::Rename, CursorIcon::Default);
            }
            WindowEvent::PointerButton {
                state,
                position,
                button,
                ..
            } => {
                if state != ElementState::Pressed {
                    return;
                }
                let Some(mouse_button) = button.mouse_button() else {
                    return;
                };
                if mouse_button != MouseButton::Left {
                    return;
                }
                let Some(size) = self
                    .dialog_windows
                    .layout_size(ShellDialogWindowKind::Rename)
                else {
                    return;
                };
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                if self.scene.rename_dialog.as_ref().is_some_and(|d| d.busy) {
                    return;
                }
                match self.scene.rename_dialog_click_at_screen_point(point, size) {
                    RenameDialogClick::Outside | RenameDialogClick::Cancel => {
                        if self.close_dialog_state_and_window(ShellDialogWindowKind::Rename) {
                            self.request_main_redraw();
                        }
                    }
                    RenameDialogClick::Commit => self.commit_rename_dialog(event_loop),
                    RenameDialogClick::Inside => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_rename_dialog_now("rename-dialog-redraw");
            }
            _ => {}
        }
    }

    fn open_with_dialog_window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        event: WindowEvent,
    ) {
        if self.handle_common_dialog_window_event(ShellDialogWindowKind::OpenWith, &event) {
            return;
        }
        match event {
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                if self
                    .dialog_windows
                    .get(ShellDialogWindowKind::OpenWith)
                    .is_some_and(|window| self.text_input_consumes_key(window.window_id(), &event))
                {
                    return;
                }
                let shortcut =
                    self.modifiers.state().control_key() || self.modifiers.state().meta_key();
                match open_with_command_for_key_event(&event, shortcut) {
                    OpenWithCommand::Commit => self.commit_open_with_chooser(),
                    OpenWithCommand::Ignore => {}
                    command => {
                        if self.scene.apply_open_with_command(command) {
                            self.finish_open_with_dialog_state_change();
                        }
                    }
                }
            }
            WindowEvent::PointerMoved { position, .. } => {
                let Some(size) = self
                    .dialog_windows
                    .layout_size(ShellDialogWindowKind::OpenWith)
                else {
                    return;
                };
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                let changed = self.scene.set_pointer(point, size);
                self.update_open_with_dialog_cursor_for_scene(size);
                if changed {
                    self.request_open_with_dialog_redraw();
                }
            }
            WindowEvent::PointerLeft { .. } => {
                self.set_open_with_dialog_cursor(CursorIcon::Default);
                if self.scene.clear_pointer() {
                    self.request_open_with_dialog_redraw();
                }
            }
            WindowEvent::PointerButton {
                state,
                position,
                button,
                ..
            } => {
                let Some(size) = self
                    .dialog_windows
                    .layout_size(ShellDialogWindowKind::OpenWith)
                else {
                    return;
                };
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                let Some(mouse_button) = button.mouse_button() else {
                    return;
                };
                if state == ElementState::Released && self.scene.is_scrollbar_dragging() {
                    let changed = self.scene.end_scrollbar_drag(point, size);
                    self.set_open_with_dialog_cursor(CursorIcon::Default);
                    if changed {
                        self.request_open_with_dialog_redraw();
                    }
                    return;
                }
                if state == ElementState::Pressed && mouse_button == MouseButton::Left {
                    if let Some(changed) = self.scene.begin_open_with_scrollbar_drag(point, size) {
                        self.set_open_with_dialog_cursor(CursorIcon::Default);
                        if changed {
                            self.request_open_with_dialog_redraw();
                        }
                        return;
                    }
                    match self
                        .scene
                        .open_with_chooser_click_at_screen_point(point, size)
                    {
                        OpenWithChooserClick::Outside | OpenWithChooserClick::Cancel => {
                            if self.close_dialog_state_and_window(ShellDialogWindowKind::OpenWith) {
                                self.request_main_redraw();
                            }
                        }
                        OpenWithChooserClick::Open => self.commit_open_with_chooser(),
                        OpenWithChooserClick::ToggleDefault => {
                            if self.scene.toggle_open_with_set_default() {
                                self.finish_open_with_dialog_state_change();
                            }
                        }
                        OpenWithChooserClick::Query(cursor) => {
                            if self.scene.set_open_with_query_cursor(cursor) {
                                self.finish_open_with_dialog_state_change();
                            }
                        }
                        OpenWithChooserClick::Row(row) => {
                            if self.scene.select_open_with_filtered_row(row) {
                                self.finish_open_with_dialog_state_change();
                            }
                        }
                        OpenWithChooserClick::Inside => {}
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta_y = scroll_delta_y(delta, self.scene.ui_scale());
                if self.scene.scroll_open_with_chooser_by(delta_y) {
                    self.request_open_with_dialog_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_open_with_dialog_now("open-with-dialog-redraw");
            }
            _ => {}
        }
    }

    fn properties_dialog_window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        event: WindowEvent,
    ) {
        if self.handle_common_dialog_window_event(ShellDialogWindowKind::Properties, &event) {
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
                if self.close_dialog_state_and_window(ShellDialogWindowKind::Properties) {
                    self.request_main_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_properties_dialog_now("properties-dialog-redraw");
            }
            _ => {}
        }
    }

    fn task_detail_dialog_window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        event: WindowEvent,
    ) {
        if self.handle_common_dialog_window_event(ShellDialogWindowKind::TaskDetail, &event) {
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
                if self.close_dialog_state_and_window(ShellDialogWindowKind::TaskDetail) {
                    self.request_main_redraw();
                }
            }
            WindowEvent::PointerButton {
                state: ElementState::Pressed,
                position,
                button,
                ..
            } => {
                if button.mouse_button() != Some(MouseButton::Left) {
                    return;
                }
                let Some(size) = self
                    .dialog_windows
                    .layout_size(ShellDialogWindowKind::TaskDetail)
                else {
                    return;
                };
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                match self
                    .scene
                    .task_detail_dialog_window_click_at_screen_point(point, size)
                {
                    TaskDetailDialogClick::Outside | TaskDetailDialogClick::Cancel => {
                        self.close_dialog_state_and_window(ShellDialogWindowKind::TaskDetail);
                        self.request_main_redraw();
                    }
                    TaskDetailDialogClick::Clear => {
                        self.scene.clear_task_statuses();
                        self.finish_task_detail_dialog_state_change();
                    }
                    TaskDetailDialogClick::Dismiss(index) => {
                        let (changed, task_id) = self.scene.dismiss_task_status(index);
                        if let Some(task_id) = task_id {
                            self.cancel_task_if_running(task_id);
                        }
                        if changed {
                            self.finish_task_detail_dialog_state_change();
                        }
                    }
                    TaskDetailDialogClick::Inside => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_task_detail_dialog_now("task-detail-dialog-redraw");
            }
            _ => {}
        }
    }

    fn trash_conflict_dialog_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        event: WindowEvent,
    ) {
        if self.handle_common_dialog_window_event(ShellDialogWindowKind::TrashConflict, &event) {
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
                if self.close_dialog_state_and_window(ShellDialogWindowKind::TrashConflict) {
                    self.request_main_redraw();
                }
            }
            WindowEvent::PointerButton {
                state: ElementState::Pressed,
                position,
                button,
                ..
            } => {
                if button.mouse_button() != Some(MouseButton::Left) {
                    return;
                }
                let Some(size) = self
                    .dialog_windows
                    .layout_size(ShellDialogWindowKind::TrashConflict)
                else {
                    return;
                };
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                match self
                    .scene
                    .trash_conflict_dialog_window_click_at_screen_point(point, size)
                {
                    TrashConflictDialogClick::Outside | TrashConflictDialogClick::Cancel => {
                        self.close_dialog_state_and_window(ShellDialogWindowKind::TrashConflict);
                        self.request_main_redraw();
                    }
                    TrashConflictDialogClick::Replace => {
                        self.close_trash_conflict_dialog_window();
                        self.replace_trash_restore_conflicts(event_loop);
                    }
                    TrashConflictDialogClick::Inside => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_trash_conflict_dialog_now("trash-conflict-dialog-redraw");
            }
            _ => {}
        }
    }
}
