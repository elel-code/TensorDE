//! Filesystem trigger for the bounded configuration reload worker.

use std::{
    env,
    path::{Path, PathBuf},
};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use thiserror::Error;
use tracing::warn;

use super::{ConfigReloadSubmitError, ConfigReloadSubmitter};

/// Request id reserved for filesystem-triggered reloads.
pub(crate) const WATCHER_RELOAD_REQUEST_ID: u64 = 0;

pub(crate) struct ConfigWatcher {
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    pub(crate) fn start(
        path: PathBuf,
        reload: ConfigReloadSubmitter,
    ) -> Result<Self, ConfigWatcherError> {
        let watched_path = absolute_watch_path(path)?;
        let root = nearest_existing_parent(&watched_path).ok_or_else(|| {
            ConfigWatcherError::NoExistingParent {
                path: watched_path.clone(),
            }
        })?;
        let event_target = watched_path.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let event = match event {
                Ok(event) => event,
                Err(error) => {
                    warn!(%error, "configuration filesystem watcher failed");
                    return;
                }
            };
            if !event_targets_config(&event, &event_target) {
                return;
            }
            match reload.submit(WATCHER_RELOAD_REQUEST_ID) {
                Ok(()) | Err(ConfigReloadSubmitError::QueueFull) => {}
                Err(ConfigReloadSubmitError::WorkerStopped) => {
                    warn!("configuration watcher cannot reach the reload worker");
                }
            }
        })
        .map_err(ConfigWatcherError::Create)?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|source| ConfigWatcherError::Watch {
                path: root.clone(),
                source,
            })?;
        Ok(Self { _watcher: watcher })
    }
}

fn absolute_watch_path(path: PathBuf) -> Result<PathBuf, ConfigWatcherError> {
    if path.is_absolute() {
        return Ok(path);
    }
    env::current_dir()
        .map(|current| current.join(&path))
        .map_err(|source| ConfigWatcherError::CurrentDirectory { path, source })
}

fn nearest_existing_parent(path: &Path) -> Option<PathBuf> {
    let mut candidate = path.parent()?;
    loop {
        if candidate.is_dir() {
            return Some(candidate.to_owned());
        }
        candidate = candidate.parent()?;
    }
}

fn event_targets_config(event: &Event, target: &Path) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    event
        .paths
        .iter()
        .any(|changed| changed == target || target.starts_with(changed))
}

#[derive(Debug, Error)]
pub(crate) enum ConfigWatcherError {
    #[error("failed to resolve relative configuration path {path}: {source}")]
    CurrentDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("configuration path {path} has no existing parent to watch")]
    NoExistingParent { path: PathBuf },
    #[error("failed to create configuration watcher: {0}")]
    Create(#[source] notify::Error),
    #[error("failed to watch configuration root {path}: {source}")]
    Watch {
        path: PathBuf,
        source: notify::Error,
    },
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use tensor_runtime::WorkerBridge;

    use super::*;
    use crate::config::{ConfigReloadWorker, MAX_PENDING_CONFIG_RELOAD_RESULTS};

    #[test]
    fn nearest_existing_root_supports_a_missing_config_directory() {
        let root = std::env::temp_dir().join(format!(
            "tensor-config-watch-root-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let target = root.join("tensor/config.kdl");

        assert_eq!(nearest_existing_parent(&target), Some(root.clone()));

        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn relative_target_uses_an_absolute_watch_root() {
        let watcher_path = absolute_watch_path(PathBuf::from("config.kdl")).unwrap();

        assert!(watcher_path.is_absolute());
        assert_eq!(watcher_path.file_name().unwrap(), "config.kdl");
        assert!(nearest_existing_parent(&watcher_path).is_some());
    }

    #[test]
    fn atomic_replacement_triggers_an_off_thread_candidate() {
        let root = std::env::temp_dir().join(format!(
            "tensor-config-watch-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let target = root.join("config.kdl");
        fs::write(&target, "layout \"scrolling-1d\"").unwrap();
        let (outcomes, receiver) = WorkerBridge::bounded(MAX_PENDING_CONFIG_RELOAD_RESULTS);
        let worker = ConfigReloadWorker::start(target.clone(), outcomes).unwrap();
        let watcher = ConfigWatcher::start(target.clone(), worker.submitter()).unwrap();

        let replacement = root.join("config.kdl.new");
        fs::write(&replacement, "layout \"spatial-2d\"").unwrap();
        fs::rename(&replacement, &target).unwrap();

        let outcome = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(outcome.request_id, WATCHER_RELOAD_REQUEST_ID);
        assert_eq!(
            outcome.candidate.unwrap().initial_layout,
            crate::layout::LayoutKind::Spatial2D
        );

        drop(watcher);
        drop(worker);
        fs::remove_dir_all(root).unwrap();
    }
}
