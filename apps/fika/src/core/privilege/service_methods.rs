#[zbus::interface(name = "org.fika.FileManager1.Privileged")]
impl PrivilegedService {
    #[zbus(name = "CreateFolder")]
    async fn create_folder(
        &self,
        parent: String,
        name: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        ensure_privileged_local_path(Path::new(&parent)).map_err(fdo::Error::Failed)?;
        Self::map_result(
            file_ops::create_folder(Path::new(&parent), &name)
                .map(|path| path.display().to_string()),
        )
    }

    #[zbus(name = "CreateFile")]
    async fn create_file(
        &self,
        parent: String,
        name: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        ensure_privileged_local_path(Path::new(&parent)).map_err(fdo::Error::Failed)?;
        Self::map_result(
            file_ops::create_file(Path::new(&parent), &name).map(|path| path.display().to_string()),
        )
    }

    #[zbus(name = "Rename")]
    async fn rename(
        &self,
        path: String,
        new_name: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        ensure_privileged_local_path(Path::new(&path)).map_err(fdo::Error::Failed)?;
        Self::map_result(
            file_ops::rename_path(Path::new(&path), &new_name)
                .map(|path| path.display().to_string()),
        )
    }

    #[zbus(name = "Trash")]
    async fn trash(
        &self,
        paths: Vec<String>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
        for path in &paths {
            ensure_privileged_local_path(path).map_err(fdo::Error::Failed)?;
        }
        Self::map_result(file_ops::trash_paths(&paths).to_result_message("moved to trash"))
    }

    #[zbus(name = "Transfer")]
    async fn transfer(
        &self,
        operation: String,
        source: String,
        target_dir: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        ensure_privileged_local_path(Path::new(&source)).map_err(fdo::Error::Failed)?;
        ensure_privileged_local_path(Path::new(&target_dir)).map_err(fdo::Error::Failed)?;
        Self::map_result(
            file_ops::perform_transfer_with_progress(
                &operation,
                Path::new(&source),
                Path::new(&target_dir),
                "keep-both",
                None,
                |_| {},
            )
            .map(|path| path.display().to_string()),
        )
    }

    #[zbus(name = "PrepareExternalEdit")]
    async fn prepare_external_edit(
        &self,
        path: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<(String, String)> {
        let authorized_uid = self.authorize(connection, header).await?;
        ensure_privileged_local_path(Path::new(&path)).map_err(fdo::Error::Failed)?;
        Self::map_result(self.prepare_external_edit_inner(PathBuf::from(path), authorized_uid))
    }

    #[zbus(name = "CommitExternalEdit")]
    async fn commit_external_edit(
        &self,
        token: String,
        scratch_path: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        Self::map_result(self.commit_external_edit_inner(&token, PathBuf::from(scratch_path)))
    }

    #[zbus(name = "DiscardExternalEdit")]
    async fn discard_external_edit(
        &self,
        token: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let _authorized_uid = self.authorize(connection, header).await?;
        Self::map_result(self.discard_external_edit_inner(&token))
    }

    #[zbus(name = "AssociateExternalEditUnit")]
    async fn associate_external_edit_unit(
        &self,
        token: String,
        unit: String,
        session_bus_address: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let _authorized_uid = self.authorize(connection, header).await?;
        let session_bus_address = if session_bus_address.is_empty() {
            None
        } else {
            Some(session_bus_address)
        };
        Self::map_result(self.associate_external_edit_unit_inner(&token, unit, session_bus_address))
    }
}

fn new_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{:x}-{counter:x}", std::process::id())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn set_owner_for_authorized_user(path: &Path, uid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        let gid = fs::metadata(format!("/run/user/{uid}"))
            .map(|metadata| metadata.gid())
            .unwrap_or(uid);
        std::os::unix::fs::chown(path, Some(uid), Some(gid)).map_err(|err| err.to_string())?;
    }

    let _ = path;
    let _ = uid;
    Ok(())
}

fn set_private_user_file(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)
            .map_err(|err| err.to_string())?
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn is_writeback_event(kind: &notify::EventKind) -> bool {
    matches!(
        kind,
        notify::EventKind::Modify(_) | notify::EventKind::Create(_) | notify::EventKind::Any
    )
}

