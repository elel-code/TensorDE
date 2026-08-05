use async_lock::Mutex;
use futures_lite::future::FutureExt;
use serde::{Serialize, de::DeserializeOwned};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tensor_dbus::{
    Connection,
    zvariant::{DynamicType, Type},
};

const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_RETRY_ATTEMPTS: usize = 3;
const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BusKind {
    Session,
    System,
}

impl fmt::Display for BusKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session => write!(f, "session"),
            Self::System => write!(f, "system"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusCallTarget {
    kind: BusKind,
    service: String,
    path: String,
    interface: String,
    method: String,
}

impl BusCallTarget {
    pub fn new(
        kind: BusKind,
        service: impl Into<String>,
        path: impl Into<String>,
        interface: impl Into<String>,
        method: impl Into<String>,
    ) -> Result<Self, BusError> {
        let target = Self {
            kind,
            service: service.into(),
            path: path.into(),
            interface: interface.into(),
            method: method.into(),
        };
        validate_bus_name("service", &target.service)?;
        validate_object_path(&target.path)?;
        validate_dotted_name("interface", &target.interface)?;
        validate_member_name(&target.method)?;
        Ok(target)
    }

    pub fn kind(&self) -> BusKind {
        self.kind
    }

    pub fn service(&self) -> &str {
        &self.service
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn interface(&self) -> &str {
        &self.interface
    }

    pub fn method(&self) -> &str {
        &self.method
    }
}

impl fmt::Display for BusCallTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} bus {} {}.{} at {}",
            self.kind, self.service, self.interface, self.method, self.path
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusConfig {
    pub idle_timeout: Duration,
    pub call_timeout: Duration,
    pub retry_attempts: usize,
    pub retry_backoff: Duration,
}

impl Default for BusConfig {
    fn default() -> Self {
        Self {
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            call_timeout: DEFAULT_CALL_TIMEOUT,
            retry_attempts: DEFAULT_RETRY_ATTEMPTS,
            retry_backoff: DEFAULT_RETRY_BACKOFF,
        }
    }
}

#[derive(Debug)]
pub enum BusError {
    InvalidTarget {
        field: &'static str,
        value: String,
        message: String,
    },
    Connect {
        kind: BusKind,
        message: String,
    },
    Call {
        target: Box<BusCallTarget>,
        message: String,
    },
    Timeout {
        target: Box<BusCallTarget>,
        timeout: Duration,
    },
}

impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTarget {
                field,
                value,
                message,
            } => write!(f, "invalid D-Bus {field} {value:?}: {message}"),
            Self::Connect { kind, message } => {
                write!(f, "cannot connect to {kind} D-Bus: {message}")
            }
            Self::Call { target, message } => {
                write!(f, "D-Bus call failed for {target}: {message}")
            }
            Self::Timeout { target, timeout } => {
                write!(f, "D-Bus call timed out after {:?} for {target}", timeout)
            }
        }
    }
}

impl Error for BusError {}

pub struct BusController {
    config: BusConfig,
    session: Mutex<Option<CachedBusConnection>>,
    system: Mutex<Option<CachedBusConnection>>,
}

struct CachedBusConnection {
    connection: Connection,
    last_used: Instant,
}

impl Default for BusController {
    fn default() -> Self {
        Self::new(BusConfig::default())
    }
}

impl BusController {
    pub fn new(config: BusConfig) -> Self {
        Self {
            config,
            session: Mutex::new(None),
            system: Mutex::new(None),
        }
    }

    /// Returns the controller local to the calling operation-runtime thread.
    ///
    /// Calls must be awaited on that same Compio runtime because the cached
    /// connection is completion-driver affine.
    pub fn shared() -> Rc<Self> {
        thread_local! {
            static CONTROLLER: Rc<BusController> = Rc::new(BusController::default());
        }
        CONTROLLER.with(Rc::clone)
    }

    pub fn config(&self) -> &BusConfig {
        &self.config
    }

