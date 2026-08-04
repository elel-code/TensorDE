use super::{
    DesktopAction, DesktopApplication, DesktopServiceMenu, MimeAppsList, MimeInfoCache,
    append_mimeapps, desktop_bool, desktop_list, desktop_usize, mimeapps_key_value, mimeapps_value,
    parse_desktop_sections, rewrite_mimeapps_key, service_menu_priority,
    validate_mimeapps_desktop_id, validate_mimeapps_key,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub fn parse_desktop_application(
    id: impl Into<String>,
    desktop_file: impl Into<PathBuf>,
    contents: &str,
) -> Option<DesktopApplication> {
    let sections = parse_desktop_sections(contents);
    let entry = sections.get("Desktop Entry")?;
    if entry.get("Hidden").is_some_and(|value| desktop_bool(value)) {
        return None;
    }
    if entry.get("Type").map(String::as_str) != Some("Application") {
        return None;
    }
    let name = entry.get("Name")?.trim();
    let exec = entry.get("Exec")?.trim();
    if name.is_empty() || exec.is_empty() {
        return None;
    }

    let action_ids = entry
        .get("Actions")
        .map(|value| desktop_list(value))
        .unwrap_or_default();
    let actions = action_ids
        .into_iter()
        .filter_map(|action_id| {
            let section = sections.get(&format!("Desktop Action {action_id}"))?;
            let name = section.get("Name")?.trim();
            let exec = section.get("Exec")?.trim();
            (!name.is_empty() && !exec.is_empty()).then(|| DesktopAction {
                id: action_id,
                name: name.to_string(),
                exec: exec.to_string(),
                icon: section.get("Icon").filter(|icon| !icon.is_empty()).cloned(),
            })
        })
        .collect();

    Some(DesktopApplication {
        id: id.into(),
        desktop_file: desktop_file.into(),
        name: name.to_string(),
        exec: exec.to_string(),
        icon: entry.get("Icon").filter(|icon| !icon.is_empty()).cloned(),
        categories: entry
            .get("Categories")
            .map(|value| desktop_list(value))
            .unwrap_or_default(),
        mime_types: entry
            .get("MimeType")
            .map(|value| desktop_list(value))
            .unwrap_or_default(),
        actions,
    })
}

pub fn parse_desktop_service_menu(
    id: impl Into<String>,
    desktop_file: impl Into<PathBuf>,
    contents: &str,
) -> Option<DesktopServiceMenu> {
    let sections = parse_desktop_sections(contents);
    let entry = sections.get("Desktop Entry")?;
    if entry.get("Hidden").is_some_and(|value| desktop_bool(value)) {
        return None;
    }
    if entry.get("Type").map(String::as_str) != Some("Service") {
        return None;
    }

    let service_types = entry
        .get("X-KDE-ServiceTypes")
        .or_else(|| entry.get("ServiceTypes"))
        .map(|value| desktop_list(value))
        .unwrap_or_default();
    if !service_types.iter().any(|service| {
        matches!(
            service.as_str(),
            "KonqPopupMenu/Plugin" | "KFileItemAction/Plugin"
        )
    }) {
        return None;
    }

    let action_ids = entry
        .get("Actions")
        .map(|value| desktop_list(value))
        .unwrap_or_default();
    let actions = action_ids
        .into_iter()
        .filter_map(|action_id| {
            let section = sections.get(&format!("Desktop Action {action_id}"))?;
            let name = section.get("Name")?.trim();
            let exec = section.get("Exec")?.trim();
            (!name.is_empty() && !exec.is_empty()).then(|| DesktopAction {
                id: action_id,
                name: name.to_string(),
                exec: exec.to_string(),
                icon: section.get("Icon").filter(|icon| !icon.is_empty()).cloned(),
            })
        })
        .collect::<Vec<_>>();
    if actions.is_empty() {
        return None;
    }

    let id = id.into();
    Some(DesktopServiceMenu {
        id: id.clone(),
        desktop_file: desktop_file.into(),
        name: entry
            .get("Name")
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or(&id)
            .to_string(),
        icon: entry.get("Icon").filter(|icon| !icon.is_empty()).cloned(),
        mime_types: entry
            .get("MimeType")
            .map(|value| desktop_list(value))
            .unwrap_or_default(),
        service_types,
        protocols: entry
            .get("X-KDE-Protocols")
            .map(|value| desktop_list(value))
            .unwrap_or_default(),
        submenu: entry
            .get("X-KDE-Submenu")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        priority: entry
            .get("X-KDE-Priority")
            .map(|value| service_menu_priority(value))
            .unwrap_or_default(),
        required_url_count: entry
            .get("X-KDE-RequiredNumberOfUrls")
            .and_then(|value| desktop_usize(value)),
        min_url_count: entry
            .get("X-KDE-MinNumberOfUrls")
            .and_then(|value| desktop_usize(value)),
        max_url_count: entry
            .get("X-KDE-MaxNumberOfUrls")
            .and_then(|value| desktop_usize(value)),
        show_if_executable: entry
            .get("X-KDE-ShowIfExecutable")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        actions,
    })
}

pub fn parse_mimeapps_list(contents: &str) -> MimeAppsList {
    let mut list = MimeAppsList::default();
    let mut section = "";
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        let Some((mime, value)) = line.split_once('=') else {
            continue;
        };
        let apps = desktop_list(value);
        if apps.is_empty() {
            continue;
        }
        match section {
            "Default Applications" => append_mimeapps(&mut list.default_apps, mime, apps),
            "Added Associations" => append_mimeapps(&mut list.added_associations, mime, apps),
            "Removed Associations" => append_mimeapps(&mut list.removed_associations, mime, apps),
            _ => {}
        }
    }
    list
}

