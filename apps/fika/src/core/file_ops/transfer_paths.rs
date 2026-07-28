fn move_path(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let bytes_total = path_size(source).unwrap_or_default();
    match fs::rename(source, destination) {
        Ok(()) => {
            progress(TransferProgress {
                bytes_done: bytes_total,
                bytes_total,
            });
            Ok(())
        }
        Err(err) if err.raw_os_error() == Some(18) => {
            let result = copy_path(source, destination, cancel, progress);
            if let Err(err) = &result
                && err.kind() == io::ErrorKind::Interrupted
            {
                let _ = remove_path(destination);
            }
            result?;
            ensure_not_cancelled(cancel)?;
            if let Err(err) = remove_path(source) {
                let _ = remove_path(destination);
                Err(err)
            } else {
                Ok(())
            }
        }
        Err(err) => Err(err),
    }
}

fn copy_path(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let destination_preexisting = fs::symlink_metadata(destination).is_ok();
    let bytes_total = path_size(source)?;
    let mut bytes_done = 0;
    let result = copy_path_inner(
        source,
        destination,
        cancel,
        &mut bytes_done,
        bytes_total,
        progress,
    );

    if result.is_err() && !destination_preexisting {
        let _ = remove_path_if_present(destination);
    }

    result
}

fn copy_path_inner(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        copy_symlink(
            source,
            destination,
            cancel,
            bytes_done,
            bytes_total,
            metadata.len(),
            progress,
        )
    } else if metadata.is_dir() {
        copy_directory(
            source,
            destination,
            cancel,
            bytes_done,
            bytes_total,
            progress,
        )
    } else {
        copy_file(
            source,
            destination,
            cancel,
            bytes_done,
            bytes_total,
            progress,
        )
    }
}

fn copy_symlink(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    link_size: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let target = fs::read_link(source)?;

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, destination)?;
    }

    #[cfg(not(unix))]
    {
        let metadata = fs::metadata(source)?;
        if metadata.is_dir() {
            std::os::windows::fs::symlink_dir(&target, destination)?;
        } else {
            std::os::windows::fs::symlink_file(&target, destination)?;
        }
    }

    *bytes_done = bytes_done.saturating_add(link_size).min(bytes_total);
    progress(TransferProgress {
        bytes_done: *bytes_done,
        bytes_total,
    });
    Ok(())
}

fn copy_directory(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    fs::create_dir(destination)?;
    let metadata = fs::metadata(source)?;
    fs::set_permissions(destination, metadata.permissions())?;

    for entry in fs::read_dir(source)? {
        ensure_not_cancelled(cancel)?;
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        copy_path_inner(
            &source_path,
            &destination_path,
            cancel,
            bytes_done,
            bytes_total,
            progress,
        )?;
    }

    Ok(())
}

fn copy_file(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    let mut reader = fs::File::open(source)?;
    let mut writer = fs::File::create(destination)?;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        ensure_not_cancelled(cancel)?;
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read])?;
        *bytes_done = bytes_done.saturating_add(read as u64);
        progress(TransferProgress {
            bytes_done: *bytes_done,
            bytes_total,
        });
    }

    if let Ok(metadata) = fs::metadata(source) {
        let _ = fs::set_permissions(destination, metadata.permissions());
    }
    Ok(())
}

async fn move_path_async(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let bytes_total = path_size_async(source).await.unwrap_or_default();
    match compio::fs::rename(source, destination).await {
        Ok(()) => {
            progress(TransferProgress {
                bytes_done: bytes_total,
                bytes_total,
            });
            Ok(())
        }
        Err(err) if err.raw_os_error() == Some(18) => {
            let result = copy_path_async(source, destination, cancel, progress).await;
            if let Err(err) = &result
                && err.kind() == io::ErrorKind::Interrupted
            {
                let _ = remove_path_async(destination).await;
            }
            result?;
            ensure_not_cancelled(cancel)?;
            if let Err(err) = remove_path_async(source).await {
                let _ = remove_path_async(destination).await;
                Err(err)
            } else {
                Ok(())
            }
        }
        Err(err) => Err(err),
    }
}

async fn copy_path_async(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let (destination_preexisting, bytes_total) =
        copy_path_preflight_async(source, destination).await?;
    let mut bytes_done = 0;
    let result = copy_path_inner_async(
        source,
        destination,
        cancel,
        &mut bytes_done,
        bytes_total,
        progress,
    )
    .await;

    if result.is_err() && !destination_preexisting {
        let _ = remove_path_if_present_async(destination).await;
    }

    result
}

fn copy_path_inner_async<'a, P>(
    source: &'a Path,
    destination: &'a Path,
    cancel: Option<&'a OperationController>,
    bytes_done: &'a mut u64,
    bytes_total: u64,
    progress: &'a mut P,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + 'a>>
