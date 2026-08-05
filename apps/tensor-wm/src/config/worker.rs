//! Bounded cold-path configuration loading.
//!
//! File I/O, KDL parsing, typed validation, and environment overrides stay on
//! this worker thread. Only a completed value crosses the Tensor worker bridge;
//! the compositor thread remains the sole owner of transactional commit and
//! live policy application.

use std::{
    io,
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

use tensor_runtime::{TrySendError as OutcomeSendError, WorkerTx};
use thiserror::Error;
use tracing::warn;

use super::{Config, ConfigError};

pub(crate) const MAX_PENDING_CONFIG_RELOAD_RESULTS: usize = 2;
const MAX_PENDING_CONFIG_RELOAD_REQUESTS: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ConfigReloadRequest {
    request_id: u64,
}

#[derive(Debug)]
enum WorkerRequest {
    Reload(ConfigReloadRequest),
    Stop,
}

#[derive(Debug)]
pub(crate) struct ConfigReloadOutcome {
    pub(crate) request_id: u64,
    pub(crate) candidate: Result<Config, ConfigError>,
}

#[derive(Clone, Debug)]
pub(crate) struct ConfigReloadSubmitter {
    requests: SyncSender<WorkerRequest>,
}

impl ConfigReloadSubmitter {
    pub(crate) fn submit(&self, request_id: u64) -> Result<(), ConfigReloadSubmitError> {
        match self
            .requests
            .try_send(WorkerRequest::Reload(ConfigReloadRequest { request_id }))
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(ConfigReloadSubmitError::QueueFull),
            Err(TrySendError::Disconnected(_)) => Err(ConfigReloadSubmitError::WorkerStopped),
        }
    }
}

pub(crate) struct ConfigReloadWorker {
    submitter: ConfigReloadSubmitter,
    thread: Option<JoinHandle<()>>,
}

impl ConfigReloadWorker {
    pub(crate) fn start(
        path: PathBuf,
        outcomes: WorkerTx<ConfigReloadOutcome>,
    ) -> Result<Self, ConfigReloadWorkerError> {
        let (requests, receiver) = mpsc::sync_channel(MAX_PENDING_CONFIG_RELOAD_REQUESTS);
        let thread = thread::Builder::new()
            .name("tensor-config-reload".to_owned())
            .spawn(move || run(path, receiver, outcomes))
            .map_err(ConfigReloadWorkerError::Spawn)?;
        Ok(Self {
            submitter: ConfigReloadSubmitter { requests },
            thread: Some(thread),
        })
    }

    pub(crate) fn submitter(&self) -> ConfigReloadSubmitter {
        self.submitter.clone()
    }
}

impl Drop for ConfigReloadWorker {
    fn drop(&mut self) {
        let _ = self.submitter.requests.send(WorkerRequest::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(path: PathBuf, requests: Receiver<WorkerRequest>, outcomes: WorkerTx<ConfigReloadOutcome>) {
    while let Ok(request) = requests.recv() {
        let WorkerRequest::Reload(request) = request else {
            return;
        };
        let outcome = ConfigReloadOutcome {
            request_id: request.request_id,
            candidate: Config::load_required_with_environment(&path),
        };
        match outcomes.try_send(outcome) {
            Ok(()) => {}
            Err(OutcomeSendError::Full) => {
                warn!("configuration reload result queue saturated; dropping completion");
            }
            Err(OutcomeSendError::Disconnected) => return,
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(crate) enum ConfigReloadSubmitError {
    #[error("configuration reload queue is full")]
    QueueFull,
    #[error("configuration reload worker has stopped")]
    WorkerStopped,
}

#[derive(Debug, Error)]
pub(crate) enum ConfigReloadWorkerError {
    #[error("failed to spawn configuration reload worker: {0}")]
    Spawn(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use tensor_runtime::WorkerBridge;

    use super::*;

    #[test]
    fn request_queue_is_fixed_and_reports_saturation() {
        let (requests, _receiver) = mpsc::sync_channel(1);
        let submitter = ConfigReloadSubmitter { requests };

        assert_eq!(submitter.submit(1), Ok(()));
        assert_eq!(submitter.submit(2), Err(ConfigReloadSubmitError::QueueFull));
    }

    #[test]
    fn worker_loads_a_complete_candidate_off_thread() {
        let path = std::env::temp_dir().join(format!(
            "tensor-config-worker-{}-{}.kdl",
            std::process::id(),
            line!()
        ));
        fs::write(&path, "layout \"spatial-2d\"").unwrap();
        let (outcomes, receiver) = WorkerBridge::bounded(MAX_PENDING_CONFIG_RELOAD_RESULTS);
        let worker = ConfigReloadWorker::start(path.clone(), outcomes).unwrap();

        worker.submitter().submit(41).unwrap();
        let outcome = receiver.recv_timeout(Duration::from_secs(1)).unwrap();

        assert_eq!(outcome.request_id, 41);
        assert_eq!(
            outcome.candidate.unwrap().initial_layout,
            crate::layout::LayoutKind::Spatial2D
        );
        drop(worker);
        fs::remove_file(path).unwrap();
    }
}
