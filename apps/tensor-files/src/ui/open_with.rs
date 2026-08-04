use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use tensor_files_core::{DesktopLaunchPlan, MimeApplication, MimeApplicationCache};

use crate::ui::file_item_view::text::estimated_text_cursor_for_offset;
use crate::ui::metrics::OPEN_WITH_CHOOSER_MAX_ROWS;
use crate::ui::shortcuts::OpenWithCommand;
use crate::ui::text_input::ShellTextPreedit;

#[path = "open_with/geometry.rs"]
pub(crate) mod geometry;
#[path = "open_with/launch.rs"]
pub(crate) mod launch;
#[path = "open_with/paint.rs"]
pub(crate) mod paint;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellOpenWithChooser {
    pub(crate) path: PathBuf,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) applications: Vec<MimeApplication>,
    pub(crate) application_categories: Vec<Vec<String>>,
    pub(crate) expanded_categories: BTreeSet<OpenWithCategoryKey>,
    pub(crate) query: String,
    pub(crate) query_cursor: usize,
    pub(crate) preedit: Option<ShellTextPreedit>,
    pub(crate) selected_index: usize,
    pub(crate) scroll_row: usize,
    pub(crate) set_as_default: bool,
    pub(crate) error: Option<String>,
}

impl ShellOpenWithChooser {
    pub(crate) fn new(
        path: PathBuf,
        mime_type: Option<Arc<str>>,
        applications: Vec<MimeApplication>,
        application_categories: Vec<Vec<String>>,
    ) -> Self {
        let mut application_categories = application_categories;
        application_categories.truncate(applications.len());
        application_categories.resize_with(applications.len(), Vec::new);
        let mut chooser = Self {
            path,
            mime_type,
            selected_index: 0,
            applications,
            application_categories,
            expanded_categories: BTreeSet::new(),
            query: String::new(),
            query_cursor: 0,
            preedit: None,
            scroll_row: 0,
            set_as_default: false,
            error: None,
        };
        chooser.ensure_selected_visible();
        chooser
    }

    pub(crate) fn filtered_indexes(&self) -> Vec<usize> {
        open_with_matching_application_indexes(&self.applications, &self.query)
    }

    pub(crate) fn filtered_count(&self) -> usize {
        self.filtered_indexes().len()
    }

    pub(crate) fn category_rows(&self) -> Vec<OpenWithCategoryRow> {
        open_with_category_rows_for_indexes(&self.application_categories, &self.filtered_indexes())
    }

    pub(crate) fn selected_category_row(&self) -> Option<OpenWithCategoryRow> {
        match self.tree_rows().get(self.selected_index)? {
            OpenWithTreeRow::Category { category, .. } => Some(*category),
            OpenWithTreeRow::Application { app_index } => self
                .application_categories
                .get(*app_index)
                .and_then(|categories| {
                    self.category_rows().into_iter().find(|category| {
                        open_with_application_matches_category(categories, Some(category))
                    })
                }),
        }
    }

    pub(crate) fn tree_rows(&self) -> Vec<OpenWithTreeRow> {
        let indexes = self.filtered_indexes();
        let categories =
            open_with_category_rows_for_indexes(&self.application_categories, &indexes);
        let mut rows = Vec::new();
        for category in categories {
            let expanded = self.category_is_expanded(category.key);
            rows.push(OpenWithTreeRow::Category { category, expanded });
            if expanded {
                for app_index in &indexes {
                    let categories = self
                        .application_categories
                        .get(*app_index)
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    if open_with_application_matches_category(categories, Some(&category)) {
                        rows.push(OpenWithTreeRow::Application {
                            app_index: *app_index,
                        });
                    }
                }
            }
        }
        rows
    }

    pub(crate) fn tree_row_count(&self) -> usize {
        self.tree_rows().len()
    }

    pub(crate) fn visible_tree_rows(&self) -> Vec<OpenWithTreeRow> {
        let rows = self.tree_rows();
        rows.into_iter()
            .skip(self.scroll_row)
            .take(OPEN_WITH_CHOOSER_MAX_ROWS)
            .collect()
    }

    pub(crate) fn selected_application(&self) -> Option<&MimeApplication> {
        let OpenWithTreeRow::Application { app_index } =
            *self.tree_rows().get(self.selected_index)?
        else {
            return None;
        };
        self.applications.get(app_index)
    }

