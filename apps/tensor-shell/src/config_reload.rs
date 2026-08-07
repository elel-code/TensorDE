use std::{
    io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use futures_util::{FutureExt, pin_mut, select_biased};
use tensor_runtime::io_uring_runtime;
use wayland_client_runtime::WakeHandle;

use crate::{MAX_SHELL_CONFIG_BYTES, ShellConfig, ShellConfigError};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const CONFIG_IO_OPERATIONS: usize = 4;

#[derive(Debug)]
pub(crate) struct ShellConfigReloadHandle {
    store: Arc<ConfigStore>,
    stop: async_channel::Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl ShellConfigReloadHandle {
    pub(crate) fn start(
        path: PathBuf,
        wake: WakeHandle,
    ) -> Result<(Self, ShellConfig), ShellConfigReloadError> {
        let store = Arc::new(ConfigStore::new());
        let (stop, stop_rx) = async_channel::bounded(1);
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let worker_store = Arc::clone(&store);
        let join = thread::Builder::new()
            .name("tensor-shell-config".into())
            .spawn(move || run_thread(worker_store, path, stop_rx, wake, startup_tx))
            .map_err(ShellConfigReloadError::Thread)?;
        let initial = match startup_rx.recv() {
            Ok(Ok(config)) => config,
            Ok(Err(error)) => {
                let _ = join.join();
                return Err(error);
            }
            Err(_) => {
                let _ = join.join();
                return Err(ShellConfigReloadError::StartupStopped);
            }
        };
        Ok((
            Self {
                store,
                stop,
                join: Some(join),
            },
            initial,
        ))
    }

    pub(crate) fn read_if_changed(
        &self,
        observed_revision: u64,
    ) -> Option<(u64, Arc<ShellConfig>)> {
        self.store.read_if_changed(observed_revision)
    }
}

impl Drop for ShellConfigReloadHandle {
    fn drop(&mut self) {
        self.stop.close();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellConfigReloadError {
    #[error(transparent)]
    Config(#[from] ShellConfigError),
    #[error("failed to create Tensor Shell configuration io_uring runtime: {0}")]
    Runtime(#[source] io::Error),
    #[error("failed to start Tensor Shell configuration worker: {0}")]
    Thread(#[source] io::Error),
    #[error("Tensor Shell configuration worker stopped during startup")]
    StartupStopped,
}

#[derive(Debug)]
struct ConfigStore {
    revision_hint: AtomicU64,
    snapshot: Mutex<ConfigSnapshot>,
}

#[derive(Debug)]
struct ConfigSnapshot {
    revision: u64,
    config: Arc<ShellConfig>,
}

impl ConfigStore {
    fn new() -> Self {
        Self {
            revision_hint: AtomicU64::new(0),
            snapshot: Mutex::new(ConfigSnapshot {
                revision: 0,
                config: Arc::new(ShellConfig::default()),
            }),
        }
    }

    fn initialize(&self, config: ShellConfig) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.config = Arc::new(config);
        }
    }

    fn publish(&self, config: ShellConfig) -> bool {
        let Ok(mut snapshot) = self.snapshot.lock() else {
            return false;
        };
        if snapshot.config.as_ref() == &config {
            return false;
        }
        snapshot.revision = snapshot.revision.wrapping_add(1);
        snapshot.config = Arc::new(config);
        self.revision_hint
            .store(snapshot.revision, Ordering::Release);
        true
    }

    fn read_if_changed(&self, observed_revision: u64) -> Option<(u64, Arc<ShellConfig>)> {
        if self.revision_hint.load(Ordering::Acquire) == observed_revision {
            return None;
        }
        let snapshot = self.snapshot.lock().ok()?;
        (snapshot.revision != observed_revision)
            .then(|| (snapshot.revision, Arc::clone(&snapshot.config)))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceFingerprint {
    Missing,
    File(FileFingerprint),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileFingerprint {
    device: u64,
    inode: u64,
    bytes: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl FileFingerprint {
    fn from_metadata(metadata: &compio::fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            bytes: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        }
    }
}

#[derive(Debug)]
struct ReloadState {
    fingerprint: SourceFingerprint,
    document: Option<Vec<u8>>,
    document_error: Option<String>,
    config: ShellConfig,
    reported_error: Option<String>,
}

impl ReloadState {
    async fn load(path: &Path) -> Result<Self, ShellConfigError> {
        let fingerprint = probe(path).await?;
        let document = read_source(path, fingerprint).await?;
        let config = parse_source(path, document.as_deref())?;
        Ok(Self {
            fingerprint,
            document,
            document_error: None,
            config,
            reported_error: None,
        })
    }

    fn observe(
        &mut self,
        path: &Path,
        fingerprint: SourceFingerprint,
        document: Option<Vec<u8>>,
    ) -> Option<ShellConfig> {
        self.fingerprint = fingerprint;
        if self.document == document {
            if let Some(error) = self.document_error.clone() {
                self.report(error);
            } else {
                self.reported_error = None;
            }
            return None;
        }
        self.document = document;
        match parse_source(path, self.document.as_deref()) {
            Ok(config) => {
                self.document_error = None;
                self.reported_error = None;
                if self.config == config {
                    None
                } else {
                    self.config = config.clone();
                    Some(config)
                }
            }
            Err(error) => {
                let message = error.to_string();
                self.document_error = Some(message.clone());
                self.report(message);
                None
            }
        }
    }

    fn report(&mut self, message: String) {
        if self.reported_error.as_deref() == Some(&message) {
            return;
        }
        eprintln!("Tensor Shell kept its last valid configuration: {message}");
        self.reported_error = Some(message);
    }
}

fn run_thread(
    store: Arc<ConfigStore>,
    path: PathBuf,
    stop: async_channel::Receiver<()>,
    wake: WakeHandle,
    startup: mpsc::SyncSender<Result<ShellConfig, ShellConfigReloadError>>,
) {
    let runtime = match io_uring_runtime(CONFIG_IO_OPERATIONS) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = startup.send(Err(ShellConfigReloadError::Runtime(error)));
            return;
        }
    };
    runtime.block_on(async move {
        let mut state = match ReloadState::load(&path).await {
            Ok(state) => state,
            Err(error) => {
                let _ = startup.send(Err(error.into()));
                return;
            }
        };
        store.initialize(state.config.clone());
        if startup.send(Ok(state.config.clone())).is_err() {
            return;
        }
        loop {
            let stop_wait = stop.recv().fuse();
            let timer_wait = compio::runtime::time::sleep(POLL_INTERVAL).fuse();
            pin_mut!(stop_wait, timer_wait);
            select_biased! {
                _ = stop_wait => return,
                _ = timer_wait => {}
            }
            let fingerprint = match probe(&path).await {
                Ok(fingerprint) => fingerprint,
                Err(error) => {
                    state.report(error.to_string());
                    continue;
                }
            };
            if fingerprint == state.fingerprint {
                continue;
            }
            let document = match read_source(&path, fingerprint).await {
                Ok(document) => document,
                Err(error) => {
                    if matches!(error, ShellConfigError::TooLarge { .. }) {
                        state.fingerprint = fingerprint;
                    }
                    state.report(error.to_string());
                    continue;
                }
            };
            if let Some(config) = state.observe(&path, fingerprint, document)
                && store.publish(config)
            {
                wake.wake();
            }
        }
    });
}

async fn probe(path: &Path) -> Result<SourceFingerprint, ShellConfigError> {
    match compio::fs::metadata(path).await {
        Ok(metadata) => Ok(SourceFingerprint::File(FileFingerprint::from_metadata(
            &metadata,
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(SourceFingerprint::Missing),
        Err(source) => Err(ShellConfigError::Read {
            path: path.to_owned(),
            source,
        }),
    }
}

async fn read_source(
    path: &Path,
    fingerprint: SourceFingerprint,
) -> Result<Option<Vec<u8>>, ShellConfigError> {
    let SourceFingerprint::File(fingerprint) = fingerprint else {
        return Ok(None);
    };
    if fingerprint.bytes > MAX_SHELL_CONFIG_BYTES {
        return Err(ShellConfigError::TooLarge {
            path: path.to_owned(),
            bytes: fingerprint.bytes,
            maximum: MAX_SHELL_CONFIG_BYTES,
        });
    }
    let document = compio::fs::read(path)
        .await
        .map_err(|source| ShellConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
    let bytes = u64::try_from(document.len()).unwrap_or(u64::MAX);
    if bytes > MAX_SHELL_CONFIG_BYTES {
        return Err(ShellConfigError::TooLarge {
            path: path.to_owned(),
            bytes,
            maximum: MAX_SHELL_CONFIG_BYTES,
        });
    }
    Ok(Some(document))
}

fn parse_source(path: &Path, document: Option<&[u8]>) -> Result<ShellConfig, ShellConfigError> {
    document.map_or_else(
        || Ok(ShellConfig::default()),
        |bytes| ShellConfig::from_bytes(path, bytes),
    )
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    fn fingerprint(sequence: u64, bytes: usize) -> SourceFingerprint {
        SourceFingerprint::File(FileFingerprint {
            device: 1,
            inode: sequence,
            bytes: bytes as u64,
            modified_seconds: sequence as i64,
            modified_nanoseconds: 0,
            changed_seconds: sequence as i64,
            changed_nanoseconds: 0,
        })
    }

    fn initial_state() -> ReloadState {
        ReloadState {
            fingerprint: SourceFingerprint::Missing,
            document: None,
            document_error: None,
            config: ShellConfig::default(),
            reported_error: None,
        }
    }

    #[test]
    fn valid_update_publishes_once_after_byte_comparison() {
        let mut state = initial_state();
        let store = ConfigStore::new();
        let document = b"layout { panel-height 52 }".to_vec();
        let config = state
            .observe(
                Path::new("shell.kdl"),
                fingerprint(1, document.len()),
                Some(document.clone()),
            )
            .unwrap();
        assert!(store.publish(config));
        assert_eq!(store.read_if_changed(0).unwrap().0, 1);
        assert!(
            state
                .observe(
                    Path::new("shell.kdl"),
                    fingerprint(2, document.len()),
                    Some(document)
                )
                .is_none()
        );
        assert!(store.read_if_changed(1).is_none());
    }

    #[test]
    fn invalid_update_retains_last_valid_snapshot_and_recovers() {
        let mut state = initial_state();
        let valid = b"layout { panel-height 52 }".to_vec();
        let accepted = state
            .observe(
                Path::new("shell.kdl"),
                fingerprint(1, valid.len()),
                Some(valid),
            )
            .unwrap();
        assert_eq!(accepted.layout.panel_height, 52);
        let invalid = b"layout { panel-height 0 }".to_vec();
        assert!(
            state
                .observe(
                    Path::new("shell.kdl"),
                    fingerprint(2, invalid.len()),
                    Some(invalid)
                )
                .is_none()
        );
        assert_eq!(state.config.layout.panel_height, 52);
        let recovered = b"layout { panel-height 64 }".to_vec();
        let accepted = state
            .observe(
                Path::new("shell.kdl"),
                fingerprint(3, recovered.len()),
                Some(recovered),
            )
            .unwrap();
        assert_eq!(accepted.layout.panel_height, 64);
    }

    #[test]
    fn initial_read_uses_compio_and_missing_file_uses_defaults() {
        let unique = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tensor-shell-config-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("shell.kdl");
        fs::write(&path, b"layout { panel-height 48 }").unwrap();
        let runtime = io_uring_runtime(CONFIG_IO_OPERATIONS).unwrap();
        let loaded = runtime.block_on(ReloadState::load(&path)).unwrap();
        assert_eq!(loaded.config.layout.panel_height, 48);
        fs::remove_file(&path).unwrap();
        let missing = runtime.block_on(ReloadState::load(&path)).unwrap();
        assert_eq!(missing.config, ShellConfig::default());
        fs::remove_dir_all(root).unwrap();
    }
}
