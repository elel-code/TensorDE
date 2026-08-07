use std::{
    io,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
};

use futures_util::{FutureExt, pin_mut, select_biased};
use tensor_dbus::{
    Connection,
    freedesktop::login1::{self, Login1Session, Login1SessionEvent},
};
use tensor_runtime::io_uring_runtime;
use wayland_client_runtime::WakeHandle;

const COMMAND_CAPACITY: usize = 4;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum SessionLockServiceStatus {
    #[default]
    Pending,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum SessionActionState {
    #[default]
    Idle,
    Pending(SessionAction),
    Succeeded(SessionAction),
    Failed(SessionAction),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionAction {
    Lock,
    Suspend,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SessionLockServiceSnapshot {
    pub(crate) desired_locked: bool,
    pub(crate) status: SessionLockServiceStatus,
}

#[derive(Debug, Default)]
struct SessionLockServiceStore {
    generation: u64,
    snapshot: SessionLockServiceSnapshot,
    action_generation: u64,
    action: SessionActionState,
}

impl SessionLockServiceStore {
    fn publish(&mut self, snapshot: SessionLockServiceSnapshot) -> bool {
        if self.snapshot == snapshot {
            return false;
        }
        self.snapshot = snapshot;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    const fn read(&self) -> (u64, SessionLockServiceSnapshot) {
        (self.generation, self.snapshot)
    }

    const fn read_action(&self) -> (u64, SessionActionState) {
        (self.action_generation, self.action)
    }

    fn publish_action(&mut self, action: SessionActionState) -> bool {
        if self.action == action {
            return false;
        }
        self.action = action;
        self.action_generation = self.action_generation.wrapping_add(1);
        true
    }
}

type SharedStore = Arc<Mutex<SessionLockServiceStore>>;

pub(crate) struct SessionLockServiceHandle {
    store: SharedStore,
    commands: async_channel::Sender<ServiceCommand>,
    join: Option<JoinHandle<()>>,
}

impl SessionLockServiceHandle {
    pub(crate) fn start(wake: WakeHandle) -> Result<Self, SessionLockServiceError> {
        let store = Arc::new(Mutex::new(SessionLockServiceStore::default()));
        let (commands, command_rx) = async_channel::bounded(COMMAND_CAPACITY);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker_store = Arc::clone(&store);
        let join = thread::Builder::new()
            .name("tensor-shell-session-lock".into())
            .spawn(move || run_thread(worker_store, wake, command_rx, ready_tx))
            .map_err(SessionLockServiceError::Spawn)?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                store,
                commands,
                join: Some(join),
            }),
            Ok(Err(message)) => {
                let _ = join.join();
                Err(SessionLockServiceError::Start(message))
            }
            Err(_) => {
                let _ = join.join();
                Err(SessionLockServiceError::StartupDisconnected)
            }
        }
    }

    pub(crate) fn read(&self) -> (u64, SessionLockServiceSnapshot) {
        self.store.lock().map(|store| store.read()).unwrap_or((
            u64::MAX,
            SessionLockServiceSnapshot {
                desired_locked: true,
                status: SessionLockServiceStatus::Failed,
            },
        ))
    }

    pub(crate) fn set_locked_hint(&self, locked: bool) -> Result<(), SessionLockServiceError> {
        self.commands
            .try_send(ServiceCommand::SetLockedHint(locked))
            .map_err(|error| match error {
                async_channel::TrySendError::Full(_) => SessionLockServiceError::CommandQueueFull,
                async_channel::TrySendError::Closed(_) => SessionLockServiceError::Stopped,
            })
    }

    pub(crate) fn action_read(&self) -> (u64, SessionActionState) {
        self.store
            .lock()
            .map(|store| store.read_action())
            .unwrap_or((u64::MAX, SessionActionState::Failed(SessionAction::Lock)))
    }

    pub(crate) fn request_action(
        &self,
        action: SessionAction,
    ) -> Result<(), SessionLockServiceError> {
        self.commands
            .try_send(ServiceCommand::Action(action))
            .map_err(|error| match error {
                async_channel::TrySendError::Full(_) => SessionLockServiceError::CommandQueueFull,
                async_channel::TrySendError::Closed(_) => SessionLockServiceError::Stopped,
            })
    }
}

impl Drop for SessionLockServiceHandle {
    fn drop(&mut self) {
        self.commands.close();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ServiceCommand {
    SetLockedHint(bool),
    Action(SessionAction),
}

fn run_thread(
    store: SharedStore,
    wake: WakeHandle,
    commands: async_channel::Receiver<ServiceCommand>,
    ready: mpsc::SyncSender<Result<(), String>>,
) {
    let runtime = match io_uring_runtime(8) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(format!("create Compio io_uring runtime: {error}")));
            return;
        }
    };
    if let Err(error) = runtime.block_on(run_service(&store, &wake, &commands, &ready)) {
        publish(
            &store,
            &wake,
            SessionLockServiceSnapshot {
                desired_locked: store
                    .lock()
                    .map(|store| store.snapshot.desired_locked)
                    .unwrap_or(true),
                status: SessionLockServiceStatus::Failed,
            },
        );
        eprintln!("Tensor Shell session-lock monitor stopped: {error}");
    }
}

