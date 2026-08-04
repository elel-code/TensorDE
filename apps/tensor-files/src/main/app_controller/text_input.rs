#[derive(Default)]
struct TensorFilesTextInputRuntime {
    synchronized: HashMap<WindowId, ImeState>,
    entered: HashSet<WindowId>,
    input_method_changes: HashSet<WindowId>,
}

impl TensorFilesTextInputRuntime {
    fn sync_window(&mut self, window: &Window, desired: Option<ImeState>) {
        let window_id = window.id();
        let input_method_change = self.input_method_changes.remove(&window_id);
        let Some(mut state) = desired else {
            if self.synchronized.remove(&window_id).is_some() {
                window.set_ime_state(None);
            }
            return;
        };
        state.change_cause = if input_method_change {
            ImeChangeCause::InputMethod
        } else {
            ImeChangeCause::Other
        };
        if self
            .synchronized
            .get(&window_id)
            .is_some_and(|current| current.same_client_state(&state))
        {
            return;
        }

        window.set_ime_state(Some(state.clone()));
        // Change cause describes this update, not persistent editor content.
        state.change_cause = ImeChangeCause::Other;
        self.synchronized.insert(window_id, state);
    }

    fn finish_sync(&mut self, live_windows: &HashSet<WindowId>) {
        self.synchronized
            .retain(|window, _| live_windows.contains(window));
        self.entered.retain(|window| live_windows.contains(window));
        self.input_method_changes
            .retain(|window| live_windows.contains(window));
    }

    fn entered(&mut self, window: WindowId) {
        self.entered.insert(window);
    }

    fn left(&mut self, window: WindowId) {
        self.entered.remove(&window);
        self.input_method_changes.remove(&window);
    }

    fn note_input_method_change(&mut self, window: WindowId) {
        self.input_method_changes.insert(window);
    }

    fn consumes_text_key(&self, window: WindowId, event: &windowing::KeyEvent) -> bool {
        if !self.entered.contains(&window) || !self.synchronized.contains_key(&window) {
            return false;
        }
        matches!(
            event.logical_key,
            Key::Character(_)
                | Key::Named(NamedKey::Backspace)
                | Key::Named(NamedKey::Delete)
        )
    }
}

impl TensorFilesApp {
    fn sync_text_input_states(&mut self) {
        let mut live_windows = HashSet::new();

        if let Some(window) = self.window.as_ref() {
            live_windows.insert(window.id());
            let state = self.main_text_input_state();
            self.text_input.sync_window(window, state);
        }

        for kind in [
            ShellDialogWindowKind::Create,
            ShellDialogWindowKind::Rename,
            ShellDialogWindowKind::OpenWith,
        ] {
            let state = self.dialog_text_input_state(kind);
            let Some(window) = self.dialog_windows.get(kind) else {
                continue;
            };
            live_windows.insert(window.window_id());
            self.text_input.sync_window(window.window(), state);
        }

        self.text_input.finish_sync(&live_windows);
    }

    fn main_text_input_state(&self) -> Option<ImeState> {
        let location = self.scene.location_draft.as_ref()?;
        let size = self.renderer.as_ref()?.size;
        let pane = self.scene.normalized_pane_id(location.pane);
        let path_bar = self.scene.pane_path_bar_rect(pane, size)?;
        let text_rect = self.scene.location_text_rect_for_path_bar_rect(path_bar);
        let anchor = if location.draft.replace_on_insert {
            0
        } else {
            location.draft.cursor
        };
        Some(editor_ime_state(
            &self.scene,
            &location.draft.value,
            location.draft.cursor,
            anchor,
            location.draft.preedit.as_ref(),
            text_rect,
            TextInputContentPurpose::Url,
        ))
    }

    fn dialog_text_input_state(&self, kind: ShellDialogWindowKind) -> Option<ImeState> {
        let size = self.dialog_windows.layout_size(kind)?;
        let scale = self.scene.ui_scale();
        match kind {
            ShellDialogWindowKind::Create => {
                let dialog = self.scene.create_dialog.as_ref()?;
                let root = create_dialog_rect_scaled(dialog, size, scale);
                let input = create_dialog_input_rect_scaled(root, scale);
                Some(editor_ime_state(
                    &self.scene,
                    &dialog.name,
                    dialog.name.len(),
                    if dialog.replace_on_insert {
                        0
                    } else {
                        dialog.name.len()
                    },
                    dialog.preedit.as_ref(),
                    dialog_input_text_rect(input, scale),
                    TextInputContentPurpose::Name,
                ))
            }
            ShellDialogWindowKind::Rename => {
                let dialog = self.scene.rename_dialog.as_ref()?;
                let root = rename_dialog_rect_scaled(dialog, size, scale);
                let input = rename_dialog_input_rect_scaled(root, scale);
                Some(editor_ime_state(
                    &self.scene,
                    &dialog.name,
                    dialog.name.len(),
                    if dialog.replace_on_insert {
                        0
                    } else {
                        dialog.name.len()
                    },
                    dialog.preedit.as_ref(),
                    dialog_input_text_rect(input, scale),
                    TextInputContentPurpose::Name,
                ))
            }
            ShellDialogWindowKind::OpenWith => {
                let chooser = self.scene.open_with_chooser.as_ref()?;
                let root = open_with_chooser_rect_scaled(chooser, size, scale);
                let text_rect = open_with_chooser_query_text_rect_scaled(root, scale);
                Some(editor_ime_state(
                    &self.scene,
                    &chooser.query,
                    chooser.query_cursor,
                    chooser.query_cursor,
                    chooser.preedit.as_ref(),
                    text_rect,
                    TextInputContentPurpose::Normal,
                ))
            }
            _ => None,
        }
    }

