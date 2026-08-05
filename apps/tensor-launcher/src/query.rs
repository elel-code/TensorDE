use std::cmp::Ordering;

use crate::{DesktopEntry, LauncherCatalog, catalog::normalize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub index: usize,
    pub score: u32,
}

impl LauncherCatalog {
    pub fn query(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let mut results = Vec::with_capacity(limit.min(self.entries().len()));
        self.query_into(query, limit, &mut results);
        results
    }

    pub fn query_into(&self, query: &str, limit: usize, results: &mut Vec<SearchResult>) {
        results.clear();
        let limit = limit.min(crate::MAX_QUERY_RESULTS);
        if limit == 0 {
            return;
        }
        let query = normalize(query);
        for (index, entry) in self.entries().iter().enumerate() {
            let score = if query.is_empty() {
                1
            } else {
                score(entry, &query)
            };
            if score == 0 {
                continue;
            }
            insert_ranked(
                results,
                SearchResult { index, score },
                self.entries(),
                limit,
            );
        }
    }
}

fn score(entry: &DesktopEntry, query: &str) -> u32 {
    if entry.normalized_name == query {
        10_000
    } else if entry.normalized_name.starts_with(query) {
        5_000
    } else if word_prefix(&entry.normalized_name, query) {
        3_000
    } else if entry.normalized_name.contains(query) {
        1_500
    } else if entry.normalized_search.contains(query) {
        750
    } else {
        subsequence_score(&entry.normalized_search, query)
    }
}

fn word_prefix(text: &str, query: &str) -> bool {
    text.split(|character: char| character.is_whitespace() || matches!(character, '-' | '_'))
        .any(|word| word.starts_with(query))
}

fn subsequence_score(text: &str, query: &str) -> u32 {
    if query.chars().count() < 3 {
        return 0;
    }
    let mut query_chars = query.chars();
    let Some(mut wanted) = query_chars.next() else {
        return 0;
    };
    let mut matched = 0_u32;
    let mut span = 0_u32;
    let mut started = false;
    for character in text.chars() {
        if started {
            span = span.saturating_add(1);
        }
        if character != wanted {
            continue;
        }
        started = true;
        matched += 1;
        match query_chars.next() {
            Some(next) => wanted = next,
            None => return 100 + matched.saturating_mul(8).saturating_sub(span.min(80)),
        }
    }
    0
}

fn insert_ranked(
    results: &mut Vec<SearchResult>,
    candidate: SearchResult,
    entries: &[DesktopEntry],
    limit: usize,
) {
    let position = results
        .binary_search_by(|current| compare(*current, candidate, entries))
        .unwrap_or_else(|position| position);
    results.insert(position, candidate);
    if results.len() > limit {
        results.pop();
    }
}

fn compare(left: SearchResult, right: SearchResult, entries: &[DesktopEntry]) -> Ordering {
    right
        .score
        .cmp(&left.score)
        .then_with(|| {
            entries[left.index]
                .normalized_name
                .cmp(&entries[right.index].normalized_name)
        })
        .then_with(|| entries[left.index].id.cmp(&entries[right.index].id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str, keywords: &str) -> DesktopEntry {
        LauncherCatalog::parse_entry(
            id,
            &format!(
                "[Desktop Entry]\nType=Application\nName={name}\nKeywords={keywords}\nExec={id}\n"
            ),
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn exact_prefix_and_keyword_matches_have_stable_priority() {
        let catalog = LauncherCatalog::from_entries(vec![
            entry("browser.desktop", "Web Browser", "internet;"),
            entry("web.desktop", "Web", "browser;"),
            entry("wide.desktop", "Wide Editor", "web;"),
        ]);
        let results = catalog.query("web", 10);
        assert_eq!(catalog.entry(results[0]).id, "web.desktop");
        assert_eq!(catalog.entry(results[1]).id, "browser.desktop");
        assert_eq!(catalog.entry(results[2]).id, "wide.desktop");
    }

    #[test]
    fn results_are_strictly_bounded_and_reuse_the_caller_vector() {
        let catalog = LauncherCatalog::from_entries(
            (0..100)
                .map(|index| entry(&format!("app-{index}.desktop"), &format!("App {index}"), ""))
                .collect(),
        );
        let mut results = Vec::with_capacity(8);
        catalog.query_into("app", 8, &mut results);
        assert_eq!(results.len(), 8);
        let capacity = results.capacity();
        catalog.query_into("app 9", 8, &mut results);
        assert!(results.len() <= 8);
        assert_eq!(results.capacity(), capacity);
    }

    #[test]
    fn short_queries_do_not_enter_fuzzy_matching() {
        let catalog =
            LauncherCatalog::from_entries(vec![entry("terminal.desktop", "Terminal", "console;")]);
        assert!(catalog.query("tl", 10).is_empty());
        assert!(!catalog.query("trm", 10).is_empty());
    }
}
