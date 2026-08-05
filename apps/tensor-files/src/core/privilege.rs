use super::bus::BusKind;
use super::file_ops;
use super::network::is_network_path;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tensor_dbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use tensor_dbus::{
    BusAddress, Connection, Message, MethodCall, MethodError, MethodResult, Proxy,
    RequestNameFlags, RequestNameReply, reply_method, reply_method_error, reply_method_result,
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

const SERVICE_NAME: &str = "org.tensorde.TensorFiles1.Privileged";
const OBJECT_PATH: &str = "/org/tensorde/TensorFiles1/Privileged";
const SERVICE_INTERFACE: &str = "org.tensorde.TensorFiles1.Privileged";
const ACTION_ID: &str = "org.tensorde.TensorFiles.privileged-helper";
const POLICY_FILE: &str = "org.tensorde.TensorFiles.policy";
const HELPER_IDLE_SECONDS: u64 = 180;
const EXTERNAL_EDIT_TTL_SECONDS: u64 = 24 * 60 * 60;
static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub enum HelperBus {
    System,
    Session { session_bus_address: Option<String> },
}

#[derive(Clone)]
enum ServiceMode {
    System,
    SessionPkexec { allowed_uid: u32 },
}

#[derive(Clone, Debug)]
pub enum PrivilegedCommand {
    CreateFolder {
        parent: PathBuf,
        name: String,
    },
    CreateFile {
        parent: PathBuf,
        name: String,
    },
    Rename {
        path: PathBuf,
        new_name: String,
    },
    Trash {
        paths: Vec<PathBuf>,
    },
    Transfer {
        operation: String,
        source: PathBuf,
        target_dir: PathBuf,
    },
}

#[derive(Debug)]
pub struct PrivilegedOperationResult {
    pub label: String,
    pub affected_dirs: Vec<PathBuf>,
    pub result: Result<String, String>,
}

impl PrivilegedCommand {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CreateFolder { .. } => "Create folder",
            Self::CreateFile { .. } => "Create file",
            Self::Rename { .. } => "Rename",
            Self::Trash { .. } => "Move to Trash",
            Self::Transfer { operation, .. } => match operation.as_str() {
                "move" => "Move",
                "copy" => "Copy",
                "link" => "Link",
                _ => "File operation",
            },
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::CreateFolder { parent, name } => {
                format!("Create '{name}' in {}", parent.display())
            }
            Self::CreateFile { parent, name } => {
                format!("Create file '{name}' in {}", parent.display())
            }
            Self::Rename { path, new_name } => {
                format!("Rename {} to '{new_name}'", path.display())
            }
            Self::Trash { paths } => match paths.as_slice() {
                [path] => format!("Move {} to Trash", path.display()),
                _ => format!("Move {} items to Trash", paths.len()),
            },
            Self::Transfer {
                operation,
                source,
                target_dir,
            } => {
                let verb = match operation.as_str() {
                    "move" => "Move",
                    "copy" => "Copy",
                    "link" => "Link",
                    _ => "Transfer",
                };
                format!("{verb} {} to {}", source.display(), target_dir.display())
            }
        }
    }

    pub fn affected_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        match self {
            Self::CreateFolder { parent, .. } | Self::CreateFile { parent, .. } => {
                dirs.push(parent.clone());
            }
            Self::Rename { path, .. } => {
                if let Some(parent) = path.parent() {
                    dirs.push(parent.to_path_buf());
                }
            }
            Self::Trash { paths } => {
                for path in paths {
                    if let Some(parent) = path.parent() {
                        push_unique(&mut dirs, parent.to_path_buf());
                    }
                }
            }
            Self::Transfer {
                source, target_dir, ..
            } => {
                dirs.push(target_dir.clone());
                if let Some(parent) = source.parent() {
                    push_unique(&mut dirs, parent.to_path_buf());
                }
            }
        }
        dirs
    }

    pub fn validate_local_paths(&self) -> Result<(), String> {
        match self {
            Self::CreateFolder { parent, .. } | Self::CreateFile { parent, .. } => {
                ensure_privileged_local_path(parent)
            }
            Self::Rename { path, .. } => ensure_privileged_local_path(path),
            Self::Trash { paths } => {
                for path in paths {
                    ensure_privileged_local_path(path)?;
                }
                Ok(())
            }
            Self::Transfer {
                source, target_dir, ..
            } => {
                ensure_privileged_local_path(source)?;
                ensure_privileged_local_path(target_dir)
            }
        }
    }
}

