use super::model::DirectoryModel;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum NameFilterMode {
    PlainText,
    #[default]
    Glob,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NameFilter {
    pattern: String,
    mode: NameFilterMode,
    case_sensitive: bool,
}

impl NameFilter {
    pub fn glob(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            mode: NameFilterMode::Glob,
            case_sensitive: false,
        }
    }

    pub fn plain_text(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            mode: NameFilterMode::PlainText,
            case_sensitive: false,
        }
    }

    pub fn with_case_sensitive(mut self, case_sensitive: bool) -> Self {
        self.case_sensitive = case_sensitive;
        self
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn mode(&self) -> NameFilterMode {
        self.mode
    }

    pub fn is_case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub fn is_empty(&self) -> bool {
        self.pattern.is_empty()
    }

    pub fn matches_name(&self, name: &str) -> bool {
        if self.pattern.is_empty() {
            return true;
        }

        match self.mode {
            NameFilterMode::PlainText => contains_text(name, &self.pattern, self.case_sensitive),
            NameFilterMode::Glob => {
                wildcard_matches_unanchored(&self.pattern, name, self.case_sensitive)
            }
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FilteredModel {
    indexes: Arc<[usize]>,
}

impl FilteredModel {
    pub fn from_model(model: &DirectoryModel, filter: &NameFilter) -> Self {
        let matcher = NameFilterMatcher::new(filter);
        let indexes = model
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| matcher.matches_name(&entry.name).then_some(index))
            .collect::<Vec<_>>()
            .into();
        Self { indexes }
    }

    pub fn len(&self) -> usize {
        self.indexes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indexes.is_empty()
    }

    pub fn as_slice(&self) -> &[usize] {
        &self.indexes
    }

    pub fn model_index(&self, layout_index: usize) -> Option<usize> {
        self.indexes.get(layout_index).copied()
    }

    pub fn layout_index_for_model_index(&self, model_index: usize) -> Option<usize> {
        self.indexes.binary_search(&model_index).ok()
    }

    pub fn iter_model_indexes(&self) -> impl Iterator<Item = usize> + '_ {
        self.indexes.iter().copied()
    }
}

struct NameFilterMatcher<'a> {
    filter: &'a NameFilter,
    plain_pattern: Option<String>,
    glob_pattern: Option<Vec<char>>,
}

impl<'a> NameFilterMatcher<'a> {
    fn new(filter: &'a NameFilter) -> Self {
        let plain_pattern = (filter.mode == NameFilterMode::PlainText && !filter.case_sensitive)
            .then(|| filter.pattern.to_lowercase());
        let glob_pattern = (filter.mode == NameFilterMode::Glob).then(|| {
            let mut pattern = Vec::with_capacity(filter.pattern.chars().count() + 2);
            pattern.push('*');
            pattern.extend(normalized_chars(&filter.pattern, filter.case_sensitive));
            pattern.push('*');
            pattern
        });
        Self {
            filter,
            plain_pattern,
            glob_pattern,
        }
    }

    fn matches_name(&self, name: &str) -> bool {
        if self.filter.pattern.is_empty() {
            return true;
        }

        match self.filter.mode {
            NameFilterMode::PlainText => {
                if self.filter.case_sensitive {
                    name.contains(&self.filter.pattern)
                } else {
                    name.to_lowercase()
                        .contains(self.plain_pattern.as_deref().unwrap_or_default())
                }
            }
            NameFilterMode::Glob => wildcard_full_match(
                self.glob_pattern.as_deref().unwrap_or(&[]),
                &normalized_chars(name, self.filter.case_sensitive),
            ),
        }
    }
}

fn contains_text(name: &str, pattern: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        return name.contains(pattern);
    }
    name.to_lowercase().contains(&pattern.to_lowercase())
}

fn wildcard_matches_unanchored(pattern: &str, name: &str, case_sensitive: bool) -> bool {
    let mut pattern_chars = Vec::with_capacity(pattern.chars().count() + 2);
    pattern_chars.push('*');
    pattern_chars.extend(normalized_chars(pattern, case_sensitive));
    pattern_chars.push('*');
    let name_chars = normalized_chars(name, case_sensitive);
    wildcard_full_match(&pattern_chars, &name_chars)
}

fn normalized_chars(text: &str, case_sensitive: bool) -> Vec<char> {
    if case_sensitive {
        text.chars().collect()
    } else {
        text.to_lowercase().chars().collect()
    }
}

fn wildcard_full_match(pattern: &[char], text: &[char]) -> bool {
    let mut previous = vec![false; text.len() + 1];
    previous[0] = true;

    for pattern_ch in pattern {
        let mut current = vec![false; text.len() + 1];
        if *pattern_ch == '*' {
            current[0] = previous[0];
            for index in 1..=text.len() {
                current[index] = previous[index] || current[index - 1];
            }
        } else {
            for index in 1..=text.len() {
                current[index] =
                    previous[index - 1] && (*pattern_ch == '?' || *pattern_ch == text[index - 1]);
            }
        }
        previous = current;
    }

    previous[text.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entries::{Entry, EntryData};
    use std::path::PathBuf;

    fn entry(name: &str) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            target_path: None,
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: true,
            mime_type: None,
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }

    #[test]
    fn plain_text_filter_matches_case_insensitive_substrings() {
        let filter = NameFilter::plain_text("read");

        assert!(filter.matches_name("README.md"));
        assert!(filter.matches_name("thread.txt"));
        assert!(!filter.matches_name("notes.txt"));
    }

    #[test]
    fn case_sensitive_filter_keeps_case_distinct() {
        let filter = NameFilter::plain_text("read").with_case_sensitive(true);

        assert!(!filter.matches_name("README.md"));
        assert!(filter.matches_name("thread.txt"));
    }

    #[test]
    fn glob_filter_supports_basic_unanchored_wildcards() {
        let filter = NameFilter::glob("*.rs");

        assert!(filter.matches_name("main.rs"));
        assert!(filter.matches_name("main.rs.bak"));
        assert!(!filter.matches_name("main.txt"));

        let filter = NameFilter::glob("fi?e");
        assert!(filter.matches_name("my-file.txt"));
        assert!(!filter.matches_name("fie.txt"));
    }

    #[test]
    fn filtered_model_keeps_stable_model_indexes() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            Arc::new(vec![
                entry("alpha.txt"),
                entry("beta.rs"),
                entry("gamma.rs"),
            ]),
        );

        let filtered = FilteredModel::from_model(&model, &NameFilter::glob("*.rs"));

        assert_eq!(filtered.as_slice(), &[1, 2]);
        assert_eq!(filtered.model_index(0), Some(1));
        assert_eq!(filtered.layout_index_for_model_index(2), Some(1));
        assert_eq!(filtered.layout_index_for_model_index(0), None);
    }
}
