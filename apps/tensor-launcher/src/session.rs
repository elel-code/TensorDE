use wayland_client_runtime::{
    MAX_SURROUNDING_TEXT_BYTES, TextInputContentHint, TextInputContentPurpose,
    TextInputContentType, TextInputDone, TextInputError, TextInputPreedit, TextInputState,
    TextInputSurroundingText,
};

use crate::{
    DesktopEntry, LaunchError, LaunchPlan, LauncherCatalog, MAX_QUERY_RESULTS, SearchResult,
};

pub const MAX_LAUNCHER_QUERY_BYTES: usize = MAX_SURROUNDING_TEXT_BYTES;

/// Retained launcher interaction state consumed by the native surface.
///
/// Catalog refresh, text-input events, keyboard navigation, and launch planning
/// all enter through this model. Rendering only reads its bounded slices.
#[derive(Clone, Debug)]
pub struct LauncherSession {
    catalog: LauncherCatalog,
    query: String,
    cursor: usize,
    preedit: Option<TextInputPreedit>,
    results: Vec<SearchResult>,
    selected: Option<usize>,
    max_results: usize,
}

impl LauncherSession {
    pub fn new(catalog: LauncherCatalog, max_results: usize) -> Self {
        let max_results = max_results.min(MAX_QUERY_RESULTS);
        let mut session = Self {
            catalog,
            query: String::new(),
            cursor: 0,
            preedit: None,
            results: Vec::with_capacity(max_results.saturating_add(1)),
            selected: None,
            max_results,
        };
        session.refresh_results();
        session
    }

    pub fn catalog(&self) -> &LauncherCatalog {
        &self.catalog
    }

