use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use crate::{LauncherConfig, SearchResult};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopEntry {
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub keywords: Vec<String>,
    pub exec: String,
    pub icon: Option<String>,
    pub terminal: bool,
    pub(crate) normalized_name: String,
    pub(crate) normalized_search: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogDiagnostic {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct LauncherCatalog {
    entries: Vec<DesktopEntry>,
    diagnostics: Vec<CatalogDiagnostic>,
}

impl LauncherCatalog {
    pub fn discover(config: &LauncherConfig) -> Result<Self, LauncherCatalogError> {
        let mut by_id = BTreeMap::new();
        let mut seen_ids = BTreeSet::new();
        let mut diagnostics = Vec::with_capacity(config.max_diagnostics);
        for root in &config.application_directories {
            let mut files = Vec::new();
            collect_desktop_files(
                root,
                root,
                &mut files,
                &mut diagnostics,
                config.max_diagnostics,
            );
            files.sort();
            for path in files {
                let id = desktop_id(root, &path);
                if !seen_ids.insert(id.clone()) {
                    continue;
                }
                if seen_ids.len() > config.max_catalog_entries {
                    return Err(LauncherCatalogError::EntryLimit {
                        limit: config.max_catalog_entries,
                    });
                }
                match fs::read_to_string(&path) {
                    Ok(source) => match DesktopEntry::parse(id.clone(), &source) {
                        Ok(Some(entry)) => {
                            by_id.insert(id, entry);
                        }
                        Ok(None) => {}
                        Err(error) => push_diagnostic(
                            &mut diagnostics,
                            config.max_diagnostics,
                            path,
                            error.to_string(),
                        ),
                    },
                    Err(error) => push_diagnostic(
                        &mut diagnostics,
                        config.max_diagnostics,
                        path,
                        error.to_string(),
                    ),
                }
            }
        }
        let mut entries = by_id.into_values().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.normalized_name
                .cmp(&right.normalized_name)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(Self {
            entries,
            diagnostics,
        })
    }

    pub fn from_entries(mut entries: Vec<DesktopEntry>) -> Self {
        entries.sort_by(|left, right| {
            left.normalized_name
                .cmp(&right.normalized_name)
                .then_with(|| left.id.cmp(&right.id))
        });
        Self {
            entries,
            diagnostics: Vec::new(),
        }
    }

    pub fn entries(&self) -> &[DesktopEntry] {
        &self.entries
    }

    pub fn diagnostics(&self) -> &[CatalogDiagnostic] {
        &self.diagnostics
    }

    pub fn entry(&self, result: SearchResult) -> &DesktopEntry {
        &self.entries[result.index]
    }

    pub fn parse_entry(
        id: impl Into<String>,
        source: &str,
    ) -> Result<Option<DesktopEntry>, LauncherCatalogError> {
        DesktopEntry::parse(id.into(), source)
    }
}

impl DesktopEntry {
    fn parse(id: String, source: &str) -> Result<Option<Self>, LauncherCatalogError> {
        let mut in_desktop_entry = false;
        let mut values = BTreeMap::<&str, String>::new();
        for raw_line in source.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                in_desktop_entry = line == "[Desktop Entry]";
                continue;
            }
            if !in_desktop_entry {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if matches!(
                key,
                "Type"
                    | "Name"
                    | "GenericName"
                    | "Comment"
                    | "Keywords"
                    | "Exec"
                    | "Icon"
                    | "Terminal"
                    | "Hidden"
                    | "NoDisplay"
            ) {
                values.entry(key).or_insert_with(|| unescape(value));
            }
        }
        if values.get("Type").is_some_and(|kind| kind != "Application")
            || parse_bool(values.get("Hidden"))
            || parse_bool(values.get("NoDisplay"))
        {
            return Ok(None);
        }
        let name = required(&values, "Name", &id)?;
        let exec = required(&values, "Exec", &id)?;
        let generic_name = nonempty(values.remove("GenericName"));
        let comment = nonempty(values.remove("Comment"));
        let keywords: Vec<String> = values
            .remove("Keywords")
            .map(|value| {
                value
                    .split(';')
                    .map(str::trim)
                    .filter(|word| !word.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let icon = nonempty(values.remove("Icon"));
        let terminal = parse_bool(values.get("Terminal"));
        let normalized_name = normalize(&name);
        let normalized_search = [
            Some(name.as_str()),
            generic_name.as_deref(),
            comment.as_deref(),
        ]
        .into_iter()
        .flatten()
        .chain(keywords.iter().map(String::as_str))
        .map(normalize)
        .collect::<Vec<_>>()
        .join(" ");
        Ok(Some(Self {
            id,
            name,
            generic_name,
            comment,
            keywords,
            exec,
            icon,
            terminal,
            normalized_name,
            normalized_search,
        }))
    }
}

fn required(
    values: &BTreeMap<&str, String>,
    key: &'static str,
    id: &str,
) -> Result<String, LauncherCatalogError> {
    values
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| LauncherCatalogError::MissingField {
            id: id.to_owned(),
            field: key,
        })
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn parse_bool(value: Option<&String>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

pub(crate) fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn unescape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match chars.next() {
            Some('s') => output.push(' '),
            Some('n') => output.push('\n'),
            Some('t') => output.push('\t'),
            Some('r') => output.push('\r'),
            Some('\\') => output.push('\\'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output
}

fn collect_desktop_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
    diagnostics: &mut Vec<CatalogDiagnostic>,
    diagnostic_limit: usize,
) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if directory == root && error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            push_diagnostic(
                diagnostics,
                diagnostic_limit,
                directory.to_owned(),
                error.to_string(),
            );
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                push_diagnostic(
                    diagnostics,
                    diagnostic_limit,
                    directory.to_owned(),
                    error.to_string(),
                );
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                push_diagnostic(diagnostics, diagnostic_limit, path, error.to_string());
                continue;
            }
        };
        if file_type.is_dir() {
            collect_desktop_files(root, &path, files, diagnostics, diagnostic_limit);
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "desktop") {
            files.push(path);
        }
    }
}

fn desktop_id(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("-")
}

fn push_diagnostic(
    diagnostics: &mut Vec<CatalogDiagnostic>,
    limit: usize,
    path: PathBuf,
    message: String,
) {
    if diagnostics.len() < limit {
        diagnostics.push(CatalogDiagnostic { path, message });
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LauncherCatalogError {
    #[error("desktop entry `{id}` is missing required field `{field}`")]
    MissingField { id: String, field: &'static str },
    #[error("application catalog exceeds configured entry limit {limit}")]
    EntryLimit { limit: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SystemdMode;
    use tempfile::tempdir;

    #[test]
    fn parses_searchable_application_fields() {
        let entry = LauncherCatalog::parse_entry(
            "org.tensor.Files.desktop",
            "[Desktop Entry]\nType=Application\nName=Tensor\\sFiles\nGenericName=File Manager\nComment=Browse files\nKeywords=folder;storage;\nExec=tensor-files %U\nIcon=org.tensorde.TensorFiles\nTerminal=false\n",
        )
        .unwrap()
        .unwrap();
        assert_eq!(entry.name, "Tensor Files");
        assert_eq!(entry.keywords, ["folder", "storage"]);
        assert!(entry.normalized_search.contains("file manager"));
    }

    #[test]
    fn hidden_and_non_application_entries_are_not_indexed() {
        for source in [
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=false\nHidden=true\n",
            "[Desktop Entry]\nType=Link\nName=Link\nExec=false\n",
        ] {
            assert!(
                LauncherCatalog::parse_entry("ignored.desktop", source)
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn missing_exec_is_a_structured_error() {
        let error = LauncherCatalog::parse_entry(
            "broken.desktop",
            "[Desktop Entry]\nType=Application\nName=Broken\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("Exec"));
    }

    #[test]
    fn higher_priority_hidden_entry_masks_the_same_lower_priority_id() {
        let directory = tempdir().unwrap();
        let high = directory.path().join("high");
        let low = directory.path().join("low");
        fs::create_dir_all(&high).unwrap();
        fs::create_dir_all(&low).unwrap();
        fs::write(
            high.join("org.tensor.Example.desktop"),
            "[Desktop Entry]\nType=Application\nName=Hidden Override\nExec=false\nHidden=true\n",
        )
        .unwrap();
        fs::write(
            low.join("org.tensor.Example.desktop"),
            "[Desktop Entry]\nType=Application\nName=Stale Lower Entry\nExec=false\n",
        )
        .unwrap();
        let config = LauncherConfig {
            application_directories: vec![high, low],
            max_results: 10,
            max_catalog_entries: 32,
            max_diagnostics: 8,
            systemd: SystemdMode::Disabled,
        };

        let catalog = LauncherCatalog::discover(&config).unwrap();
        assert!(catalog.entries().is_empty());
    }
}
