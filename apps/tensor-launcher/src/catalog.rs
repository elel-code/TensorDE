use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use crate::{LauncherConfig, SearchResult};

mod watcher;
pub use watcher::LauncherCatalogWatcher;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopEntry {
    pub id: String,
    pub desktop_file: Option<PathBuf>,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub keywords: Vec<String>,
    pub exec: String,
    pub icon: Option<String>,
    pub terminal: bool,
    pub working_directory: Option<PathBuf>,
    pub(crate) normalized_name: String,
    pub(crate) normalized_search: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogDiagnostic {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug)]
struct CatalogContext {
    locale_names: Vec<String>,
    current_desktops: Vec<String>,
    executable_directories: Vec<PathBuf>,
}

impl CatalogContext {
    fn from_environment() -> Self {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()));
        let current_desktops = env::var("XDG_CURRENT_DESKTOP")
            .ok()
            .into_iter()
            .flat_map(|value| value.split(':').map(str::to_owned).collect::<Vec<_>>())
            .filter(|desktop| !desktop.is_empty())
            .collect();
        let executable_directories = env::var_os("PATH")
            .map(|value| env::split_paths(&value).collect())
            .unwrap_or_else(|| vec![PathBuf::from("/usr/local/bin"), PathBuf::from("/usr/bin")]);
        Self {
            locale_names: locale.map_or_else(Vec::new, |locale| locale_names(&locale)),
            current_desktops,
            executable_directories,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LauncherCatalog {
    entries: Vec<DesktopEntry>,
    diagnostics: Vec<CatalogDiagnostic>,
}

impl LauncherCatalog {
    pub fn discover(config: &LauncherConfig) -> Result<Self, LauncherCatalogError> {
        let context = CatalogContext::from_environment();
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
                    Ok(source) => {
                        match DesktopEntry::parse(id.clone(), Some(path.clone()), &source, &context)
                        {
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
                        }
                    }
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
        DesktopEntry::parse(id.into(), None, source, &CatalogContext::from_environment())
    }
}

impl DesktopEntry {
    fn parse(
        id: String,
        desktop_file: Option<PathBuf>,
        source: &str,
        context: &CatalogContext,
    ) -> Result<Option<Self>, LauncherCatalogError> {
        let mut in_desktop_entry = false;
        let mut values = BTreeMap::<String, String>::new();
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
            if is_catalog_key(key) {
                values
                    .entry(key.to_owned())
                    .or_insert_with(|| unescape(value));
            }
        }
        if values.get("Type").is_some_and(|kind| kind != "Application")
            || parse_bool(values.get("Hidden"))
            || parse_bool(values.get("NoDisplay"))
        {
            return Ok(None);
        }
        if !desktop_visible(&values, context)
            || values
                .get("TryExec")
                .is_some_and(|command| !executable_exists(command, context))
        {
            return Ok(None);
        }
        let name = required_localized(&values, "Name", &id, context)?;
        let exec = required(&values, "Exec", &id)?;
        let generic_name = localized(&values, "GenericName", context);
        let comment = localized(&values, "Comment", context);
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
        let working_directory = nonempty(values.remove("Path"))
            .map(PathBuf::from)
            .map(|path| {
                if path.is_absolute() {
                    Ok(path)
                } else {
                    Err(LauncherCatalogError::RelativeWorkingDirectory {
                        id: id.clone(),
                        path,
                    })
                }
            })
            .transpose()?;
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
            desktop_file,
            name,
            generic_name,
            comment,
            keywords,
            exec,
            icon,
            terminal,
            working_directory,
            normalized_name,
            normalized_search,
        }))
    }
}

