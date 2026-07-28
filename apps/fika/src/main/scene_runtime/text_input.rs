impl ShellScene {
    fn apply_location_text_input(
        &mut self,
        batch: ShellTextInputBatch,
        size: PhysicalSize<u32>,
    ) -> ShellTextInputOutcome {
        let Some(location) = self.location_draft.as_mut() else {
            return ShellTextInputOutcome::default();
        };
        let anchor = if location.draft.replace_on_insert {
            0
        } else {
            location.draft.cursor
        };
        let mut selection = ShellTextSelection {
            cursor: location.draft.cursor,
            anchor,
        };
        let outcome = apply_text_input_batch(
            &mut location.draft.value,
            &mut selection,
            &mut location.draft.preedit,
            batch,
        );
        location.draft.cursor = selection.cursor;
        if outcome.content_changed {
            location.draft.replace_on_insert = false;
        }
        if outcome.visual_changed {
            self.reset_text_caret_blink();
            self.location_changes += 1;
            self.rubber_band = None;
            self.clamp_scroll(size);
        }
        outcome
    }

    fn apply_create_text_input(
        &mut self,
        batch: ShellTextInputBatch,
    ) -> ShellTextInputOutcome {
        let Some(dialog) = self.create_dialog.as_mut() else {
            return ShellTextInputOutcome::default();
        };
        let mut selection = ShellTextSelection {
            cursor: dialog.name.len(),
            anchor: if dialog.replace_on_insert {
                0
            } else {
                dialog.name.len()
            },
        };
        let outcome = apply_text_input_batch(
            &mut dialog.name,
            &mut selection,
            &mut dialog.preedit,
            batch,
        );
        if outcome.content_changed {
            dialog.replace_on_insert = false;
            dialog.error = None;
        }
        if outcome.visual_changed {
            self.reset_text_caret_blink();
            self.create_changes += 1;
        }
        outcome
    }

    fn apply_rename_text_input(
        &mut self,
        batch: ShellTextInputBatch,
    ) -> ShellTextInputOutcome {
        let Some(dialog) = self.rename_dialog.as_mut() else {
            return ShellTextInputOutcome::default();
        };
        let mut selection = ShellTextSelection {
            cursor: dialog.name.len(),
            anchor: if dialog.replace_on_insert {
                0
            } else {
                dialog.name.len()
            },
        };
        let outcome = apply_text_input_batch(
            &mut dialog.name,
            &mut selection,
            &mut dialog.preedit,
            batch,
        );
        if outcome.content_changed {
            dialog.replace_on_insert = false;
            dialog.error = None;
        }
        if outcome.visual_changed {
            self.reset_text_caret_blink();
            self.rename_changes += 1;
        }
        outcome
    }

    fn apply_open_with_text_input(
        &mut self,
        batch: ShellTextInputBatch,
    ) -> ShellTextInputOutcome {
        let Some(chooser) = self.open_with_chooser.as_mut() else {
            return ShellTextInputOutcome::default();
        };
        let mut selection = ShellTextSelection::caret(chooser.query_cursor);
        let outcome = apply_text_input_batch(
            &mut chooser.query,
            &mut selection,
            &mut chooser.preedit,
            batch,
        );
        chooser.query_cursor = selection.cursor;
        if outcome.content_changed {
            chooser.query_text_changed();
        }
        if outcome.visual_changed {
            self.reset_text_caret_blink();
            self.open_with_changes += 1;
        }
        outcome
    }
}
