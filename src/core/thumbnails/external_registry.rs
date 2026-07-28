fn thumbnailer_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        dirs.push(PathBuf::from(data_home).join("thumbnailers"));
    } else if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/thumbnailers"));
    }

    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| OsString::from("/usr/local/share:/usr/share"));
    dirs.extend(env::split_paths(&data_dirs).map(|data_dir| data_dir.join("thumbnailers")));
    dirs
}

fn parse_thumbnailer_definition(contents: &str) -> Option<ThumbnailerDefinition> {
    let entry = parse_desktop_entry_group(contents, "Thumbnailer Entry")?;
    let exec = entry.get("Exec")?.trim().to_string();
    if exec.is_empty() {
        return None;
    }
    let mime_types = entry
        .get("MimeType")?
        .split(';')
        .map(str::trim)
        .filter(|mime| !mime.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if mime_types.is_empty() {
        return None;
    }
    Some(ThumbnailerDefinition {
        exec,
        try_exec: entry
            .get("TryExec")
            .map(|try_exec| try_exec.trim().to_string())
            .filter(|try_exec| !try_exec.is_empty()),
        mime_types,
    })
}

fn parse_desktop_entry_group(contents: &str, group: &str) -> Option<HashMap<String, String>> {
    let mut current_group = None::<String>;
    let mut values = HashMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current_group = Some(section.trim().to_string());
            continue;
        }
        if current_group.as_deref() != Some(group) {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        values.insert(key.to_string(), value.trim().to_string());
    }
    (!values.is_empty()).then_some(values)
}

fn thumbnailer_mime_matches(pattern: &str, mime_type: &str) -> bool {
    if pattern == mime_type {
        return true;
    }
    let Some(prefix) = pattern.strip_suffix("/*") else {
        return false;
    };
    mime_type
        .strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('/'))
}

fn expand_thumbnailer_exec(
    exec: &str,
    input: &Path,
    uri: &str,
    output: &Path,
    size: ThumbnailSize,
) -> Option<ExternalThumbnailerCommand> {
    let tokens = split_exec_template(exec);
    let (program, args) = tokens.split_first()?;
    let program = expand_thumbnailer_exec_token(program, input, uri, output, size)
        .into_string()
        .ok()?;
    if program.is_empty() {
        return None;
    }
    let args = args
        .iter()
        .map(|token| expand_thumbnailer_exec_token(token, input, uri, output, size))
        .collect::<Vec<_>>();
    Some(ExternalThumbnailerCommand { program, args })
}
