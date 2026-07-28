use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use gio::prelude::*;

use super::entries::{Entry, EntryData, name_width_units, sort_entries};

pub const NETWORK_ROOT_URI: &str = "network:///";
pub const LEGACY_REMOTE_ROOT_URI: &str = "remote:/";
pub const NETWORK_ROOT_LABEL: &str = "Network";
pub const NETWORK_ROOT_ICON: &str = "folder-remote";

const SUPPORTED_NETWORK_SCHEMES: &[&str] = &[
    "network", "remote", "smb", "sftp", "fish", "ftp", "ftps", "nfs", "dav", "davs", "webdav",
    "webdavs",
];
const NETWORK_MOUNT_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkLocation {
    pub uri: String,
    pub display_name: String,
    pub local_path: Option<PathBuf>,
    pub scheme: String,
    pub icon_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkUrlError {
    Empty,
    MissingScheme(String),
    InvalidScheme(String),
    UnsupportedScheme(String),
    RootOnlyScheme(String),
    MissingAuthority(String),
    InvalidControlCharacter,
}

impl fmt::Display for NetworkUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "network URL is empty"),
            Self::MissingScheme(uri) => write!(f, "network URL has no scheme: {uri}"),
            Self::InvalidScheme(scheme) => write!(f, "invalid network URL scheme: {scheme}"),
            Self::UnsupportedScheme(scheme) => {
                write!(f, "unsupported network URL scheme: {scheme}")
            }
            Self::RootOnlyScheme(scheme) => {
                write!(
                    f,
                    "network URL scheme is only supported as a root: {scheme}"
                )
            }
            Self::MissingAuthority(scheme) => {
                write!(
                    f,
                    "network URL scheme requires a host/share authority: {scheme}"
                )
            }
            Self::InvalidControlCharacter => write!(f, "network URL contains a control character"),
        }
    }
}

impl Error for NetworkUrlError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkScanError {
    Url(NetworkUrlError),
    Cancelled,
    AuthenticationRequired {
        uri: String,
        message: String,
        default_username: Option<String>,
        default_domain: Option<String>,
    },
    Gio {
        uri: String,
        operation: &'static str,
        message: String,
    },
}

impl fmt::Display for NetworkScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(error) => write!(f, "{error}"),
            Self::Cancelled => write!(f, "network scan was cancelled"),
            Self::AuthenticationRequired { uri, message, .. } => {
                write!(f, "authentication required for {uri}: {message}")
            }
            Self::Gio {
                uri,
                operation,
                message,
            } => {
                write!(f, "GIO {operation} failed for {uri}: {message}")
            }
        }
    }
}

impl Error for NetworkScanError {}