pub fn parse_mimeinfo_cache(contents: &str) -> MimeInfoCache {
    let mut cache = MimeInfoCache::default();
    let mut section = "";
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        if section != "MIME Cache" {
            continue;
        }
        let Some((mime, value)) = line.split_once('=') else {
            continue;
        };
        let apps = desktop_list(value);
        if apps.is_empty() {
            continue;
        }
        append_mimeapps(&mut cache.associations, mime, apps);
    }
    cache
}

pub fn default_mimeapps_list_path() -> PathBuf {
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        PathBuf::from(config_home).join("mimeapps.list")
    } else if let Some(home) = env::var_os("HOME").filter(|path| !path.is_empty()) {
        PathBuf::from(home).join(".config/mimeapps.list")
    } else {
        PathBuf::from("mimeapps.list")
    }
}

pub fn set_default_mime_application(mime: &str, desktop_id: &str) -> Result<PathBuf, String> {
    let path = default_mimeapps_list_path();
    set_default_mime_application_at(&path, mime, desktop_id)?;
    Ok(path)
}

pub fn set_default_mime_application_at(
    path: &Path,
    mime: &str,
    desktop_id: &str,
) -> Result<(), String> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(format!(
                "failed to read mimeapps list {}: {error}",
                path.display()
            ));
        }
    };
    let updated = set_default_mime_application_in_contents(&contents, mime, desktop_id)?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create mimeapps list directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(path, updated)
        .map_err(|error| format!("failed to write mimeapps list {}: {error}", path.display()))
}

pub fn set_default_mime_application_in_contents(
    contents: &str,
    mime: &str,
    desktop_id: &str,
) -> Result<String, String> {
    let mime = validate_mimeapps_key(mime)?;
    let desktop_id = validate_mimeapps_desktop_id(desktop_id)?;
    let mut lines = contents.lines().map(str::to_string).collect::<Vec<_>>();

    rewrite_mimeapps_key(
        &mut lines,
        "Default Applications",
        &mime,
        Some(mimeapps_value(std::slice::from_ref(&desktop_id))),
        true,
    );

    let mut added = mimeapps_key_value(&lines, "Added Associations", &mime)
        .map(|value| desktop_list(&value))
        .unwrap_or_default();
    added.retain(|id| id != &desktop_id);
    added.insert(0, desktop_id.clone());
    rewrite_mimeapps_key(
        &mut lines,
        "Added Associations",
        &mime,
        Some(mimeapps_value(&added)),
        true,
    );

    let mut removed = mimeapps_key_value(&lines, "Removed Associations", &mime)
        .map(|value| desktop_list(&value))
        .unwrap_or_default();
    removed.retain(|id| id != &desktop_id);
    rewrite_mimeapps_key(
        &mut lines,
        "Removed Associations",
        &mime,
        (!removed.is_empty()).then(|| mimeapps_value(&removed)),
        false,
    );

    let mut updated = lines.join("\n");
    updated.push('\n');
    Ok(updated)
}