pub(super) fn sync_external_edit(edit: &mut ExternalEdit) -> Result<PathBuf, String> {
    if !edit.scratch_path.is_file() {
        return Err("scratch file no longer exists".to_string());
    }

    let current = fs::metadata(&edit.original_path).map_err(|err| err.to_string())?;
    if current.len() != edit.original_len || current.modified().ok() != edit.original_modified {
        return Err("original file changed outside this edit session".to_string());
    }

    let data = fs::read(&edit.scratch_path).map_err(|err| err.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&edit.original_path)
        .map_err(|err| err.to_string())?;
    file.write_all(&data).map_err(|err| err.to_string())?;
    file.sync_all().map_err(|err| err.to_string())?;

    let metadata = fs::metadata(&edit.original_path).map_err(|err| err.to_string())?;
    edit.original_len = metadata.len();
    edit.original_modified = metadata.modified().ok();
    Ok(edit.original_path.clone())
}

fn wait_for_user_unit_to_finish(unit: &str, session_bus_address: Option<&str>) {
    let unit = unit.to_string();
    let session_bus_address = session_bus_address.map(str::to_string);
    async_io::block_on(wait_for_user_unit_to_finish_async(
        &unit,
        session_bus_address.as_deref(),
    ));
}

async fn wait_for_user_unit_to_finish_async(unit: &str, session_bus_address: Option<&str>) {
    if let Err(err) = wait_for_user_unit_to_finish_by_signal(unit, session_bus_address).await {
        eprintln!(
            "[fika privileged helper] cannot subscribe to unit {unit} lifecycle, falling back to polling: {err}"
        );
        wait_for_user_unit_to_finish_by_poll(unit, session_bus_address).await;
    }
}

async fn wait_for_user_unit_to_finish_by_signal(
    unit: &str,
    session_bus_address: Option<&str>,
) -> Result<(), String> {
    let connection = user_bus_connection(session_bus_address).await?;
    let manager = zbus::Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(|err| format!("cannot create systemd manager proxy: {err}"))?;
    let _: () = manager
        .call("Subscribe", &())
        .await
        .map_err(|err| format!("Subscribe failed: {err}"))?;
    let unit_path: OwnedObjectPath = match manager.call("GetUnit", &(unit)).await {
        Ok(path) => path,
        Err(err) if err.to_string().contains("NoSuchUnit") => return Ok(()),
        Err(err) => return Err(format!("GetUnit failed: {err}")),
    };
    let unit_proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        unit_path.as_str(),
        "org.freedesktop.systemd1.Unit",
    )
    .await
    .map_err(|err| format!("cannot create systemd unit proxy: {err}"))?;

    let deadline = unit_watch_deadline();
    let mut active_state_changes = unit_proxy
        .receive_property_changed::<String>("ActiveState")
        .await;
    loop {
        if SystemTime::now() >= deadline {
            eprintln!("[fika privileged helper] external edit unit watch timed out: {unit}");
            return Ok(());
        }

        let Some(change) = active_state_changes.next().await else {
            return Ok(());
        };
        match change.get().await {
            Ok(state) if is_finished_unit_state(&state) => return Ok(()),
            Ok(_) => {}
            Err(err) => return Err(format!("ActiveState signal read failed: {err}")),
        }
    }
}

async fn wait_for_user_unit_to_finish_by_poll(unit: &str, session_bus_address: Option<&str>) {
    let deadline = SystemTime::now()
        .checked_add(Duration::from_secs(24 * 60 * 60))
        .unwrap_or_else(SystemTime::now);
    loop {
        if SystemTime::now() >= deadline {
            eprintln!("[fika privileged helper] external edit unit watch timed out: {unit}");
            return;
        }

        match user_unit_active_state(unit, session_bus_address).await {
            Ok(Some(state)) if is_finished_unit_state(&state) => return,
            Ok(None) => return,
            Ok(Some(_)) => {}
            Err(err) => eprintln!("[fika privileged helper] cannot query unit {unit}: {err}"),
        }
        async_io::Timer::after(Duration::from_secs(2)).await;
    }
}