impl From<NetworkUrlError> for NetworkScanError {
    fn from(error: NetworkUrlError) -> Self {
        Self::Url(error)
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct NetworkAuth {
    pub username: Option<String>,
    pub domain: Option<String>,
    pub password: Option<String>,
    pub anonymous: bool,
    pub remember: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NetworkAuthPrompt {
    message: String,
    default_username: Option<String>,
    default_domain: Option<String>,
}

impl fmt::Debug for NetworkAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NetworkAuth")
            .field("username", &self.username)
            .field("domain", &self.domain)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("anonymous", &self.anonymous)
            .field("remember", &self.remember)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkFilesystemKind {
    Local,
    Remote,
    Gvfs,
}

pub fn supported_network_schemes() -> &'static [&'static str] {
    SUPPORTED_NETWORK_SCHEMES
}

pub fn is_supported_network_scheme(scheme: &str) -> bool {
    let scheme = scheme.to_ascii_lowercase();
    SUPPORTED_NETWORK_SCHEMES.contains(&scheme.as_str())
}

pub fn network_root_location() -> NetworkLocation {
    NetworkLocation {
        uri: NETWORK_ROOT_URI.to_string(),
        display_name: NETWORK_ROOT_LABEL.to_string(),
        local_path: None,
        scheme: "network".to_string(),
        icon_name: NETWORK_ROOT_ICON.to_string(),
    }
}

pub fn network_root_path() -> PathBuf {
    PathBuf::from(NETWORK_ROOT_URI)
}

pub fn network_path_from_uri(uri: &str) -> Result<PathBuf, NetworkUrlError> {
    normalize_network_uri(uri).map(PathBuf::from)
}

pub fn network_uri_from_path(path: &Path) -> Option<String> {
    path.to_str()
        .and_then(|path| normalize_network_uri(path).ok())
}

pub fn is_network_path(path: &Path) -> bool {
    network_uri_from_path(path).is_some()
}

pub fn remember_network_auth(uri: &str, auth: NetworkAuth) -> Result<(), NetworkUrlError> {
    let key = network_auth_key(uri)?;
    network_auth_store()
        .lock()
        .expect("network auth store poisoned")
        .insert(key, auth);
    Ok(())
}

pub fn forget_network_auth(uri: &str) -> Result<(), NetworkUrlError> {
    let key = network_auth_key(uri)?;
    network_auth_store()
        .lock()
        .expect("network auth store poisoned")
        .remove(&key);
    Ok(())
}

pub fn is_network_root_uri(uri: &str) -> bool {
    normalize_network_uri(uri).is_ok_and(|normalized| normalized == NETWORK_ROOT_URI)
}

pub fn is_network_root_path(path: &Path) -> bool {
    path.to_str().is_some_and(is_network_root_uri)
}

pub fn parse_network_location(uri: &str) -> Result<NetworkLocation, NetworkUrlError> {
    let normalized = normalize_network_uri(uri)?;
    if normalized == NETWORK_ROOT_URI {
        return Ok(network_root_location());
    }
    let (scheme, rest) = split_scheme(&normalized)?;
    let after_slashes = rest
        .strip_prefix("//")
        .ok_or_else(|| NetworkUrlError::MissingAuthority(scheme.to_string()))?;
    let display_name = network_share_display_name(after_slashes);
    let scheme = scheme.to_string();
    Ok(NetworkLocation {
        uri: normalized,
        display_name,
        local_path: None,
        scheme,
        icon_name: "folder-remote".to_string(),
    })
}

pub fn network_parent_path(path: &Path) -> Option<PathBuf> {
    let uri = network_uri_from_path(path)?;
    if uri == NETWORK_ROOT_URI {
        return None;
    }

    let (scheme, rest) = split_scheme(&uri).ok()?;
    let after_slashes = rest.strip_prefix("//")?;
    let (authority, path_and_tail) = after_slashes
        .split_once('/')
        .map_or((after_slashes, ""), |(authority, path)| (authority, path));
    let path_without_tail = path_and_tail
        .split(['?', '#'])
        .next()
        .unwrap_or(path_and_tail)
        .trim_matches('/');
    if path_without_tail.is_empty() {
        return Some(network_root_path());
    }

    let mut segments = path_without_tail
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() <= 1 {
        return Some(network_root_path());
    }
    segments.pop();
    Some(PathBuf::from(format!(
        "{scheme}://{authority}/{}/",
        segments.join("/")
    )))
}

pub fn network_child_path(directory: &Path, child: &str) -> Option<PathBuf> {
    let uri = network_uri_from_path(directory)?;
    if uri == NETWORK_ROOT_URI || child.trim().is_empty() || child.contains('/') {
        return None;
    }
    Some(PathBuf::from(format!(
        "{}{}",
        ensure_uri_trailing_slash(&uri),
        percent_encode_path_segment(child)
    )))
}

pub fn network_path_display_name(path: &Path) -> Option<String> {
    let uri = network_uri_from_path(path)?;
    parse_network_location(&uri)
        .map(|location| location.display_name)
        .ok()
}

pub fn read_network_entry_batches_sync_cancellable(
    path: &Path,
    batch_size: usize,
    mut is_cancelled: impl FnMut() -> bool,
    mut on_batch: impl FnMut(Vec<Entry>),
) -> Result<Option<()>, NetworkScanError> {
    let uri = network_uri_from_path(path).ok_or_else(|| {
        NetworkScanError::Url(NetworkUrlError::MissingScheme(path.display().to_string()))
    })?;
    if is_cancelled() {
        return Ok(None);
    }

    let batch_size = batch_size.max(1);
    let result = if uri == NETWORK_ROOT_URI {
        read_network_root_entries()
    } else {
        read_gio_network_entries(&uri, &mut is_cancelled)
    }?;
    if is_cancelled() {
        return Ok(None);
    }

    for mut batch in result.chunks(batch_size).map(|chunk| chunk.to_vec()) {
        sort_entries(&mut batch, false);
        on_batch(batch);
        if is_cancelled() {
            return Ok(None);
        }
    }
    Ok(Some(()))
}

pub fn normalize_network_uri(uri: &str) -> Result<String, NetworkUrlError> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err(NetworkUrlError::Empty);
    }
    if trimmed.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
        return Err(NetworkUrlError::InvalidControlCharacter);
    }

    let (raw_scheme, rest) = split_scheme(trimmed)?;
    if !valid_scheme(raw_scheme) {
        return Err(NetworkUrlError::InvalidScheme(raw_scheme.to_string()));
    }
    let scheme = raw_scheme.to_ascii_lowercase();
    if !is_supported_network_scheme(&scheme) {
        return Err(NetworkUrlError::UnsupportedScheme(scheme));
    }

    match scheme.as_str() {
        "network" | "remote" => normalize_network_root(&scheme, rest),
        _ => normalize_network_share(&scheme, rest),
    }
}

