impl ShellScene {

    fn service_menu_targets_for_context_item(
        &self,
        pane_id: ShellPaneId,
        index: usize,
        is_dir: bool,
        mime_type: Option<&str>,
    ) -> Vec<ServiceMenuTarget> {
        let Some(pane) = self.pane_view(pane_id) else {
            return vec![ServiceMenuTarget::new(
                mime_type.or_else(|| is_dir.then_some("inode/directory")),
                is_dir,
            )];
        };
        if pane.selection.contains(index) {
            let targets = self
                .pane_selection(pane_id)
                .into_iter()
                .flat_map(|selection| selection.selected.iter())
                .copied()
                .filter_map(|selected| pane.entries.get(selected))
                .map(|entry| {
                    ServiceMenuTarget::new(
                        entry
                            .mime_type
                            .as_deref()
                            .or_else(|| entry.is_dir.then_some("inode/directory")),
                        entry.is_dir,
                    )
                })
                .collect::<Vec<_>>();
            if !targets.is_empty() {
                return targets;
            }
        }
        vec![ServiceMenuTarget::new(
            mime_type.or_else(|| is_dir.then_some("inode/directory")),
            is_dir,
        )]
    }

    fn close_context_menu(&mut self) -> bool {
        if self.context_menu.take().is_none() {
            return false;
        }
        self.context_menu_safe_triangle.reset();
        fika_log!("[fika] context-menu open=0");
        true
    }