fn unit_watch_deadline() -> SystemTime {
    SystemTime::now()
        .checked_add(Duration::from_secs(24 * 60 * 60))
        .unwrap_or_else(SystemTime::now)
}

async fn user_unit_active_state(
    unit: &str,
    session_bus_address: Option<&str>,
) -> Result<Option<String>, String> {
    let connection = user_bus_connection(session_bus_address).await?;
    let manager = zbus::Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(|err| format!("cannot create systemd manager proxy: {err}"))?;
    let unit_path: OwnedObjectPath = match manager.call("GetUnit", &(unit)).await {
        Ok(path) => path,
        Err(err) if err.to_string().contains("NoSuchUnit") => return Ok(None),
        Err(err) => return Err(format!("GetUnit failed: {err}")),
    };
    let properties = zbus::Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        unit_path.as_str(),
        "org.freedesktop.DBus.Properties",
    )
    .await
    .map_err(|err| format!("cannot create systemd unit properties proxy: {err}"))?;
    let value: OwnedValue = properties
        .call("Get", &("org.freedesktop.systemd1.Unit", "ActiveState"))
        .await
        .map_err(|err| format!("Get ActiveState failed: {err}"))?;
    String::try_from(value)
        .map(Some)
        .map_err(|err| format!("ActiveState was not a string: {err}"))
}

async fn user_bus_connection(session_bus_address: Option<&str>) -> Result<Connection, String> {
    match session_bus_address {
        Some(address) => zbus::connection::Builder::address(address)
            .map_err(|err| format!("cannot use provided session bus address: {err}"))?
            .build()
            .await
            .map_err(|err| format!("cannot connect to provided session bus: {err}")),
        None => Connection::session()
            .await
            .map_err(|err| format!("cannot connect to session bus: {err}")),
    }
}

pub(super) fn is_finished_unit_state(state: &str) -> bool {
    matches!(state, "inactive" | "failed")
}

pub(super) fn cleanup_scratch_token_dir(scratch_path: &Path) -> Result<(), String> {
    let Some(token_dir) = scratch_path.parent() else {
        return Err("scratch path has no token directory".to_string());
    };
    let Some(root_dir) = token_dir.parent() else {
        return Err("scratch token directory has no root".to_string());
    };
    if root_dir.file_name() != Some(std::ffi::OsStr::new("fika-edit")) {
        return Err("scratch path is outside fika-edit".to_string());
    }
    fs::remove_dir_all(token_dir).map_err(|err| err.to_string())
}

pub(super) fn polkit_authority_unavailable_message(err: &str) -> String {
    format!(
        "cannot contact polkit authority for action {ACTION_ID}: {err}; ensure polkit is running, {POLICY_FILE} is installed, and a desktop polkit agent is available"
    )
}

pub(super) fn polkit_check_failed_message(err: &str) -> String {
    format!(
        "polkit authorization failed for action {ACTION_ID}: {err}; ensure {POLICY_FILE} is installed in the polkit actions directory"
    )
}

pub(super) fn polkit_denied_message() -> String {
    format!("polkit denied authorization for action {ACTION_ID}")
}

pub(super) fn privileged_helper_start_failed_message(
    system_error: &str,
    session_error: &str,
    fallback_error: &str,
) -> String {
    format!(
        "cannot reach privileged helper. \
         System bus activation failed: {system_error}. \
         Development session-bus helper failed: {session_error}. \
         pkexec fallback failed: {fallback_error}. \
         Install Fika's D-Bus service and {POLICY_FILE}, then ensure a desktop polkit agent is running."
    )
}

pub(super) fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

trait SummaryMessage {
    fn to_result_message(self, success_label: &str) -> Result<String, String>;
}

impl SummaryMessage for file_ops::FileActionSummary {
    fn to_result_message(self, success_label: &str) -> Result<String, String> {
        match (self.successes.len(), self.failures.is_empty()) {
            (0, false) => Err(self.failures.join("; ")),
            (count, true) => Ok(format!("{count} item(s) {success_label}")),
            (count, false) => Ok(format!(
                "{count} item(s) {success_label}; {} failure(s): {}",
                self.failures.len(),
                self.failures.join("; ")
            )),
        }
    }
}