fn ensure_privileged_local_path(path: &Path) -> Result<(), String> {
    if is_network_path(path) {
        Err(format!(
            "network locations are not supported by the privileged helper: {}",
            path.display()
        ))
    } else {
        Ok(())
    }
}

pub async fn run_via_dbus(command: PrivilegedCommand) -> PrivilegedOperationResult {
    let label = command.label().to_string();
    let affected_dirs = command.affected_dirs();
    let result = match command.validate_local_paths() {
        Ok(()) => run_via_dbus_inner(&command).await.map(|message| {
            if message.is_empty() {
                "completed with administrator privileges".to_string()
            } else {
                message
            }
        }),
        Err(err) => Err(err),
    };
    PrivilegedOperationResult {
        label,
        affected_dirs,
        result,
    }
}

async fn run_via_dbus_inner(command: &PrivilegedCommand) -> Result<String, String> {
    match call_dbus_command_on_system_bus(command).await {
        Ok(message) => Ok(message),
        Err(system_error) => match call_dbus_command_on_session_bus(command).await {
            Ok(message) => Ok(message),
            Err(session_error) => {
                start_session_helper_and_call(command, system_error, session_error).await
            }
        },
    }
}

async fn start_session_helper_and_call(
    command: &PrivilegedCommand,
    system_error: String,
    session_error: String,
) -> Result<String, String> {
    let mut helper = start_dbus_helper().map_err(|err| {
        privileged_helper_start_failed_message(&system_error, &session_error, &err)
    })?;
    let wait_result = wait_for_service().await;
    if wait_result.is_err() {
        let _ = helper.try_wait();
    }
    wait_result.map_err(|err| {
        privileged_helper_start_failed_message(&system_error, &session_error, &err)
    })?;
    call_dbus_command_on_session_bus(command).await
}

async fn call_dbus_command_on_system_bus(command: &PrivilegedCommand) -> Result<String, String> {
    let mut connection = privileged_bus_connection(BusKind::System).await?;
    call_dbus_command(command, &mut connection).await
}

async fn call_dbus_command_on_session_bus(command: &PrivilegedCommand) -> Result<String, String> {
    let mut connection = privileged_bus_connection(BusKind::Session).await?;
    call_dbus_command(command, &mut connection).await
}

async fn call_dbus_command(
    command: &PrivilegedCommand,
    connection: &mut Connection,
) -> Result<String, String> {
    command.validate_local_paths()?;
    let mut proxy = Proxy::new(
        connection,
        Some(SERVICE_NAME),
        OBJECT_PATH,
        Some(SERVICE_INTERFACE),
    )
    .map_err(|err| format!("cannot create privileged helper proxy: {err}"))?;

    match command {
        PrivilegedCommand::CreateFolder { parent, name } => proxy
            .call(
                "CreateFolder",
                &(parent.display().to_string(), name.as_str()),
            )
            .await
            .map_err(|err| err.to_string()),
        PrivilegedCommand::CreateFile { parent, name } => proxy
            .call("CreateFile", &(parent.display().to_string(), name.as_str()))
            .await
            .map_err(|err| err.to_string()),
        PrivilegedCommand::Rename { path, new_name } => proxy
            .call("Rename", &(path.display().to_string(), new_name.as_str()))
            .await
            .map_err(|err| err.to_string()),
        PrivilegedCommand::Trash { paths } => proxy
            .call(
                "Trash",
                &(paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>(),),
            )
            .await
            .map_err(|err| err.to_string()),
        PrivilegedCommand::Transfer {
            operation,
            source,
            target_dir,
        } => proxy
            .call(
                "Transfer",
                &(
                    operation.as_str(),
                    source.display().to_string(),
                    target_dir.display().to_string(),
                ),
            )
            .await
            .map_err(|err| err.to_string()),
    }
}