pub fn classify_network_filesystem(filesystem_type: &str) -> NetworkFilesystemKind {
    let fs = filesystem_type.to_ascii_lowercase();
    if fs == "fuse.gvfsd-fuse" || fs == "gvfsd-fuse" {
        return NetworkFilesystemKind::Gvfs;
    }
    if filesystem_type_is_remote(&fs) {
        NetworkFilesystemKind::Remote
    } else {
        NetworkFilesystemKind::Local
    }
}

pub fn filesystem_type_is_remote(filesystem_type: &str) -> bool {
    let fs = filesystem_type.to_ascii_lowercase();
    matches!(
        fs.as_str(),
        "cifs"
            | "smb3"
            | "smbfs"
            | "nfs"
            | "nfs4"
            | "sshfs"
            | "davfs"
            | "davfs2"
            | "ceph"
            | "glusterfs"
            | "lustre"
            | "rclone"
            | "s3fs"
            | "goofys"
            | "gcsfuse"
            | "fuse.gvfsd-fuse"
            | "gvfsd-fuse"
    ) || fs.starts_with("fuse.sshfs")
        || fs.starts_with("fuse.rclone")
        || fs.starts_with("fuse.s3fs")
        || fs.starts_with("fuse.gcsfuse")
        || fs.starts_with("fuse.davfs")
}

fn split_scheme(uri: &str) -> Result<(&str, &str), NetworkUrlError> {
    uri.split_once(':')
        .ok_or_else(|| NetworkUrlError::MissingScheme(uri.to_string()))
}

fn valid_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

fn normalize_network_root(scheme: &str, rest: &str) -> Result<String, NetworkUrlError> {
    if rest.is_empty() || rest.chars().all(|ch| ch == '/') {
        Ok(NETWORK_ROOT_URI.to_string())
    } else {
        Err(NetworkUrlError::RootOnlyScheme(scheme.to_string()))
    }
}

fn normalize_network_share(scheme: &str, rest: &str) -> Result<String, NetworkUrlError> {
    let Some(after_slashes) = rest.strip_prefix("//") else {
        return Err(NetworkUrlError::MissingAuthority(scheme.to_string()));
    };
    let authority = after_slashes
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if authority.is_empty() {
        return Err(NetworkUrlError::MissingAuthority(scheme.to_string()));
    }
    if after_slashes.contains('/') {
        Ok(format!("{scheme}://{after_slashes}"))
    } else {
        Ok(format!("{scheme}://{after_slashes}/"))
    }
}

fn network_share_display_name(after_slashes: &str) -> String {
    let without_query = after_slashes
        .split(['?', '#'])
        .next()
        .unwrap_or(after_slashes);
    let (authority, path) = without_query
        .split_once('/')
        .map_or((without_query, ""), |(authority, path)| (authority, path));
    let host = display_host(authority);
    let last_segment = path.rsplit('/').find(|segment| !segment.is_empty());
    match last_segment {
        Some(segment) => format!("{} on {host}", percent_decode_lossy(segment)),
        None => host.to_string(),
    }
}

