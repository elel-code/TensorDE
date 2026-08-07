use std::{
    future::Future,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use futures_util::{FutureExt, pin_mut, select_biased};
use tensor_ipc::land::{
    ClientError, Command, CompioClient, IpcErrorBody, OverviewSnapshot, ResultBody,
};
use tensor_runtime::io_uring_runtime;
use wayland_client_runtime::WakeHandle;

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const RECONNECT_INTERVAL: Duration = Duration::from_secs(1);
const COMMAND_CAPACITY: usize = 16;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum OverviewServiceSnapshot {
    #[default]
    Pending,
    Ready(OverviewSnapshot),
    Unavailable,
    Failed,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct OverviewServiceStore {
    snapshot: OverviewServiceSnapshot,
    revision: u64,
}

impl OverviewServiceStore {
    fn publish(&mut self, snapshot: OverviewServiceSnapshot) -> bool {
        if self.snapshot == snapshot {
            return false;
        }
        self.snapshot = snapshot;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    fn read(&self) -> (u64, OverviewServiceSnapshot) {
        (self.revision, self.snapshot.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OverviewCommand {
    ActivateView(u64),
    SetWorkspace(u32),
    MoveViewToWorkspace { view: u64, index: u32 },
    CloseView(u64),
    Spawn(Vec<String>),
}

#[derive(Debug)]
pub(crate) struct OverviewServiceHandle {
    store: Arc<Mutex<OverviewServiceStore>>,
    commands: async_channel::Sender<OverviewCommand>,
    stop: async_channel::Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl OverviewServiceHandle {
    pub(crate) fn start(socket: PathBuf, wake: WakeHandle) -> Self {
        let store = Arc::new(Mutex::new(OverviewServiceStore::default()));
        let (commands, command_rx) = async_channel::bounded(COMMAND_CAPACITY);
        let (stop, stop_rx) = async_channel::bounded(1);
        let worker_store = Arc::clone(&store);
        let join = match thread::Builder::new()
            .name("tensor-shell-overview".into())
            .spawn(move || run_thread(worker_store, socket, command_rx, stop_rx, wake))
        {
            Ok(join) => Some(join),
            Err(error) => {
                publish(&store, OverviewServiceSnapshot::Failed, None);
                eprintln!("Tensor Shell could not start its overview adapter: {error}");
                None
            }
        };
        Self {
            store,
            commands,
            stop,
            join,
        }
    }

    pub(crate) fn read(&self) -> (u64, OverviewServiceSnapshot) {
        self.store
            .lock()
            .map(|store| store.read())
            .unwrap_or((u64::MAX, OverviewServiceSnapshot::Failed))
    }

    pub(crate) fn activate_view(&self, view: u64) -> Result<(), OverviewCommandError> {
        self.send(OverviewCommand::ActivateView(view))
    }

    pub(crate) fn set_workspace(&self, index: u32) -> Result<(), OverviewCommandError> {
        self.send(OverviewCommand::SetWorkspace(index))
    }

    pub(crate) fn move_view_to_workspace(
        &self,
        view: u64,
        index: u32,
    ) -> Result<(), OverviewCommandError> {
        self.send(OverviewCommand::MoveViewToWorkspace { view, index })
    }

    pub(crate) fn close_view(&self, view: u64) -> Result<(), OverviewCommandError> {
        self.send(OverviewCommand::CloseView(view))
    }

    pub(crate) fn spawn(&self, command: &[String]) -> Result<(), OverviewCommandError> {
        self.send(OverviewCommand::Spawn(command.to_vec()))
    }

    fn send(&self, command: OverviewCommand) -> Result<(), OverviewCommandError> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                async_channel::TrySendError::Full(_) => {
                    OverviewCommandError::QueueFull(COMMAND_CAPACITY)
                }
                async_channel::TrySendError::Closed(_) => OverviewCommandError::Stopped,
            })
    }
}

impl Drop for OverviewServiceHandle {
    fn drop(&mut self) {
        self.stop.close();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_thread(
    store: Arc<Mutex<OverviewServiceStore>>,
    socket: PathBuf,
    commands: async_channel::Receiver<OverviewCommand>,
    stop: async_channel::Receiver<()>,
    wake: WakeHandle,
) {
    let runtime = match io_uring_runtime(4) {
        Ok(runtime) => runtime,
        Err(error) => {
            publish(&store, OverviewServiceSnapshot::Failed, Some(&wake));
            eprintln!("Tensor Shell could not create its overview io_uring runtime: {error}");
            return;
        }
    };
    runtime.block_on(run_service(store, socket, commands, stop, wake));
}

async fn run_service(
    store: Arc<Mutex<OverviewServiceStore>>,
    socket: PathBuf,
    commands: async_channel::Receiver<OverviewCommand>,
    stop: async_channel::Receiver<()>,
    wake: WakeHandle,
) {
    loop {
        let Some(connection) = stop_or(&stop, CompioClient::connect(&socket)).await else {
            return;
        };
        let mut client = match connection {
            Ok(client) => client,
            Err(error) => {
                publish(
                    &store,
                    classify_error(&OverviewWorkerError::Client(error)),
                    Some(&wake),
                );
                if !wait_or_stop(&stop, RECONNECT_INTERVAL).await {
                    return;
                }
                continue;
            }
        };

        if let Err(error) = refresh(&mut client, &store, &wake).await {
            publish(&store, classify_error(&error), Some(&wake));
            if !wait_or_stop(&stop, RECONNECT_INTERVAL).await {
                return;
            }
            continue;
        }

        loop {
            let stop_wait = stop.recv().fuse();
            let command_wait = commands.recv().fuse();
            let timer_wait = compio::runtime::time::sleep(POLL_INTERVAL).fuse();
            pin_mut!(stop_wait, command_wait, timer_wait);
            select_biased! {
                _ = stop_wait => return,
                command = command_wait => {
                    let Ok(command) = command else { return };
                    match execute(&mut client, command).await {
                        Ok(Some(error)) => {
                            eprintln!(
                                "Tensorland rejected an overview command with {}: {}",
                                error.code, error.message
                            );
                        }
                        Ok(None) => {}
                        Err(error) => {
                            publish(&store, classify_error(&error), Some(&wake));
                            if !wait_or_stop(&stop, RECONNECT_INTERVAL).await {
                                return;
                            }
                            break;
                        }
                    }
                    if let Err(error) = refresh(&mut client, &store, &wake).await {
                        publish(&store, classify_error(&error), Some(&wake));
                        if !wait_or_stop(&stop, RECONNECT_INTERVAL).await {
                            return;
                        }
                        break;
                    }
                }
                _ = timer_wait => {
                    if let Err(error) = refresh(&mut client, &store, &wake).await {
                        publish(&store, classify_error(&error), Some(&wake));
                        if !wait_or_stop(&stop, RECONNECT_INTERVAL).await {
                            return;
                        }
                        break;
                    }
                }
            }
        }
    }
}

async fn refresh(
    client: &mut CompioClient,
    store: &Arc<Mutex<OverviewServiceStore>>,
    wake: &WakeHandle,
) -> Result<(), OverviewWorkerError> {
    let result = client.call(Command::GetOverview).await?;
    let ResultBody::Overview(snapshot) = result else {
        return Err(OverviewWorkerError::UnexpectedResult("GetOverview"));
    };
    publish(store, OverviewServiceSnapshot::Ready(snapshot), Some(wake));
    Ok(())
}

async fn execute(
    client: &mut CompioClient,
    command: OverviewCommand,
) -> Result<Option<IpcErrorBody>, OverviewWorkerError> {
    let result = client.call(command_to_ipc(command)).await?;
    match result {
        ResultBody::Accepted => Ok(None),
        ResultBody::Error(error) => Ok(Some(error)),
        _ => Err(OverviewWorkerError::UnexpectedResult("overview command")),
    }
}

fn command_to_ipc(command: OverviewCommand) -> Command {
    match command {
        OverviewCommand::ActivateView(view) => Command::ActivateView { view },
        OverviewCommand::SetWorkspace(index) => Command::SetWorkspace { index },
        OverviewCommand::MoveViewToWorkspace { view, index } => Command::MoveViewToWorkspace {
            view,
            index,
            follow: false,
        },
        OverviewCommand::CloseView(view) => Command::CloseView { view },
        OverviewCommand::Spawn(argv) => Command::Spawn { argv, cwd: None },
    }
}

async fn stop_or<T>(
    stop: &async_channel::Receiver<()>,
    future: impl Future<Output = T>,
) -> Option<T> {
    let stop = stop.recv().fuse();
    let future = future.fuse();
    pin_mut!(stop, future);
    select_biased! {
        _ = stop => None,
        output = future => Some(output),
    }
}

async fn wait_or_stop(stop: &async_channel::Receiver<()>, duration: Duration) -> bool {
    let stop = stop.recv().fuse();
    let timer = compio::runtime::time::sleep(duration).fuse();
    pin_mut!(stop, timer);
    select_biased! {
        _ = stop => false,
        _ = timer => true,
    }
}

fn classify_error(error: &OverviewWorkerError) -> OverviewServiceSnapshot {
    match error {
        OverviewWorkerError::Client(ClientError::Connect { source, .. })
            if matches!(
                source.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            OverviewServiceSnapshot::Unavailable
        }
        _ => OverviewServiceSnapshot::Failed,
    }
}

#[derive(Debug, thiserror::Error)]
enum OverviewWorkerError {
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("Tensorland returned an unexpected result for {0}")]
    UnexpectedResult(&'static str),
}

fn publish(
    store: &Arc<Mutex<OverviewServiceStore>>,
    snapshot: OverviewServiceSnapshot,
    wake: Option<&WakeHandle>,
) {
    if store
        .lock()
        .map(|mut store| store.publish(snapshot))
        .unwrap_or(false)
        && let Some(wake) = wake
    {
        wake.wake();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum OverviewCommandError {
    #[error("Tensor Shell overview command queue is full (capacity {0})")]
    QueueFull(usize),
    #[error("Tensor Shell overview service has stopped")]
    Stopped,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_snapshots_do_not_wake_or_advance_revision() {
        let store = Arc::new(Mutex::new(OverviewServiceStore::default()));
        assert!(
            !store
                .lock()
                .unwrap()
                .publish(OverviewServiceSnapshot::Pending)
        );
        assert_eq!(store.lock().unwrap().read().0, 0);
    }

    #[test]
    fn connect_failures_distinguish_missing_socket_from_transport_faults() {
        let unavailable = classify_error(&OverviewWorkerError::Client(ClientError::Connect {
            path: std::path::Path::new("/missing").to_owned(),
            source: io::Error::from(io::ErrorKind::NotFound),
        }));
        assert_eq!(unavailable, OverviewServiceSnapshot::Unavailable);
        let failed = classify_error(&OverviewWorkerError::Client(ClientError::Io(
            io::Error::from(io::ErrorKind::BrokenPipe),
        )));
        assert_eq!(failed, OverviewServiceSnapshot::Failed);
    }

    #[test]
    fn overview_actions_map_to_stable_tensorland_commands() {
        assert!(matches!(
            command_to_ipc(OverviewCommand::ActivateView(7)),
            Command::ActivateView { view: 7 }
        ));
        assert!(matches!(
            command_to_ipc(OverviewCommand::SetWorkspace(2)),
            Command::SetWorkspace { index: 2 }
        ));
        assert!(matches!(
            command_to_ipc(OverviewCommand::MoveViewToWorkspace { view: 7, index: 2 }),
            Command::MoveViewToWorkspace {
                view: 7,
                index: 2,
                follow: false
            }
        ));
        assert!(matches!(
            command_to_ipc(OverviewCommand::CloseView(7)),
            Command::CloseView { view: 7 }
        ));
        let command = command_to_ipc(OverviewCommand::Spawn(vec!["tensor-launcher".into()]));
        assert!(matches!(
            command,
            Command::Spawn { argv, cwd: None } if argv == ["tensor-launcher"]
        ));
    }
}