fn required(
    values: &BTreeMap<String, String>,
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

fn required_localized(
    values: &BTreeMap<String, String>,
    key: &'static str,
    id: &str,
    context: &CatalogContext,
) -> Result<String, LauncherCatalogError> {
    localized(values, key, context).ok_or_else(|| LauncherCatalogError::MissingField {
        id: id.to_owned(),
        field: key,
    })
}

fn localized(
    values: &BTreeMap<String, String>,
    key: &str,
    context: &CatalogContext,
) -> Option<String> {
    context
        .locale_names
        .iter()
        .find_map(|locale| nonempty(values.get(&format!("{key}[{locale}]")).cloned()))
        .or_else(|| nonempty(values.get(key).cloned()))
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn parse_bool(value: Option<&String>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn is_catalog_key(key: &str) -> bool {
    let base = key.split_once('[').map_or(key, |(base, _)| base);
    matches!(
        base,
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
            | "OnlyShowIn"
            | "NotShowIn"
            | "TryExec"
            | "Path"
    )
}

fn desktop_visible(values: &BTreeMap<String, String>, context: &CatalogContext) -> bool {
    let listed = |key| {
        values
            .get(key)
            .into_iter()
            .flat_map(|value| value.split(';'))
            .filter(|desktop| !desktop.is_empty())
            .any(|desktop| {
                context
                    .current_desktops
                    .iter()
                    .any(|current| current == desktop)
            })
    };
    let only_show = values.get("OnlyShowIn");
    (only_show.is_none() || listed("OnlyShowIn")) && !listed("NotShowIn")
}

fn executable_exists(command: &str, context: &CatalogContext) -> bool {
    let command = Path::new(command);
    if command.components().count() > 1 {
        return is_executable(command);
    }
    context
        .executable_directories
        .iter()
        .any(|directory| is_executable(&directory.join(command)))
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn locale_names(locale: &str) -> Vec<String> {
    if matches!(locale, "C" | "POSIX") {
        return Vec::new();
    }
    let (base_with_encoding, modifier) = locale
        .split_once('@')
        .map_or((locale, None), |(base, modifier)| (base, Some(modifier)));
    let base = base_with_encoding
        .split_once('.')
        .map_or(base_with_encoding, |(base, _)| base);
    let (language, territory) = base
        .split_once('_')
        .map_or((base, None), |(language, territory)| {
            (language, Some(territory))
        });
    let mut names = Vec::with_capacity(4);
    if let (Some(territory), Some(modifier)) = (territory, modifier) {
        names.push(format!("{language}_{territory}@{modifier}"));
    }
    if let Some(territory) = territory {
        names.push(format!("{language}_{territory}"));
    }
    if let Some(modifier) = modifier {
        names.push(format!("{language}@{modifier}"));
    }
    names.push(language.to_owned());
    names
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
    #[error("desktop entry `{id}` has relative working directory `{}`", path.display())]
    RelativeWorkingDirectory { id: String, path: PathBuf },
    #[error("failed to inspect application catalog path {path}: {source}")]
    Metadata {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("application catalog fingerprint exceeds configured bound {limit}")]
    FingerprintLimit { limit: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::Permissions;
    use tempfile::tempdir;

    fn context(
        locale: &str,
        desktops: &[&str],
        executable_directories: Vec<PathBuf>,
    ) -> CatalogContext {
        CatalogContext {
            locale_names: locale_names(locale),
            current_desktops: desktops
                .iter()
                .map(|desktop| (*desktop).to_owned())
                .collect(),
            executable_directories,
        }
    }

    fn parse_with_context(
        id: &str,
        source: &str,
        context: &CatalogContext,
    ) -> Result<Option<DesktopEntry>, LauncherCatalogError> {
        DesktopEntry::parse(id.to_owned(), None, source, context)
    }

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
        };

        let catalog = LauncherCatalog::discover(&config).unwrap();
        assert!(catalog.entries().is_empty());
    }

    #[test]
    fn watcher_refreshes_only_after_a_bounded_catalog_change() {
        let directory = tempdir().unwrap();
        let app = directory.path().join("org.tensor.Demo.desktop");
        fs::write(
            &app,
            "[Desktop Entry]\nType=Application\nName=Demo\nExec=demo\n",
        )
        .unwrap();
        let config = LauncherConfig {
            application_directories: vec![directory.path().to_owned()],
            max_results: 8,
            max_catalog_entries: 16,
            max_diagnostics: 4,
        };
        let mut watcher = LauncherCatalogWatcher::start(config).unwrap();
        assert_eq!(watcher.catalog().entries().len(), 1);
        assert!(!watcher.refresh_if_changed().unwrap());

        fs::write(
            directory.path().join("org.tensor.New.desktop"),
            "[Desktop Entry]\nType=Application\nName=New\nExec=new\n",
        )
        .unwrap();
        assert!(watcher.refresh_if_changed().unwrap());
        assert_eq!(watcher.catalog().entries().len(), 2);
    }

    #[test]
    fn locale_and_desktop_visibility_are_resolved_on_the_discovery_path() {
        let tensor_context = context("zh_CN.UTF-8", &["Tensor", "Wayland"], Vec::new());
        let entry = parse_with_context(
            "org.tensor.Example.desktop",
            "[Desktop Entry]\nType=Application\nName=Example\nName[zh]=Localized\nName[zh_CN]=Localized CN\nComment=Base\nComment[zh]=Localized comment\nOnlyShowIn=Tensor;GNOME;\nNotShowIn=KDE;\nExec=example\nPath=/srv/example\n",
            &tensor_context,
        )
        .unwrap()
        .unwrap();

        assert_eq!(entry.name, "Localized CN");
        assert_eq!(entry.comment.as_deref(), Some("Localized comment"));
        assert_eq!(
            entry.working_directory.as_deref(),
            Some(Path::new("/srv/example"))
        );

        let hidden = context("zh_CN.UTF-8", &["KDE"], Vec::new());
        assert!(
            parse_with_context(
                "org.tensor.Example.desktop",
                "[Desktop Entry]\nType=Application\nName=Example\nOnlyShowIn=Tensor;\nExec=example\n",
                &hidden,
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn try_exec_requires_an_executable_found_on_the_catalog_path() {
        let directory = tempdir().unwrap();
        let executable = directory.path().join("tensor-example");
        fs::write(&executable, "binary fixture").unwrap();
        fs::set_permissions(&executable, Permissions::from_mode(0o755)).unwrap();
        let context = context("C", &[], vec![directory.path().to_owned()]);
        let source = "[Desktop Entry]\nType=Application\nName=Example\nTryExec=tensor-example\nExec=tensor-example\n";
        assert!(
            parse_with_context("org.tensor.Example.desktop", source, &context)
                .unwrap()
                .is_some()
        );

        fs::set_permissions(&executable, Permissions::from_mode(0o644)).unwrap();
        assert!(
            parse_with_context("org.tensor.Example.desktop", source, &context)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn relative_working_directory_is_rejected_at_catalog_load() {
        let error = parse_with_context(
            "org.tensor.Example.desktop",
            "[Desktop Entry]\nType=Application\nName=Example\nExec=example\nPath=relative\n",
            &context("C", &[], Vec::new()),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            LauncherCatalogError::RelativeWorkingDirectory { .. }
        ));
    }
}