    pub(crate) fn apply_command(&mut self, command: OpenWithCommand) -> bool {
        let old = self.clone();
        if matches!(
            &command,
            OpenWithCommand::Insert(_)
                | OpenWithCommand::Backspace
                | OpenWithCommand::Delete
                | OpenWithCommand::MoveLeft
                | OpenWithCommand::MoveRight
                | OpenWithCommand::MoveHome
                | OpenWithCommand::MoveEnd
        ) {
            self.preedit = None;
        }
        match command {
            OpenWithCommand::Insert(value) => {
                self.insert_query_text(&value);
            }
            OpenWithCommand::Backspace => {
                self.backspace_query_text();
            }
            OpenWithCommand::Delete => {
                self.delete_query_text();
            }
            OpenWithCommand::MoveLeft => {
                if self.query.is_empty() {
                    self.collapse_selected_category();
                } else {
                    self.move_query_cursor_left();
                }
            }
            OpenWithCommand::MoveRight => {
                if self.query.is_empty() {
                    self.expand_selected_category();
                } else {
                    self.move_query_cursor_right();
                }
            }
            OpenWithCommand::MoveHome => {
                self.query_cursor = 0;
            }
            OpenWithCommand::MoveEnd => {
                self.query_cursor = self.query.len();
            }
            OpenWithCommand::Cancel => return false,
            OpenWithCommand::MoveUp => self.move_selection(-1),
            OpenWithCommand::MoveDown => self.move_selection(1),
            OpenWithCommand::Commit | OpenWithCommand::Ignore => return false,
        }
        self.query_cursor = normalized_open_with_query_cursor(&self.query, self.query_cursor);
        self.ensure_selected_visible();
        old != *self
    }

    pub(crate) fn query_cursor_for_text_offset(&self, offset_x: f32, font_size: f32) -> usize {
        estimated_text_cursor_for_offset(&self.query, offset_x, font_size)
    }

    pub(crate) fn set_query_cursor(&mut self, cursor: usize) -> bool {
        self.preedit = None;
        let cursor = normalized_open_with_query_cursor(&self.query, cursor);
        if self.query_cursor == cursor {
            return false;
        }
        self.query_cursor = cursor;
        self.error = None;
        true
    }

    pub(crate) fn select_filtered_row(&mut self, row: usize) -> bool {
        self.select_tree_row(row)
    }

    pub(crate) fn select_tree_row(&mut self, row: usize) -> bool {
        let count = self.tree_row_count();
        if count == 0 {
            return false;
        }
        let old = self.clone();
        self.selected_index = row.min(count - 1);
        if let Some(OpenWithTreeRow::Category { category, .. }) =
            self.tree_rows().get(self.selected_index)
        {
            self.toggle_category(category.key);
        }
        self.error = None;
        self.ensure_selected_visible();
        old != *self
    }

    pub(crate) fn toggle_set_as_default(&mut self) -> bool {
        if self.mime_type.is_none() {
            return false;
        }
        self.set_as_default = !self.set_as_default;
        self.error = None;
        true
    }

