fn load_desktop_applications() -> Vec<DesktopApplication> {
    let mut applications = Vec::new();
    for applications_dir in desktop_application_dirs() {
        collect_desktop_applications(&applications_dir, &applications_dir, &mut applications);
    }
    applications
}

fn load_desktop_service_menus() -> Vec<DesktopServiceMenu> {
    let mut service_menus = Vec::new();
    for service_menu_dir in desktop_service_menu_dirs() {
        collect_desktop_service_menus(&service_menu_dir, &service_menu_dir, &mut service_menus);
    }
    service_menus
}

fn collect_desktop_applications(
    root: &Path,
    dir: &Path,
    applications: &mut Vec<DesktopApplication>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_desktop_applications(root, &path, applications);
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("desktop") {
            continue;
        }
        let Some(id) = desktop_id_for_path(root, &path) else {
            continue;
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(application) = parse_desktop_application(id, path, &contents) {
            applications.push(application);
        }
    }
}

fn collect_desktop_service_menus(
    root: &Path,
    dir: &Path,
    service_menus: &mut Vec<DesktopServiceMenu>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_desktop_service_menus(root, &path, service_menus);
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("desktop") {
            continue;
        }
        let Some(id) = desktop_id_for_path(root, &path) else {
            continue;
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(service_menu) = parse_desktop_service_menu(id, path, &contents) {
            service_menus.push(service_menu);
        }
    }
}

fn desktop_application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        push_unique_path(&mut dirs, PathBuf::from(data_home).join("applications"));
    } else if let Some(home) = env::var_os("HOME").filter(|path| !path.is_empty()) {
        push_unique_path(
            &mut dirs,
            PathBuf::from(home).join(".local/share/applications"),
        );
    }
    for data_dir in env::var_os("XDG_DATA_DIRS")
        .filter(|path| !path.is_empty())
        .map(|paths| {
            env::split_paths(&paths)
                .map(|path| path.join("applications"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share/applications"),
                PathBuf::from("/usr/share/applications"),
            ]
        })
    {
        push_unique_path(&mut dirs, data_dir);
    }
    dirs
}

fn desktop_service_menu_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        push_service_menu_roots(&mut dirs, PathBuf::from(data_home));
    } else if let Some(home) = env::var_os("HOME").filter(|path| !path.is_empty()) {
        push_service_menu_roots(&mut dirs, PathBuf::from(home).join(".local/share"));
    }
    for data_dir in env::var_os("XDG_DATA_DIRS")
        .filter(|path| !path.is_empty())
        .map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        })
    {
        push_service_menu_roots(&mut dirs, data_dir);
    }
    dirs
}

fn mimeinfo_cache_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for applications_dir in desktop_application_dirs() {
        push_unique_path(&mut paths, applications_dir.join("mimeinfo.cache"));
    }
    paths
}

fn push_service_menu_roots(dirs: &mut Vec<PathBuf>, root: PathBuf) {
    push_unique_path(dirs, root.join("fika/servicemenus"));
    push_unique_path(dirs, root.join("kio/servicemenus"));
    push_unique_path(dirs, root.join("kservices5/ServiceMenus"));
    push_unique_path(dirs, root.join("konqueror/servicemenus"));
}

fn mimeapps_list_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        push_unique_path(&mut paths, PathBuf::from(config_home).join("mimeapps.list"));
    } else if let Some(home) = env::var_os("HOME").filter(|path| !path.is_empty()) {
        push_unique_path(
            &mut paths,
            PathBuf::from(home).join(".config/mimeapps.list"),
        );
    }
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        push_unique_path(
            &mut paths,
            PathBuf::from(data_home).join("applications/mimeapps.list"),
        );
    } else if let Some(home) = env::var_os("HOME").filter(|path| !path.is_empty()) {
        push_unique_path(
            &mut paths,
            PathBuf::from(home).join(".local/share/applications/mimeapps.list"),
        );
    }
    for data_dir in env::var_os("XDG_DATA_DIRS")
        .filter(|path| !path.is_empty())
        .map(|paths| {
            env::split_paths(&paths)
                .map(|path| path.join("applications/mimeapps.list"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share/applications/mimeapps.list"),
                PathBuf::from("/usr/share/applications/mimeapps.list"),
            ]
        })
    {
        push_unique_path(&mut paths, data_dir);
    }
    paths
}