async fn wait_for_service() -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(75);
    let mut connection = privileged_bus_connection(BusKind::Session).await?;
    let mut dbus = Proxy::new(
        &mut connection,
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
    )
    .map_err(|error| format!("cannot create D-Bus daemon proxy: {error}"))?;
    loop {
        if Instant::now() >= deadline {
            return Err("timed out waiting for privileged D-Bus helper".to_string());
        }
        let owner: tensor_dbus::Result<String> = dbus.call("GetNameOwner", &(SERVICE_NAME,)).await;
        if owner.is_ok() {
            return Ok(());
        }
        compio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn privileged_bus_connection(kind: BusKind) -> Result<Connection, String> {
    match kind {
        BusKind::Session => Connection::session_bus().await,
        BusKind::System => Connection::system_bus().await,
    }
    .map_err(|error| format!("cannot connect to {kind} D-Bus: {error}"))
}

fn start_dbus_helper() -> Result<Child, String> {
    let exe = helper_executable()?;
    let mut command = Command::new("pkexec");
    command.arg("--disable-internal-agent").arg(exe);
    if let Ok(address) = env::var("DBUS_SESSION_BUS_ADDRESS") {
        command.arg("--session-bus").arg(address);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("cannot start pkexec: {err}"))
}

fn helper_executable() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("TENSOR_FILES_PRIVILEGED_HELPER") {
        return Ok(PathBuf::from(path));
    }

    let exe = env::current_exe().map_err(|err| format!("cannot locate executable: {err}"))?;
    let Some(dir) = exe.parent() else {
        return Err(format!(
            "cannot locate helper executable next to {}",
            exe.display()
        ));
    };
    Ok(dir.join("tensor-files-privileged-helper"))
}

pub async fn run_dbus_service(bus: HelperBus) -> Result<(), String> {
    let (service_mode, session_bus_address) = match bus {
        HelperBus::System => (ServiceMode::System, None),
        HelperBus::Session {
            session_bus_address,
        } => {
            let allowed_uid = env::var("PKEXEC_UID")
                .ok()
                .and_then(|uid| uid.parse::<u32>().ok())
                .ok_or_else(|| "refusing to start session helper without PKEXEC_UID".to_string())?;
            (
                ServiceMode::SessionPkexec { allowed_uid },
                session_bus_address,
            )
        }
    };
    privileged_debug_log(&helper_lifecycle_summary(
        "starting",
        &service_mode,
        &session_bus_address,
        0,
        0,
    ));
    let service = PrivilegedService::new(service_mode, session_bus_address.clone());
    let service_monitor = service.clone();
    let bus_address = match bus_connection_address(&session_bus_address, &service_monitor.mode) {
        BusConnection::System => BusAddress::system(),
        BusConnection::SessionAddress(address) => BusAddress::parse(address),
        BusConnection::Session => BusAddress::session(),
    }
    .map_err(|err| format!("cannot resolve privileged helper D-Bus address: {err}"))?;
    let mut connection = Connection::connect_bus(bus_address.clone())
        .await
        .map_err(|err| format!("cannot connect privileged helper to D-Bus: {err}"))?;
    let ownership = connection
        .request_name(SERVICE_NAME, RequestNameFlags::DO_NOT_QUEUE)
        .await
        .map_err(|err| format!("cannot request privileged helper name: {err}"))?;
    if !matches!(
        ownership,
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner
    ) {
        return Err(format!(
            "cannot request privileged helper name: bus returned {ownership:?}"
        ));
    }
    compio::runtime::spawn(run_idle_waker(bus_address)).detach();

    loop {
        let call = connection
            .receive()
            .await
            .map_err(|err| format!("receive privileged helper D-Bus message: {err}"))?;
        service
            .dispatch(&mut connection, call)
            .await
            .map_err(|err| format!("dispatch privileged helper D-Bus method: {err}"))?;
        service.expire_stale_external_edits();
        if service.can_exit() {
            let (idle_for, active_edits) = service_monitor.exit_state();
            privileged_debug_log(&helper_lifecycle_summary(
                "exiting",
                &service_monitor.mode,
                &service_monitor.session_bus_address,
                idle_for,
                active_edits,
            ));
            break;
        }
    }
    Ok(())
}

