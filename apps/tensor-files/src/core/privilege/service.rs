use super::*;

impl PrivilegedService {
    pub(super) fn new(mode: ServiceMode, session_bus_address: Option<String>) -> Self {
        Self {
            mode,
            external_edits: Arc::new(Mutex::new(std::collections::HashMap::new())),
            external_edit_watchers: Arc::new(Mutex::new(std::collections::HashMap::new())),
            last_activity_secs: Arc::new(AtomicU64::new(now_secs())),
            scratch_root_override: None,
            session_bus_address,
        }
    }

    #[cfg(test)]
    pub(super) fn new_for_tests(scratch_root: PathBuf) -> Self {
        Self {
            mode: ServiceMode::SessionPkexec { allowed_uid: 0 },
            external_edits: Arc::new(Mutex::new(std::collections::HashMap::new())),
            external_edit_watchers: Arc::new(Mutex::new(std::collections::HashMap::new())),
            last_activity_secs: Arc::new(AtomicU64::new(now_secs())),
            scratch_root_override: Some(scratch_root),
            session_bus_address: None,
        }
    }

    async fn authorize(&self, connection: &Connection, header: Header<'_>) -> fdo::Result<u32> {
        match &self.mode {
            ServiceMode::System => self.authorize_with_polkit(connection, header).await,
            ServiceMode::SessionPkexec { allowed_uid } => {
                let caller_uid = caller_uid(connection, &header).await?;
                if caller_uid == *allowed_uid {
                    self.mark_activity();
                    Ok(caller_uid)
                } else {
                    Err(fdo::Error::AccessDenied(format!(
                        "caller uid {caller_uid} does not match authorized uid {allowed_uid}"
                    )))
                }
            }
        }
    }

    async fn authorize_with_polkit(
        &self,
        connection: &Connection,
        header: Header<'_>,
    ) -> fdo::Result<u32> {
        let caller_uid = caller_uid(connection, &header).await?;
        let subject = Subject::new_for_message_header(&header).map_err(|err| {
            fdo::Error::AccessDenied(format!("cannot create polkit subject: {err}"))
        })?;
        let authority = AuthorityProxy::new(connection).await.map_err(|err| {
            fdo::Error::Failed(polkit_authority_unavailable_message(&err.to_string()))
        })?;
        let details = std::collections::HashMap::new();
        let result = authority
            .check_authorization(
                &subject,
                ACTION_ID,
                &details,
                CheckAuthorizationFlags::AllowUserInteraction.into(),
                "",
            )
            .await
            .map_err(|err| {
                fdo::Error::AccessDenied(polkit_check_failed_message(&err.to_string()))
            })?;
        if result.is_authorized {
            self.mark_activity();
            Ok(caller_uid)
        } else {
            Err(fdo::Error::AccessDenied(polkit_denied_message()))
        }
    }

    fn map_result<T>(result: Result<T, String>) -> fdo::Result<T> {
        result.map_err(fdo::Error::Failed)
    }

    fn mark_activity(&self) {
        self.last_activity_secs.store(now_secs(), Ordering::Relaxed);
    }

    pub(super) fn can_exit(&self) -> bool {
        let (idle_for, active_edits) = self.exit_state();
        idle_for >= HELPER_IDLE_SECONDS && active_edits == 0
    }

    pub(super) fn exit_state(&self) -> (u64, usize) {
        let idle_for = now_secs().saturating_sub(self.last_activity_secs.load(Ordering::Relaxed));
        let active_edits = self.external_edits.lock().map_or(0, |edits| edits.len());
        (idle_for, active_edits)
    }

    pub(super) fn prepare_external_edit_inner(
        &self,
        path: PathBuf,
        authorized_uid: u32,
    ) -> Result<(String, String), String> {
        if !path.is_file() {
            return Err("external edit target is not a regular file".to_string());
        }
        let metadata = fs::metadata(&path).map_err(|err| err.to_string())?;
        let token = new_token();
        let scratch_dir = self.scratch_root(authorized_uid)?.join(&token);
        fs::create_dir_all(&scratch_dir).map_err(|err| err.to_string())?;
        self.chown_for_authorized_user(&scratch_dir, authorized_uid)?;

        let file_name = path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("edit"));
        let scratch_path = scratch_dir.join(file_name);
        fs::copy(&path, &scratch_path).map_err(|err| err.to_string())?;
        self.set_private_user_file(&scratch_path, authorized_uid)?;

