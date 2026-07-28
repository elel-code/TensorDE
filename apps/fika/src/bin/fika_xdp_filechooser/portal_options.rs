fn gui_executable() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("FIKA_GUI") {
        return Ok(PathBuf::from(path));
    }

    let exe = env::current_exe().map_err(|err| format!("cannot locate executable: {err}"))?;
    let Some(dir) = exe.parent() else {
        return Err(format!("cannot locate fika next to {}", exe.display()));
    };
    Ok(dir.join("fika"))
}

fn current_folder(options: &HashMap<String, OwnedValue>) -> Option<PathBuf> {
    let value = options.get("current_folder")?;
    nul_terminated_bytes_to_location(Vec::<u8>::try_from(value.clone()).ok()?)
}

fn save_file_start_and_name(
    options: &HashMap<String, OwnedValue>,
) -> (Option<PathBuf>, Option<String>) {
    if let Some(current_file) = options
        .get("current_file")
        .and_then(|value| Vec::<u8>::try_from(value.clone()).ok())
        .and_then(nul_terminated_bytes_to_location)
    {
        let start_dir = fika_core::parent_location(&current_file);
        let name = current_file
            .file_name()
            .map(|name| name.to_string_lossy().to_string());
        return (start_dir, name);
    }

    (
        current_folder(options),
        option_string(options, "current_name"),
    )
}