fn desktop_id_for_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let id = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("-");
    (!id.is_empty()).then_some(id)
}

fn parse_desktop_sections(contents: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections = HashMap::<String, HashMap<String, String>>::new();
    let mut section = String::new();
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            sections.entry(section.clone()).or_default();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if section.is_empty() || key.contains('[') {
            continue;
        }
        sections
            .entry(section.clone())
            .or_default()
            .insert(key.trim().to_string(), desktop_unescape(value.trim()));
    }
    sections
}

fn desktop_unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            match ch {
                's' => out.push(' '),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '\\' => out.push('\\'),
                _ => out.push(ch),
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            out.push(ch);
        }
    }
    if escaped {
        out.push('\\');
    }
    out
}

fn desktop_list(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn append_mimeapps(target: &mut HashMap<String, Vec<String>>, mime: &str, apps: Vec<String>) {
    target
        .entry(mime.trim().to_string())
        .or_default()
        .extend(apps);
}

fn validate_mimeapps_key(mime: &str) -> Result<String, String> {
    let mime = mime.trim();
    if mime.is_empty()
        || !mime.contains('/')
        || mime
            .bytes()
            .any(|byte| byte.is_ascii_control() || matches!(byte, b'=' | b';' | b'[' | b']'))
    {
        Err(format!("invalid MIME type for mimeapps.list: {mime:?}"))
    } else {
        Ok(mime.to_string())
    }
}

fn validate_mimeapps_desktop_id(desktop_id: &str) -> Result<String, String> {
    let desktop_id = desktop_id.trim();
    if desktop_id.is_empty()
        || !desktop_id.ends_with(".desktop")
        || desktop_id
            .bytes()
            .any(|byte| byte.is_ascii_control() || matches!(byte, b'=' | b';' | b'[' | b']'))
    {
        Err(format!(
            "invalid desktop id for mimeapps.list: {desktop_id:?}"
        ))
    } else {
        Ok(desktop_id.to_string())
    }
}

fn mimeapps_value(apps: &[String]) -> String {
    let mut value = apps.join(";");
    value.push(';');
    value
}

fn mimeapps_key_value(lines: &[String], section: &str, key: &str) -> Option<String> {
    let (start, end) = mimeapps_section_range(lines, section)?;
    lines[start + 1..end].iter().find_map(|line| {
        let trimmed = line.trim();
        let (line_key, value) = trimmed.split_once('=')?;
        (line_key.trim() == key).then(|| value.trim().to_string())
    })
}

fn rewrite_mimeapps_key(
    lines: &mut Vec<String>,
    section: &str,
    key: &str,
    value: Option<String>,
    insert_if_missing: bool,
) {
    let Some((start, end)) = mimeapps_section_range(lines, section) else {
        if insert_if_missing && let Some(value) = value {
            if !lines.is_empty() && lines.last().is_some_and(|line| !line.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[{section}]"));
            lines.push(format!("{key}={value}"));
        }
        return;
    };

    let key_indexes = (start + 1..end)
        .filter(|index| {
            lines[*index]
                .trim()
                .split_once('=')
                .is_some_and(|(line_key, _)| line_key.trim() == key)
        })
        .collect::<Vec<_>>();

    match (key_indexes.first().copied(), value) {
        (Some(first), Some(value)) => {
            lines[first] = format!("{key}={value}");
            for index in key_indexes
                .into_iter()
                .rev()
                .filter(|index| *index != first)
            {
                lines.remove(index);
            }
        }
        (Some(_), None) => {
            for index in key_indexes.into_iter().rev() {
                lines.remove(index);
            }
        }
        (None, Some(value)) if insert_if_missing => {
            lines.insert(end, format!("{key}={value}"));
        }
        _ => {}
    }
}

fn mimeapps_section_range(lines: &[String], section: &str) -> Option<(usize, usize)> {
    let start = lines
        .iter()
        .position(|line| line.trim() == format!("[{section}]"))?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('[') && trimmed.ends_with(']')
        })
        .map_or(lines.len(), |offset| start + 1 + offset);
    Some((start, end))
}