    pub(crate) fn scroll_rows(&mut self, delta: isize) -> bool {
        let count = self.tree_row_count();
        if count <= OPEN_WITH_CHOOSER_MAX_ROWS {
            return false;
        }
        let old_scroll = self.scroll_row;
        let max_scroll = count.saturating_sub(OPEN_WITH_CHOOSER_MAX_ROWS);
        self.scroll_row = if delta < 0 {
            self.scroll_row.saturating_sub(delta.unsigned_abs())
        } else {
            (self.scroll_row + delta as usize).min(max_scroll)
        };
        old_scroll != self.scroll_row
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.tree_row_count();
        if count == 0 {
            self.selected_index = 0;
            self.scroll_row = 0;
            return;
        }
        let current = self.selected_index.min(count - 1);
        self.selected_index = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            (current + delta as usize).min(count - 1)
        };
    }

    fn insert_query_text(&mut self, value: &str) {
        self.query_cursor = normalized_open_with_query_cursor(&self.query, self.query_cursor);
        self.query.insert_str(self.query_cursor, value);
        self.query_cursor += value.len();
        self.query_text_changed();
    }

    fn backspace_query_text(&mut self) {
        let Some(previous) = previous_open_with_query_boundary(&self.query, self.query_cursor)
        else {
            return;
        };
        self.query.drain(previous..self.query_cursor);
        self.query_cursor = previous;
        self.query_text_changed();
    }

    fn delete_query_text(&mut self) {
        let cursor = normalized_open_with_query_cursor(&self.query, self.query_cursor);
        let Some(next) = next_open_with_query_boundary(&self.query, cursor) else {
            self.query_cursor = cursor;
            return;
        };
        self.query.drain(cursor..next);
        self.query_cursor = cursor;
        self.query_text_changed();
    }

    fn move_query_cursor_left(&mut self) {
        if let Some(previous) = previous_open_with_query_boundary(&self.query, self.query_cursor) {
            self.query_cursor = previous;
        }
    }

    fn move_query_cursor_right(&mut self) {
        if let Some(next) = next_open_with_query_boundary(&self.query, self.query_cursor) {
            self.query_cursor = next;
        }
    }

    pub(crate) fn query_text_changed(&mut self) {
        self.scroll_row = 0;
        self.error = None;
        self.selected_index = self.first_application_row().unwrap_or(0);
    }

    fn collapse_selected_category(&mut self) {
        let rows = self.tree_rows();
        match rows.get(self.selected_index).copied() {
            Some(OpenWithTreeRow::Category { category, .. }) => {
                self.expanded_categories.remove(&category.key);
            }
            Some(OpenWithTreeRow::Application { .. }) => {
                if let Some(parent_row) = self.parent_category_row_for_selected_application(&rows) {
                    self.selected_index = parent_row;
                }
            }
            None => {}
        }
        self.error = None;
    }

    fn expand_selected_category(&mut self) {
        if let Some(OpenWithTreeRow::Category { category, .. }) =
            self.tree_rows().get(self.selected_index).copied()
        {
            self.expanded_categories.insert(category.key);
            self.error = None;
        }
    }

    fn parent_category_row_for_selected_application(
        &self,
        rows: &[OpenWithTreeRow],
    ) -> Option<usize> {
        rows.iter()
            .take(self.selected_index)
            .enumerate()
            .rev()
            .find_map(|(row, tree_row)| {
                matches!(tree_row, OpenWithTreeRow::Category { .. }).then_some(row)
            })
    }

    fn toggle_category(&mut self, key: OpenWithCategoryKey) {
        if !self.query.is_empty() {
            return;
        }
        if !self.expanded_categories.remove(&key) {
            self.expanded_categories.insert(key);
        }
    }

    fn ensure_selected_visible(&mut self) {
        let count = self.tree_row_count();
        if count == 0 {
            self.selected_index = 0;
            self.scroll_row = 0;
            return;
        }
        self.selected_index = self.selected_index.min(count - 1);
        if self.selected_index < self.scroll_row {
            self.scroll_row = self.selected_index;
        } else if self.selected_index >= self.scroll_row + OPEN_WITH_CHOOSER_MAX_ROWS {
            self.scroll_row = self.selected_index + 1 - OPEN_WITH_CHOOSER_MAX_ROWS;
        }
        self.scroll_row = self
            .scroll_row
            .min(count.saturating_sub(OPEN_WITH_CHOOSER_MAX_ROWS));
    }

    fn first_application_row(&self) -> Option<usize> {
        self.tree_rows()
            .iter()
            .position(|row| matches!(row, OpenWithTreeRow::Application { .. }))
    }

    fn category_is_expanded(&self, key: OpenWithCategoryKey) -> bool {
        !self.query.is_empty() || self.expanded_categories.contains(&key)
    }
}

