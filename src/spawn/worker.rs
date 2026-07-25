//! Bounded off-thread application launch execution.
//!
//! Process creation and transient-systemd scope setup can wait on fork setup,
//! the user D-Bus, and a systemd job. They therefore never execute from a
//! calloop callback. The compositor submits value-only requests, while this
//! worker returns value-only outcomes through a bounded calloop channel.

use std::{
    ffi::{OsStr, OsString},
    io,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

use smithay::reexports::calloop::channel::SyncSender as CalloopSender;
use thiserror::Error;
use tracing::warn;

use super::{ProcessLauncher, SpawnError, SpawnedProcess};

const MAX_PENDING_LAUNCHES: usize = 64;

/// A fully resolved command that is safe to move off the compositor thread.
#[derive(Clone, Debug)]
pub struct LaunchRequest {
    id: u64,
    program: OsString,
    args: Vec<OsString>,
}

impl LaunchRequest {
    pub fn new<I, S>(id: u64, program: impl Into<OsString>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        Self {
            id,
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn program(&self) -> &OsStr {
        &self.program
    }
}

/// Completion information delivered back to the calloop thread.
#[derive(Debug)]
pub struct LaunchOutcome {
    id: u64,
    program: OsString,
    result: Result<SpawnedProcess, SpawnError>,
}

impl LaunchOutcome {
    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn program(&self) -> &OsStr {
        &self.program
    }

    pub fn result(&self) -> Result<SpawnedProcess, &SpawnError> {
        self.result.as_ref().copied()
    }
}

/// Cloneable, value-only handle for queueing launches from calloop callbacks.
///
/// The worker thread owns process creation; this handle only performs a
/// non-blocking send of a fully resolved request.
#[derive(Clone, Debug)]
pub struct LaunchSubmitter {
    requests: SyncSender<LaunchRequest>,
}

impl LaunchSubmitter {
    /// Queue a command without waiting for process creation or systemd.
    pub fn submit(&self, request: LaunchRequest) -> Result<(), LaunchSubmitError> {
        let id = request.id;
        match self.requests.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(LaunchSubmitError::QueueFull { id }),
            Err(TrySendError::Disconnected(_)) => Err(LaunchSubmitError::WorkerStopped { id }),
        }
    }
}

/// Persistent process-launch worker.
pub struct LaunchWorker {
    submitter: LaunchSubmitter,
    _thread: JoinHandle<()>,
}

impl LaunchWorker {
    pub fn new(
        launcher: ProcessLauncher,
        outcomes: CalloopSender<LaunchOutcome>,
    ) -> Result<Self, LaunchWorkerError> {
        let (requests, receiver) = mpsc::sync_channel(MAX_PENDING_LAUNCHES);
        let thread = thread::Builder::new()
            .name("tensor-client-launch".to_owned())
            .spawn(move || run(launcher, receiver, outcomes))
            .map_err(LaunchWorkerError::Spawn)?;
        Ok(Self {
            submitter: LaunchSubmitter { requests },
            _thread: thread,
        })
    }

    /// Cloneable submit handle for IPC and other calloop-owned paths.
    pub fn submitter(&self) -> LaunchSubmitter {
        self.submitter.clone()
    }

    /// Queue a command without waiting for process creation or systemd.
    pub fn submit(&self, request: LaunchRequest) -> Result<(), LaunchSubmitError> {
        self.submitter.submit(request)
    }
}

fn run(
    launcher: ProcessLauncher,
    requests: Receiver<LaunchRequest>,
    outcomes: CalloopSender<LaunchOutcome>,
) {
    while let Ok(request) = requests.recv() {
        let LaunchRequest { id, program, args } = request;
        let result = launcher.spawn(program.as_os_str(), args.iter().map(OsString::as_os_str));
        let outcome = LaunchOutcome {
            id,
            program,
            result,
        };
        match outcomes.try_send(outcome) {
            Ok(()) => {}
            Err(TrySendError::Full(outcome)) => {
                warn!(
                    request_id = outcome.id,
                    program = ?outcome.program,
                    "launch outcome queue saturated; dropping completion"
                );
            }
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
}

#[derive(Debug, Error)]
pub enum LaunchSubmitError {
    #[error("launch queue is full for request {id}")]
    QueueFull { id: u64 },
    #[error("launch worker stopped before accepting request {id}")]
    WorkerStopped { id: u64 },
}

#[derive(Debug, Error)]
pub enum LaunchWorkerError {
    #[error("failed to create launch worker: {0}")]
    Spawn(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, thread, time::Duration};

    use smithay::reexports::calloop::channel::sync_channel;

    use super::*;
    use crate::service::SystemdMode;

    #[test]
    fn worker_returns_a_direct_launch_outcome() {
        let path = PathBuf::from(format!(
            "target/tensor-launch-worker-{}",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let (outcomes, receiver) = sync_channel(1);
        let worker = LaunchWorker::new(
            ProcessLauncher::with_systemd_detection(SystemdMode::Disabled, false),
            outcomes,
        )
        .unwrap();

        worker
            .submit(LaunchRequest::new(41, "touch", [path.as_os_str()]))
            .unwrap();
        let outcome = receiver.recv().unwrap();

        assert_eq!(outcome.id(), 41);
        assert_eq!(outcome.program(), OsStr::new("touch"));
        assert!(outcome.result().is_ok());
        for _ in 0..100 {
            if path.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(path.exists());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn worker_returns_launch_failure_to_calloop() {
        let (outcomes, receiver) = sync_channel(1);
        let worker = LaunchWorker::new(
            ProcessLauncher::with_systemd_detection(SystemdMode::Disabled, false),
            outcomes,
        )
        .unwrap();
        let program = format!("tensor-missing-launch-{}", std::process::id());

        worker
            .submit(LaunchRequest::new(
                73,
                program.clone(),
                Vec::<OsString>::new(),
            ))
            .unwrap();
        let outcome = receiver.recv().unwrap();

        assert_eq!(outcome.id(), 73);
        assert_eq!(outcome.program(), OsStr::new(&program));
        assert!(matches!(outcome.result(), Err(SpawnError::Command { .. })));
    }
}
