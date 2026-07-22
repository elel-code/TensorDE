use std::io;

use clap::Parser;
use thiserror::Error;
use tracing::info;
use tracing_subscriber::EnvFilter;

use super::cli::Cli;
use crate::{compositor::Compositor, config::Config};

pub fn run() -> Result<(), StartupError> {
    let cli = Cli::parse();
    let config_path = Config::resolve_path(cli.config);
    let config = Config::load_with_environment(&config_path)?;
    initialize_logging()?;

    info!(config = %config_path.display(), "configuration loaded");
    let mut compositor = Compositor::new(config)?;
    compositor.check_ready();

    if cli.check {
        info!("startup check completed");
    } else {
        info!(
            "event loop handoff is not enabled in the bootstrap binary; no readiness notification sent"
        );
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
}
