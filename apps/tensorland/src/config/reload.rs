//! Transactional configuration replacement for the reload completion worker.
//!
//! Parsing and file I/O remain cold-path operations. This state object only
//! commits a fully loaded candidate, so a watcher failure cannot partially
//! mutate compositor policy.

use std::path::{Path, PathBuf};

use super::{Config, ConfigDiagnosticMetadata, ConfigError, diagnostic::metadata_for_config_error};

#[derive(Debug)]
pub struct ConfigTransaction {
    path: PathBuf,
    active: Config,
    generation: u64,
    last_failure: Option<ConfigDiagnosticMetadata>,
}

impl ConfigTransaction {
    pub fn new(path: impl Into<PathBuf>, active: Config) -> Self {
        Self {
            path: path.into(),
            active,
            generation: 0,
            last_failure: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn active(&self) -> &Config {
        &self.active
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn last_failure(&self) -> Option<&ConfigDiagnosticMetadata> {
        self.last_failure.as_ref()
    }

    /// Load the configured path and atomically commit only a valid candidate.
    ///
    /// Synchronous helper for tests and tools. The compositor runtime uses the
    /// bounded reload worker and commits its completed candidate here.
    pub fn reload(&mut self) -> ConfigReloadResult {
        self.apply_candidate(Config::load_required_with_environment(&self.path))
    }

    pub fn apply_candidate(
        &mut self,
        candidate: Result<Config, ConfigError>,
    ) -> ConfigReloadResult {
        match candidate {
            Ok(candidate) => {
                self.active = candidate;
                self.generation = self.generation.saturating_add(1);
                self.last_failure = None;
                ConfigReloadResult::Applied {
                    generation: self.generation,
                }
            }
            Err(error) => {
                let diagnostic = metadata_for_config_error(&self.path, &error);
                self.last_failure = Some(diagnostic.clone());
                ConfigReloadResult::Rejected(Box::new(ConfigReloadFailure {
                    generation: self.generation,
                    diagnostic,
                    error,
                }))
            }
        }
    }

    pub(crate) fn reject_restart_required(&mut self, field: &'static str) -> ConfigReloadResult {
        self.apply_candidate(Err(ConfigError::ReloadRequiresRestart { field }))
    }
}

#[derive(Debug)]
pub enum ConfigReloadResult {
    Applied { generation: u64 },
    Rejected(Box<ConfigReloadFailure>),
}

#[derive(Debug)]
pub struct ConfigReloadFailure {
    pub generation: u64,
    pub diagnostic: ConfigDiagnosticMetadata,
    pub error: ConfigError,
}

impl ConfigReloadResult {
    pub const fn generation(&self) -> u64 {
        match self {
            Self::Applied { generation } => *generation,
            Self::Rejected(failure) => failure.generation,
        }
    }

    pub const fn applied(&self) -> bool {
        matches!(self, Self::Applied { .. })
    }
}