        let edit = ExternalEdit {
            original_path: path,
            scratch_path: scratch_path.clone(),
            original_len: metadata.len(),
            original_modified: metadata.modified().ok(),
            unit: None,
            session_bus_address: None,
            created_secs: now_secs(),
        };
        self.external_edits
            .lock()
            .map_err(|_| "external edit state is poisoned".to_string())?
            .insert(token.clone(), edit);
        if self.scratch_root_override.is_none() {
            self.watch_external_edit(token.clone(), scratch_path.clone())?;
        }

        Ok((scratch_path.display().to_string(), token))
    }

    pub(super) fn commit_external_edit_inner(
        &self,
        token: &str,
        scratch_path: PathBuf,
    ) -> Result<String, String> {
        let mut edit = self
            .external_edits
            .lock()
            .map_err(|_| "external edit state is poisoned".to_string())?
            .remove(token)
            .ok_or_else(|| "unknown external edit token".to_string())?;

        if edit.scratch_path != scratch_path {
            return Err("scratch path does not match edit token".to_string());
        }
        let original_path = sync_external_edit(&mut edit)?;
        let _ = self
            .external_edit_watchers
            .lock()
            .map_err(|_| "external edit watcher state is poisoned".to_string())?
            .remove(token);
        let _ = cleanup_scratch_token_dir(&scratch_path);
        Ok(original_path.display().to_string())
    }

    pub(super) fn discard_external_edit_inner(&self, token: &str) -> Result<(), String> {
        let edit = self
            .external_edits
            .lock()
            .map_err(|_| "external edit state is poisoned".to_string())?
            .remove(token)
            .ok_or_else(|| "unknown external edit token".to_string())?;
        let _ = self
            .external_edit_watchers
            .lock()
            .map_err(|_| "external edit watcher state is poisoned".to_string())?
            .remove(token);
        let _ = cleanup_scratch_token_dir(&edit.scratch_path);
        Ok(())
    }

    fn associate_external_edit_unit_inner(
        &self,
        token: &str,
        unit: String,
        session_bus_address: Option<String>,
    ) -> Result<(), String> {
        let session_bus_address = session_bus_address.or_else(|| self.session_bus_address.clone());
        {
            let mut edits = self
                .external_edits
                .lock()
                .map_err(|_| "external edit state is poisoned".to_string())?;
            let edit = edits
                .get_mut(token)
                .ok_or_else(|| "unknown external edit token".to_string())?;
            edit.unit = Some(unit.clone());
            edit.session_bus_address = session_bus_address.clone();
        }
        self.watch_external_edit_unit(token.to_string(), unit, session_bus_address);
        Ok(())
    }

    fn scratch_root(&self, authorized_uid: u32) -> Result<PathBuf, String> {
        if let Some(root) = &self.scratch_root_override {
            fs::create_dir_all(root).map_err(|err| err.to_string())?;
            return Ok(root.clone());
        }

        let root = PathBuf::from(format!("/run/user/{authorized_uid}"));
        let scratch_root = root.join("tensor-files-edit");
        fs::create_dir_all(&scratch_root).map_err(|err| err.to_string())?;
        self.chown_for_authorized_user(&scratch_root, authorized_uid)?;
        Ok(scratch_root)
    }

    fn chown_for_authorized_user(&self, path: &Path, authorized_uid: u32) -> Result<(), String> {
        if self.scratch_root_override.is_some() {
            return Ok(());
        }
        set_owner_for_authorized_user(path, authorized_uid)
    }

    fn set_private_user_file(&self, path: &Path, authorized_uid: u32) -> Result<(), String> {
        if self.scratch_root_override.is_some() {
            return set_private_user_file(path);
        }
        set_owner_for_authorized_user(path, authorized_uid)?;
        set_private_user_file(path)
    }

    fn watch_external_edit(&self, token: String, scratch_path: PathBuf) -> Result<(), String> {
        use notify::Watcher;

        let edits = Arc::clone(&self.external_edits);
        let token_for_callback = token.clone();
        let scratch_for_callback = scratch_path.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else {
                    return;
                };
                if !event.paths.iter().any(|path| path == &scratch_for_callback) {
                    return;
                }
                if !is_writeback_event(&event.kind) {
                    return;
                }

                let edits = Arc::clone(&edits);
                let token = token_for_callback.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(350));
                    let Ok(mut edits) = edits.lock() else {
                        return;
                    };
                    if let Some(edit) = edits.get_mut(&token)
                        && let Err(err) = sync_external_edit(edit)
                    {
                        eprintln!(
                            "[tensor-files privileged helper] external edit writeback failed: {err}"
                        );
                    }
                });
            })
            .map_err(|err| err.to_string())?;
        watcher
            .watch(&scratch_path, notify::RecursiveMode::NonRecursive)
            .map_err(|err| err.to_string())?;
        self.external_edit_watchers
            .lock()
            .map_err(|_| "external edit watcher state is poisoned".to_string())?
            .insert(token, watcher);
        Ok(())
    }

    fn watch_external_edit_unit(
        &self,
        token: String,
        unit: String,
        session_bus_address: Option<String>,
    ) {
        let edits = Arc::clone(&self.external_edits);
        let watchers = Arc::clone(&self.external_edit_watchers);
        std::thread::spawn(move || {
            wait_for_user_unit_to_finish(&unit, session_bus_address.as_deref());

            let Ok(mut edits) = edits.lock() else {
                return;
            };
            let Some(mut edit) = edits.remove(&token) else {
                return;
            };
            if let Err(err) = sync_external_edit(&mut edit) {
                eprintln!(
                    "[tensor-files privileged helper] final external edit writeback failed: {err}"
                );
            }
            let _ = cleanup_scratch_token_dir(&edit.scratch_path);
            drop(edits);

            if let Ok(mut watchers) = watchers.lock() {
                let _ = watchers.remove(&token);
            }
        });
    }

    pub(super) fn expire_stale_external_edits(&self) {
        let cutoff = now_secs().saturating_sub(EXTERNAL_EDIT_TTL_SECONDS);
        let expired = {
            let Ok(edits) = self.external_edits.lock() else {
                return;
            };
            edits
                .iter()
                .filter(|(_, edit)| edit.created_secs <= cutoff)
                .map(|(token, _)| token.clone())
                .collect::<Vec<_>>()
        };

        for token in expired {
            let edit = {
                let Ok(mut edits) = self.external_edits.lock() else {
                    return;
                };
                edits.remove(&token)
            };
            let Some(mut edit) = edit else {
                continue;
            };
            if let Err(err) = sync_external_edit(&mut edit) {
                eprintln!(
                    "[tensor-files privileged helper] expired edit final writeback failed: {err}"
                );
            }
            let _ = cleanup_scratch_token_dir(&edit.scratch_path);
            if let Ok(mut watchers) = self.external_edit_watchers.lock() {
                let _ = watchers.remove(&token);
            }
        }
    }
}

async fn caller_uid(connection: &Connection, header: &Header<'_>) -> fdo::Result<u32> {
    let sender = header
        .sender()
        .ok_or_else(|| fdo::Error::AccessDenied("missing D-Bus sender".to_string()))?;
    let proxy = DBusProxy::new(connection)
        .await
        .map_err(|err| fdo::Error::Failed(format!("cannot query D-Bus credentials: {err}")))?;
    proxy
        .get_connection_unix_user(BusName::from(sender.clone()))
        .await
        .map_err(|err| fdo::Error::Failed(format!("cannot query caller uid: {err}")))
}

include!("service_methods.rs");