fn read_network_root_entries() -> Result<Vec<Entry>, NetworkScanError> {
    let monitor = gio::VolumeMonitor::get();
    let mut locations = Vec::<NetworkLocation>::new();

    for mount in monitor.mounts() {
        if MountExt::is_shadowed(&mount) {
            continue;
        }
        let root = MountExt::root(&mount);
        let uri = root.uri();
        let Some(mut location) = network_location_from_gio_uri(uri.as_str()) else {
            continue;
        };
        location.display_name =
            non_empty_string(MountExt::name(&mount).as_str()).unwrap_or(location.display_name);
        location.local_path = root.path();
        push_unique_network_location(&mut locations, location);
    }

    for volume in monitor.volumes() {
        if volume.get_mount().is_some() {
            continue;
        }
        let Some(root) = VolumeExt::activation_root(&volume) else {
            continue;
        };
        let uri = root.uri();
        let Some(mut location) = network_location_from_gio_uri(uri.as_str()) else {
            continue;
        };
        location.display_name =
            non_empty_string(VolumeExt::name(&volume).as_str()).unwrap_or(location.display_name);
        push_unique_network_location(&mut locations, location);
    }

    let mut entries = locations
        .into_iter()
        .map(entry_from_network_location)
        .collect::<Vec<_>>();
    sort_entries(&mut entries, false);
    Ok(entries)
}

fn read_gio_network_entries(
    uri: &str,
    is_cancelled: &mut impl FnMut() -> bool,
) -> Result<Vec<Entry>, NetworkScanError> {
    let file = gio::File::for_uri(uri);
    let cancellable = gio::Cancellable::new();
    mount_network_uri_if_needed(&file, uri, &cancellable, is_cancelled)?;
    if is_cancelled() {
        cancellable.cancel();
        return Err(NetworkScanError::Cancelled);
    }

    let enumerator = file
        .enumerate_children(
            "standard::name,standard::display-name,standard::type,standard::size,standard::content-type,time::modified",
            gio::FileQueryInfoFlags::NONE,
            Some(&cancellable),
        )
        .map_err(|err| network_gio_error(uri, "enumerate", err))?;
    let mut entries = Vec::new();
    let mut index = 0usize;
    loop {
        if index.is_multiple_of(64) && is_cancelled() {
            cancellable.cancel();
            return Err(NetworkScanError::Cancelled);
        }
        let Some(info) = enumerator
            .next_file(Some(&cancellable))
            .map_err(|err| network_gio_error(uri, "enumerate", err))?
        else {
            break;
        };
        let child = enumerator.child(&info);
        let child_uri = child.uri();
        if let Some(entry) = entry_from_gio_file_info(&info, child_uri.as_str()) {
            entries.push(entry);
        }
        index += 1;
    }
    let (closed, close_error) = enumerator.close(Some(&cancellable));
    if !closed {
        return Err(close_error.map_or_else(
            || NetworkScanError::Gio {
                uri: uri.to_string(),
                operation: "close-enumerator",
                message: "enumerator close failed without an error".to_string(),
            },
            |err| network_gio_error(uri, "close-enumerator", err),
        ));
    }
    sort_entries(&mut entries, false);
    Ok(entries)
}

