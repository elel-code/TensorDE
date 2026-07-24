use std::io;

use clap::Parser;
use thiserror::Error;
use tracing::info;
use tracing_subscriber::EnvFilter;

use super::{StartupGateError, cli::Cli, gates::StartupGates};
use crate::{compositor::Compositor, config::Config, xwayland};

pub fn run() -> Result<(), StartupError> {
    let cli = Cli::parse();
    crate::signals::block_early().map_err(StartupError::SignalMask)?;
    let config_path = Config::resolve_path(cli.config);
    let config = Config::load_with_environment(&config_path)?;
    initialize_logging()?;

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

fn initialize_logging() -> Result<(), StartupError> {
    let filter = EnvFilter::builder()
        .with_default_directive("tensor_compositor=info".parse().unwrap())
        .from_env_lossy();
    tracing_subscriber::fmt()
        .compact()
        .with_writer(io::stderr)
        .with_env_filter(filter)
        .try_init()
        .map_err(|error| StartupError::Logging(error.to_string()))
}

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("failed to block compositor termination signals: {0}")]
    SignalMask(io::Error),
    #[error("failed to initialize logging: {0}")]
    Logging(String),
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
    use super::*;
    use crate::service::SystemdMode;

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