where
    P: FnMut(TransferProgress) + 'a,
{
    Box::pin(async move {
        let metadata = symlink_metadata_async(source).await?;
        if metadata.file_type().is_symlink() {
            copy_symlink_async(
                source,
                destination,
                cancel,
                bytes_done,
                bytes_total,
                metadata.len(),
                progress,
            )
            .await
        } else if metadata.is_dir() {
            copy_directory_async(
                source,
                destination,
                cancel,
                bytes_done,
                bytes_total,
                progress,
            )
            .await
        } else {
            copy_file_async(
                source,
                destination,
                cancel,
                bytes_done,
                bytes_total,
                progress,
            )
            .await
        }
    })
}

async fn copy_symlink_async(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    link_size: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let target = read_link_async(source).await?;

    #[cfg(unix)]
    {
        compio::fs::symlink(&target, destination).await?;
    }

    #[cfg(not(unix))]
    {
        let metadata = metadata_async(source).await?;
        if metadata.is_dir() {
            compio::fs::symlink_dir(&target, destination).await?;
        } else {
            compio::fs::symlink_file(&target, destination).await?;
        }
    }

    *bytes_done = bytes_done.saturating_add(link_size).min(bytes_total);
    progress(TransferProgress {
        bytes_done: *bytes_done,
        bytes_total,
    });
    Ok(())
}

async fn copy_directory_async(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    compio::fs::create_dir(destination).await?;
    if let Ok(metadata) = compio::fs::metadata(source).await {
        let _ = compio::fs::set_permissions(destination, metadata.permissions()).await;
    }

    for (source_path, file_name) in read_dir_entries_async(source).await? {
        ensure_not_cancelled(cancel)?;
        let destination_path = destination.join(file_name);
        copy_path_inner_async(
            &source_path,
            &destination_path,
            cancel,
            bytes_done,
            bytes_total,
            progress,
        )
        .await?;
    }

    Ok(())
}

async fn copy_file_async(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    let from_file = compio::fs::File::open(source).await?;
    let mut to_file = compio::fs::File::create(destination).await?;
    if let Ok(metadata) = compio::fs::metadata(source).await {
        let _ = to_file.set_permissions(metadata.permissions()).await;
    }

    let mut buffer = vec![0_u8; 64 * 1024];
    let mut position = 0_u64;

    loop {
        ensure_not_cancelled(cancel)?;
        let BufResult(read_result, read_buffer) = from_file.read_at(buffer, position).await;
        let read = read_result?;
        if read == 0 {
            break;
        }
        let BufResult(write_result, write_buffer) = to_file
            .write_all_at(compio::buf::IoBuf::slice(read_buffer, ..read), position)
            .await;
        buffer = write_buffer.into_inner();
        write_result?;
        *bytes_done = bytes_done.saturating_add(read as u64);
        position = position.saturating_add(read as u64);
        progress(TransferProgress {
            bytes_done: *bytes_done,
            bytes_total,
        });
    }

    let _ = from_file.close().await;
    let _ = to_file.close().await;
    Ok(())
}

fn link_path(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, destination)
    }

    #[cfg(not(unix))]
    {
        let metadata = fs::metadata(source)?;
        if metadata.is_dir() {
            std::os::windows::fs::symlink_dir(source, destination)
        } else {
            std::os::windows::fs::symlink_file(source, destination)
        }
    }
}

async fn link_path_async(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        compio::fs::symlink(source, destination).await
    }

    #[cfg(not(unix))]
    {
        let metadata = metadata_async(source).await?;
        if metadata.is_dir() {
            compio::fs::symlink_dir(source, destination).await
        } else {
            compio::fs::symlink_file(source, destination).await
        }
    }
}

async fn copy_path_preflight_async(source: &Path, destination: &Path) -> io::Result<(bool, u64)> {
    Ok((
        symlink_metadata_async(destination).await.is_ok(),
        path_size_async(source).await?,
    ))
}

fn path_size_async<'a>(
    path: &'a Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<u64>> + 'a>> {
    Box::pin(async move {
        let metadata = symlink_metadata_async(path).await?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            let mut total: u64 = 0;
            for (entry_path, _) in read_dir_entries_async(path).await? {
                total = total.saturating_add(path_size_async(&entry_path).await?);
            }
            Ok(total)
        } else {
            Ok(metadata.len())
        }
    })
}

async fn symlink_metadata_async(path: &Path) -> io::Result<compio::fs::Metadata> {
    compio::fs::symlink_metadata(path).await
}

async fn metadata_async(path: &Path) -> io::Result<compio::fs::Metadata> {
    compio::fs::metadata(path).await
}

async fn read_link_async(path: &Path) -> io::Result<PathBuf> {
    let path = path.to_path_buf();
    compio_blocking_io(move || fs::read_link(path)).await
}

