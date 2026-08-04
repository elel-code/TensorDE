use std::borrow::Cow;
use std::ops::Range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellTextPreedit {
    pub(crate) text: String,
    pub(crate) cursor_range: Option<Range<usize>>,
}

impl ShellTextPreedit {
    pub(crate) fn new(text: String, cursor_range: Option<Range<usize>>) -> Option<Self> {
        (!text.is_empty()).then_some(Self { text, cursor_range })
    }

    fn cursor(&self) -> usize {
        self.cursor_range
            .as_ref()
            .map(|range| range.end)
            .filter(|cursor| *cursor <= self.text.len() && self.text.is_char_boundary(*cursor))
            .unwrap_or(self.text.len())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShellTextDelete {
    pub(crate) before_bytes: usize,
    pub(crate) after_bytes: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellTextInputBatch {
    pub(crate) delete_surrounding: Option<ShellTextDelete>,
    pub(crate) commit: Option<String>,
    pub(crate) preedit: Option<ShellTextPreedit>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShellTextSelection {
    pub(crate) cursor: usize,
    pub(crate) anchor: usize,
}

impl ShellTextSelection {
    pub(crate) const fn caret(cursor: usize) -> Self {
        Self {
            cursor,
            anchor: cursor,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellTextInputOutcome {
    pub(crate) content_changed: bool,
    pub(crate) visual_changed: bool,
}

/// Apply one atomic text-input-v3 batch to an editor's committed buffer.
///
/// The existing preedit is replaced first, deletion uses UTF-8 byte offsets
/// around the committed cursor, commit replaces any remaining selection, and
/// the new preedit remains separate from the committed buffer.
pub(crate) fn apply_text_input_batch(
    value: &mut String,
    selection: &mut ShellTextSelection,
    preedit: &mut Option<ShellTextPreedit>,
    batch: ShellTextInputBatch,
) -> ShellTextInputOutcome {
    normalize_selection(value, selection);
    let original_value = value.clone();
    let original_selection = *selection;
    let original_preedit = preedit.clone();

    *preedit = None;
    if let Some(delete) = batch.delete_surrounding {
        apply_delete(value, selection, delete);
    }
    if let Some(commit) = batch.commit {
        replace_selection(value, selection, &commit);
    }
    *preedit = batch.preedit;

    let content_changed = *value != original_value || *selection != original_selection;
    ShellTextInputOutcome {
        content_changed,
        visual_changed: content_changed || *preedit != original_preedit,
    }
}

pub(crate) fn text_with_preedit<'a>(
    value: &'a str,
    cursor: usize,
    anchor: usize,
    preedit: Option<&ShellTextPreedit>,
) -> Cow<'a, str> {
    let Some(preedit) = preedit else {
        return Cow::Borrowed(value);
    };
    let cursor = normalized_boundary(value, cursor);
    let anchor = normalized_boundary(value, anchor);
    let start = cursor.min(anchor);
    let end = cursor.max(anchor);
    let mut composed = String::with_capacity(value.len() - (end - start) + preedit.text.len());
    composed.push_str(&value[..start]);
    composed.push_str(&preedit.text);
    composed.push_str(&value[end..]);
    Cow::Owned(composed)
}

pub(crate) fn cursor_with_preedit(
    value: &str,
    cursor: usize,
    anchor: usize,
    preedit: Option<&ShellTextPreedit>,
) -> usize {
    let cursor = normalized_boundary(value, cursor);
    let anchor = normalized_boundary(value, anchor);
    preedit.map_or(cursor, |preedit| cursor.min(anchor) + preedit.cursor())
}

fn apply_delete(value: &mut String, selection: &mut ShellTextSelection, delete: ShellTextDelete) {
    let Some(start) = selection.cursor.checked_sub(delete.before_bytes) else {
        return;
    };
    let Some(end) = selection.cursor.checked_add(delete.after_bytes) else {
        return;
    };
    if end > value.len() || !value.is_char_boundary(start) || !value.is_char_boundary(end) {
        return;
    }
    value.replace_range(start..end, "");
    selection.cursor = offset_after_delete(selection.cursor, start..end);
    selection.anchor = offset_after_delete(selection.anchor, start..end);
}

fn replace_selection(value: &mut String, selection: &mut ShellTextSelection, replacement: &str) {
    let start = selection.cursor.min(selection.anchor);
    let end = selection.cursor.max(selection.anchor);
    value.replace_range(start..end, replacement);
    selection.cursor = start + replacement.len();
    selection.anchor = selection.cursor;
}

fn normalize_selection(value: &str, selection: &mut ShellTextSelection) {
    selection.cursor = normalized_boundary(value, selection.cursor);
    selection.anchor = normalized_boundary(value, selection.anchor);
}

fn normalized_boundary(value: &str, offset: usize) -> usize {
    let mut offset = offset.min(value.len());
    while !value.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn offset_after_delete(offset: usize, deleted: Range<usize>) -> usize {
    if offset <= deleted.start {
        offset
    } else if offset < deleted.end {
        deleted.start
    } else {
        offset - (deleted.end - deleted.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_deletes_utf8_bytes_then_commits_at_the_updated_cursor() {
        let mut value = "aβcd".to_string();
        let mut selection = ShellTextSelection::caret("aβc".len());
        let mut preedit = None;

        let outcome = apply_text_input_batch(
            &mut value,
            &mut selection,
            &mut preedit,
            ShellTextInputBatch {
                delete_surrounding: Some(ShellTextDelete {
                    before_bytes: "βc".len(),
                    after_bytes: 0,
                }),
                commit: Some("中".to_string()),
                preedit: None,
            },
        );

        assert_eq!(value, "a中d");
        assert_eq!(selection, ShellTextSelection::caret("a中".len()));
        assert!(outcome.content_changed);
    }

    #[test]
    fn commit_replaces_selection_and_preedit_stays_out_of_committed_text() {
        let mut value = "New Folder".to_string();
        let mut selection = ShellTextSelection {
            cursor: value.len(),
            anchor: 0,
        };
        let mut preedit = None;

        apply_text_input_batch(
            &mut value,
            &mut selection,
            &mut preedit,
            ShellTextInputBatch {
                commit: Some("文件".to_string()),
                preedit: ShellTextPreedit::new("夹".to_string(), Some(3..3)),
                ..ShellTextInputBatch::default()
            },
        );

        assert_eq!(value, "文件");
        assert_eq!(
            text_with_preedit(&value, selection.cursor, selection.anchor, preedit.as_ref()),
            "文件夹"
        );
        assert_eq!(
            cursor_with_preedit(&value, selection.cursor, selection.anchor, preedit.as_ref()),
            9
        );
    }

    #[test]
    fn preedit_visually_replaces_selection_without_mutating_committed_text() {
        let value = "New Folder";
        let preedit = ShellTextPreedit::new("中".to_string(), Some(3..3));

        assert_eq!(
            text_with_preedit(value, value.len(), 0, preedit.as_ref()),
            "中"
        );
        assert_eq!(
            cursor_with_preedit(value, value.len(), 0, preedit.as_ref()),
            3
        );
    }

    #[test]
    fn invalid_utf8_delete_is_ignored_without_losing_commit() {
        let mut value = "aβc".to_string();
        let mut selection = ShellTextSelection::caret(3);
        let mut preedit = None;

        apply_text_input_batch(
            &mut value,
            &mut selection,
            &mut preedit,
            ShellTextInputBatch {
                delete_surrounding: Some(ShellTextDelete {
                    before_bytes: 1,
                    after_bytes: 0,
                }),
                commit: Some("x".to_string()),
                preedit: None,
            },
        );

        assert_eq!(value, "aβxc");
    }
}
