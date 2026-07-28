fn path_size(path: &Path) -> io::Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        let mut total: u64 = 0;
        for entry in fs::read_dir(path)? {
            total = total.saturating_add(path_size(&entry?.path())?);
        }
        Ok(total)
    } else {
        Ok(metadata.len())
    }
}

fn ensure_not_cancelled(cancel: Option<&OperationController>) -> io::Result<()> {
    if cancel.is_some_and(OperationController::is_cancelled) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "operation cancelled",
        ))
    } else {
        Ok(())
    }
}

fn remove_path(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn remove_path_if_present(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => remove_path(path),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn transfer_destination(
    source: &Path,
    target_dir: &Path,
    file_name: &std::ffi::OsStr,
    conflict_policy: &str,
) -> Result<PathBuf, String> {
    if let Some(name) = conflict_policy.strip_prefix("rename:") {
        return renamed_destination(target_dir, name);
    }

    let base = target_dir.join(file_name);
    match conflict_policy {
        "keep-both" => Ok(unique_destination(target_dir, file_name)),
        "overwrite" => {
            if base == source {
                return Err("cannot overwrite an item with itself".to_string());
            }
            Ok(base)
        }
        _ => Err(format!("unknown conflict policy: {conflict_policy}")),
    }
}

fn backup_existing_destination(destination: &Path) -> Result<PathBuf, String> {
    let backup = overwrite_backup_path(destination)?;
    fs::rename(destination, &backup).map_err(|err| err.to_string())?;
    Ok(backup)
}

pub fn cleanup_overwrite_backup(backup: &Path) -> Result<(), String> {
    if path_exists(backup) {
        remove_path(backup).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn restore_overwrite_backup(destination: &Path, backup: &Path) -> Result<(), String> {
    if path_exists(destination) {
        remove_path(destination).map_err(|err| err.to_string())?;
    }
    fs::rename(backup, destination).map_err(|err| err.to_string())
}

fn remove_trashinfo(trash_path: &Path) -> Result<(), String> {
    let Some(info_path) = trash_info_path(trash_path) else {
        return Ok(());
    };
    if path_exists(&info_path) {
        fs::remove_file(info_path).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn trash_original_path(trash_path: &Path) -> Result<PathBuf, String> {
    trash_metadata(trash_path).map(|metadata| metadata.original_path)
}

pub fn trash_metadata(trash_path: &Path) -> Result<TrashMetadata, String> {
    trash_metadata_in_dir(trash_path, &trash_info_dir())
}

fn trash_metadata_in_dir(trash_path: &Path, info_dir: &Path) -> Result<TrashMetadata, String> {
    let info_path = trash_info_path_in_dir(trash_path, info_dir)
        .ok_or_else(|| "trash item has no metadata name".to_string())?;
    let contents = fs::read_to_string(&info_path).map_err(|err| {
        format!(
            "failed to read trash metadata {}: {err}",
            info_path.display()
        )
    })?;
    trash_metadata_from_info(&contents)
}

#[cfg(test)]
fn trash_original_path_from_info(contents: &str) -> Result<PathBuf, String> {
    trash_metadata_from_info(contents).map(|metadata| metadata.original_path)
}

fn trash_metadata_from_info(contents: &str) -> Result<TrashMetadata, String> {
    let encoded = contents
        .lines()
        .find_map(|line| line.strip_prefix("Path="))
        .ok_or_else(|| "trash metadata is missing Path".to_string())?;
    let path = percent_decode_path(encoded)?;
    if !path.is_absolute() {
        return Err(format!(
            "trash metadata Path is not absolute: {}",
            path.display()
        ));
    }
    let deletion_date = contents
        .lines()
        .find_map(|line| line.strip_prefix("DeletionDate="))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Ok(TrashMetadata {
        original_path: path,
        deletion_date,
    })
}

fn trash_info_path(trash_path: &Path) -> Option<PathBuf> {
    trash_info_path_in_dir(trash_path, &trash_info_dir())
}

fn trash_info_path_in_dir(trash_path: &Path, info_dir: &Path) -> Option<PathBuf> {
    let name = trash_path.file_name()?.to_str()?;
    Some(info_dir.join(format!("{name}.trashinfo")))
}

fn overwrite_backup_path(destination: &Path) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "overwrite target has no parent folder".to_string())?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("item");
    let pid = std::process::id();
    for index in 0.. {
        let candidate = parent.join(format!(".{name}.fika-overwrite-backup-{pid}-{index}"));
        if !path_exists(&candidate) {
            return Ok(candidate);
        }
    }
    unreachable!("unbounded overwrite backup search should always return")
}

fn sanitize_child_name(name: &str) -> Result<std::ffi::OsString, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("name cannot be empty".to_string());
    }
    if name.contains('/') || name == "." || name == ".." {
        return Err("name must not contain path separators".to_string());
    }
    Ok(name.into())
}

fn trash_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Trash")
}

fn trash_status_empty_at(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| trash_status_empty_from_contents(&contents))
        .unwrap_or(true)
}

fn write_trash_status_empty_at(path: &Path, empty: bool) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, trash_status_contents(empty)).map_err(|err| err.to_string())
}

async fn write_trash_status_empty_at_async(path: &Path, empty: bool) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        create_dir_all_async(parent)
            .await
            .map_err(|err| err.to_string())?;
    }
    write_file_async(path, trash_status_contents(empty))
        .await
        .map_err(|err| err.to_string())
}

fn trash_status_contents(empty: bool) -> String {
    format!("[Status]\nEmpty={}\n", if empty { "true" } else { "false" })
}

fn trash_status_empty_from_contents(contents: &str) -> Option<bool> {
    let mut in_status = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_status = line[1..line.len() - 1].trim() == "Status";
            continue;
        }
        if !in_status {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "Empty" {
            continue;
        }
        return match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        };
    }
    None
}

fn trashinfo(path: &Path) -> String {
    format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        percent_encode_path(path),
        current_trash_time()
    )
}

fn current_trash_time() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let days = secs / 86_400;
    let seconds_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m as u32, d as u32)
}

fn percent_encode_path(path: &Path) -> String {
    path.to_string_lossy()
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'-' | b'_' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn percent_decode_path(value: &str) -> Result<PathBuf, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("trash metadata Path contains truncated percent escape".to_string());
            }
            let high = hex_value(bytes[index + 1])
                .ok_or_else(|| "trash metadata Path contains invalid percent escape".to_string())?;
            let low = hex_value(bytes[index + 2])
                .ok_or_else(|| "trash metadata Path contains invalid percent escape".to_string())?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded)
        .map(PathBuf::from)
        .map_err(|_| "trash metadata Path is not valid UTF-8".to_string())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn unique_destination(target_dir: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    let initial = target_dir.join(file_name);
    if !path_exists(&initial) {
        return initial;
    }

    let source_name = Path::new(file_name);
    let stem = source_name
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("item");
    let extension = source_name
        .extension()
        .and_then(|extension| extension.to_str());

    for index in 1.. {
        let suffix = if index == 1 {
            "copy".to_string()
        } else {
            format!("copy {index}")
        };
        let candidate_name = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem} {suffix}.{extension}"),
            _ => format!("{stem} {suffix}"),
        };
        let candidate = target_dir.join(candidate_name);
        if !path_exists(&candidate) {
            return candidate;
        }
    }

    unreachable!("unbounded destination search should always return")
}

pub fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