    fn activate_or_close_context_menu_command(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextMenuCommand> {
        let action = self.context_menu_command_at_screen_point(point, size);
        let menu_was_open = self.context_menu.take().is_some();
        if menu_was_open {
            self.context_menu_safe_triangle.reset();
        }
        if let Some(action) = action {
            self.context_menu_actions += 1;
            fika_log!(
                "[fika] context-menu action={} target={} actions={}",
                action.as_str(),
                self.context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none"),
                self.context_menu_actions
            );
            return Some(action);
        } else if menu_was_open {
            fika_log!("[fika] context-menu open=0");
        }
        None
    }

    #[cfg(test)]
    fn activate_or_close_context_menu(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextMenuAction> {
        self.activate_or_close_context_menu_command(point, size)
            .and_then(|command| match command {
                ShellContextMenuCommand::Builtin(action) => Some(action),
                _ => None,
            })
    }

    fn context_menu_command_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextMenuCommand> {
        let menu = self.context_menu.as_ref()?;
        if let Some(submenu) = menu.active_submenu
            && let Some(row) =
                context_submenu_row_at_screen_point(menu, submenu, point, size, self.ui_scale())
        {
            return context_submenu_actions(submenu, menu)
                .get(row)
                .map(|item| item.command.clone());
        }
        let row = context_menu_row_at_screen_point(menu, point, size, self.ui_scale())?;
        context_menu_items(menu)
            .get(row)
            .map(|item| item.command.clone())
    }

    #[cfg(test)]
    fn context_menu_action_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextMenuAction> {
        self.context_menu_command_at_screen_point(point, size)
            .and_then(|command| match command {
                ShellContextMenuCommand::Builtin(action) => Some(action),
                _ => None,
            })
    }

    fn update_context_menu_hover(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let Some(snapshot) = self.context_menu.clone() else {
            return false;
        };
        let scale = self.ui_scale();
        let hover = self
            .context_menu_safe_triangle
            .hover_state(&snapshot, point, size, scale);
        let Some(menu) = self.context_menu.as_mut() else {
            return false;
        };
        let changed = menu.hovered_row != hover.hovered_row
            || menu.hovered_submenu_row != hover.hovered_submenu_row
            || menu.active_submenu != hover.active_submenu
            || menu.active_submenu_row != hover.active_submenu_row;
        menu.hovered_row = hover.hovered_row;
        menu.hovered_submenu_row = hover.hovered_submenu_row;
        menu.active_submenu = hover.active_submenu;
        menu.active_submenu_row = hover.active_submenu_row;
        changed
    }

    fn apply_due_context_menu_hover(&mut self, now: Instant) -> bool {
        let Some(hover) = self.context_menu_safe_triangle.take_due_hover_state(now) else {
            return false;
        };
        let Some(menu) = self.context_menu.as_mut() else {
            self.context_menu_safe_triangle.reset();
            return false;
        };
        let changed = menu.hovered_row != hover.hovered_row
            || menu.hovered_submenu_row != hover.hovered_submenu_row
            || menu.active_submenu != hover.active_submenu
            || menu.active_submenu_row != hover.active_submenu_row;
        menu.hovered_row = hover.hovered_row;
        menu.hovered_submenu_row = hover.hovered_submenu_row;
        menu.active_submenu = hover.active_submenu;
        menu.active_submenu_row = hover.active_submenu_row;
        changed
    }

    fn log_context_target(&self) {
        match self.context_target.as_ref() {
            Some(ShellContextTarget::Item {
                pane,
                index,
                path,
                is_dir,
                selection_count,
                ..
            }) => fika_log!(
                "[fika] context-target kind=item pane={} index={} dir={} selection={} path={} changes={}",
                pane.as_str(),
                index,
                *is_dir as u8,
                selection_count,
                path.display(),
                self.context_target_changes
            ),
            Some(ShellContextTarget::Blank { pane, path, .. }) => fika_log!(
                "[fika] context-target kind=blank pane={} path={} changes={}",
                pane.as_str(),
                path.display(),
                self.context_target_changes
            ),
            Some(ShellContextTarget::Place {
                index,
                label,
                path,
                device,
                network,
                trash,
                root,
                editable,
                ..
            }) => fika_log!(
                "[fika] context-target kind=place index={} label={:?} device={} mounted={} ejectable={} poweroff={} network={} trash={} root={} editable={} path={} changes={}",
                index,
                label,
                device.is_some() as u8,
                device.as_ref().is_none_or(|device| device.mounted) as u8,
                device.as_ref().is_some_and(|device| device.ejectable) as u8,
                device.as_ref().is_some_and(|device| device.can_power_off) as u8,
                *network as u8,
                *trash as u8,
                *root as u8,
                *editable as u8,
                path.display(),
                self.context_target_changes
            ),
            None => fika_log!(
                "[fika] context-target kind=none changes={}",
                self.context_target_changes
            ),
        }
    }

    fn selected_directory_path(&self) -> Option<(ShellPaneId, PathBuf)> {
        let pane = self.active_pane();
        self.pane_selection(pane)?
            .focus_or_first_selected()
            .and_then(|index| self.directory_path_for_pane_index(pane, index))
            .map(|path| (pane, path))
    }

    fn selected_file_open_request(&self) -> Option<OpenFileRequest> {
        let pane = self.active_pane();
        let index = self.pane_selection(pane)?.focus_or_first_selected()?;
        let view = self.pane_view(pane)?;
        let entry = view.entries.get(index)?;
        if entry.is_dir {
            return None;
        }
        let path = self.entry_path_for_pane_view(view, index)?;
        Some(OpenFileRequest::from_path(path, entry.mime_type.as_deref()))
    }

    fn record_open_file_request(&mut self, request: &OpenFileRequest) {
        self.open_changes += 1;
        fika_log!(
            "[fika] open path={} uri={} changes={}",
            request.path.display(),
            request.uri,
            self.open_changes
        );
        self.record_task_status(ShellTaskStatus::completed(
            "Opened",
            request.path.display().to_string(),
            false,
        ));
    }

    fn open_open_with_chooser_from_context(&mut self, cache: &MimeApplicationCache) -> bool {
        let chooser = match self.open_with_chooser_for_context(cache) {
            Ok(chooser) => chooser,
            Err(error) => {
                fika_log!("[fika] open-with-error {error}");
                self.record_task_status(ShellTaskStatus::failed("Open With failed", error, false));
                return false;
            }
        };
        let changed = self.open_with_chooser.as_ref() != Some(&chooser);
        self.open_with_chooser = Some(chooser);
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.trash_conflict_dialog = None;
        self.rubber_band = None;
        if changed {
            self.reset_text_caret_blink();
            self.open_with_changes += 1;
            self.log_open_with_chooser_state();
        }
        changed
    }

    fn open_with_chooser_for_context(
        &self,
        cache: &MimeApplicationCache,
    ) -> Result<ShellOpenWithChooser, String> {
        let target = self.context_target.as_ref().ok_or_else(|| {
            format!(
                "target={} is not a file or folder target",
                self.context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none")
            )
        })?;
        let item_mime_type = match target {
            ShellContextTarget::Item {
                pane, index, path, ..
            } if !file_ops::is_in_trash_files_dir(path) => self
                .pane_state(*pane)
                .and_then(|pane| pane.entries.get(*index))
                .ok_or_else(|| format!("entry index {index} is out of range"))?
                .mime_type
                .clone(),
            _ => None,
        };
        chooser_for_context_target(target, item_mime_type, cache)
    }

    fn is_open_with_chooser_open(&self) -> bool {
        self.open_with_chooser.is_some()
    }

    fn apply_open_with_command(&mut self, command: OpenWithCommand) -> bool {
        if command == OpenWithCommand::Cancel {
            return self.close_open_with_chooser();
        }
        let reset_caret_on_noop = matches!(
            &command,
            OpenWithCommand::Insert(_)
                | OpenWithCommand::Backspace
                | OpenWithCommand::Delete
                | OpenWithCommand::MoveLeft
                | OpenWithCommand::MoveRight
                | OpenWithCommand::MoveHome
                | OpenWithCommand::MoveEnd
        );
        let Some(chooser) = self.open_with_chooser.as_mut() else {
            return false;
        };
        if chooser.apply_command(command) {
            self.reset_text_caret_blink();
            self.open_with_changes += 1;
            self.log_open_with_chooser_state();
            true
        } else if reset_caret_on_noop {
            self.reset_text_caret_blink();
            true
        } else {
            false
        }
    }

    fn select_open_with_filtered_row(&mut self, row: usize) -> bool {
        let Some(chooser) = self.open_with_chooser.as_mut() else {
            return false;
        };
        if chooser.select_filtered_row(row) {
            self.open_with_changes += 1;
            self.log_open_with_chooser_state();
            true
        } else {
            false
        }
    }

    fn toggle_open_with_set_default(&mut self) -> bool {
        let Some(chooser) = self.open_with_chooser.as_mut() else {
            return false;
        };
        if chooser.toggle_set_as_default() {
            self.open_with_changes += 1;
            self.log_open_with_chooser_state();
            true
        } else {
            false
        }
    }

    fn set_open_with_query_cursor(&mut self, cursor: usize) -> bool {
        let Some(chooser) = self.open_with_chooser.as_mut() else {
            return false;
        };
        let cursor_changed = chooser.set_query_cursor(cursor);
        self.reset_text_caret_blink();
        if cursor_changed {
            self.open_with_changes += 1;
            self.log_open_with_chooser_state();
        }
        true
    }

    fn scroll_open_with_chooser_by(&mut self, delta_y: f32) -> bool {
        let Some(delta) = open_with_scroll_delta_rows(delta_y, self.ui_scale()) else {
            return false;
        };
        let Some(chooser) = self.open_with_chooser.as_mut() else {
            return false;
        };
        if chooser.scroll_rows(delta) {
            self.open_with_changes += 1;
            self.log_open_with_chooser_state();
            true
        } else {
            false
        }
    }

    fn open_with_launch_request(
        &self,
        cache: &MimeApplicationCache,
    ) -> Result<OpenWithLaunchRequest, String> {
        let chooser = self
            .open_with_chooser
            .as_ref()
            .ok_or_else(|| "Open With chooser is not open".to_string())?;
        launch_request_for_chooser(chooser, cache)
    }

    fn open_with_launch_request_for_context_application(
        &self,
        cache: &MimeApplicationCache,
        desktop_id: &str,
    ) -> Result<OpenWithLaunchRequest, String> {
        let target = self
            .context_target
            .as_ref()
            .ok_or_else(|| "Open With application requires a file or folder target".to_string())?;
        launch_request_for_context_application(target, cache, desktop_id)
    }

    fn service_menu_launch_request(
        &self,
        cache: &MimeApplicationCache,
        action_id: &str,
    ) -> Result<ServiceMenuLaunchRequest, String> {
        let paths = self
            .context_target_service_menu_paths()?
            .ok_or_else(|| "no service menu target paths".to_string())?;
        if ui::ark::is_builtin_action(action_id) {
            let target = self
                .context_target
                .as_ref()
                .ok_or_else(|| "Ark action requires a context target".to_string())?;
            let items = self.context_target_ark_items(target)?;
            if let Some(request) = ui::ark::builtin_launch_request(action_id, &items)? {
                return Ok(request);
            }
        }
        let plan = cache
            .service_action_launch_plan(action_id, &paths)
            .ok_or_else(|| format!("service action not found or unsupported: {action_id}"))?;
        Ok(ServiceMenuLaunchRequest {
            paths,
            app_name: plan.app_name.clone(),
            plan,
        })
    }

    fn context_target_service_menu_paths(&self) -> Result<Option<Vec<PathBuf>>, String> {
        match self.context_target.as_ref() {
            Some(ShellContextTarget::Item { .. }) => self.context_target_item_paths(),
            Some(ShellContextTarget::Blank { path, .. }) => Ok(Some(vec![path.clone()])),
            Some(ShellContextTarget::Place { path, .. }) => Ok(Some(vec![path.clone()])),
            None => Ok(None),
        }
    }
}
