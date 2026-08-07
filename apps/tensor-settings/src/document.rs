use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    ConfigDiagnostic, ConfigFormat, ConfigPreview, ProductEndpoint, ProductKind, ProductRegistry,
    SettingsConfig, validate_product_config,
};

static NEXT_TEMPORARY_ID: AtomicU64 = AtomicU64::new(1);

/// Maximum retained editor draft. Parsing and validation remain bounded even
/// when an input method sends a large commit batch.
pub const MAX_CONFIG_DRAFT_BYTES: usize = 1 << 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigDocumentState {
    Clean,
    Dirty,
    Invalid,
    ReadOnly,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveConfirmation {
    Ordinary,
    PrivilegedConfirmed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveOutcome {
    Unchanged,
    Saved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveAndReloadOutcome {
    Unchanged,
    Saved { reload_requested: bool },
}

#[derive(Clone, Debug)]
pub struct ConfigDocument {
    endpoint: ProductEndpoint,
    baseline: Option<String>,
    draft: String,
    preview: Result<ConfigPreview, ConfigDiagnostic>,
    read_only: bool,
    confirm_privileged_changes: bool,
}

impl ConfigDocument {
    pub fn open(
        endpoint: ProductEndpoint,
        settings: &SettingsConfig,
    ) -> Result<Self, ConfigDocumentError> {
        let baseline = match fs::read_to_string(&endpoint.config_path) {
            Ok(source) => Some(source),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(source) => {
                return Err(ConfigDocumentError::Read {
                    path: endpoint.config_path.clone(),
                    source,
                });
            }
        };
        let draft = baseline.clone().unwrap_or_default();
        let preview = preview(&endpoint, &draft);
        Ok(Self {
            endpoint,
            baseline,
            draft,
            preview,
            read_only: settings.read_only,
            confirm_privileged_changes: settings.confirm_privileged_changes,
        })
    }

    pub fn endpoint(&self) -> &ProductEndpoint {
        &self.endpoint
    }

    pub fn draft(&self) -> &str {
        &self.draft
    }

    pub fn preview(&self) -> Result<&ConfigPreview, &ConfigDiagnostic> {
        self.preview.as_ref()
    }

    pub fn is_dirty(&self) -> bool {
        self.baseline.as_deref().unwrap_or_default() != self.draft
    }

    pub fn state(&self) -> ConfigDocumentState {
        if self.endpoint.config_format != ConfigFormat::Kdl {
            ConfigDocumentState::Unsupported
        } else if self.read_only {
            ConfigDocumentState::ReadOnly
        } else if self.preview.is_err() {
            ConfigDocumentState::Invalid
        } else if self.is_dirty() {
            ConfigDocumentState::Dirty
        } else {
            ConfigDocumentState::Clean
        }
    }

    pub fn replace_draft(&mut self, draft: impl Into<String>) {
        self.draft = draft.into();
        self.preview = preview(&self.endpoint, &self.draft);
    }

    /// Apply one text-input-v3 edit at a UTF-8 byte cursor.
    pub fn apply_edit(
        &mut self,
        cursor: usize,
        delete_before: usize,
        delete_after: usize,
        commit: &str,
    ) -> Result<usize, ConfigDocumentError> {
        if self.read_only {
            return Err(ConfigDocumentError::ReadOnly);
        }
        if self.endpoint.config_format != ConfigFormat::Kdl {
            return Err(ConfigDocumentError::UnsupportedFormat {
                product: self.endpoint.product,
                format: self.endpoint.config_format,
            });
        }
        if cursor > self.draft.len()
            || delete_before > cursor
            || delete_after > self.draft.len().saturating_sub(cursor)
        {
            return Err(ConfigDocumentError::EditOutOfBounds);
        }
        let start = cursor - delete_before;
        let end = cursor + delete_after;
        if !self.draft.is_char_boundary(start) || !self.draft.is_char_boundary(end) {
            return Err(ConfigDocumentError::EditSplitsCodepoint);
        }
        if commit.contains('\0') {
            return Err(ConfigDocumentError::EditContainsNul);
        }
        let next_len = self
            .draft
            .len()
            .saturating_sub(end - start)
            .saturating_add(commit.len());
        if next_len > MAX_CONFIG_DRAFT_BYTES {
            return Err(ConfigDocumentError::DraftTooLarge {
                bytes: next_len,
                maximum: MAX_CONFIG_DRAFT_BYTES,
            });
        }
        self.draft.replace_range(start..end, commit);
        self.preview = preview(&self.endpoint, &self.draft);
        Ok(start + commit.len())
    }

    pub fn reload(&mut self) -> Result<(), ConfigDocumentError> {
        self.baseline = match fs::read_to_string(&self.endpoint.config_path) {
            Ok(source) => Some(source),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(source) => {
                return Err(ConfigDocumentError::Read {
                    path: self.endpoint.config_path.clone(),
                    source,
                });
            }
        };
        self.draft = self.baseline.clone().unwrap_or_default();
        self.preview = preview(&self.endpoint, &self.draft);
        Ok(())
    }

    pub fn save(
        &mut self,
        confirmation: SaveConfirmation,
    ) -> Result<SaveOutcome, ConfigDocumentError> {
        if self.endpoint.config_format != ConfigFormat::Kdl {
            return Err(ConfigDocumentError::UnsupportedFormat {
                product: self.endpoint.product,
                format: self.endpoint.config_format,
            });
        }
        if self.read_only {
            return Err(ConfigDocumentError::ReadOnly);
        }
        if let Err(diagnostic) = &self.preview {
            return Err(ConfigDocumentError::Invalid(diagnostic.clone()));
        }
        if !self.is_dirty() {
            return Ok(SaveOutcome::Unchanged);
        }
        if self.confirm_privileged_changes
            && requires_privileged_confirmation(&self.endpoint.config_path)
            && confirmation != SaveConfirmation::PrivilegedConfirmed
        {
            return Err(ConfigDocumentError::PrivilegedConfirmationRequired {
                path: self.endpoint.config_path.clone(),
            });
        }
        let current = match fs::read_to_string(&self.endpoint.config_path) {
            Ok(source) => Some(source),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(source) => {
                return Err(ConfigDocumentError::Read {
                    path: self.endpoint.config_path.clone(),
                    source,
                });
            }
        };
        if current != self.baseline {
            return Err(ConfigDocumentError::ChangedOnDisk {
                path: self.endpoint.config_path.clone(),
            });
        }
        atomic_replace(&self.endpoint.config_path, self.draft.as_bytes())?;
        self.baseline = Some(self.draft.clone());
        Ok(SaveOutcome::Saved)
    }

    /// Save a validated draft and request the owning product to reload it.
    ///
    /// The IPC client is borrowed from the caller so Settings never creates a
    /// runtime or hides a task. A successful request means the compositor
    /// accepted the reload; its eventual applied/rejected event remains owned
    /// by the product's IPC stream.
    pub async fn save_and_reload(
        &mut self,
        confirmation: SaveConfirmation,
        client: Option<&mut tensor_ipc::land::CompioClient>,
    ) -> Result<SaveAndReloadOutcome, ConfigDocumentError> {
        if self.endpoint.reload == crate::ReloadRoute::TensorMsgLand && client.is_none() {
            return Err(ConfigDocumentError::ReloadUnavailable {
                product: self.endpoint.product,
            });
        }
        if self.endpoint.reload == crate::ReloadRoute::TensorMsgWallpaper {
            return Err(ConfigDocumentError::ReloadUnavailable {
                product: self.endpoint.product,
            });
        }
        let saved = self.save(confirmation)?;
        if saved == SaveOutcome::Unchanged {
            return Ok(SaveAndReloadOutcome::Unchanged);
        }
        match self.endpoint.reload {
            crate::ReloadRoute::None => Ok(SaveAndReloadOutcome::Saved {
                reload_requested: false,
            }),
            crate::ReloadRoute::TensorMsgLand => {
                let client = client.ok_or(ConfigDocumentError::ReloadUnavailable {
                    product: self.endpoint.product,
                })?;
                match client.call(tensor_ipc::land::Command::ReloadConfig).await? {
                    tensor_ipc::land::ResultBody::Accepted => Ok(SaveAndReloadOutcome::Saved {
                        reload_requested: true,
                    }),
                    _ => Err(ConfigDocumentError::ReloadResponse {
                        product: self.endpoint.product,
                    }),
                }
            }
            crate::ReloadRoute::TensorMsgWallpaper => unreachable!("checked above"),
        }
    }
}

#[derive(Debug)]
pub struct SettingsWorkspace {
    documents: Vec<ConfigDocument>,
    selected: ProductKind,
    query: String,
}

impl SettingsWorkspace {
    pub fn open(
        registry: &ProductRegistry,
        settings: &SettingsConfig,
    ) -> Result<Self, ConfigDocumentError> {
        let documents = registry
            .endpoints()
            .iter()
            .cloned()
            .map(|endpoint| ConfigDocument::open(endpoint, settings))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            documents,
            selected: ProductKind::Land,
            query: String::new(),
        })
    }

    pub fn documents(&self) -> &[ConfigDocument] {
        &self.documents
    }

    pub fn selected(&self) -> ProductKind {
        self.selected
    }

    pub fn select(&mut self, product: ProductKind) {
        self.selected = product;
    }

    pub fn selected_document(&self) -> &ConfigDocument {
        self.document(self.selected)
    }

    pub fn selected_document_mut(&mut self) -> &mut ConfigDocument {
        self.document_mut(self.selected)
    }

    pub fn document(&self, product: ProductKind) -> &ConfigDocument {
        self.documents
            .iter()
            .find(|document| document.endpoint.product == product)
            .expect("the workspace mirrors the fixed product registry")
    }

    pub fn document_mut(&mut self, product: ProductKind) -> &mut ConfigDocument {
        self.documents
            .iter_mut()
            .find(|document| document.endpoint.product == product)
            .expect("the workspace mirrors the fixed product registry")
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn filtered_products(&self) -> impl Iterator<Item = ProductKind> + '_ {
        let query = self.query.trim().to_ascii_lowercase();
        self.documents.iter().filter_map(move |document| {
            let product = document.endpoint.product;
            let path = document.endpoint.config_path.to_string_lossy();
            (query.is_empty()
                || product.search_terms().contains(&query)
                || product.title().to_ascii_lowercase().contains(&query)
                || path.to_ascii_lowercase().contains(&query))
            .then_some(product)
        })
    }

    pub async fn save_selected_and_reload(
        &mut self,
        confirmation: SaveConfirmation,
        client: Option<&mut tensor_ipc::land::CompioClient>,
    ) -> Result<SaveAndReloadOutcome, ConfigDocumentError> {
        self.selected_document_mut()
            .save_and_reload(confirmation, client)
            .await
    }
}

fn preview(endpoint: &ProductEndpoint, source: &str) -> Result<ConfigPreview, ConfigDiagnostic> {
    match endpoint.config_format {
        ConfigFormat::Kdl => {
            validate_product_config(endpoint.product, &endpoint.config_path, source)
        }
        ConfigFormat::MigrationDebtToml => Ok(ConfigPreview::Unsupported),
    }
}

fn requires_privileged_confirmation(path: &Path) -> bool {
    path.starts_with("/etc") || path.starts_with("/usr")
}

fn atomic_replace(path: &Path, contents: &[u8]) -> Result<(), ConfigDocumentError> {
    let parent = path.parent().ok_or_else(|| ConfigDocumentError::NoParent {
        path: path.to_owned(),
    })?;
    fs::create_dir_all(parent).map_err(|source| ConfigDocumentError::Write {
        path: path.to_owned(),
        source,
    })?;
    let temporary = temporary_path(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options
        .open(&temporary)
        .map_err(|source| ConfigDocumentError::Write {
            path: temporary.clone(),
            source,
        })?;
    let result = (|| {
        copy_permissions(path, &file)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok::<(), io::Error>(())
    })();
    if let Err(source) = result {
        let _ = fs::remove_file(&temporary);
        return Err(ConfigDocumentError::Write {
            path: path.to_owned(),
            source,
        });
    }
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    let id = NEXT_TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(
        ".{name}.tensor-settings-{}-{id}.tmp",
        std::process::id()
    ))
}

#[cfg(unix)]
fn copy_permissions(path: &Path, temporary: &File) -> io::Result<()> {
    match fs::metadata(path) {
        Ok(metadata) => temporary.set_permissions(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(not(unix))]
fn copy_permissions(_path: &Path, _temporary: &File) -> io::Result<()> {
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigDocumentError {
    #[error("failed to read product configuration {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write product configuration {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("configuration path has no parent: {path}")]
    NoParent { path: PathBuf },
    #[error("Tensor Settings is in read-only mode")]
    ReadOnly,
    #[error("configuration is invalid: {0}")]
    Invalid(ConfigDiagnostic),
    #[error("{product:?} configuration format {format:?} is not editable yet")]
    UnsupportedFormat {
        product: ProductKind,
        format: ConfigFormat,
    },
    #[error("configuration changed on disk since it was opened: {path}")]
    ChangedOnDisk { path: PathBuf },
    #[error("writing {path} requires explicit privileged-change confirmation")]
    PrivilegedConfirmationRequired { path: PathBuf },
    #[error("reload route for {product:?} is not available to Tensor Settings")]
    ReloadUnavailable { product: ProductKind },
    #[error("reload route for {product:?} returned an unexpected response")]
    ReloadResponse { product: ProductKind },
    #[error("configuration edit is outside the draft bounds")]
    EditOutOfBounds,
    #[error("configuration edit splits a UTF-8 codepoint")]
    EditSplitsCodepoint,
    #[error("configuration edit contains a NUL byte")]
    EditContainsNul,
    #[error("configuration draft has {bytes} bytes; maximum is {maximum}")]
    DraftTooLarge { bytes: usize, maximum: usize },
    #[error("configuration reload IPC failed: {0}")]
    Reload(#[from] tensor_ipc::land::ClientError),
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
    };

    use super::*;

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tensor-settings-{name}-{}-{}",
            std::process::id(),
            NEXT_TEMPORARY_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn endpoint(
        path: PathBuf,
        product: ProductKind,
        reload: crate::ReloadRoute,
    ) -> ProductEndpoint {
        ProductEndpoint {
            product,
            config_path: path,
            config_format: ConfigFormat::Kdl,
            socket_path: None,
            reload,
        }
    }

    #[test]
    fn text_edits_preserve_utf8_boundaries_and_refresh_preview() {
        let root = test_path("edit");
        let path = root.join("idle.kdl");
        let mut document = ConfigDocument::open(
            endpoint(path, ProductKind::Idle, crate::ReloadRoute::None),
            &SettingsConfig::default(),
        )
        .unwrap();
        let cursor = document.apply_edit(0, 0, 0, "enabled #true\n").unwrap();
        assert_eq!(cursor, "enabled #true\n".len());
        let cursor = document.apply_edit(cursor, 0, 0, "// 名称").unwrap();
        assert_eq!(cursor, "enabled #true\n// 名称".len());
        assert!(matches!(
            document.apply_edit(cursor - 2, 1, 0, ""),
            Err(ConfigDocumentError::EditSplitsCodepoint)
        ));
        assert!(document.preview().is_ok());
    }

    #[test]
    fn valid_shell_draft_moves_through_dirty_save_and_clean_states() {
        let root = test_path("save");
        let path = root.join("shell.kdl");
        let mut document = ConfigDocument::open(
            endpoint(path.clone(), ProductKind::Shell, crate::ReloadRoute::None),
            &SettingsConfig::default(),
        )
        .unwrap();
        assert_eq!(document.state(), ConfigDocumentState::Clean);
        document.replace_draft("layout { panel-height 52 }\n");
        assert_eq!(document.state(), ConfigDocumentState::Dirty);
        assert_eq!(
            document.save(SaveConfirmation::Ordinary).unwrap(),
            SaveOutcome::Saved
        );
        assert_eq!(document.state(), ConfigDocumentState::Clean);
        assert_eq!(fs::read_to_string(path).unwrap(), document.draft());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_draft_never_replaces_the_last_valid_file() {
        let root = test_path("invalid");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("shell.kdl");
        fs::write(&path, "layout { panel-height 40 }\n").unwrap();
        let mut document = ConfigDocument::open(
            endpoint(path.clone(), ProductKind::Shell, crate::ReloadRoute::None),
            &SettingsConfig::default(),
        )
        .unwrap();
        document.replace_draft("layout { panel-height 0 }\n");
        assert_eq!(document.state(), ConfigDocumentState::Invalid);
        assert!(matches!(
            document.save(SaveConfirmation::Ordinary),
            Err(ConfigDocumentError::Invalid(_))
        ));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "layout { panel-height 40 }\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_change_is_detected_instead_of_overwritten() {
        let root = test_path("conflict");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("idle.kdl");
        fs::write(&path, "enabled #true\n").unwrap();
        let mut document = ConfigDocument::open(
            endpoint(path.clone(), ProductKind::Idle, crate::ReloadRoute::None),
            &SettingsConfig::default(),
        )
        .unwrap();
        document.replace_draft("enabled #false\n");
        fs::write(&path, "enabled #true\nrespect-inhibitors #false\n").unwrap();
        assert!(matches!(
            document.save(SaveConfirmation::Ordinary),
            Err(ConfigDocumentError::ChangedOnDisk { .. })
        ));
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("respect-inhibitors")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn read_only_policy_blocks_a_valid_dirty_document() {
        let root = test_path("read-only");
        let path = root.join("idle.kdl");
        let settings = SettingsConfig {
            read_only: true,
            ..SettingsConfig::default()
        };
        let mut document = ConfigDocument::open(
            endpoint(path, ProductKind::Idle, crate::ReloadRoute::None),
            &settings,
        )
        .unwrap();
        document.replace_draft("enabled #false\n");
        assert_eq!(document.state(), ConfigDocumentState::ReadOnly);
        assert!(matches!(
            document.save(SaveConfirmation::Ordinary),
            Err(ConfigDocumentError::ReadOnly)
        ));
    }

    #[test]
    fn land_save_and_reload_requires_a_caller_owned_ipc_client_before_writing() {
        let root = test_path("reload-route");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("config.kdl");
        fs::write(&path, "workspace-count 2\n").unwrap();
        let mut document = ConfigDocument::open(
            endpoint(
                path.clone(),
                ProductKind::Land,
                crate::ReloadRoute::TensorMsgLand,
            ),
            &SettingsConfig::default(),
        )
        .unwrap();
        document.replace_draft("workspace-count 3\n");
        let runtime = tensor_runtime::io_uring_runtime(4).unwrap();
        let result = runtime.block_on(document.save_and_reload(SaveConfirmation::Ordinary, None));
        assert!(matches!(
            result,
            Err(ConfigDocumentError::ReloadUnavailable {
                product: ProductKind::Land
            })
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), "workspace-count 2\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn land_save_and_reload_uses_the_versioned_compio_wire_request() {
        let root = test_path("reload-wire");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("config.kdl");
        fs::write(&path, "workspace-count 2\n").unwrap();
        let socket = root.join("tensor.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut decoder = tensor_ipc::land::FrameDecoder::new();
            let mut bytes = [0_u8; 4096];
            let request = loop {
                let read = stream.read(&mut bytes).unwrap();
                let mut requests = decoder
                    .push::<tensor_ipc::land::Request>(&bytes[..read])
                    .unwrap();
                if let Some(request) = requests.pop() {
                    break request;
                }
            };
            assert!(matches!(
                request.command,
                tensor_ipc::land::Command::ReloadConfig
            ));
            let response =
                tensor_ipc::land::ServerMessage::Response(tensor_ipc::land::Response::new(
                    request.request_id,
                    tensor_ipc::land::ResultBody::Accepted,
                ));
            stream
                .write_all(&tensor_ipc::land::encode(&response).unwrap())
                .unwrap();
        });
        let mut document = ConfigDocument::open(
            endpoint(
                path.clone(),
                ProductKind::Land,
                crate::ReloadRoute::TensorMsgLand,
            ),
            &SettingsConfig::default(),
        )
        .unwrap();
        document.replace_draft("workspace-count 3\n");
        let runtime = tensor_runtime::io_uring_runtime(8).unwrap();
        let result = runtime.block_on(async {
            let mut client = tensor_ipc::land::CompioClient::connect(&socket)
                .await
                .unwrap();
            document
                .save_and_reload(SaveConfirmation::Ordinary, Some(&mut client))
                .await
        });
        assert_eq!(
            result.unwrap(),
            SaveAndReloadOutcome::Saved {
                reload_requested: true
            }
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "workspace-count 3\n");
        server.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