    pub fn replace_catalog(&mut self, catalog: LauncherCatalog) {
        self.catalog = catalog;
        self.refresh_results();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn preedit(&self) -> Option<&TextInputPreedit> {
        self.preedit.as_ref()
    }

    pub fn results(&self) -> &[SearchResult] {
        &self.results
    }

    pub const fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected_entry(&self) -> Option<&DesktopEntry> {
        self.selected
            .and_then(|index| self.results.get(index))
            .map(|result| self.catalog.entry(*result))
    }

    pub fn selected_launch_plan(&self) -> Result<Option<LaunchPlan>, LaunchError> {
        self.selected_entry().map(LaunchPlan::for_entry).transpose()
    }

    pub fn replace_query(&mut self, query: impl Into<String>) -> Result<(), LauncherSessionError> {
        let query = query.into();
        validate_query(&query)?;
        self.cursor = query.len();
        self.query = query;
        self.preedit = None;
        self.refresh_results();
        Ok(())
    }

    pub fn insert_text(&mut self, text: &str) -> Result<(), LauncherSessionError> {
        self.apply_edit(0, 0, Some(text), None)
    }

    pub fn backspace(&mut self) -> Result<bool, LauncherSessionError> {
        let Some(character) = self.query[..self.cursor].chars().next_back() else {
            return Ok(false);
        };
        self.apply_edit(character.len_utf8(), 0, None, None)?;
        Ok(true)
    }

    pub fn apply_text_input(&mut self, done: &TextInputDone) -> Result<(), LauncherSessionError> {
        let (delete_before, delete_after) = done
            .delete_surrounding
            .map(|deletion| (deletion.before_bytes, deletion.after_bytes))
            .unwrap_or((0, 0));
        self.apply_edit(
            delete_before,
            delete_after,
            done.commit.as_deref(),
            done.preedit.clone(),
        )
    }

    pub fn select_next(&mut self) {
        if self.results.is_empty() {
            self.selected = None;
            return;
        }
        self.selected = Some(
            self.selected
                .map_or(0, |index| (index + 1) % self.results.len()),
        );
    }

    pub fn select_previous(&mut self) {
        if self.results.is_empty() {
            self.selected = None;
            return;
        }
        self.selected = Some(match self.selected {
            Some(0) | None => self.results.len() - 1,
            Some(index) => index - 1,
        });
    }

    pub fn select_index(&mut self, index: usize) -> bool {
        if index >= self.results.len() || self.selected == Some(index) {
            return false;
        }
        self.selected = Some(index);
        true
    }

    pub fn text_input_state(&self) -> Result<TextInputState, TextInputError> {
        let surrounding =
            TextInputSurroundingText::new(self.query.clone(), self.cursor, self.cursor)?;
        Ok(TextInputState::new()
            .with_surrounding_text(surrounding)
            .with_content_type(TextInputContentType {
                hints: TextInputContentHint::COMPLETION,
                purpose: TextInputContentPurpose::Normal,
            }))
    }

    fn apply_edit(
        &mut self,
        delete_before: usize,
        delete_after: usize,
        commit: Option<&str>,
        preedit: Option<TextInputPreedit>,
    ) -> Result<(), LauncherSessionError> {
        if delete_before > self.cursor
            || delete_after > self.query.len().saturating_sub(self.cursor)
        {
            return Err(LauncherSessionError::DeleteOutOfBounds);
        }
        let start = self.cursor - delete_before;
        let end = self.cursor + delete_after;
        if !self.query.is_char_boundary(start) || !self.query.is_char_boundary(end) {
            return Err(LauncherSessionError::DeleteSplitsCodepoint);
        }
        let commit = commit.unwrap_or_default();
        if commit.contains('\0') {
            return Err(LauncherSessionError::QueryContainsNul);
        }
        validate_preedit(preedit.as_ref())?;
        let next_len = self
            .query
            .len()
            .saturating_sub(end - start)
            .saturating_add(commit.len());
        if next_len > MAX_LAUNCHER_QUERY_BYTES {
            return Err(LauncherSessionError::QueryTooLong {
                bytes: next_len,
                maximum: MAX_LAUNCHER_QUERY_BYTES,
            });
        }
        self.query.replace_range(start..end, commit);
        self.cursor = start + commit.len();
        self.preedit = preedit;
        self.refresh_results();
        Ok(())
    }

    fn refresh_results(&mut self) {
        self.catalog
            .query_into(&self.query, self.max_results, &mut self.results);
        self.selected = (!self.results.is_empty()).then_some(0);
    }
}

fn validate_query(query: &str) -> Result<(), LauncherSessionError> {
    if query.contains('\0') {
        return Err(LauncherSessionError::QueryContainsNul);
    }
    if query.len() > MAX_LAUNCHER_QUERY_BYTES {
        return Err(LauncherSessionError::QueryTooLong {
            bytes: query.len(),
            maximum: MAX_LAUNCHER_QUERY_BYTES,
        });
    }
    Ok(())
}

fn validate_preedit(preedit: Option<&TextInputPreedit>) -> Result<(), LauncherSessionError> {
    let Some(preedit) = preedit else {
        return Ok(());
    };
    if preedit.text.len() > MAX_LAUNCHER_QUERY_BYTES {
        return Err(LauncherSessionError::PreeditTooLong {
            bytes: preedit.text.len(),
            maximum: MAX_LAUNCHER_QUERY_BYTES,
        });
    }
    if preedit.text.contains('\0') {
        return Err(LauncherSessionError::PreeditContainsNul);
    }
    if let Some(range) = &preedit.cursor_range
        && (range.start > range.end
            || range.end > preedit.text.len()
            || !preedit.text.is_char_boundary(range.start)
            || !preedit.text.is_char_boundary(range.end))
    {
        return Err(LauncherSessionError::InvalidPreeditRange);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LauncherSessionError {
    #[error("launcher query is {bytes} bytes; maximum is {maximum}")]
    QueryTooLong { bytes: usize, maximum: usize },
    #[error("launcher query must not contain NUL bytes")]
    QueryContainsNul,
    #[error("launcher preedit is {bytes} bytes; maximum is {maximum}")]
    PreeditTooLong { bytes: usize, maximum: usize },
    #[error("launcher preedit must not contain NUL bytes")]
    PreeditContainsNul,
    #[error("launcher preedit range is outside its UTF-8 text")]
    InvalidPreeditRange,
    #[error("text-input deletion is outside the launcher query")]
    DeleteOutOfBounds,
    #[error("text-input deletion splits a UTF-8 codepoint")]
    DeleteSplitsCodepoint,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str) -> DesktopEntry {
        LauncherCatalog::parse_entry(
            id,
            &format!("[Desktop Entry]\nType=Application\nName={name}\nExec={id}\n"),
        )
        .unwrap()
        .unwrap()
    }

    fn session() -> LauncherSession {
        LauncherSession::new(
            LauncherCatalog::from_entries(vec![
                entry("browser.desktop", "Web Browser"),
                entry("editor.desktop", "Text Editor"),
                entry("terminal.desktop", "Terminal"),
            ]),
            2,
        )
    }

    #[test]
    fn query_and_selection_are_retained_and_bounded() {
        let mut session = session();
        assert_eq!(session.results().len(), 2);
        session.replace_query("term").unwrap();
        assert_eq!(session.selected_entry().unwrap().id, "terminal.desktop");
        assert_eq!(
            session.selected_launch_plan().unwrap().unwrap().argv,
            ["terminal.desktop"]
        );
    }

    #[test]
    fn utf8_edits_use_protocol_byte_offsets() {
        let mut session = session();
        session.replace_query("终端x").unwrap();
        assert!(session.backspace().unwrap());
        assert_eq!(session.query(), "终端");
        assert_eq!(
            session.apply_edit(1, 0, None, None),
            Err(LauncherSessionError::DeleteSplitsCodepoint)
        );
        session.apply_edit(3, 0, Some("器"), None).unwrap();
        assert_eq!(session.query(), "终器");
    }

    #[test]
    fn selection_wraps_without_leaving_the_result_slice() {
        let mut session = session();
        assert_eq!(session.selected_index(), Some(0));
        session.select_previous();
        assert_eq!(session.selected_index(), Some(1));
        session.select_next();
        assert_eq!(session.selected_index(), Some(0));
        assert!(session.select_index(1));
        assert_eq!(session.selected_index(), Some(1));
        assert!(!session.select_index(2));
    }

    #[test]
    fn query_matches_shared_text_input_limit() {
        let mut session = session();
        session
            .replace_query("a".repeat(MAX_LAUNCHER_QUERY_BYTES))
            .unwrap();
        assert!(session.text_input_state().is_ok());
        assert!(matches!(
            session.insert_text("b"),
            Err(LauncherSessionError::QueryTooLong { .. })
        ));
    }

    #[test]
    fn invalid_preedit_is_not_retained() {
        let mut session = session();
        assert_eq!(
            session.apply_edit(
                0,
                0,
                None,
                Some(TextInputPreedit {
                    text: "终端".to_owned(),
                    cursor_range: Some(1..3),
                }),
            ),
            Err(LauncherSessionError::InvalidPreeditRange)
        );
        assert!(session.preedit().is_none());
    }

    #[test]
    fn catalog_replacement_requeries_without_growing_results() {
        let mut session = session();
        session.replace_query("wea").unwrap();
        session.replace_catalog(LauncherCatalog::from_entries(vec![entry(
            "weather.desktop",
            "Weather",
        )]));
        assert_eq!(session.results().len(), 1);
        assert_eq!(session.selected_entry().unwrap().id, "weather.desktop");
    }
}