    fn handle_text_input_event(&mut self, window_id: WindowId, event: ImeEvent) {
        match event {
            ImeEvent::Enabled => self.text_input.entered(window_id),
            ImeEvent::Disabled => {
                self.text_input.left(window_id);
                self.apply_text_input_batch_for_window(
                    window_id,
                    ShellTextInputBatch::default(),
                    false,
                );
            }
            ImeEvent::Done {
                serial,
                delete_surrounding,
                commit,
                preedit,
            } => {
                let batch = ShellTextInputBatch {
                    delete_surrounding: delete_surrounding.map(|delete| ShellTextDelete {
                        before_bytes: delete.before_bytes,
                        after_bytes: delete.after_bytes,
                    }),
                    commit,
                    preedit: preedit.and_then(|preedit| {
                        ShellTextPreedit::new(preedit.text, preedit.cursor_range)
                    }),
                };
                tensor_files_dialog_trace!(
                    "[tensor-files] text-input-done window={window_id:?} serial={serial}"
                );
                self.apply_text_input_batch_for_window(window_id, batch, true);
            }
        }
    }

    fn apply_text_input_batch_for_window(
        &mut self,
        window_id: WindowId,
        batch: ShellTextInputBatch,
        input_method_change: bool,
    ) {
        let main_id = self.window.as_ref().map(|window| window.id());
        if main_id == Some(window_id) {
            let size = self
                .renderer
                .as_ref()
                .map(|renderer| renderer.size)
                .unwrap_or_else(|| PhysicalSize::new(1, 1));
            let outcome = self.scene.apply_location_text_input(batch, size);
            if outcome.content_changed && input_method_change {
                self.text_input.note_input_method_change(window_id);
            }
            if outcome.visual_changed {
                self.request_main_redraw();
            }
            return;
        }

        let Some(kind) = self.dialog_windows.window_kind_for_id(window_id) else {
            return;
        };
        let outcome = match kind {
            ShellDialogWindowKind::Create => self.scene.apply_create_text_input(batch),
            ShellDialogWindowKind::Rename => self.scene.apply_rename_text_input(batch),
            ShellDialogWindowKind::OpenWith => self.scene.apply_open_with_text_input(batch),
            _ => return,
        };
        if outcome.content_changed && input_method_change {
            self.text_input.note_input_method_change(window_id);
        }
        if !outcome.visual_changed {
            return;
        }
        self.request_dialog_redraw(kind);
    }

    fn text_input_consumes_key(&self, window: WindowId, event: &windowing::KeyEvent) -> bool {
        self.text_input.consumes_text_key(window, event)
    }
}

fn editor_ime_state(
    scene: &ShellScene,
    value: &str,
    cursor: usize,
    anchor: usize,
    preedit: Option<&ShellTextPreedit>,
    text_rect: ViewRect,
    purpose: TextInputContentPurpose,
) -> ImeState {
    let display_value = text_with_preedit(value, cursor, anchor, preedit);
    let display_cursor = cursor_with_preedit(value, cursor, anchor, preedit);
    let cursor_x = scene.text_hit_tests.borrow_mut().cursor_x(
        &display_value,
        display_cursor,
        TextCursorLayout {
            rect: text_rect,
            alignment: LabelAlignment::Start,
            wrap: LabelWrap::None,
            max_font_size: scene.scale_metric(TEXT_FONT_SIZE),
            max_line_height: scene.text_line_height(),
        },
    );
    let caret_width = scene.scale_metric(1.0).max(1.0);
    let caret_x = (text_rect.x + cursor_x).clamp(
        text_rect.x,
        (text_rect.right() - caret_width).max(text_rect.x),
    );
    let mut state = ImeState::new(value, cursor, anchor, purpose)
        .with_cursor_area(ImeCursorArea::new(
            f64::from(caret_x),
            f64::from(text_rect.y),
            f64::from(caret_width),
            f64::from(text_rect.height.max(1.0)),
        ))
        .with_change_cause(ImeChangeCause::Other);
    state.hints = TextInputContentHint::COMPLETION;
    state
}

fn dialog_input_text_rect(input: ViewRect, scale: f32) -> ViewRect {
    let horizontal = scaled_dialog_metric(10.0, scale);
    let height = scaled_dialog_metric(18.0, scale).min(input.height).max(1.0);
    ViewRect {
        x: input.x + horizontal,
        y: input.y + (input.height - height) / 2.0,
        width: (input.width - horizontal * 2.0).max(1.0),
        height,
    }
}
