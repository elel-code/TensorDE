mod launcher;

#[cfg(feature = "systemd")]
mod scope;

pub use launcher::{ProcessLauncher, SpawnError, SpawnStrategy, SpawnedProcess};
