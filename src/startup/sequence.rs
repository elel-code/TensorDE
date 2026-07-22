use std::io;

use clap::Parser;
use thiserror::Error;
use tracing::info;
use tracing_subscriber::EnvFilter;

use super::cli::Cli;
use crate::{compositor::Compositor, config::Config, xwayland};

pub fn run() -> Result<(), StartupError> {
    let cli = Cli::parse();
    let config_path = Config::resolve_path(cli.config);
    let config = Config::load_with_environment(&config_path)?;
    initialize_logging()?;

    if cli.session {
        xwayland::reject_x11_session()?;
    }

    info!(config = %config_path.display(), "configuration loaded");
    let mut compositor = Compositor::new(config)?;
    compositor.check_ready();

    if cli.check {
        info!("startup check completed");
    } else {
        compositor.prepare_runtime()?;
        let systemd_active = cli.session && compositor.systemd_mode().active();
        if systemd_active {
            #[cfg(feature = "systemd")]
            {
                let environment = compositor.session_environment()?;
                crate::service::import_environment(&environment)?;
            }
            #[cfg(not(feature = "systemd"))]
            tracing::warn!(
                mode = %compositor.systemd_mode().name(),
                "systemd session detected but support is not compiled in"
            );
        }
        #[cfg(feature = "systemd")]
        if systemd_active {
            crate::service::notify_ready().map_err(StartupError::SystemdNotify)?;
        }
        info!("entering compositor event loop");
        let run_result = compositor.run();
        #[cfg(feature = "systemd")]
        if systemd_active {
            if let Err(error) = crate::service::unset_environment() {
                tracing::warn!(%error, "failed to clear session environment");
            }
        }
        run_result?;
    }

    Ok(())
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
    #[error("failed to initialize logging: {0}")]
    Logging(String),
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
    #[error(transparent)]
    Compositor(#[from] crate::compositor::CompositorError),
    #[error(transparent)]
    XWayland(#[from] crate::xwayland::XWaylandError),
    #[cfg(feature = "systemd")]
    #[error(transparent)]
    Systemd(#[from] crate::service::SystemdError),
    #[cfg(feature = "systemd")]
    #[error("failed to notify systemd: {0}")]
    SystemdNotify(std::io::Error),
}
