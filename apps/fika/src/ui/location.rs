use std::path::PathBuf;

use crate::ui::metrics::PATH_HISTORY_LIMIT;
use crate::ui::pane::ShellPaneId;
use crate::ui::text_input::ShellTextPreedit;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PathHistory {
    pub(crate) back: Vec<PathBuf>,
    pub(crate) forward: Vec<PathBuf>,
}

impl PathHistory {
    pub(crate) fn push_back(&mut self, path: PathBuf) {
        push_limited_path(&mut self.back, path);
    }

    pub(crate) fn push_forward(&mut self, path: PathBuf) {
        push_limited_path(&mut self.forward, path);
    }

    pub(crate) fn clear_forward(&mut self) {
        self.forward.clear();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LocationDraft {
    pub(crate) value: String,
    pub(crate) cursor: usize,
    pub(crate) replace_on_insert: bool,
    pub(crate) preedit: Option<ShellTextPreedit>,
}

impl LocationDraft {
    pub(crate) fn new(value: String) -> Self {
        Self {
            cursor: value.len(),
            value,
            replace_on_insert: true,
            preedit: None,
        }
    }

    pub(crate) fn insert(&mut self, value: &str) {
        self.preedit = None;
        self.prepare_for_edit();
        self.value.insert_str(self.cursor, value);
        self.cursor += value.len();
    }

    pub(crate) fn backspace(&mut self) {
        self.preedit = None;
        if self.replace_on_insert {
            self.value.clear();
            self.cursor = 0;
            self.replace_on_insert = false;
            return;
        }
        let Some(previous) = previous_char_boundary(&self.value, self.cursor) else {
            return;
        };
        self.value.drain(previous..self.cursor);
        self.cursor = previous;
    }

    pub(crate) fn delete(&mut self) {
        self.preedit = None;
        self.replace_on_insert = false;
        let cursor = normalized_text_cursor(&self.value, self.cursor);
        let Some(next) = next_char_boundary(&self.value, cursor) else {
            self.cursor = cursor;
            return;
        };
        self.value.drain(cursor..next);
        self.cursor = cursor;
    }

    pub(crate) fn move_left(&mut self) {
        self.preedit = None;
        self.replace_on_insert = false;
        if let Some(previous) = previous_char_boundary(&self.value, self.cursor) {
            self.cursor = previous;
        }
    }

    pub(crate) fn move_right(&mut self) {
        self.preedit = None;
        self.replace_on_insert = false;
        if let Some(next) = next_char_boundary(&self.value, self.cursor) {
            self.cursor = next;
        }
    }

    pub(crate) fn move_home(&mut self) {
        self.preedit = None;
        self.replace_on_insert = false;
        self.cursor = 0;
    }

    pub(crate) fn move_end(&mut self) {
        self.preedit = None;
        self.replace_on_insert = false;
        self.cursor = self.value.len();
    }

    pub(crate) fn set_cursor(&mut self, cursor: usize) -> bool {
        self.preedit = None;
        let cursor = normalized_text_cursor(&self.value, cursor);
        let changed = self.cursor != cursor || self.replace_on_insert;
        self.cursor = cursor;
        self.replace_on_insert = false;
        changed
    }

    pub(crate) fn set_completed(&mut self, value: String) {
        self.preedit = None;
        self.value = value;
        self.cursor = self.value.len();
        self.replace_on_insert = false;
    }

    fn prepare_for_edit(&mut self) {
        if self.replace_on_insert {
            self.value.clear();
            self.cursor = 0;
            self.replace_on_insert = false;
        }
        self.cursor = normalized_text_cursor(&self.value, self.cursor);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellLocationDraft {
    pub(crate) pane: ShellPaneId,
    pub(crate) draft: LocationDraft,
    pub(crate) purpose: LocationDraftPurpose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocationDraftPurpose {
    Navigate,
    AddNetworkFolder,
}

impl ShellLocationDraft {
    pub(crate) fn new(pane: ShellPaneId, value: String) -> Self {
        Self {
            pane,
            draft: LocationDraft::new(value),
            purpose: LocationDraftPurpose::Navigate,
        }
    }

    pub(crate) fn add_network_folder(pane: ShellPaneId) -> Self {
        Self {
            pane,
            draft: LocationDraft::new("smb://".to_string()),
            purpose: LocationDraftPurpose::AddNetworkFolder,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellPaneHistories {
    histories: [PathHistory; 2],
}

impl ShellPaneHistories {
    pub(crate) fn get(&self, pane: ShellPaneId) -> &PathHistory {
        &self.histories[pane.index()]
    }

    pub(crate) fn get_mut(&mut self, pane: ShellPaneId) -> &mut PathHistory {
        &mut self.histories[pane.index()]
    }

    pub(crate) fn clear(&mut self, pane: ShellPaneId) {
        self.histories[pane.index()] = PathHistory::default();
    }

    pub(crate) fn take(&mut self, pane: ShellPaneId) -> PathHistory {
        std::mem::take(&mut self.histories[pane.index()])
    }

    pub(crate) fn set(&mut self, pane: ShellPaneId, history: PathHistory) {
        self.histories[pane.index()] = history;
    }
}

pub(crate) fn normalized_text_cursor(value: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(value.len());
    while !value.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

fn previous_char_boundary(value: &str, cursor: usize) -> Option<usize> {
    let cursor = normalized_text_cursor(value, cursor);
    value[..cursor]
        .char_indices()
        .last()
        .map(|(index, _)| index)
}

fn next_char_boundary(value: &str, cursor: usize) -> Option<usize> {
    let cursor = normalized_text_cursor(value, cursor);
    if cursor >= value.len() {
        return None;
    }
    value[cursor..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| cursor + index)
        .or(Some(value.len()))
}

fn push_limited_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if paths.last().is_some_and(|existing| existing == &path) {
        return;
    }
    paths.push(path);
    if paths.len() > PATH_HISTORY_LIMIT {
        let overflow = paths.len() - PATH_HISTORY_LIMIT;
        paths.drain(0..overflow);
    }
}
