
fn archive_extract_dir(cache_dir: &Path, archive_path: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    archive_path.hash(&mut hasher);
    let file_name = archive_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("wallpaper")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    cache_dir
        .join("render-cache")
        .join(format!("{}-{:016x}.gwpdir", file_name, hasher.finish()))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RenderCachePruneReport {
    entries_after: usize,
    bytes_after: u64,
    evictions: u64,
    errors: u64,
}

fn prune_render_cache(
    cache_dir: &Path,
    max_entries: usize,
    protected_archive_dirs: &BTreeSet<PathBuf>,
) -> RenderCachePruneReport {
    let render_cache_dir = cache_dir.join("render-cache");
    let Ok(mut entries) = render_cache_entries(&render_cache_dir) else {
        return RenderCachePruneReport::default();
    };
    let entries_before = entries.len();
    let remove_count = entries_before.saturating_sub(max_entries);
    if remove_count == 0 {
        return RenderCachePruneReport {
            entries_after: entries_before,
            bytes_after: 0,
            evictions: 0,
            errors: 0,
        };
    }

    entries.sort_by_key(|entry| (entry.last_used, entry.path.clone()));
    let mut evictions = 0;
    let mut errors = 0;
    for entry in entries
        .iter()
        .filter(|entry| !protected_archive_dirs.contains(&entry.path))
        .take(remove_count)
    {
        match fs::remove_dir_all(&entry.path) {
            Ok(()) => evictions += 1,
            Err(_) => errors += 1,
        }
    }

    let entries_after = render_cache_entries(&render_cache_dir)
        .map(|entries| entries.len())
        .unwrap_or_else(|_| entries_before.saturating_sub(evictions as usize));
    RenderCachePruneReport {
        entries_after,
        bytes_after: 0,
        evictions,
        errors,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderCacheEntry {
    path: PathBuf,
    last_used: SystemTime,
    size_bytes: u64,
}

fn render_cache_entries(render_cache_dir: &Path) -> Result<Vec<RenderCacheEntry>, std::io::Error> {
    let mut entries = Vec::new();
    let read_dir = match fs::read_dir(render_cache_dir) {
        Ok(read_dir) => read_dir,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(entries),
        Err(err) => return Err(err),
    };
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        if !is_archive_cache_dir(&path, &entry.file_type()?) {
            continue;
        }
        entries.push(RenderCacheEntry {
            last_used: archive_cache_last_used(&path),
            path,
            size_bytes: 0,
        });
    }
    Ok(entries)
}

fn is_archive_cache_dir(path: &Path, file_type: &fs::FileType) -> bool {
    file_type.is_dir()
        && path.extension().and_then(|extension| extension.to_str()) == Some("gwpdir")
}

fn archive_cache_last_used(path: &Path) -> SystemTime {
    fs::metadata(path.join(".tensor-wallpaper-cache-used"))
        .or_else(|_| fs::metadata(path))
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH)
}

fn mark_archive_cache_used(extract_dir: &Path) {
    let _ = fs::write(extract_dir.join(".tensor-wallpaper-cache-used"), b"");
}

fn prune_static_image_cache(
    cache_dir: &Path,
    max_entries: usize,
    max_bytes: u64,
    protected_files: &BTreeSet<PathBuf>,
) -> RenderCachePruneReport {
    let static_cache_dir = cache_dir.join("static-image-cache");
    let Ok(mut entries) = static_image_cache_entries(&static_cache_dir) else {
        return RenderCachePruneReport::default();
    };
    entries.sort_by_key(|entry| (entry.last_used, entry.path.clone()));
    let mut evictions = 0;
    let mut errors = 0;
    let mut retained_entries = entries.len();
    let mut retained_bytes = entries.iter().map(|entry| entry.size_bytes).sum::<u64>();
    let mut removable_index = 0;
    while retained_entries > max_entries || (max_bytes > 0 && retained_bytes > max_bytes) {
        let Some(entry) = entries
            .iter()
            .skip(removable_index)
            .find(|entry| !protected_files.contains(&entry.path))
        else {
            break;
        };
        removable_index = entries
            .iter()
            .position(|candidate| candidate.path == entry.path)
            .map(|index| index + 1)
            .unwrap_or(entries.len());
        let marker = static_image_cache_used_marker(&entry.path);
        match fs::remove_file(&entry.path) {
            Ok(()) => {
                evictions += 1;
                retained_entries = retained_entries.saturating_sub(1);
                retained_bytes = retained_bytes.saturating_sub(entry.size_bytes);
                let _ = fs::remove_file(marker);
            }
            Err(_) => errors += 1,
        }
    }

    let (entries_after, bytes_after) = static_image_cache_entries(&static_cache_dir)
        .map(|entries| {
            (
                entries.len(),
                entries.iter().map(|entry| entry.size_bytes).sum::<u64>(),
            )
        })
        .unwrap_or((retained_entries, retained_bytes));
    RenderCachePruneReport {
        entries_after,
        bytes_after,
        evictions,
        errors,
    }
}

fn static_image_cache_entries(
    static_cache_dir: &Path,
) -> Result<Vec<RenderCacheEntry>, std::io::Error> {
    let mut entries = Vec::new();
    let read_dir = match fs::read_dir(static_cache_dir) {
        Ok(read_dir) => read_dir,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(entries),
        Err(err) => return Err(err),
    };
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        if !is_static_image_cache_file(&path, &entry.file_type()?) {
            continue;
        }
        entries.push(RenderCacheEntry {
            last_used: static_image_cache_last_used(&path),
            size_bytes: entry.metadata().map(|metadata| metadata.len()).unwrap_or(0),
            path,
        });
    }
    Ok(entries)
}

fn is_static_image_cache_file(path: &Path, file_type: &fs::FileType) -> bool {
    file_type.is_file() && path.extension().and_then(|extension| extension.to_str()) == Some("png")
}

fn static_image_cache_used_marker(path: &Path) -> PathBuf {
    path.with_extension("png.used")
}

fn static_image_cache_last_used(path: &Path) -> SystemTime {
    fs::metadata(static_image_cache_used_marker(path))
        .or_else(|_| fs::metadata(path))
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH)
}

fn mark_static_image_cache_used(path: &Path) {
    let _ = fs::write(static_image_cache_used_marker(path), b"");
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    include!("render_plan_tests.rs");
    include!("cache_policy_tests.rs");
}