async fn run_idle_waker(address: BusAddress) {
    let mut connection = match Connection::connect_bus(address).await {
        Ok(connection) => connection,
        Err(err) => {
            privileged_debug_log(&format!("cannot start helper idle waker: {err}"));
            return;
        }
    };
    loop {
        compio::time::sleep(Duration::from_secs(5)).await;
        let result: tensor_dbus::Result<()> = connection
            .call(
                Some(SERVICE_NAME),
                OBJECT_PATH,
                Some(SERVICE_INTERFACE),
                "CheckIdle",
                &(),
            )
            .await;
        if result.is_err() {
            return;
        }
    }
}

enum BusConnection<'a> {
    System,
    SessionAddress(&'a str),
    Session,
}

fn bus_connection_address<'a>(
    session_bus_address: &'a Option<String>,
    mode: &ServiceMode,
) -> BusConnection<'a> {
    match mode {
        ServiceMode::System => BusConnection::System,
        ServiceMode::SessionPkexec { .. } => session_bus_address
            .as_deref()
            .map(BusConnection::SessionAddress)
            .unwrap_or(BusConnection::Session),
    }
}

fn helper_lifecycle_summary(
    phase: &str,
    mode: &ServiceMode,
    session_bus_address: &Option<String>,
    idle_for: u64,
    active_edits: usize,
) -> String {
    let (mode_label, authorized_subject) = match mode {
        ServiceMode::System => ("system-bus", "polkit".to_string()),
        ServiceMode::SessionPkexec { allowed_uid } => {
            ("session-bus-pkexec", format!("uid:{allowed_uid}"))
        }
    };
    let bus_connection = match bus_connection_address(session_bus_address, mode) {
        BusConnection::System => "system",
        BusConnection::SessionAddress(_) => "provided-session",
        BusConnection::Session => "session",
    };
    format!(
        "phase={phase} mode={mode_label} bus_connection={bus_connection} authorized_subject={authorized_subject} session_address={} idle_for={} active_external_edits={active_edits}",
        session_bus_address.is_some(),
        idle_for
    )
}

fn privileged_debug_log(message: &str) {
    static DEBUG_PRIVILEGE: OnceLock<bool> = OnceLock::new();
    if *DEBUG_PRIVILEGE.get_or_init(|| {
        env::var("TENSOR_FILES_DEBUG_PRIVILEGE")
            .is_ok_and(|value| env_flag_is_truthy(value.as_str()))
    }) {
        eprintln!("[tensor-files privileged helper] {message}");
    }
}

fn env_flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[derive(Clone, Debug)]
struct ExternalEdit {
    original_path: PathBuf,
    scratch_path: PathBuf,
    original_len: u64,
    original_modified: Option<SystemTime>,
    unit: Option<String>,
    session_bus_address: Option<String>,
    created_secs: u64,
}

#[derive(Clone)]
struct PrivilegedService {
    mode: ServiceMode,
    external_edits: Arc<Mutex<std::collections::HashMap<String, ExternalEdit>>>,
    external_edit_watchers:
        Arc<Mutex<std::collections::HashMap<String, notify::RecommendedWatcher>>>,
    last_activity_secs: Arc<AtomicU64>,
    scratch_root_override: Option<PathBuf>,
    session_bus_address: Option<String>,
}

mod service;
#[cfg(test)]
use service::{
    cleanup_scratch_token_dir, is_finished_unit_state, polkit_authority_unavailable_message,
    polkit_check_failed_message, polkit_denied_message, sync_external_edit,
};
use service::{privileged_helper_start_failed_message, push_unique};
#[cfg(test)]
#[path = "privilege/tests.rs"]
mod tests;