fn normalized_open_with_query_cursor(value: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(value.len());
    while !value.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

fn previous_open_with_query_boundary(value: &str, cursor: usize) -> Option<usize> {
    let cursor = normalized_open_with_query_cursor(value, cursor);
    value[..cursor]
        .char_indices()
        .last()
        .map(|(index, _)| index)
}

fn next_open_with_query_boundary(value: &str, cursor: usize) -> Option<usize> {
    let cursor = normalized_open_with_query_cursor(value, cursor);
    if cursor >= value.len() {
        return None;
    }
    value[cursor..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| cursor + index)
        .or(Some(value.len()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenWithTreeRow {
    Category {
        category: OpenWithCategoryRow,
        expanded: bool,
    },
    Application {
        app_index: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OpenWithCategoryRow {
    pub(crate) key: OpenWithCategoryKey,
    pub(crate) label: &'static str,
    pub(crate) icon: &'static str,
    pub(crate) count: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum OpenWithCategoryKey {
    Known(&'static str),
    Other,
}

struct OpenWithCategoryDefinition {
    key: &'static str,
    label: &'static str,
    icon: &'static str,
    desktop_categories: &'static [&'static str],
}

const OPEN_WITH_CATEGORY_DEFINITIONS: &[OpenWithCategoryDefinition] = &[
    OpenWithCategoryDefinition {
        key: "accessories",
        label: "Accessories",
        icon: "applications-accessories",
        desktop_categories: &["Utility"],
    },
    OpenWithCategoryDefinition {
        key: "education",
        label: "Education",
        icon: "applications-education",
        desktop_categories: &["Education"],
    },
    OpenWithCategoryDefinition {
        key: "game",
        label: "Games",
        icon: "applications-games",
        desktop_categories: &["Game"],
    },
    OpenWithCategoryDefinition {
        key: "graphics",
        label: "Graphics",
        icon: "applications-graphics",
        desktop_categories: &["Graphics"],
    },
    OpenWithCategoryDefinition {
        key: "internet",
        label: "Internet",
        icon: "applications-internet",
        desktop_categories: &["Network"],
    },
    OpenWithCategoryDefinition {
        key: "office",
        label: "Office",
        icon: "applications-office",
        desktop_categories: &["Office"],
    },
    OpenWithCategoryDefinition {
        key: "programming",
        label: "Programming",
        icon: "applications-development",
        desktop_categories: &["Development"],
    },
    OpenWithCategoryDefinition {
        key: "science",
        label: "Science",
        icon: "applications-science",
        desktop_categories: &["Science"],
    },
    OpenWithCategoryDefinition {
        key: "settings",
        label: "Settings",
        icon: "preferences-system",
        desktop_categories: &["Settings"],
    },
    OpenWithCategoryDefinition {
        key: "sound-video",
        label: "Sound & Video",
        icon: "applications-multimedia",
        desktop_categories: &["AudioVideo", "Audio", "Video"],
    },
    OpenWithCategoryDefinition {
        key: "system-tools",
        label: "System Tools",
        icon: "applications-system",
        desktop_categories: &["System"],
    },
    OpenWithCategoryDefinition {
        key: "wine",
        label: "Wine",
        icon: "wine",
        desktop_categories: &["Wine"],
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OpenWithLaunchRequest {
    pub(crate) path: PathBuf,
    pub(crate) app_name: String,
    pub(crate) default_update: Option<OpenWithDefaultUpdate>,
    pub(crate) plan: DesktopLaunchPlan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OpenWithDefaultUpdate {
    pub(crate) mime_type: String,
    pub(crate) desktop_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenWithChooserClick {
    Outside,
    Inside,
    Cancel,
    Open,
    ToggleDefault,
    Query(usize),
    Row(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenWithChooserPointerRole {
    Default,
    Text,
    Action,
}

pub(crate) fn open_with_applications_for_mime(
    cache: &MimeApplicationCache,
    mime: Option<&str>,
) -> Vec<MimeApplication> {
    let mut applications = Vec::new();
    let mut seen = BTreeSet::new();
    let associated = mime
        .map(|mime| cache.applications_for_mime(mime))
        .unwrap_or_default();
    for application in associated.into_iter().chain(cache.all_applications()) {
        if seen.insert(application.id.clone()) {
            applications.push(application);
        }
    }
    applications
}

pub(crate) fn open_with_application_categories_for_applications(
    cache: &MimeApplicationCache,
    applications: &[MimeApplication],
) -> Vec<Vec<String>> {
    applications
        .iter()
        .map(|application| {
            cache
                .application(&application.id)
                .map(|application| application.categories.clone())
                .unwrap_or_default()
        })
        .collect()
}

fn open_with_matching_application_indexes(
    applications: &[MimeApplication],
    query: &str,
) -> Vec<usize> {
    let terms = query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .collect::<Vec<_>>();
    applications
        .iter()
        .enumerate()
        .filter(|(_, application)| open_with_application_matches_terms(application, &terms))
        .map(|(index, _)| index)
        .collect()
}

pub(crate) fn open_with_category_rows_for_indexes(
    application_categories: &[Vec<String>],
    application_indexes: &[usize],
) -> Vec<OpenWithCategoryRow> {
    let mut rows = Vec::new();
    let mut other_pushed = false;
    for definition in OPEN_WITH_CATEGORY_DEFINITIONS {
        if definition.key == "programming" && !other_pushed {
            push_open_with_other_category_row(
                application_categories,
                application_indexes,
                &mut rows,
            );
            other_pushed = true;
        }
        let count = application_categories
            .iter()
            .enumerate()
            .filter(|(index, _)| application_indexes.contains(index))
            .map(|(_, categories)| categories)
            .filter(|categories| {
                open_with_categories_match_any(categories, definition.desktop_categories)
            })
            .count();
        if count > 0 {
            rows.push(OpenWithCategoryRow {
                key: OpenWithCategoryKey::Known(definition.key),
                label: definition.label,
                icon: definition.icon,
                count,
            });
        }
    }
    if !other_pushed {
        push_open_with_other_category_row(application_categories, application_indexes, &mut rows);
    }
    rows
}

fn push_open_with_other_category_row(
    application_categories: &[Vec<String>],
    application_indexes: &[usize],
    rows: &mut Vec<OpenWithCategoryRow>,
) {
    let other_count = application_categories
        .iter()
        .enumerate()
        .filter(|(index, _)| application_indexes.contains(index))
        .map(|(_, categories)| categories)
        .filter(|categories| !open_with_categories_have_known_category(categories))
        .count();
    if other_count > 0 {
        rows.push(OpenWithCategoryRow {
            key: OpenWithCategoryKey::Other,
            label: "Other",
            icon: "applications-other",
            count: other_count,
        });
    }
}

fn open_with_application_matches_terms(application: &MimeApplication, terms: &[String]) -> bool {
    if terms.is_empty() {
        return true;
    }
    let haystacks = [
        application.name.to_ascii_lowercase(),
        application.id.to_ascii_lowercase(),
        application.exec.to_ascii_lowercase(),
        application
            .desktop_file
            .display()
            .to_string()
            .to_ascii_lowercase(),
        application
            .icon
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    ];
    terms.iter().all(|term| {
        haystacks
            .iter()
            .any(|haystack| haystack.contains(term.as_str()))
    })
}

fn open_with_application_matches_category(
    categories: &[String],
    category: Option<&OpenWithCategoryRow>,
) -> bool {
    match category.map(|category| category.key) {
        None => true,
        Some(OpenWithCategoryKey::Known(key)) => {
            open_with_category_definition(key).is_some_and(|definition| {
                open_with_categories_match_any(categories, definition.desktop_categories)
            })
        }
        Some(OpenWithCategoryKey::Other) => !open_with_categories_have_known_category(categories),
    }
}

fn open_with_category_definition(key: &str) -> Option<&'static OpenWithCategoryDefinition> {
    OPEN_WITH_CATEGORY_DEFINITIONS
        .iter()
        .find(|definition| definition.key == key)
}

fn open_with_categories_have_known_category(categories: &[String]) -> bool {
    OPEN_WITH_CATEGORY_DEFINITIONS
        .iter()
        .any(|definition| open_with_categories_match_any(categories, definition.desktop_categories))
}

fn open_with_categories_match_any(categories: &[String], expected: &[&str]) -> bool {
    categories
        .iter()
        .any(|category| expected.iter().any(|expected| category == expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str) -> MimeApplication {
        MimeApplication {
            id: id.to_string(),
            desktop_file: PathBuf::from(format!("/apps/{id}")),
            name: name.to_string(),
            exec: format!("{name} %f"),
            icon: None,
            is_default: false,
        }
    }

    #[test]
    fn category_rows_follow_desktop_main_categories() {
        let indexes = [0, 1, 2];
        let rows = open_with_category_rows_for_indexes(
            &[
                vec!["Graphics".to_string(), "Viewer".to_string()],
                vec!["Network".to_string()],
                vec!["Viewer".to_string()],
            ],
            &indexes,
        );

        assert_eq!(
            rows.iter().map(|row| row.label).collect::<Vec<_>>(),
            vec!["Graphics", "Internet", "Other"]
        );
        assert_eq!(rows[0].count, 1);
        assert_eq!(rows[2].count, 1);
    }

    #[test]
    fn chooser_filters_applications_by_selected_category() {
        let mut chooser = ShellOpenWithChooser::new(
            PathBuf::from("/tmp/file.txt"),
            Some(Arc::from("text/plain")),
            vec![
                app("viewer.desktop", "Viewer"),
                app("browser.desktop", "Browser"),
            ],
            vec![vec!["Graphics".to_string()], vec!["Network".to_string()]],
        );

        assert!(chooser.select_tree_row(0));
        assert!(chooser.select_tree_row(1));

        let selected = chooser.selected_application().unwrap();
        assert_eq!(selected.id, "viewer.desktop");
        assert_eq!(chooser.selected_index, 1);
    }
}