fn mount_network_uri_if_needed(
    file: &gio::File,
    uri: &str,
    cancellable: &gio::Cancellable,
    is_cancelled: &mut impl FnMut() -> bool,
) -> Result<(), NetworkScanError> {
    if file.find_enclosing_mount(Some(cancellable)).is_ok() {
        return Ok(());
    }
    if is_cancelled() {
        cancellable.cancel();
        return Err(NetworkScanError::Cancelled);
    }

    let mount_operation = gio::MountOperation::new();
    let auth_prompt = Rc::new(RefCell::new(None::<NetworkAuthPrompt>));
    let auth_prompt_for_callback = auth_prompt.clone();
    let auth_attempted = Rc::new(RefCell::new(false));
    let auth_attempted_for_callback = auth_attempted.clone();
    let uri_for_callback = uri.to_string();
    mount_operation.connect_ask_password(
        move |operation, message, default_user, default_domain, _flags| {
            if !*auth_attempted_for_callback.borrow()
                && let Some(auth) = stored_network_auth_for_uri(&uri_for_callback)
            {
                apply_network_auth_to_mount_operation(operation, &auth);
                *auth_attempted_for_callback.borrow_mut() = true;
                operation.reply(gio::MountOperationResult::Handled);
                return;
            }
            let _ = forget_network_auth(&uri_for_callback);
            *auth_prompt_for_callback.borrow_mut() = Some(network_auth_required_prompt(
                message,
                default_user,
                default_domain,
            ));
            operation.reply(gio::MountOperationResult::Aborted);
        },
    );

    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let result_for_callback = result.clone();
    file.mount_enclosing_volume(
        gio::MountMountFlags::NONE,
        Some(&mount_operation),
        Some(cancellable),
        move |res| {
            *result_for_callback.borrow_mut() = Some(res);
        },
    );

    let main_context = gio::glib::MainContext::default();
    while result.borrow().is_none() {
        while main_context.pending() {
            main_context.iteration(false);
            if result.borrow().is_some() {
                break;
            }
        }
        if result.borrow().is_some() {
            break;
        }
        if is_cancelled() {
            cancellable.cancel();
            return Err(NetworkScanError::Cancelled);
        }
        thread::sleep(NETWORK_MOUNT_POLL_INTERVAL);
    }

    if is_cancelled() {
        cancellable.cancel();
        return Err(NetworkScanError::Cancelled);
    }
    let mount_result = result
        .borrow_mut()
        .take()
        .ok_or_else(|| NetworkScanError::Gio {
            uri: uri.to_string(),
            operation: "mount",
            message: "mount finished without a result".to_string(),
        })?;
    if let Some(prompt) = auth_prompt.borrow_mut().take() {
        return Err(NetworkScanError::AuthenticationRequired {
            uri: uri.to_string(),
            message: prompt.message,
            default_username: prompt.default_username,
            default_domain: prompt.default_domain,
        });
    }
    mount_result.map_err(|err| network_gio_error(uri, "mount", err))
}

fn entry_from_network_location(location: NetworkLocation) -> Entry {
    let name = location.display_name;
    Entry::new(EntryData {
        name_width_units: name_width_units(&name),
        name: Arc::from(name),
        target_path: Some(PathBuf::from(location.uri)),
        size_bytes: 0,
        modified_secs: None,
        metadata_complete: true,
        mime_type: Some(Arc::from("inode/directory")),
        mime_magic_checked: true,
        trash_original_path: None,
        trash_deletion_time: None,
        is_dir: true,
    })
}

fn entry_from_gio_file_info(info: &gio::FileInfo, child_uri: &str) -> Option<Entry> {
    let normalized_uri = normalize_network_uri(child_uri).ok()?;
    let name = non_empty_string(info.display_name().as_str()).or_else(|| {
        info.name()
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(non_empty_string)
    })?;
    let is_dir = info.file_type() == gio::FileType::Directory;
    let size_bytes = if is_dir {
        0
    } else {
        u64::try_from(info.size()).unwrap_or_default()
    };
    let modified_secs = info
        .attribute_uint64("time::modified")
        .checked_sub(0)
        .filter(|secs| *secs > 0);
    let mime_type = if is_dir {
        Some(Arc::from("inode/directory"))
    } else {
        info.content_type()
            .and_then(|mime| non_empty_string(mime.as_str()))
            .map(Arc::from)
    };
    Some(Entry::new(EntryData {
        name_width_units: name_width_units(&name),
        name: Arc::from(name),
        target_path: Some(PathBuf::from(normalized_uri)),
        size_bytes,
        modified_secs,
        metadata_complete: true,
        mime_type,
        mime_magic_checked: true,
        trash_original_path: None,
        trash_deletion_time: None,
        is_dir,
    }))
}

fn network_location_from_gio_uri(uri: &str) -> Option<NetworkLocation> {
    parse_network_location(uri).ok().filter(|location| {
        location.uri != NETWORK_ROOT_URI && is_supported_network_scheme(&location.scheme)
    })
}

fn push_unique_network_location(locations: &mut Vec<NetworkLocation>, location: NetworkLocation) {
    if !locations
        .iter()
        .any(|existing| existing.uri == location.uri)
    {
        locations.push(location);
    }
}