fn desktop_bool(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value == "1"
}

fn desktop_usize(value: &str) -> Option<usize> {
    value.trim().parse().ok()
}

fn service_menu_priority(value: &str) -> ServiceMenuPriority {
    if value.trim().eq_ignore_ascii_case("TopLevel") {
        ServiceMenuPriority::TopLevel
    } else {
        ServiceMenuPriority::Normal
    }
}

fn desktop_application_cmp(
    left: &DesktopApplication,
    right: &DesktopApplication,
) -> std::cmp::Ordering {
    left.name
        .to_ascii_lowercase()
        .cmp(&right.name.to_ascii_lowercase())
        .then_with(|| left.id.cmp(&right.id))
}

fn desktop_service_menu_cmp(
    left: &DesktopServiceMenu,
    right: &DesktopServiceMenu,
) -> std::cmp::Ordering {
    service_menu_priority_rank(right.priority)
        .cmp(&service_menu_priority_rank(left.priority))
        .then_with(|| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        })
        .then_with(|| left.id.cmp(&right.id))
}

fn service_menu_priority_rank(priority: ServiceMenuPriority) -> usize {
    match priority {
        ServiceMenuPriority::Normal => 0,
        ServiceMenuPriority::TopLevel => 1,
    }
}

fn target_mime(mime: Option<&str>, is_dir: bool) -> Option<&str> {
    if is_dir {
        Some("inode/directory")
    } else {
        mime.map(str::trim).filter(|mime| !mime.is_empty())
    }
}

fn mime_parent_candidates(mime: &str) -> Vec<String> {
    let mime = mime.trim().to_ascii_lowercase();
    let Some((top, subtype)) = mime.split_once('/') else {
        return Vec::new();
    };
    let mut parents = Vec::new();

    if mime != "text/plain"
        && (top == "text"
            || subtype.ends_with("+json")
            || subtype.ends_with("+xml")
            || matches!(
                mime.as_str(),
                "application/json"
                    | "application/xml"
                    | "application/javascript"
                    | "application/x-shellscript"
                    | "application/x-python"
                    | "application/x-perl"
                    | "application/x-ruby"
            ))
    {
        parents.push("text/plain".to_string());
    }

    parents
}

fn desktop_mimes_match_target(
    mime_types: &[String],
    target_mime: Option<&str>,
    is_dir: bool,
) -> bool {
    mime_types
        .iter()
        .any(|mime| desktop_mime_matches_target(mime, target_mime, is_dir))
}

fn service_menu_matches_target(
    menu: &DesktopServiceMenu,
    mime: Option<&str>,
    is_dir: bool,
) -> bool {
    let target_mime = target_mime(mime, is_dir);
    desktop_mimes_match_target(&menu.mime_types, target_mime, is_dir)
}

fn service_menu_matches_targets(menu: &DesktopServiceMenu, targets: &[ServiceMenuTarget]) -> bool {
    if targets.is_empty() {
        return false;
    }
    if !service_menu_protocols_match(menu) {
        return false;
    }
    if !service_menu_url_count_matches(menu, targets.len()) {
        return false;
    }
    if !service_menu_executable_condition_matches(menu) {
        return false;
    }
    targets
        .iter()
        .all(|target| service_menu_matches_target(menu, target.mime_type.as_deref(), target.is_dir))
}

fn service_menu_protocols_match(menu: &DesktopServiceMenu) -> bool {
    menu.protocols.is_empty()
        || menu
            .protocols
            .iter()
            .any(|protocol| protocol.eq_ignore_ascii_case("file"))
}

fn service_menu_url_count_matches(menu: &DesktopServiceMenu, count: usize) -> bool {
    if menu
        .required_url_count
        .is_some_and(|required| count != required)
    {
        return false;
    }
    if menu.min_url_count.is_some_and(|minimum| count < minimum) {
        return false;
    }
    if menu.max_url_count.is_some_and(|maximum| count > maximum) {
        return false;
    }
    true
}

fn service_menu_executable_condition_matches(menu: &DesktopServiceMenu) -> bool {
    menu.show_if_executable
        .as_deref()
        .is_none_or(|program| executable_path_for_systemd(program).is_ok())
}

