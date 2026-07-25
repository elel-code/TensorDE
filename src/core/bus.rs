use async_io::Timer;
use async_lock::Mutex;
use futures_lite::future::FutureExt;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

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
    Proxy {
        target: Box<BusCallTarget>,
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
            Self::Proxy { target, message } => {
                write!(f, "cannot create D-Bus proxy for {target}: {message}")
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

#[derive(Debug)]
pub struct BusController {
    config: BusConfig,
    session: Mutex<Option<CachedBusConnection>>,
    system: Mutex<Option<CachedBusConnection>>,
}

#[derive(Debug)]
struct CachedBusConnection {
    connection: zbus::Connection,
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

    pub fn shared() -> &'static Self {
        static CONTROLLER: OnceLock<BusController> = OnceLock::new();
        CONTROLLER.get_or_init(Self::default)
    }

    pub fn config(&self) -> &BusConfig {
        &self.config
    }

    pub async fn connection(&self, kind: BusKind) -> Result<zbus::Connection, BusError> {
        let now = Instant::now();
        let mut guard = self.cache(kind).lock().await;
        if let Some(cached) = guard.as_mut()
            && !bus_connection_expired(cached.last_used, now, self.config.idle_timeout)
        {
            cached.last_used = now;
            return Ok(cached.connection.clone());
        }

        let connection = match kind {
            BusKind::Session => zbus::Connection::session().await,
            BusKind::System => zbus::Connection::system().await,
        }
        .map_err(|err| BusError::Connect {
            kind,
            message: err.to_string(),
        })?;
        *guard = Some(CachedBusConnection {
            connection: connection.clone(),
            last_used: now,
        });
        Ok(connection)
    }

    pub async fn proxy(&self, target: &BusCallTarget) -> Result<zbus::Proxy<'static>, BusError> {
        let connection = self.connection(target.kind()).await?;
        zbus::Proxy::new_owned(
            connection,
            target.service().to_string(),
            target.path().to_string(),
            target.interface().to_string(),
        )
        .await
        .map_err(|err| BusError::Proxy {
            target: Box::new(target.clone()),
            message: err.to_string(),
        })
    }

    pub async fn call_with_retry<T, F, Fut>(
        &self,
        target: &BusCallTarget,
        mut call: F,
    ) -> Result<T, BusError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, zbus::Error>>,
    {
        let attempts = self.config.retry_attempts.max(1);
        let mut last_error = None;
        for attempt in 0..attempts {
            enum AttemptOutcome<T> {
                Value(T),
                CallError(zbus::Error),
                TimedOut,
            }
            let result = async {
                match call().await {
                    Ok(value) => AttemptOutcome::Value(value),
                    Err(error) => AttemptOutcome::CallError(error),
                }
            }
            .or(async {
                Timer::after(self.config.call_timeout).await;
                AttemptOutcome::TimedOut
            })
            .await;
            match result {
                AttemptOutcome::Value(value) => return Ok(value),
                AttemptOutcome::TimedOut => {
                    last_error = Some(BusError::Timeout {
                        target: Box::new(target.clone()),
                        timeout: self.config.call_timeout,
                    });
                }
                AttemptOutcome::CallError(error) => {
                    last_error = Some(BusError::Call {
                        target: Box::new(target.clone()),
                        message: error.to_string(),
                    });
                }
            }
            if attempt + 1 < attempts && !self.config.retry_backoff.is_zero() {
                Timer::after(self.config.retry_backoff).await;
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
    use std::sync::atomic::{AtomicUsize, Ordering};

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
    fn call_with_retry_retries_until_success() {
        let controller = BusController::new(BusConfig {
            retry_attempts: 3,
            retry_backoff: Duration::ZERO,
            call_timeout: Duration::from_secs(1),
            idle_timeout: Duration::from_secs(30),
        });
        let target = BusCallTarget::new(
            BusKind::Session,
            "org.example.Service",
            "/org/example",
            "org.example.Service",
            "Run",
        )
        .unwrap();
        let attempts = AtomicUsize::new(0);

        let value = futures_lite::future::block_on(controller.call_with_retry(&target, || {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            async move {
                if attempt < 2 {
                    Err(zbus::Error::Failure("not yet".to_string()))
                } else {
                    Ok("done")
                }
            }
        }))
        .unwrap();

        assert_eq!(value, "done");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn call_with_retry_works_without_dedicated_reactor_thread() {
        let controller = BusController::new(BusConfig {
            retry_attempts: 1,
            retry_backoff: Duration::ZERO,
            call_timeout: Duration::from_secs(1),
            idle_timeout: Duration::from_secs(30),
        });
        let target = BusCallTarget::new(
            BusKind::Session,
            "org.example.Service",
            "/org/example",
            "org.example.Service",
            "Run",
        )
        .unwrap();

        let value = futures_lite::future::block_on(
            controller.call_with_retry(&target, || async { Ok::<_, zbus::Error>("done") }),
        )
        .unwrap();

        assert_eq!(value, "done");
    }

    #[test]
    fn call_with_retry_timeout_wakes_via_async_io_timer() {
        let controller = BusController::new(BusConfig {
            retry_attempts: 1,
            retry_backoff: Duration::ZERO,
            call_timeout: Duration::from_millis(10),
            idle_timeout: Duration::from_secs(30),
        });
        let target = BusCallTarget::new(
            BusKind::Session,
            "org.example.Service",
            "/org/example",
            "org.example.Service",
            "Run",
        )
        .unwrap();

        let started = Instant::now();
        let error = futures_lite::future::block_on(controller.call_with_retry(&target, || async {
            std::future::pending::<Result<&'static str, zbus::Error>>().await
        }))
        .unwrap_err();

        assert!(matches!(error, BusError::Timeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn call_with_retry_reports_timeout_after_attempts() {
        let controller = BusController::new(BusConfig {
            retry_attempts: 2,
            retry_backoff: Duration::ZERO,
            call_timeout: Duration::from_millis(1),
            idle_timeout: Duration::from_secs(30),
        });
        let target = BusCallTarget::new(
            BusKind::Session,
            "org.example.Service",
            "/org/example",
            "org.example.Service",
            "Run",
        )
        .unwrap();

        let error = futures_lite::future::block_on(controller.call_with_retry::<(), _, _>(
            &target,
            || async {
                Timer::after(Duration::from_millis(20)).await;
                Ok(())
            },
        ))
        .unwrap_err();

        assert!(matches!(error, BusError::Timeout { .. }));
    }
}