    /// Calls a method on the caller's current Compio runtime.
    pub async fn call<B, R>(&self, target: &BusCallTarget, body: &B) -> Result<R, BusError>
    where
        B: ?Sized + Serialize + DynamicType,
        R: DeserializeOwned + Type,
    {
        let attempts = self.config.retry_attempts.max(1);
        let mut last_error = None;
        for attempt in 0..attempts {
            let now = Instant::now();
            let mut guard = self.cache(target.kind()).lock().await;
            if guard.as_ref().is_some_and(|cached| {
                bus_connection_expired(cached.last_used, now, self.config.idle_timeout)
            }) {
                *guard = None;
            }
            if guard.is_none() {
                let connection = match target.kind() {
                    BusKind::Session => Connection::session_bus().await,
                    BusKind::System => Connection::system_bus().await,
                }
                .map_err(|error| BusError::Connect {
                    kind: target.kind(),
                    message: error.to_string(),
                })?;
                *guard = Some(CachedBusConnection {
                    connection,
                    last_used: now,
                });
            }
            let cached = guard.as_mut().expect("bus connection was installed");
            cached.last_used = now;
            let result = match cached
                .connection
                .send_call::<_, R>(
                    target.service(),
                    target.path(),
                    target.interface(),
                    target.method(),
                    body,
                )
                .await
            {
                Ok(pending) => {
                    match with_timeout(
                        pending.wait_message(&mut cached.connection),
                        self.config.call_timeout,
                    )
                    .await
                    {
                        Some(Ok(message)) => Some(pending.decode(message)),
                        Some(Err(error)) => Some(Err(error)),
                        None => {
                            if let Err(error) = pending.abandon(&mut cached.connection) {
                                Some(Err(error))
                            } else {
                                None
                            }
                        }
                    }
                }
                Err(error) => Some(Err(error)),
            };
            match result {
                Some(Ok(value)) => return Ok(value),
                None => {
                    *guard = None;
                    last_error = Some(BusError::Timeout {
                        target: Box::new(target.clone()),
                        timeout: self.config.call_timeout,
                    });
                }
                Some(Err(error)) => {
                    *guard = None;
                    last_error = Some(BusError::Call {
                        target: Box::new(target.clone()),
                        message: error.to_string(),
                    });
                }
            }
            drop(guard);
            if attempt + 1 < attempts && !self.config.retry_backoff.is_zero() {
                compio::time::sleep(self.config.retry_backoff).await;
            }
        }
        Err(last_error.unwrap_or_else(|| BusError::Call {
            target: Box::new(target.clone()),
            message: "D-Bus call was not attempted".to_string(),
        }))
    }

    fn cache(&self, kind: BusKind) -> &Mutex<Option<CachedBusConnection>> {
        match kind {
            BusKind::Session => &self.session,
            BusKind::System => &self.system,
        }
    }
}

async fn with_timeout<T>(future: impl Future<Output = T>, timeout: Duration) -> Option<T> {
    async { Some(future.await) }
        .or(async {
            compio::time::sleep(timeout).await;
            None
        })
        .await
}

fn bus_connection_expired(last_used: Instant, now: Instant, idle_timeout: Duration) -> bool {
    now.duration_since(last_used) >= idle_timeout
}

fn validate_bus_name(field: &'static str, value: &str) -> Result<(), BusError> {
    if value.is_empty() || value.len() > 255 {
        return Err(invalid_target(
            field,
            value,
            "bus names must be 1..=255 bytes",
        ));
    }
    if value.starts_with(':') {
        return validate_unique_bus_name(field, value);
    }
    validate_dotted_name(field, value)
}

fn validate_unique_bus_name(field: &'static str, value: &str) -> Result<(), BusError> {
    let rest = value
        .strip_prefix(':')
        .ok_or_else(|| invalid_target(field, value, "unique bus names must start with ':'"))?;
    if rest.is_empty() {
        return Err(invalid_target(
            field,
            value,
            "unique bus names need a non-empty suffix",
        ));
    }
    for part in rest.split('.') {
        if part.is_empty()
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(invalid_target(
                field,
                value,
                "unique bus name parts must be alphanumeric, '_' or '-'",
            ));
        }
    }
    Ok(())
}

