impl PrivilegedService {
    pub(super) async fn dispatch(
        &self,
        connection: &mut Connection,
        call: Message,
    ) -> tensor_dbus::Result<()> {
        let Some(call) = MethodCall::new(call) else {
            return Ok(());
        };
        if let Err(error) = call.require_path(OBJECT_PATH, "unknown privileged helper object path")
        {
            return reply_method_error(connection, &call, error).await;
        }
        if let Err(error) =
            call.require_interface(SERVICE_INTERFACE, "unknown privileged helper interface")
        {
            return reply_method_error(connection, &call, error).await;
        }

        match call.member() {
            "CheckIdle" => reply_method(connection, &call, &()).await,
            "CreateFolder" => {
                let result = async {
                    let (parent, name): (String, String) = call.body()?;
                    self.authorize(connection, call.message()).await?;
                    ensure_privileged_local_path(Path::new(&parent))
                        .map_err(MethodError::failed)?;
                    file_ops::create_folder(Path::new(&parent), &name)
                        .map(|path| path.display().to_string())
                        .map_err(MethodError::failed)
                }
                .await;
                reply_method_result(connection, &call, result).await
            }
            "CreateFile" => {
                let result = async {
                    let (parent, name): (String, String) = call.body()?;
                    self.authorize(connection, call.message()).await?;
                    ensure_privileged_local_path(Path::new(&parent))
                        .map_err(MethodError::failed)?;
                    file_ops::create_file(Path::new(&parent), &name)
                        .map(|path| path.display().to_string())
                        .map_err(MethodError::failed)
                }
                .await;
                reply_method_result(connection, &call, result).await
            }
            "Rename" => {
                let result = async {
                    let (path, new_name): (String, String) = call.body()?;
                    self.authorize(connection, call.message()).await?;
                    ensure_privileged_local_path(Path::new(&path)).map_err(MethodError::failed)?;
                    file_ops::rename_path(Path::new(&path), &new_name)
                        .map(|path| path.display().to_string())
                        .map_err(MethodError::failed)
                }
                .await;
                reply_method_result(connection, &call, result).await
            }
            "Trash" => {
                let result = async {
                    let (paths,): (Vec<String>,) = call.body()?;
                    self.authorize(connection, call.message()).await?;
                    let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
                    for path in &paths {
                        ensure_privileged_local_path(path).map_err(MethodError::failed)?;
                    }
                    file_ops::trash_paths(&paths)
                        .to_result_message("moved to trash")
                        .map_err(MethodError::failed)
                }
                .await;
                reply_method_result(connection, &call, result).await
            }
            "Transfer" => {
                let result = async {
                    let (operation, source, target_dir): (String, String, String) = call.body()?;
                    self.authorize(connection, call.message()).await?;
                    ensure_privileged_local_path(Path::new(&source))
                        .map_err(MethodError::failed)?;
                    ensure_privileged_local_path(Path::new(&target_dir))
                        .map_err(MethodError::failed)?;
                    file_ops::perform_transfer_with_progress(
                        &operation,
                        Path::new(&source),
                        Path::new(&target_dir),
                        "keep-both",
                        None,
                        |_| {},
                    )
                    .map(|path| path.display().to_string())
                    .map_err(MethodError::failed)
                }
                .await;
                reply_method_result(connection, &call, result).await
            }
            "PrepareExternalEdit" => {
                let result = async {
                    let (path,): (String,) = call.body()?;
                    let authorized_uid = self.authorize(connection, call.message()).await?;
                    ensure_privileged_local_path(Path::new(&path)).map_err(MethodError::failed)?;
                    self.prepare_external_edit_inner(PathBuf::from(path), authorized_uid)
                        .map_err(MethodError::failed)
                }
                .await;
                reply_method_result(connection, &call, result).await
            }
            "CommitExternalEdit" => {
                let result = async {
                    let (token, scratch_path): (String, String) = call.body()?;
                    self.authorize(connection, call.message()).await?;
                    self.commit_external_edit_inner(&token, PathBuf::from(scratch_path))
                        .map_err(MethodError::failed)
                }
                .await;
                reply_method_result(connection, &call, result).await
            }
            "DiscardExternalEdit" => {
                let result = async {
                    let (token,): (String,) = call.body()?;
                    self.authorize(connection, call.message()).await?;
                    self.discard_external_edit_inner(&token)
                        .map_err(MethodError::failed)
                }
                .await;
                reply_method_result(connection, &call, result).await
            }
            "AssociateExternalEditUnit" => {
                let result = async {
                    let (token, unit, session_bus_address): (String, String, String) =
                        call.body()?;
                    self.authorize(connection, call.message()).await?;
                    let address = (!session_bus_address.is_empty()).then_some(session_bus_address);
                    self.associate_external_edit_unit_inner(&token, unit, address)
                        .map_err(MethodError::failed)
                }
                .await;
                reply_method_result(connection, &call, result).await
            }
            _ => {
                reply_method_error(
                    connection,
                    &call,
                    MethodError::unknown_method("unknown privileged helper method"),
                )
                .await
            }
        }
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
    let runtime = match compio::runtime::RuntimeBuilder::new().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!(
                "[tensor-files privileged helper] cannot create Compio unit-watch runtime: {err}"
            );
            return;
        }
    };
    runtime.block_on(wait_for_user_unit_to_finish_async(
        &unit,
        session_bus_address.as_deref(),
    ));
}