fn network_auth_store() -> &'static Mutex<HashMap<String, NetworkAuth>> {
    static STORE: OnceLock<Mutex<HashMap<String, NetworkAuth>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn stored_network_auth_for_uri(uri: &str) -> Option<NetworkAuth> {
    let key = network_auth_key(uri).ok()?;
    network_auth_store()
        .lock()
        .expect("network auth store poisoned")
        .get(&key)
        .cloned()
}

fn network_auth_key(uri: &str) -> Result<String, NetworkUrlError> {
    let normalized = normalize_network_uri(uri)?;
    if normalized == NETWORK_ROOT_URI {
        return Ok(normalized);
    }
    let (scheme, rest) = split_scheme(&normalized)?;
    let after_slashes = rest
        .strip_prefix("//")
        .ok_or_else(|| NetworkUrlError::MissingAuthority(scheme.to_string()))?;
    let (authority, path) = after_slashes
        .split_once('/')
        .map_or((after_slashes, ""), |(authority, path)| (authority, path));
    let path_without_tail = path
        .split(['?', '#'])
        .next()
        .unwrap_or(path)
        .trim_matches('/');
    let share_segment = path_without_tail
        .split('/')
        .find(|segment| !segment.is_empty());

    match (scheme, share_segment) {
        ("smb" | "nfs", Some(share)) => Ok(format!("{scheme}://{authority}/{share}/")),
        _ => Ok(format!("{scheme}://{authority}/")),
    }
}

fn apply_network_auth_to_mount_operation(operation: &gio::MountOperation, auth: &NetworkAuth) {
    operation.set_anonymous(auth.anonymous);
    operation.set_username(auth.username.as_deref());
    operation.set_domain(auth.domain.as_deref());
    operation.set_password(auth.password.as_deref());
    operation.set_password_save(if auth.remember {
        gio::PasswordSave::ForSession
    } else {
        gio::PasswordSave::Never
    });
}

fn network_auth_required_prompt(
    message: &str,
    default_user: &str,
    default_domain: &str,
) -> NetworkAuthPrompt {
    let mut parts = Vec::new();
    if let Some(message) = non_empty_string(message) {
        parts.push(message);
    }
    let default_username = non_empty_string(default_user);
    let default_domain = non_empty_string(default_domain);
    if let Some(default_user) = default_username.as_deref() {
        parts.push(format!("user: {default_user}"));
    }
    if let Some(default_domain) = default_domain.as_deref() {
        parts.push(format!("domain: {default_domain}"));
    }
    let message = if parts.is_empty() {
        "authentication required".to_string()
    } else {
        parts.join("; ")
    };
    NetworkAuthPrompt {
        message,
        default_username,
        default_domain,
    }
}

fn network_gio_error(
    uri: &str,
    operation: &'static str,
    err: gio::glib::Error,
) -> NetworkScanError {
    if err.matches::<gio::IOErrorEnum>(gio::IOErrorEnum::Cancelled) {
        return NetworkScanError::Cancelled;
    }
    let message = err.to_string();
    let lower = message.to_ascii_lowercase();
    if lower.contains("password")
        || lower.contains("credential")
        || lower.contains("authentication")
        || lower.contains("permission denied")
    {
        NetworkScanError::AuthenticationRequired {
            uri: uri.to_string(),
            message,
            default_username: None,
            default_domain: None,
        }
    } else {
        NetworkScanError::Gio {
            uri: uri.to_string(),
            operation,
            message,
        }
    }
}

fn ensure_uri_trailing_slash(uri: &str) -> String {
    if uri.ends_with('/') {
        uri.to_string()
    } else {
        format!("{uri}/")
    }
}

fn percent_encode_path_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(*byte as char);
        } else {
            encoded.push('%');
            encoded.push(hex_digit(byte >> 4));
            encoded.push(hex_digit(byte & 0x0f));
        }
    }
    encoded
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + value - 10) as char,
        _ => unreachable!("hex digit nibble is always 0..=15"),
    }
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn display_host(authority: &str) -> &str {
    let host = authority.rsplit('@').next().unwrap_or(authority);
    host.strip_prefix('[')
        .and_then(|rest| rest.split_once(']').map(|(ipv6, _)| ipv6))
        .unwrap_or_else(|| host.split(':').next().unwrap_or(host))
}

fn percent_decode_lossy(input: &str) -> String {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            output.push((high << 4) | low);
            index += 3;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
#[path = "network/tests.rs"]
mod tests;