fn save_files(options: &HashMap<String, OwnedValue>) -> Vec<String> {
    options
        .get("files")
        .and_then(|value| Vec::<Vec<u8>>::try_from(value.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(nul_terminated_bytes_to_string)
        .collect()
}

fn portal_filters(options: &HashMap<String, OwnedValue>) -> Vec<PortalFilter> {
    options
        .get("filters")
        .and_then(|value| Vec::<PortalFilter>::try_from(value.clone()).ok())
        .unwrap_or_default()
}

fn portal_choices(options: &HashMap<String, OwnedValue>) -> Vec<PortalChoice> {
    options
        .get("choices")
        .and_then(|value| Vec::<PortalChoice>::try_from(value.clone()).ok())
        .unwrap_or_default()
}

fn chooser_filter_map(
    options: &HashMap<String, OwnedValue>,
    portal_filters: Vec<PortalFilter>,
) -> ChooserFilterMap {
    let current_filter = options
        .get("current_filter")
        .and_then(|value| PortalFilter::try_from(value.clone()).ok());
    let mut map = ChooserFilterMap {
        portal_filters,
        ..ChooserFilterMap::default()
    };

    for (portal_index, filter) in map.portal_filters.iter().enumerate() {
        let Some((spec, mime_mapped)) = chooser_filter_spec(filter, portal_index) else {
            map.hidden_filters += 1;
            continue;
        };
        if current_filter
            .as_ref()
            .is_some_and(|current| current == filter)
        {
            map.initial_chooser_index = Some(map.chooser_specs.len());
        }
        map.chooser_specs.push(spec);
        map.portal_indices.push(portal_index);
        if mime_mapped {
            map.mime_mapped_filters += 1;
        }
    }

    map
}

fn chooser_filter_spec(
    (label, patterns): &PortalFilter,
    portal_index: usize,
) -> Option<(String, bool)> {
    let (globs, mime_mapped) = chooser_filter_globs(patterns);
    if globs.is_empty() {
        return None;
    }
    let label = chooser_filter_label(label, portal_index);
    Some((format!("{label}\t{}", globs.join(";")), mime_mapped))
}

fn chooser_filter_label(label: &str, portal_index: usize) -> String {
    let label = label.trim();
    if label.is_empty() {
        format!("Filter {}", portal_index + 1)
    } else {
        label.to_string()
    }
}

fn chooser_filter_globs(patterns: &[(u32, String)]) -> (Vec<String>, bool) {
    let mut globs = Vec::new();
    let mut mime_mapped = false;
    for (kind, pattern) in patterns {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            continue;
        }

        match kind {
            0 => push_unique_glob(&mut globs, pattern),
            1 => {
                let mime_globs = mime_filter_globs(pattern);
                if !mime_globs.is_empty() {
                    mime_mapped = true;
                }
                for glob in mime_globs {
                    push_unique_glob(&mut globs, glob);
                }
            }
            _ => {}
        }
    }
    (globs, mime_mapped)
}

fn push_unique_glob(globs: &mut Vec<String>, glob: &str) {
    if !globs.iter().any(|existing| existing == glob) {
        globs.push(glob.to_string());
    }
}

fn mime_filter_globs(mime_type: &str) -> &'static [&'static str] {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/*" => &[
            "*.avif", "*.avifs", "*.bmp", "*.gif", "*.heic", "*.heif", "*.jpeg", "*.jpg", "*.png",
            "*.svg", "*.webp",
        ],
        "image/avif" | "image/avif-sequence" => &["*.avif", "*.avifs"],
        "image/bmp" | "image/x-bmp" => &["*.bmp"],
        "image/gif" => &["*.gif"],
        "image/heic" => &["*.heic"],
        "image/heif" => &["*.heif"],
        "image/jpeg" | "image/pjpeg" => &["*.jpg", "*.jpeg"],
        "image/png" | "image/x-png" => &["*.png"],
        "image/svg+xml" => &["*.svg"],
        "image/webp" => &["*.webp"],
        "text/*" => &[
            "*.txt",
            "*.text",
            "*.md",
            "*.markdown",
            "*.rst",
            "*.csv",
            "*.tsv",
            "*.log",
            "*.ini",
            "*.conf",
            "*.toml",
            "*.json",
            "*.yaml",
            "*.yml",
            "*.xml",
            "*.html",
            "*.htm",
            "*.css",
            "*.js",
            "*.rs",
            "*.c",
            "*.h",
            "*.cpp",
            "*.hpp",
            "*.py",
            "*.sh",
        ],
        "text/plain" => &[
            "*.txt",
            "*.text",
            "*.md",
            "*.markdown",
            "*.rst",
            "*.log",
            "*.ini",
            "*.conf",
        ],
        "text/csv" => &["*.csv"],
        "text/html" => &["*.html", "*.htm"],
        "text/markdown" | "text/x-markdown" => &["*.md", "*.markdown"],
        "text/xml" | "application/xml" => &["*.xml"],
        "application/json" => &["*.json"],
        "application/pdf" => &["*.pdf"],
        "application/zip" => &["*.zip"],
        "application/x-zip-compressed" => &["*.zip"],
        "application/gzip" => &["*.gz"],
        "application/x-bzip2" => &["*.bz2"],
        "application/x-tar" => &["*.tar"],
        "application/x-xz" => &["*.xz"],
        "application/zstd" | "application/x-zstd" => &["*.zst"],
        "application/x-7z-compressed" => &["*.7z"],
        "application/vnd.rar" | "application/x-rar-compressed" => &["*.rar"],
        "audio/*" => &[
            "*.aac", "*.flac", "*.m4a", "*.mp3", "*.oga", "*.ogg", "*.opus", "*.wav",
        ],
        "audio/aac" => &["*.aac"],
        "audio/flac" | "audio/x-flac" => &["*.flac"],
        "audio/mp4" | "audio/x-m4a" => &["*.m4a"],
        "audio/mpeg" => &["*.mp3"],
        "audio/ogg" => &["*.oga", "*.ogg"],
        "audio/opus" => &["*.opus"],
        "audio/wav" | "audio/x-wav" => &["*.wav"],
        "video/*" => &[
            "*.avi", "*.m4v", "*.mkv", "*.mov", "*.mp4", "*.mpeg", "*.mpg", "*.ogv", "*.webm",
        ],
        "video/mp4" => &["*.mp4", "*.m4v"],
        "video/mpeg" => &["*.mpeg", "*.mpg"],
        "video/quicktime" => &["*.mov"],
        "video/webm" => &["*.webm"],
        "video/x-matroska" => &["*.mkv"],
        "video/x-msvideo" | "video/avi" => &["*.avi"],
        "application/msword" => &["*.doc"],
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => &["*.docx"],
        "application/vnd.oasis.opendocument.text" => &["*.odt"],
        "application/vnd.ms-excel" => &["*.xls"],
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => &["*.xlsx"],
        "application/vnd.oasis.opendocument.spreadsheet" => &["*.ods"],
        "application/vnd.ms-powerpoint" => &["*.ppt"],
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => &["*.pptx"],
        "application/vnd.oasis.opendocument.presentation" => &["*.odp"],
        _ => &[],
    }
}

fn chooser_choice_specs(choices: &[PortalChoice]) -> Vec<String> {
    choices
        .iter()
        .filter_map(|(id, label, items, default)| {
            let id = chooser_choice_token(id)?;
            let label = chooser_choice_label(label).unwrap_or_else(|| id.clone());
            let items = items
                .iter()
                .filter_map(|(item_id, item_label)| {
                    let item_id = chooser_choice_token(item_id)?;
                    let item_label =
                        chooser_choice_label(item_label).unwrap_or_else(|| item_id.clone());
                    Some((item_id, item_label))
                })
                .collect::<Vec<_>>();
            if items.is_empty() {
                return None;
            }
            let default = chooser_choice_token(default)
                .filter(|default| items.iter().any(|(item_id, _)| item_id == default))
                .unwrap_or_default();
            let items = items
                .into_iter()
                .map(|(item_id, item_label)| format!("{item_id}={item_label}"))
                .collect::<Vec<_>>()
                .join(";");
            Some(format!("{id}\t{label}\t{default}\t{items}"))
        })
        .collect()
}

fn chooser_choice_token(value: &str) -> Option<String> {
    (!value.trim().is_empty()
        && !value
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, ';' | '=')))
    .then(|| value.to_string())
}

fn chooser_choice_label(value: &str) -> Option<String> {
    let normalized = value
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, ';' | '=') {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn nul_terminated_bytes_to_location(bytes: Vec<u8>) -> Option<PathBuf> {
    nul_terminated_bytes_to_string(bytes).map(|value| {
        fika_core::normalize_network_uri(&value)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(value))
    })
}

fn nul_terminated_bytes_to_string(bytes: Vec<u8>) -> Option<String> {
    let bytes = bytes
        .split(|byte| *byte == 0)
        .next()
        .filter(|bytes| !bytes.is_empty())?;
    Some(String::from_utf8_lossy(bytes).to_string())
}

fn option_bool(options: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    options
        .get(key)
        .and_then(|value| bool::try_from(value.clone()).ok())
}

fn option_string(options: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    options
        .get(key)
        .and_then(|value| String::try_from(value.clone()).ok())
        .filter(|value| !value.is_empty())
}

fn portal_debug_log_request(request: PortalRequestDebug<'_>) {
    if !portal_debug_enabled() {
        return;
    }

    eprintln!("[fika portal] {}", portal_request_summary(request));
}

fn portal_request_summary(request: PortalRequestDebug<'_>) -> String {
    format!(
        "request method={} handle={} start_dir={} directory={} multiple={} save_kind={} save_files={} portal_filters={} chooser_filters={} mime_mapped_filters={} hidden_filters={} initial_filter={} portal_choices={} parent_status={} parent_forwarded={} parent_binding={} parent_binding_reason={} native_transient=false",
        request.method,
        request.handle,
        request
            .start_dir
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        request.directory,
        request.multiple,
        request.save_kind,
        request.save_files,
        request.portal_filters,
        request.chooser_filters,
        request.mime_mapped_filters,
        request.hidden_filters,
        request
            .initial_filter_index
            .map(|index| index.to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        request.portal_choices,
        request.parent_status.as_str(),
        request.parent_forwarded,
        parent_window_binding(request.parent_status),
        parent_window_binding_reason(request.parent_status),
    )
}

fn portal_parent_window(parent_window: String) -> ParentWindowDecision {
    let parent_window = parent_window.trim();
    if parent_window.is_empty() {
        let decision = ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::Empty,
        };
        portal_debug_log_parent(&decision);
        return decision;
    }

    let Some((scheme, handle)) = parent_window.split_once(':') else {
        let decision = ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::Malformed,
        };
        portal_debug_log_parent(&decision);
        return decision;
    };

    if handle.is_empty() {
        let decision = ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::EmptyHandle,
        };
        portal_debug_log_parent(&decision);
        return decision;
    }

    let decision = if scheme == "wayland" {
        ParentWindowDecision {
            handle: Some(parent_window.to_string()),
            status: ParentWindowStatus::Accepted,
        }
    } else {
        ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::UnsupportedScheme,
        }
    };
    portal_debug_log_parent(&decision);
    decision
}

fn portal_debug_log_parent(decision: &ParentWindowDecision) {
    if !portal_debug_enabled() {
        return;
    }

    eprintln!(
        "[fika portal] parent_window status={} handle={} parent_binding={} parent_binding_reason={} native_transient=false",
        decision.status.as_str(),
        decision.handle.as_deref().unwrap_or(""),
        parent_window_binding(decision.status),
        parent_window_binding_reason(decision.status),
    );
}

fn portal_debug_log_chooser_lifecycle(handle: &str, outcome: &Result<ChooserRun, String>) {
    if !portal_debug_enabled() {
        return;
    }

    eprintln!(
        "[fika portal] {}",
        chooser_lifecycle_summary(handle, outcome)
    );
}

fn chooser_lifecycle_summary(handle: &str, outcome: &Result<ChooserRun, String>) -> String {
    match outcome {
        Ok(ChooserRun::Selected(result)) => format!(
            "chooser_finished handle={} outcome=selected paths={} filter={} choices={}",
            handle,
            result.paths.len(),
            result
                .filter_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            result.choices.len()
        ),
        Ok(ChooserRun::Cancelled(reason)) => format!(
            "chooser_finished handle={} outcome=cancelled reason={}",
            handle,
            reason.as_str()
        ),
        Err(err) => format!(
            "chooser_finished handle={} outcome=failed error={}",
            handle,
            first_log_line(err)
        ),
    }
}

fn first_log_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}

fn portal_debug_enabled() -> bool {
    static DEBUG_PORTAL: OnceLock<bool> = OnceLock::new();
    *DEBUG_PORTAL.get_or_init(|| {
        env::var("FIKA_DEBUG_PORTAL").is_ok_and(|value| env_flag_is_truthy(value.as_str()))
    })
}

fn env_flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn portal_title(title: String) -> Option<String> {
    (!title.is_empty()).then_some(title)
}

fn cancelled() -> (u32, HashMap<String, OwnedValue>) {
    (1, HashMap::new())
}

fn results_for_paths(
    result: ChooserResult,
    options: &HashMap<String, OwnedValue>,
    filter_map: &ChooserFilterMap,
) -> HashMap<String, OwnedValue> {
    let uris = result
        .paths
        .iter()
        .map(|path| path_to_portal_uri(path))
        .collect::<Vec<_>>();
    let mut results = HashMap::new();
    if let Ok(value) = OwnedValue::try_from(Value::new(uris)) {
        results.insert("uris".to_string(), value);
    }
    if let Some(current_filter) = result
        .filter_index
        .and_then(|index| filter_map.portal_indices.get(index))
        .and_then(|portal_index| filter_map.portal_filters.get(*portal_index))
        .cloned()
        .and_then(|filter| OwnedValue::try_from(Value::new(filter)).ok())
    {
        results.insert("current_filter".to_string(), current_filter);
    }
    if let Some(choices) = selected_choices_for_options(options, &result.choices)
        && let Ok(value) = OwnedValue::try_from(Value::new(choices))
    {
        results.insert("choices".to_string(), value);
    }
    results
}

fn selected_choices_for_options(
    options: &HashMap<String, OwnedValue>,
    requested_choices: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    let choices = options
        .get("choices")
        .and_then(|value| Vec::<PortalChoice>::try_from(value.clone()).ok())?;
    Some(
        choices
            .into_iter()
            .filter_map(|(id, _label, items, default)| {
                let selected = requested_choices
                    .iter()
                    .find(|(choice_id, selected)| {
                        choice_id == &id && items.iter().any(|(item_id, _)| item_id == selected)
                    })
                    .map(|(_, selected)| selected.clone())
                    .or_else(|| {
                        items
                            .iter()
                            .any(|(item_id, _)| item_id == &default)
                            .then_some(default)
                    })
                    .or_else(|| items.first().map(|(item_id, _)| item_id.clone()))?;
                Some((id, selected))
            })
            .collect(),
    )
}

fn path_to_portal_uri(path: &Path) -> String {
    fika_core::path_uri_from_path(path)
}

fn print_help() {
    println!(
        "Usage: fika-xdp-filechooser\n\n\
         Runs Fika's experimental xdg-desktop-portal FileChooser backend.\n\
         The backend owns {BUS_NAME} and launches `fika --chooser` for FileChooser requests."
    );
}