fn read_dir_entries(path: &Path) -> io::Result<Vec<(PathBuf, OsString)>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        entries.push((entry.path(), entry.file_name()));
    }
    Ok(entries)
}

async fn read_dir_entries_async(path: &Path) -> io::Result<Vec<(PathBuf, OsString)>> {
    let path = path.to_path_buf();
    compio_blocking_io(move || {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            entries.push((entry.path(), entry.file_name()));
        }
        Ok(entries)
    })
    .await
}

async fn canonicalize_async(path: &Path) -> io::Result<PathBuf> {
    let path = path.to_path_buf();
    compio_blocking_io(move || path.canonicalize()).await
}

async fn compio_blocking_io<T>(
    task: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T>
where
    T: Send + 'static,
{
    compio::runtime::spawn_blocking(task)
        .await
        .map_err(|err| io::Error::other(err.to_string()))?
}

fn create_dir_all_async<'a>(
    path: &'a Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + 'a>> {
    Box::pin(async move {
        if path.as_os_str().is_empty() {
            return Ok(());
        }
        if metadata_async(path)
            .await
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            return Ok(());
        }
        if let Some(parent) = path.parent().filter(|parent| *parent != path) {
            create_dir_all_async(parent).await?;
        }
        match compio::fs::create_dir(path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                if metadata_async(path)
                    .await
                    .map(|metadata| metadata.is_dir())
                    .unwrap_or(false)
                {
                    Ok(())
                } else {
                    Err(err)
                }
            }
            Err(err) => Err(err),
        }
    })
}

async fn write_file_async(path: &Path, contents: String) -> io::Result<()> {
    let mut file = compio::fs::File::create(path).await?;
    let BufResult(write_result, file_contents) = file.write_all_at(contents.into_bytes(), 0).await;
    write_result?;
    drop(file_contents);
    file.close().await
}

async fn remove_path_async(path: &Path) -> io::Result<()> {
    let metadata = symlink_metadata_async(path).await?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        remove_dir_all_async(path).await
    } else {
        compio::fs::remove_file(path).await
    }
}

async fn remove_path_if_present_async(path: &Path) -> io::Result<()> {
    match symlink_metadata_async(path).await {
        Ok(_) => remove_path_async(path).await,
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn remove_dir_all_async<'a>(
    path: &'a Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + 'a>> {
    Box::pin(async move {
        for (entry_path, _) in read_dir_entries_async(path).await? {
            remove_path_async(&entry_path).await?;
        }
        compio::fs::remove_dir(path).await
    })
}

async fn path_exists_async(path: &Path) -> bool {
    symlink_metadata_async(path).await.is_ok()
}

async fn transfer_destination_async(
    source: &Path,
    target_dir: &Path,
    file_name: &std::ffi::OsStr,
    conflict_policy: &str,
) -> Result<PathBuf, String> {
    if let Some(name) = conflict_policy.strip_prefix("rename:") {
        return renamed_destination_async(target_dir, name).await;
    }

    let base = target_dir.join(file_name);
    match conflict_policy {
        "keep-both" => Ok(unique_destination_async(target_dir, file_name).await),
        "overwrite" => {
            if base == source {
                return Err("cannot overwrite an item with itself".to_string());
            }
            Ok(base)
        }
        _ => Err(format!("unknown conflict policy: {conflict_policy}")),
    }
}

async fn renamed_destination_async(target_dir: &Path, name: &str) -> Result<PathBuf, String> {
    if !metadata_async(target_dir)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err("target is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = target_dir.join(name);
    if path_exists_async(&destination).await {
        return Err("an item with that name already exists".to_string());
    }
    Ok(destination)
}

async fn unique_destination_async(target_dir: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    let initial = target_dir.join(file_name);
    if !path_exists_async(&initial).await {
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
        if !path_exists_async(&candidate).await {
            return candidate;
        }
    }

    unreachable!("unbounded destination search should always return")
}

async fn backup_existing_destination_async(destination: &Path) -> Result<PathBuf, String> {
    let backup = overwrite_backup_path_async(destination).await?;
    compio::fs::rename(destination, &backup)
        .await
        .map_err(|err| err.to_string())?;
    Ok(backup)
}

async fn restore_overwrite_backup_async(destination: &Path, backup: &Path) -> Result<(), String> {
    if path_exists_async(destination).await {
        remove_path_async(destination)
            .await
            .map_err(|err| err.to_string())?;
    }
    compio::fs::rename(backup, destination)
        .await
        .map_err(|err| err.to_string())
}

async fn overwrite_backup_path_async(destination: &Path) -> Result<PathBuf, String> {
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
        if !path_exists_async(&candidate).await {
            return Ok(candidate);
        }
    }
    unreachable!("unbounded overwrite backup search should always return")
}