fn desktop_mime_matches_target(pattern: &str, target_mime: Option<&str>, is_dir: bool) -> bool {
    let pattern = pattern.trim();
    if pattern == "all/all" {
        return true;
    }
    if pattern == "all/allfiles" {
        return !is_dir;
    }
    let Some(target_mime) = target_mime else {
        return false;
    };
    if pattern == target_mime {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return target_mime
            .split_once('/')
            .is_some_and(|(target_prefix, _)| target_prefix == prefix);
    }
    false
}

fn desktop_action_supports_path_count(action: &DesktopAction, path_count: usize) -> bool {
    path_count <= 1 || exec_supports_multiple_paths(&action.exec)
}

fn exec_supports_multiple_paths(exec: &str) -> bool {
    split_exec_line(exec).is_some_and(|tokens| {
        tokens
            .iter()
            .any(|token| matches!(token.as_str(), "%F" | "%U"))
    })
}

fn exec_token_contains_file_code(token: &str) -> bool {
    let mut chars = token.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            continue;
        }
        match chars.next() {
            Some('%') => {}
            Some('f' | 'F' | 'u' | 'U') => return true,
            Some(_) | None => {}
        }
    }
    false
}

fn dedup_service_actions(actions: Vec<ServiceMenuAction>) -> Vec<ServiceMenuAction> {
    let mut deduped: Vec<ServiceMenuAction> = Vec::new();
    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    for action in actions {
        if service_action_duplicates_builtin_menu_item(&action) {
            continue;
        }
        let key = service_action_display_key(&action);
        if let Some(existing_index) = seen.get(&key).copied() {
            if deduped[existing_index].priority != ServiceMenuPriority::TopLevel
                && action.priority == ServiceMenuPriority::TopLevel
            {
                deduped[existing_index] = action;
            }
            continue;
        }
        seen.insert(key, deduped.len());
        deduped.push(action);
    }
    deduped
}

fn service_action_display_key(action: &ServiceMenuAction) -> (String, String) {
    (
        action
            .submenu
            .as_deref()
            .map(normalize_service_action_label)
            .unwrap_or_default(),
        normalize_service_action_label(&action.label),
    )
}

fn service_action_duplicates_builtin_menu_item(action: &ServiceMenuAction) -> bool {
    matches!(
        normalize_service_action_label(&action.label).as_str(),
        "open in new window"
            | "open new window"
            | "open in new tab"
            | "open new tab"
            | "open in new pane"
            | "open new pane"
    )
}

fn normalize_service_action_label(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn desktop_service_menu_action_id(menu_id: &str, action_id: &str) -> String {
    format!("service-menu:{menu_id}::{action_id}")
}

fn service_action_launch_desktop_id(source_id: &str, action_id: &str) -> String {
    format!("{source_id}-{action_id}")
}

fn service_action_display_name(source_name: &str, action_name: &str) -> String {
    if source_name.is_empty() {
        action_name.to_string()
    } else {
        format!("{source_name}: {action_name}")
    }
}

fn split_exec_line(exec: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for ch in exec.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        match quote {
            Some(active) if ch == active => quote = None,
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }
    if escaped {
        current.push('\\');
    }
    if quote.is_some() {
        return None;
    }
    if !current.is_empty() {
        args.push(current);
    }
    (!args.is_empty()).then_some(args)
}

fn expand_exec_token(
    token: &str,
    app_name: &str,
    desktop_file: &Path,
    paths: &[PathBuf],
) -> Option<String> {
    let mut out = String::with_capacity(token.len());
    let mut chars = token.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('c') => out.push_str(app_name),
            Some('k') => out.push_str(&desktop_file.display().to_string()),
            Some('f') | Some('u') => {
                let path = paths.first()?;
                out.push_str(&path.display().to_string());
            }
            Some('F') | Some('U') => {
                let path = paths.first()?;
                if paths.len() > 1 {
                    return None;
                }
                out.push_str(&path.display().to_string());
            }
            Some('i') | Some('d') | Some('D') | Some('n') | Some('N') | Some('v') | Some('m') => {}
            Some(_) | None => {}
        }
    }
    (!out.is_empty()).then_some(out)
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

