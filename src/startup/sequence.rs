use std::{
    env,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use clap::Parser;
use thiserror::Error;
use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

use super::{StartupGateError, cli::Cli, gates::StartupGates};
use crate::{compositor::Compositor, config::Config, xwayland};

const LOG_FILE_ENV: &str = "TENSOR_LOG_FILE";
const LOG_BUFFERED_LINES: usize = 16 * 1024;

pub fn run() -> Result<(), StartupError> {
    let cli = Cli::parse();
    crate::signals::block_early().map_err(StartupError::SignalMask)?;
    let config_path = Config::resolve_path(cli.config);
    let config = Config::load_with_environment(&config_path)?;
    let logging = initialize_logging()?;
    if let Some(path) = logging.file_path() {
        info!(path = %path.display(), "compositor file logging initialized");
    }

    if cli.session {
        xwayland::reject_x11_session()?;
    }

    info!(config = %config_path.display(), "configuration loaded");
    let systemd_active = resolve_systemd_integration(cli.check, cli.session, config.systemd)?;
    let mut compositor = Compositor::new(config)?;
    let mut gates = StartupGates::new(cli.check, cli.session, systemd_active);

    if cli.check {
        compositor.check_ready();
        info!("startup check completed");
    } else {
        compositor.prepare_runtime()?;
        gates.mark_runtime_prepared();
        compositor.check_ready();
        #[cfg(feature = "systemd")]
        let mut _manager_environment = None;
        if cli.session {
            let environment = compositor.publish_session_environment()?;
            gates.mark_process_environment_published()?;
            #[cfg(feature = "systemd")]
            if systemd_active {
                _manager_environment = Some(crate::service::import_environment(&environment)?);
                gates.mark_manager_environment_published()?;
            }
            #[cfg(not(feature = "systemd"))]
            let _ = environment;
        }
        #[cfg(feature = "systemd")]
        if systemd_active {
            crate::service::notify_ready().map_err(StartupError::SystemdNotify)?;
        }
        gates.mark_readiness_published()?;
        if let Some(permit) = gates.authorize_autostart()? {
            compositor.spawn_startup_commands(permit);
        }
        info!("entering compositor event loop");
        compositor.run()?;
    }

    Ok(())
}

fn resolve_systemd_integration(
    check: bool,
    session: bool,
    mode: crate::service::SystemdMode,
) -> Result<bool, StartupError> {
    if check || !session {
        return Ok(false);
    }
    if mode == crate::service::SystemdMode::Enabled && !cfg!(feature = "systemd") {
        return Err(StartupError::SystemdUnavailable);
    }
    Ok(cfg!(feature = "systemd") && mode.active())
}

fn initialize_logging() -> Result<LoggingGuard, StartupError> {
    let filter = EnvFilter::builder()
        .with_default_directive("tensor_compositor=info".parse().unwrap())
        .from_env_lossy();
    let Some(path) = env::var_os(LOG_FILE_ENV).map(PathBuf::from) else {
        tracing_subscriber::fmt()
            .compact()
            .with_writer(io::stderr)
            .with_env_filter(filter)
            .try_init()
            .map_err(|error| StartupError::Logging(error.to_string()))?;
        return Ok(LoggingGuard::Stderr);
    };

    let file = open_log_file(&path)?;
    // Rendering and event dispatch must never wait for terminal I/O or a slow
    // filesystem. Normal logging is intentionally quiet; a diagnostic debug
    // burst is best-effort once this bounded queue is full.
    let (writer, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .buffered_lines_limit(LOG_BUFFERED_LINES)
        .lossy(true)
        .finish(file);
    tracing_subscriber::fmt()
        .compact()
        .with_ansi(false)
        .with_writer(writer)
        .with_env_filter(filter)
        .try_init()
        .map_err(|error| StartupError::Logging(error.to_string()))?;
    Ok(LoggingGuard::File {
        _guard: guard,
        path,
    })
}

fn open_log_file(path: &Path) -> Result<File, StartupError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|source| StartupError::LogFile {
            path: path.to_owned(),
            source,
        })?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| StartupError::LogFile {
            path: path.to_owned(),
            source,
        })
}

enum LoggingGuard {
    Stderr,
    File { _guard: WorkerGuard, path: PathBuf },
}

impl LoggingGuard {
    fn file_path(&self) -> Option<&Path> {
        match self {
            Self::Stderr => None,
            Self::File { path, .. } => Some(path),
        }
    }
}

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("failed to block compositor termination signals: {0}")]
    SignalMask(io::Error),
    #[error("failed to initialize logging: {0}")]
    Logging(String),
    #[error("could not open compositor log file {path}: {source}")]
    LogFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
    #[error(transparent)]
    Compositor(#[from] crate::compositor::CompositorError),
    #[error(transparent)]
    XWayland(#[from] crate::xwayland::XWaylandError),
    #[error(transparent)]
    Gate(#[from] StartupGateError),
    #[error("systemd integration is enabled but support is not compiled in")]
    SystemdUnavailable,
    #[cfg(feature = "systemd")]
    #[error(transparent)]
    Systemd(#[from] crate::service::SystemdError),
    #[cfg(feature = "systemd")]
    #[error("failed to notify systemd: {0}")]
    SystemdNotify(std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::service::SystemdMode;

    static LOG_TEST_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn file_logging_creates_parent_and_appends() {
        let root = std::env::temp_dir().join(format!(
            "tensor-log-test-{}-{}",
            std::process::id(),
            LOG_TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("nested").join("tensor.log");

        use std::io::Write as _;
        let mut first = open_log_file(&path).unwrap();
        first.write_all(b"first\n").unwrap();
        drop(first);
        let mut second = open_log_file(&path).unwrap();
        second.write_all(b"second\n").unwrap();
        drop(second);

        assert_eq!(fs::read_to_string(&path).unwrap(), "first\nsecond\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn systemd_is_inactive_outside_session_mode() {
        assert!(!resolve_systemd_integration(false, false, SystemdMode::Enabled).unwrap());
    }

    #[test]
    fn check_mode_does_not_activate_systemd() {
        assert!(!resolve_systemd_integration(true, true, SystemdMode::Enabled).unwrap());
    }

    #[cfg(not(feature = "systemd"))]
    #[test]
    fn active_systemd_policy_requires_compiled_support() {
        assert!(matches!(
            resolve_systemd_integration(false, true, SystemdMode::Enabled),
            Err(StartupError::SystemdUnavailable)
        ));
    }

    #[cfg(not(feature = "systemd"))]
    #[test]
    fn auto_mode_stays_direct_without_compiled_support() {
        assert!(!resolve_systemd_integration(false, true, SystemdMode::Auto).unwrap());
    }

    #[cfg(feature = "systemd")]
    #[test]
    fn explicit_systemd_policy_activates_in_session_mode() {
        assert!(resolve_systemd_integration(false, true, SystemdMode::Enabled).unwrap());
    }
}