async fn wait_for_user_unit_to_finish_async(unit: &str, session_bus_address: Option<&str>) {
    if let Err(err) = wait_for_user_unit_to_finish_by_signal(unit, session_bus_address).await {
        eprintln!(
            "[tensor-files privileged helper] cannot subscribe to unit {unit} lifecycle, falling back to polling: {err}"
        );
        wait_for_user_unit_to_finish_by_poll(unit, session_bus_address).await;
    }
}

async fn wait_for_user_unit_to_finish_by_signal(
    unit: &str,
    session_bus_address: Option<&str>,
) -> Result<(), String> {
    let mut connection = user_bus_connection(session_bus_address).await?;
    let mut manager = Proxy::new(
        &mut connection,
        Some("org.freedesktop.systemd1"),
        "/org/freedesktop/systemd1",
        Some("org.freedesktop.systemd1.Manager"),
    )
    .map_err(|err| format!("cannot create systemd manager proxy: {err}"))?;
    let _: () = manager
        .call("Subscribe", &())
        .await
        .map_err(|err| format!("Subscribe failed: {err}"))?;
    let unit_path: OwnedObjectPath = match manager.call("GetUnit", &(unit,)).await {
        Ok(path) => path,
        Err(err) if err.to_string().contains("NoSuchUnit") => return Ok(()),
        Err(err) => return Err(format!("GetUnit failed: {err}")),
    };
    drop(manager);
    let mut unit_proxy = Proxy::new(
        &mut connection,
        Some("org.freedesktop.systemd1"),
        unit_path.as_str(),
        Some("org.freedesktop.DBus.Properties"),
    )
    .map_err(|err| format!("cannot create systemd unit proxy: {err}"))?;
    let properties_changed = unit_proxy
        .subscribe("PropertiesChanged")
        .await
        .map_err(|err| format!("cannot subscribe to ActiveState changes: {err}"))?;
    let active_state: OwnedValue = unit_proxy
        .call("Get", &("org.freedesktop.systemd1.Unit", "ActiveState"))
        .await
        .map_err(|err| format!("Get ActiveState failed: {err}"))?;
    let active_state = String::try_from(active_state)
        .map_err(|err| format!("ActiveState was not a string: {err}"))?;
    if is_finished_unit_state(&active_state) {
        let _ = unit_proxy.signal_stream(properties_changed).close().await;
        return Ok(());
    }

    let mut changes = unit_proxy.signal_stream(properties_changed);
    loop {
        let message = changes
            .next()
            .await
            .map_err(|err| format!("ActiveState signal read failed: {err}"))?;
        let (interface, changed, _invalidated): (String, HashMap<String, OwnedValue>, Vec<String>) =
            message
                .body()
                .map_err(|err| format!("ActiveState signal body was invalid: {err}"))?;
        if interface != "org.freedesktop.systemd1.Unit" {
            continue;
        }
        let Some(value) = changed.get("ActiveState") else {
            continue;
        };
        let state = String::try_from(
            value
                .try_clone()
                .map_err(|err| format!("cannot clone ActiveState value: {err}"))?,
        )
        .map_err(|err| format!("ActiveState was not a string: {err}"))?;
        if is_finished_unit_state(&state) {
            let _ = changes.close().await;
            return Ok(());
        }
    }
}

async fn wait_for_user_unit_to_finish_by_poll(unit: &str, session_bus_address: Option<&str>) {
    let deadline = SystemTime::now()
        .checked_add(Duration::from_secs(24 * 60 * 60))
        .unwrap_or_else(SystemTime::now);
    loop {
        if SystemTime::now() >= deadline {
            eprintln!(
                "[tensor-files privileged helper] external edit unit watch timed out: {unit}"
            );
            return;
        }

        match user_unit_active_state(unit, session_bus_address).await {
            Ok(Some(state)) if is_finished_unit_state(&state) => return,
            Ok(None) => return,
            Ok(Some(_)) => {}
            Err(err) => {
                eprintln!("[tensor-files privileged helper] cannot query unit {unit}: {err}")
            }
        }
        compio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn user_unit_active_state(
    unit: &str,
    session_bus_address: Option<&str>,
) -> Result<Option<String>, String> {
    let mut connection = user_bus_connection(session_bus_address).await?;
    let mut manager = Proxy::new(
        &mut connection,
        Some("org.freedesktop.systemd1"),
        "/org/freedesktop/systemd1",
        Some("org.freedesktop.systemd1.Manager"),
    )
    .map_err(|err| format!("cannot create systemd manager proxy: {err}"))?;
    let unit_path: OwnedObjectPath = match manager.call("GetUnit", &(unit,)).await {
        Ok(path) => path,
        Err(err) if err.to_string().contains("NoSuchUnit") => return Ok(None),
        Err(err) => return Err(format!("GetUnit failed: {err}")),
    };
    drop(manager);
    let mut properties = Proxy::new(
        &mut connection,
        Some("org.freedesktop.systemd1"),
        unit_path.as_str(),
        Some("org.freedesktop.DBus.Properties"),
    )
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
        Some(address) => Connection::connect_bus(
            BusAddress::parse(address)
                .map_err(|err| format!("cannot use provided session bus address: {err}"))?,
        )
        .await
        .map_err(|err| format!("cannot connect to provided session bus: {err}")),
        None => Connection::session_bus()
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
    if root_dir.file_name() != Some(std::ffi::OsStr::new("tensor-files-edit")) {
        return Err("scratch path is outside tensor-files-edit".to_string());
    }
    fs::remove_dir_all(token_dir).map_err(|err| err.to_string())
}

#[cfg(test)]
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
         Install Tensor Files's D-Bus service and {POLICY_FILE}, then ensure a desktop polkit agent is running."
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
