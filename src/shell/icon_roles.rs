use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) const FILE_ICON_CORNER_RADIUS_RATIO: f32 = 0.16;
pub(crate) const FOLDER_ICON_CORNER_RADIUS_RATIO: f32 = 0.14;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum FileIconKind {
    Directory,
    Mime {
        mime: Arc<str>,
    },
    PreliminaryFile {
        extension: Option<String>,
    },
    File {
        extension: Option<String>,
    },
    Named {
        icon_name: String,
        fallback: NamedIconFallback,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum NamedIconFallback {
    Service,
    Application,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FileIconRoleCacheKey {
    pub(crate) kind: FileIconKind,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FileIconPathCacheKey {
    pub(crate) role: FileIconRoleCacheKey,
    pub(crate) size_px: u16,
}

pub(crate) struct FileIconProfile {
    pub(crate) icon_candidates: Vec<String>,
    pub(crate) generic_candidates: Vec<String>,
}

pub(crate) fn file_icon_path_cache_key(
    path: &Path,
    is_dir: bool,
    mime_type: Option<Arc<str>>,
    mime_magic_checked: bool,
    icon_size: f32,
) -> FileIconPathCacheKey {
    file_icon_path_cache_key_with_stamp(
        path,
        is_dir,
        mime_type,
        mime_magic_checked,
        None,
        icon_size,
    )
}

pub(crate) fn file_icon_path_cache_key_with_stamp(
    path: &Path,
    is_dir: bool,
    mime_type: Option<Arc<str>>,
    mime_magic_checked: bool,
    modified_secs: Option<u64>,
    icon_size: f32,
) -> FileIconPathCacheKey {
    FileIconPathCacheKey {
        role: FileIconRoleCacheKey {
            kind: file_icon_kind_with_stamp(
                path,
                is_dir,
                mime_type,
                mime_magic_checked,
                modified_secs,
            ),
        },
        size_px: icon_cache_size(icon_size),
    }
}

#[cfg(test)]
pub(crate) fn file_icon_kind(
    path: &Path,
    is_dir: bool,
    mime_type: Option<Arc<str>>,
    mime_magic_checked: bool,
) -> FileIconKind {
    file_icon_kind_with_stamp(path, is_dir, mime_type, mime_magic_checked, None)
}

fn file_icon_kind_with_stamp(
    path: &Path,
    is_dir: bool,
    mime_type: Option<Arc<str>>,
    mime_magic_checked: bool,
    modified_secs: Option<u64>,
) -> FileIconKind {
    if is_dir {
        return FileIconKind::Directory;
    }
    // Dolphin/KIO: `.desktop` launchers use the Desktop Entry `Icon=` field, not
    // a shared text/mime icon. Wine/OCS shortcuts land here too.
    if is_desktop_entry_file(path, mime_type.as_deref())
        && let Some(icon_name) = desktop_entry_icon_name_cached(path, modified_secs)
    {
        return FileIconKind::Named {
            icon_name,
            fallback: NamedIconFallback::Application,
        };
    }
    let extension = file_extension(path);
    if !mime_magic_checked && mime_type.as_deref() == Some(fika_core::GENERIC_BINARY_MIME) {
        return FileIconKind::PreliminaryFile { extension };
    }
    match mime_type {
        Some(mime) if mime.as_ref() == fika_core::GENERIC_BINARY_MIME => {
            FileIconKind::File { extension: None }
        }
        Some(mime) => FileIconKind::Mime { mime },
        None => FileIconKind::File { extension: None },
    }
}

fn desktop_entry_icon_name_cached(path: &Path, modified_secs: Option<u64>) -> Option<String> {
    const MAX_ENTRIES: usize = 512;
    static CACHE: OnceLock<
        Mutex<std::collections::HashMap<(std::path::PathBuf, u64), Option<String>>>,
    > = OnceLock::new();

    let Some(stamp) = modified_secs else {
        return desktop_entry_icon_name(path);
    };
    let key = (path.to_path_buf(), stamp);
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(cache) = cache.lock()
        && let Some(icon) = cache.get(&key)
    {
        return icon.clone();
    }

    let icon = desktop_entry_icon_name(path);
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= MAX_ENTRIES {
            cache.clear();
        }
        cache.insert(key, icon.clone());
    }
    icon
}

fn is_desktop_entry_file(path: &Path, mime_type: Option<&str>) -> bool {
    file_extension(path).as_deref() == Some("desktop")
        || mime_type.is_some_and(|mime| {
            matches!(
                mime,
                "application/x-desktop" | "application/x-desktop-entry" | "text/x-desktop"
            )
        })
}

/// Read `Icon=` from the primary `[Desktop Entry]` group (freedesktop spec).
pub(crate) fn desktop_entry_icon_name(path: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    let mut in_desktop_entry = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line.eq_ignore_ascii_case("[Desktop Entry]");
            continue;
        }
        if !in_desktop_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "Icon" {
            continue;
        }
        let icon = value.trim();
        if icon.is_empty() {
            return None;
        }
        return Some(icon.to_string());
    }
    None
}

pub(crate) fn icon_cache_size(icon_size: f32) -> u16 {
    let requested = icon_size.round().clamp(16.0, 256.0) as u16;
    dolphin_icon_cache_sizes()
        .iter()
        .copied()
        .min_by_key(|size| size.abs_diff(requested))
        .unwrap_or(48)
}

/// Raster-cache size for content thumbnails given on-screen icon pixels.
///
/// Floors at the freestanding source dimension (capped at 256 for GPU memory)
/// so we decode a sharp PNG instead of upscaling a small bitmap on the GPU.
pub(crate) fn thumbnail_display_cache_size(display_px: f32) -> u16 {
    let display = icon_cache_size(display_px);
    let freestanding = fika_core::ThumbnailSize::for_display_px(display)
        .max_dimension()
        .min(256);
    display.max(freestanding)
}

fn dolphin_icon_cache_sizes() -> [u16; 17] {
    [
        16, 22, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 256,
    ]
}

pub(crate) fn file_icon_profile(
    kind: &FileIconKind,
    mime: &fika_core::MimeDatabase,
) -> FileIconProfile {
    let (icon_candidates, generic_candidates) = match kind {
        FileIconKind::Directory => (
            vec!["folder".to_string(), "inode-directory".to_string()],
            Vec::new(),
        ),
        FileIconKind::Mime { mime: mime_name } => (
            mime_icon_candidates(mime_name, mime),
            mime_generic_icon_candidates(mime_name, mime),
        ),
        FileIconKind::PreliminaryFile { extension } => (
            preliminary_file_icon_candidates(extension.as_deref(), mime),
            Vec::new(),
        ),
        FileIconKind::File { .. } => (
            fallback_file_icon_candidates(),
            mime_generic_icon_candidates(fika_core::GENERIC_BINARY_MIME, mime),
        ),
        FileIconKind::Named {
            icon_name,
            fallback,
        } => named_icon_candidates(icon_name, *fallback),
    };

    FileIconProfile {
        icon_candidates,
        generic_candidates,
    }
}

fn file_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn mime_icon_candidates(mime_name: &str, mime: &fika_core::MimeDatabase) -> Vec<String> {
    let mut candidates = Vec::new();

    if mime_name == fika_core::GENERIC_BINARY_MIME {
        for icon_name in fallback_file_icon_candidates() {
            push_icon_candidate(&mut candidates, icon_name);
        }
        return candidates;
    }

    for icon_name in mime_theme_icon_candidates(mime_name, None) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    if let Some(icon_name) = mime.icon_name_for_mime(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    candidates
}

fn mime_generic_icon_candidates(mime_name: &str, mime: &fika_core::MimeDatabase) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(icon_name) = mime.generic_icon_name_for_mime(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    candidates
}

fn mime_theme_icon_candidates(mime_name: &str, extension: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(icon_name) = fika_core::mime_icon_name(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    push_portable_executable_icon_candidates(&mut candidates, mime_name);
    if let Some((family, subtype)) = mime_name.split_once('/')
        && family == "text"
    {
        let subtype = subtype.strip_prefix("x-").unwrap_or(subtype);
        if !subtype.is_empty() {
            push_icon_candidate(&mut candidates, format!("text-x-{subtype}"));
        }
        if let Some(extension) = extension.filter(|extension| !extension.is_empty()) {
            push_icon_candidate(&mut candidates, format!("text-x-{extension}"));
        }
    }
    candidates
}

fn push_portable_executable_icon_candidates(candidates: &mut Vec<String>, mime_name: &str) {
    let aliases = match mime_name {
        "application/vnd.microsoft.portable-executable" => [
            "application-x-msdownload",
            "application-x-ms-dos-executable",
            "application-x-executable",
        ]
        .as_slice(),
        "application/x-msdownload" => [
            "application-vnd.microsoft.portable-executable",
            "application-x-ms-dos-executable",
            "application-x-executable",
        ]
        .as_slice(),
        "application/x-ms-dos-executable" => [
            "application-x-msdownload",
            "application-vnd.microsoft.portable-executable",
            "application-x-executable",
        ]
        .as_slice(),
        _ => [].as_slice(),
    };
    for icon_name in aliases {
        push_icon_candidate(candidates, *icon_name);
    }
}

fn fallback_file_icon_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    push_icon_candidate(&mut candidates, "application-octet-stream");
    candidates
}

fn preliminary_file_icon_candidates(
    extension: Option<&str>,
    mime: &fika_core::MimeDatabase,
) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(extension) = extension.filter(|extension| !extension.is_empty()) {
        if let Some(mime_name) = mime.mime_for_extension(extension) {
            for icon_name in mime_theme_icon_candidates(mime_name, Some(extension)) {
                push_icon_candidate(&mut candidates, icon_name);
            }
        }
        push_icon_candidate(&mut candidates, format!("text-x-{extension}"));
        push_icon_candidate(&mut candidates, format!("application-x-{extension}"));
    }
    push_icon_candidate(&mut candidates, "text-x-generic");
    push_icon_candidate(&mut candidates, "unknown");
    candidates
}

fn push_icon_candidate(candidates: &mut Vec<String>, icon_name: impl Into<String>) {
    let icon_name = icon_name.into();
    if !candidates.iter().any(|existing| existing == &icon_name) {
        candidates.push(icon_name);
    }
}

fn named_icon_candidates(
    icon_name: &str,
    fallback: NamedIconFallback,
) -> (Vec<String>, Vec<String>) {
    let mut candidates = Vec::new();
    push_icon_candidate(&mut candidates, icon_name.trim());
    let generic = match fallback {
        NamedIconFallback::Service => ["configure", "preferences-system", "system-run"].as_slice(),
        NamedIconFallback::Application => [
            "application-x-executable",
            "system-run",
            "application-default-icon",
        ]
        .as_slice(),
    }
    .iter()
    .map(|candidate| (*candidate).to_string())
    .collect();
    (candidates, generic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn desktop_entry_files_use_named_icon_from_desktop_entry() {
        let root = std::env::temp_dir().join(format!(
            "fika-desktop-icon-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("OCS Desktop.desktop");
        fs::write(
            &path,
            "[Desktop Entry]\n\
             Name=OCS Desktop\n\
             Type=Application\n\
             Icon=7352_OCS Desktop.0\n\
             Exec=true\n",
        )
        .unwrap();

        assert_eq!(
            file_icon_kind(&path, false, Some(Arc::from("application/x-desktop")), true),
            FileIconKind::Named {
                icon_name: "7352_OCS Desktop.0".to_string(),
                fallback: NamedIconFallback::Application,
            }
        );
        // Extension alone is enough even without mime.
        assert_eq!(
            file_icon_kind(&path, false, None, false),
            FileIconKind::Named {
                icon_name: "7352_OCS Desktop.0".to_string(),
                fallback: NamedIconFallback::Application,
            }
        );
        assert_eq!(
            desktop_entry_icon_name(&path).as_deref(),
            Some("7352_OCS Desktop.0")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn desktop_entry_icon_name_ignores_non_desktop_entry_groups() {
        let root = std::env::temp_dir().join(format!(
            "fika-desktop-icon-group-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("app.desktop");
        fs::write(
            &path,
            "[Desktop Action Foo]\n\
             Icon=action-icon\n\
             [Desktop Entry]\n\
             Name=App\n\
             Icon=real-app-icon\n\
             Type=Application\n",
        )
        .unwrap();
        assert_eq!(
            desktop_entry_icon_name(&path).as_deref(),
            Some("real-app-icon")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn desktop_entry_role_cache_reuses_stamp_and_refreshes_changed_stamp() {
        let root = std::env::temp_dir().join(format!(
            "fika-desktop-role-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("cached.desktop");
        fs::write(&path, "[Desktop Entry]\nIcon=first-icon\n").unwrap();

        let first = file_icon_path_cache_key_with_stamp(
            &path,
            false,
            Some(Arc::from("application/x-desktop")),
            true,
            Some(1),
            48.0,
        );
        fs::write(&path, "[Desktop Entry]\nIcon=second-icon\n").unwrap();
        let same_stamp = file_icon_path_cache_key_with_stamp(
            &path,
            false,
            Some(Arc::from("application/x-desktop")),
            true,
            Some(1),
            48.0,
        );
        let changed_stamp = file_icon_path_cache_key_with_stamp(
            &path,
            false,
            Some(Arc::from("application/x-desktop")),
            true,
            Some(2),
            48.0,
        );

        assert_eq!(same_stamp.role, first.role);
        assert!(matches!(
            changed_stamp.role.kind,
            FileIconKind::Named { ref icon_name, .. } if icon_name == "second-icon"
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_display_cache_size_prefers_large_for_default_icons_mode() {
        // Default Icons mode (48px) should decode from freestanding large (256).
        assert_eq!(thumbnail_display_cache_size(48.0), 256);
        assert_eq!(thumbnail_display_cache_size(28.0), 128); // compact → normal
        assert_eq!(thumbnail_display_cache_size(96.0), 256);
        assert_eq!(thumbnail_display_cache_size(160.0), 256); // capped GPU texture size
    }

    #[test]
    fn portable_executable_mime_candidates_match_kde_theme_aliases() {
        let profile = file_icon_profile(
            &FileIconKind::Mime {
                mime: Arc::from("application/vnd.microsoft.portable-executable"),
            },
            fika_core::MimeDatabase::shared(),
        );

        assert_eq!(
            profile.icon_candidates.first().map(String::as_str),
            Some("application-vnd.microsoft.portable-executable")
        );
        assert!(
            profile
                .icon_candidates
                .iter()
                .any(|name| name == "application-x-msdownload")
        );
        assert!(
            profile
                .icon_candidates
                .iter()
                .any(|name| name == "application-x-ms-dos-executable")
        );
        assert!(
            profile
                .icon_candidates
                .iter()
                .any(|name| name == "application-x-executable")
        );
    }

    #[test]
    fn windows_shortcut_mime_prefers_exact_dolphin_icon_before_generic_fallback() {
        let mime = fika_core::MimeDatabase::shared();
        let profile = file_icon_profile(
            &FileIconKind::Mime {
                mime: Arc::from("application/x-ms-shortcut"),
            },
            mime,
        );

        assert_eq!(
            profile.icon_candidates.first().map(String::as_str),
            Some("application-x-ms-shortcut")
        );
        assert_eq!(
            profile.generic_candidates.first().map(String::as_str),
            Some("emblem-symbolic-link")
        );
    }

    #[test]
    fn exe_preliminary_icon_candidates_include_executable_alias() {
        let database = fika_core::MimeDatabase::from_maps(
            [("exe".to_string(), "application/x-msdownload".to_string())].into(),
            Default::default(),
            Default::default(),
        );
        let profile = file_icon_profile(
            &FileIconKind::PreliminaryFile {
                extension: Some("exe".to_string()),
            },
            &database,
        );

        assert!(
            profile
                .icon_candidates
                .iter()
                .any(|name| name == "application-x-msdownload")
        );
        assert!(
            profile
                .icon_candidates
                .iter()
                .any(|name| name == "application-x-executable")
        );
    }
}
