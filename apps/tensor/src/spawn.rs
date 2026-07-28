mod launcher;
mod worker;

#[cfg(feature = "systemd")]
mod scope;

pub use launcher::{ProcessLauncher, SpawnError, SpawnStrategy, SpawnedProcess};
pub use worker::{
    LaunchOutcome, LaunchRequest, LaunchSubmitError, LaunchSubmitter, LaunchWorker,
    LaunchWorkerError, MAX_PENDING_LAUNCHES,
};