fn validate_dotted_name(field: &'static str, value: &str) -> Result<(), BusError> {
    if value.is_empty()
        || value.len() > 255
        || !value.contains('.')
        || value.starts_with('.')
        || value.ends_with('.')
    {
        return Err(invalid_target(
            field,
            value,
            "well-known names must contain non-empty dot-separated parts",
        ));
    }
    for part in value.split('.') {
        let Some(first) = part.as_bytes().first().copied() else {
            return Err(invalid_target(field, value, "empty name part"));
        };
        if first.is_ascii_digit()
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(invalid_target(
                field,
                value,
                "name parts must start with a non-digit and contain alphanumeric or '_'",
            ));
        }
    }
    Ok(())
}

fn validate_object_path(value: &str) -> Result<(), BusError> {
    if value == "/" {
        return Ok(());
    }
    if !value.starts_with('/') || value.ends_with('/') || value.contains("//") {
        return Err(invalid_target(
            "path",
            value,
            "object paths must start with '/', avoid '//', and not end with '/'",
        ));
    }
    for part in value.split('/').skip(1) {
        if part.is_empty()
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(invalid_target(
                "path",
                value,
                "object path parts must be alphanumeric or '_'",
            ));
        }
    }
    Ok(())
}

fn validate_member_name(value: &str) -> Result<(), BusError> {
    if value.is_empty() || value.len() > 255 {
        return Err(invalid_target(
            "method",
            value,
            "member names must be 1..=255 bytes",
        ));
    }
    let Some(first) = value.as_bytes().first().copied() else {
        return Err(invalid_target("method", value, "empty member name"));
    };
    if first.is_ascii_digit()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(invalid_target(
            "method",
            value,
            "member names must start with a non-digit and contain alphanumeric or '_'",
        ));
    }
    Ok(())
}

fn invalid_target(field: &'static str, value: &str, message: &str) -> BusError {
    BusError::InvalidTarget {
        field,
        value: value.to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_call_target_validates_dbus_names_and_paths() {
        let target = BusCallTarget::new(
            BusKind::Session,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            "StartTransientUnit",
        )
        .unwrap();

        assert_eq!(target.kind(), BusKind::Session);
        assert_eq!(target.service(), "org.freedesktop.systemd1");
        assert_eq!(target.path(), "/org/freedesktop/systemd1");
        assert_eq!(target.interface(), "org.freedesktop.systemd1.Manager");
        assert_eq!(target.method(), "StartTransientUnit");

        assert!(
            BusCallTarget::new(
                BusKind::Session,
                "org.example",
                "not/a/path",
                "org.example.Interface",
                "Run",
            )
            .is_err()
        );
        assert!(
            BusCallTarget::new(
                BusKind::Session,
                "org.example",
                "/org/example",
                "org.example.Interface",
                "1Invalid",
            )
            .is_err()
        );
    }

    #[test]
    fn bus_call_target_accepts_unique_bus_names_for_ark_dnd() {
        let target = BusCallTarget::new(
            BusKind::Session,
            ":1.245",
            "/DndExtract",
            "org.kde.ark.DndExtract",
            "extractSelectedFilesTo",
        )
        .unwrap();

        assert_eq!(target.service(), ":1.245");
    }

    #[test]
    fn bus_error_display_includes_target_context() {
        let target = BusCallTarget::new(
            BusKind::System,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "ListSessions",
        )
        .unwrap();

        let error = BusError::Timeout {
            target: Box::new(target),
            timeout: Duration::from_secs(2),
        };

        assert!(error.to_string().contains("system bus"));
        assert!(error.to_string().contains("ListSessions"));
    }

    #[test]
    fn bus_connection_expiry_uses_idle_timeout() {
        let now = Instant::now();
        let recent = now - Duration::from_secs(5);
        let stale = now - Duration::from_secs(31);

        assert!(!bus_connection_expired(
            recent,
            now,
            Duration::from_secs(30)
        ));
        assert!(bus_connection_expired(stale, now, Duration::from_secs(30)));
    }

    #[test]
    fn call_timeout_is_driven_by_the_compio_runtime() {
        let runtime = compio::runtime::RuntimeBuilder::new().build().unwrap();
        let started = Instant::now();
        let result = runtime.block_on(with_timeout(
            std::future::pending::<()>(),
            Duration::from_millis(10),
        ));

        assert_eq!(result, None);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
