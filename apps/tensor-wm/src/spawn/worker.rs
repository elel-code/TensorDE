//! Bounded off-thread application launch execution.
//!
//! Process creation and transient-systemd scope setup can wait on fork setup,
//! the user D-Bus, and a systemd job. They therefore never execute from a
//! compositor wait callback. The compositor submits value-only requests, while
//! this worker returns value-only outcomes through a bounded Tensor runtime
//! bridge. The bridge wake is observed as a Compio completion.

use std::{
    ffi::{OsStr, OsString},
    io,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

use tensor_runtime::{TrySendError as OutcomeSendError, WorkerTx};
use thiserror::Error;
use tracing::warn;

#[cfg(feature = "systemd")]
use compio::{
    driver::{DriverType, ProactorBuilder},
    runtime::{Runtime, RuntimeBuilder},
};

use super::{ProcessLauncher, SpawnError, SpawnedProcess};

/// Bound for both the worker request queue and the outcome bridge.
pub const MAX_PENDING_LAUNCHES: usize = 64;

/// A fully resolved command that is safe to move off the compositor thread.
#[derive(Clone, Debug)]
pub struct LaunchRequest {
    id: u64,
    program: OsString,
    args: Vec<OsString>,
    /// Optional `XDG_ACTIVATION_TOKEN` string minted by the compositor.
    activation_token: Option<OsString>,
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
            activation_token: None,
        }
    }

    pub fn with_activation_token(mut self, token: impl Into<OsString>) -> Self {
        self.activation_token = Some(token.into());
        self
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn program(&self) -> &OsStr {
        &self.program
    }

    pub fn activation_token(&self) -> Option<&OsStr> {
        self.activation_token.as_deref()
    }
}

/// Completion information delivered back to the compositor thread.
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

/// Cloneable, value-only handle for queueing launches from the compositor.
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
        outcomes: WorkerTx<LaunchOutcome>,
    ) -> Result<Self, LaunchWorkerError> {
        let (requests, receiver) = mpsc::sync_channel(MAX_PENDING_LAUNCHES);
        #[cfg(feature = "systemd")]
        let (started, startup) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("tensor-client-launch".to_owned())
            .spawn(move || {
                #[cfg(feature = "systemd")]
                run_async(launcher, receiver, outcomes, started);
                #[cfg(not(feature = "systemd"))]
                run(launcher, receiver, outcomes);
            })
            .map_err(LaunchWorkerError::Spawn)?;
        #[cfg(feature = "systemd")]
        startup.recv().map_err(|_| {
            LaunchWorkerError::Runtime(io::Error::other("worker stopped during startup"))
        })??;
        Ok(Self {
            submitter: LaunchSubmitter { requests },
            _thread: thread,
        })
    }

    /// Cloneable submit handle for IPC and other compositor-owned paths.
    pub fn submitter(&self) -> LaunchSubmitter {
        self.submitter.clone()
    }

    /// Queue a command without waiting for process creation or systemd.
    pub fn submit(&self, request: LaunchRequest) -> Result<(), LaunchSubmitError> {
        self.submitter.submit(request)
    }
}

#[cfg(not(feature = "systemd"))]
fn run(
    launcher: ProcessLauncher,
    requests: Receiver<LaunchRequest>,
    outcomes: WorkerTx<LaunchOutcome>,
) {
    while let Ok(request) = requests.recv() {
        let LaunchRequest {
            id,
            program,
            args,
            activation_token,
        } = request;
        let result = match activation_token {
            Some(token) => launcher.spawn_with_activation(
                program.as_os_str(),
                args.iter().map(OsString::as_os_str),
                token,
            ),
            None => launcher.spawn(program.as_os_str(), args.iter().map(OsString::as_os_str)),
        };
        let outcome = LaunchOutcome {
            id,
            program,
            result,
        };
        let outcome_id = outcome.id;
        let outcome_program = outcome.program.clone();
        match outcomes.try_send(outcome) {
            Ok(()) => {}
            Err(OutcomeSendError::Full) => {
                warn!(
                    request_id = outcome_id,
                    program = ?outcome_program,
                    "launch outcome queue saturated; dropping completion"
                );
            }
            Err(OutcomeSendError::Disconnected) => return,
        }
    }
}

#[cfg(feature = "systemd")]
fn run_async(
    launcher: ProcessLauncher,
    requests: Receiver<LaunchRequest>,
    outcomes: WorkerTx<LaunchOutcome>,
    started: SyncSender<Result<(), LaunchWorkerError>>,
) {
    let runtime = match io_uring_runtime() {
        Ok(runtime) => {
            let _ = started.send(Ok(()));
            runtime
        }
        Err(error) => {
            let _ = started.send(Err(LaunchWorkerError::Runtime(error)));
            return;
        }
    };
    let mut connection = None;
    while let Ok(request) = requests.recv() {
        let LaunchRequest {
            id,
            program,
            args,
            activation_token,
        } = request;
        let result = runtime.block_on(async {
            match activation_token {
                Some(token) => {
                    launcher
                        .spawn_with_activation_async(
                            program.as_os_str(),
                            args.iter().map(OsString::as_os_str),
                            token,
                            &mut connection,
                        )
                        .await
                }
                None => {
                    launcher
                        .spawn_async(
                            program.as_os_str(),
                            args.iter().map(OsString::as_os_str),
                            &mut connection,
                        )
                        .await
                }
            }
        });
        if !publish_outcome(
            &outcomes,
            LaunchOutcome {
                id,
                program,
                result,
            },
        ) {
            return;
        }
    }
}

#[cfg(feature = "systemd")]
fn io_uring_runtime() -> io::Result<Runtime> {
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    builder.build()
}

#[cfg(feature = "systemd")]
fn publish_outcome(outcomes: &WorkerTx<LaunchOutcome>, outcome: LaunchOutcome) -> bool {
    let outcome_id = outcome.id;
    let outcome_program = outcome.program.clone();
    match outcomes.try_send(outcome) {
        Ok(()) => true,
        Err(OutcomeSendError::Full) => {
            warn!(
                request_id = outcome_id,
                program = ?outcome_program,
                "launch outcome queue saturated; dropping completion"
            );
            true
        }
        Err(OutcomeSendError::Disconnected) => false,
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
    #[cfg(feature = "systemd")]
    #[error("failed to create the launch worker Compio io_uring runtime: {0}")]
    Runtime(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, thread, time::Duration};

    use tensor_runtime::WorkerBridge;

    use super::*;
    use crate::service::SystemdMode;

    #[test]
    fn worker_returns_a_direct_launch_outcome() {
        let path = PathBuf::from(format!(
            "target/tensor-launch-worker-{}",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let (outcomes, receiver) = WorkerBridge::bounded(1);
        let worker = LaunchWorker::new(
            ProcessLauncher::with_systemd_detection(SystemdMode::Disabled, false),
            outcomes,
        )
        .unwrap();

        worker
            .submit(LaunchRequest::new(41, "touch", [path.as_os_str()]))
            .unwrap();
        let outcome = receiver.recv_timeout(Duration::from_secs(1)).unwrap();

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
    fn worker_returns_launch_failure_to_tensor_bridge() {
        let (outcomes, receiver) = WorkerBridge::bounded(1);
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
        let outcome = receiver.recv_timeout(Duration::from_secs(1)).unwrap();

        assert_eq!(outcome.id(), 73);
        assert_eq!(outcome.program(), OsStr::new(&program));
        assert!(matches!(outcome.result(), Err(SpawnError::Command { .. })));
    }
}