async fn run_service(
    store: &SharedStore,
    wake: &WakeHandle,
    commands: &async_channel::Receiver<ServiceCommand>,
    ready: &mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let mut connection = Connection::system_bus()
        .await
        .map_err(|error| startup_error(ready, "connect to system D-Bus", error))?;
    let session = Login1Session::current(&mut connection)
        .await
        .map_err(|error| startup_error(ready, "resolve current logind session", error))?;
    let mut monitor = session
        .monitor(&mut connection)
        .await
        .map_err(|error| startup_error(ready, "subscribe to logind session lock", error))?;
    publish(
        store,
        wake,
        SessionLockServiceSnapshot {
            desired_locked: monitor.initially_locked(),
            status: SessionLockServiceStatus::Ready,
        },
    );
    if ready.send(Ok(())).is_err() {
        return Ok(());
    }

    loop {
        let mut received_message = None;
        let mut received_command = None;
        {
            let message = connection.receive().fuse();
            let command = commands.recv().fuse();
            pin_mut!(message, command);
            select_biased! {
                command = command => received_command = Some(command),
                message = message => received_message = Some(message),
            }
        }
        if let Some(command) = received_command {
            match command {
                Ok(ServiceCommand::SetLockedHint(locked)) => monitor
                    .set_locked_hint(&mut connection, locked)
                    .await
                    .map_err(|error| format!("set logind LockedHint={locked}: {error}"))?,
                Ok(ServiceCommand::Action(action)) => {
                    publish_action(store, wake, SessionActionState::Pending(action));
                    let result = match action {
                        SessionAction::Lock => login1::lock_sessions(&mut connection).await,
                        SessionAction::Suspend => login1::suspend(&mut connection, false).await,
                    };
                    match result {
                        Ok(()) => {
                            publish_action(store, wake, SessionActionState::Succeeded(action))
                        }
                        Err(error) => {
                            publish_action(store, wake, SessionActionState::Failed(action));
                            eprintln!("Tensor Shell {action:?} request failed: {error}");
                        }
                    }
                }
                Err(_) => {
                    let _ = monitor.close(&mut connection).await;
                    return Ok(());
                }
            }
            continue;
        }
        let message = received_message
            .expect("session-lock select completed one branch")
            .map_err(|error| format!("receive logind session signal: {error}"))?;
        match monitor
            .observe(&message)
            .map_err(|error| format!("decode logind session signal: {error}"))?
        {
            Login1SessionEvent::Lock => publish_ready(store, wake, true),
            Login1SessionEvent::Unlock => publish_ready(store, wake, false),
            Login1SessionEvent::OwnerChanged => {
                return Err("logind service owner changed".into());
            }
            Login1SessionEvent::Ignored => {}
        }
    }
}

fn publish_ready(store: &SharedStore, wake: &WakeHandle, desired_locked: bool) {
    publish(
        store,
        wake,
        SessionLockServiceSnapshot {
            desired_locked,
            status: SessionLockServiceStatus::Ready,
        },
    );
}

fn publish_action(store: &SharedStore, wake: &WakeHandle, action: SessionActionState) {
    let changed = store
        .lock()
        .map(|mut store| store.publish_action(action))
        .unwrap_or(false);
    if changed {
        wake.wake();
    }
}

fn publish(store: &SharedStore, wake: &WakeHandle, snapshot: SessionLockServiceSnapshot) {
    let changed = store
        .lock()
        .map(|mut store| store.publish(snapshot))
        .unwrap_or(false);
    if changed {
        wake.wake();
    }
}

fn startup_error(
    error_tx: &mpsc::SyncSender<Result<(), String>>,
    step: &str,
    error: impl std::fmt::Display,
) -> String {
    let message = format!("{step}: {error}");
    let _ = error_tx.send(Err(message.clone()));
    message
}

#[derive(Debug, thiserror::Error)]
pub enum SessionLockServiceError {
    #[error("failed to spawn Tensor Shell session-lock service: {0}")]
    Spawn(#[source] io::Error),
    #[error("failed to start Tensor Shell session-lock service: {0}")]
    Start(String),
    #[error("Tensor Shell session-lock service stopped during startup")]
    StartupDisconnected,
    #[error("Tensor Shell session-lock command queue is full")]
    CommandQueueFull,
    #[error("Tensor Shell session-lock service has stopped")]
    Stopped,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_coalesces_lock_intent_and_retains_failure_state() {
        let mut store = SessionLockServiceStore::default();
        let locked = SessionLockServiceSnapshot {
            desired_locked: true,
            status: SessionLockServiceStatus::Ready,
        };
        assert!(store.publish(locked));
        assert!(!store.publish(locked));
        assert!(store.publish(SessionLockServiceSnapshot {
            desired_locked: true,
            status: SessionLockServiceStatus::Failed,
        }));
        assert_eq!(store.read().0, 2);
    }

    #[test]
    fn action_state_is_revisioned_and_coalesced() {
        let mut store = SessionLockServiceStore::default();
        assert_eq!(store.read_action(), (0, SessionActionState::Idle));
        assert!(store.publish_action(SessionActionState::Pending(SessionAction::Lock)));
        assert!(!store.publish_action(SessionActionState::Pending(SessionAction::Lock)));
        assert!(store.publish_action(SessionActionState::Succeeded(SessionAction::Lock)));
        assert_eq!(store.read_action().0, 2);
    }
}
