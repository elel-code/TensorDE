use std::{
    future::Future,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use futures_util::{FutureExt, pin_mut, select_biased};
use tensor_dbus::{
    Connection, MethodError, ObjectServer, RequestNameFlags, RequestNameReply,
    freedesktop::mpris::{MprisAction, MprisError, MprisMonitor, MprisMonitorEvent},
    tensor::shell::{DESTINATION, MEDIA_INTERFACE, MEDIA_PATH, media_member},
};
use tensor_runtime::io_uring_runtime;
use wayland_client_runtime::WakeHandle;

use super::{MediaActionState, MediaServiceSnapshot, MediaServiceStore};

const COMMAND_CAPACITY: usize = 1;

type SharedStore = Arc<Mutex<MediaServiceStore>>;

pub(crate) struct MediaServiceHandle {
    store: SharedStore,
    commands: async_channel::Sender<MprisAction>,
    stop: async_channel::Sender<()>,
    join: Option<JoinHandle<()>>,
    wake: WakeHandle,
}

impl MediaServiceHandle {
    pub(crate) fn start(wake: WakeHandle) -> Self {
        let store = Arc::new(Mutex::new(MediaServiceStore::default()));
        let (commands, command_rx) = async_channel::bounded(COMMAND_CAPACITY);
        let (stop, stop_rx) = async_channel::bounded(1);
        let worker_store = Arc::clone(&store);
        let worker_wake = wake.clone();
        let worker_commands = commands.clone();
        let join = match thread::Builder::new()
            .name("tensor-shell-media".into())
            .spawn(move || {
                run_thread(
                    worker_store,
                    worker_commands,
                    command_rx,
                    stop_rx,
                    worker_wake,
                )
            }) {
            Ok(join) => Some(join),
            Err(error) => {
                publish_snapshot(&store, MediaServiceSnapshot::Failed, &wake);
                eprintln!("Tensor Shell could not start its MPRIS adapter: {error}");
                None
            }
        };
        Self {
            store,
            commands,
            stop,
            join,
            wake,
        }
    }

    pub(crate) fn read(&self) -> (u64, MediaServiceSnapshot, MediaActionState) {
        self.store.lock().map(|store| store.read()).unwrap_or((
            u64::MAX,
            MediaServiceSnapshot::Failed,
            MediaActionState::Idle,
        ))
    }

    pub(crate) fn request(&self, action: MprisAction) -> Result<(), MediaServiceError> {
        queue_action(&self.store, &self.commands, &self.wake, action)
    }
}

impl Drop for MediaServiceHandle {
    fn drop(&mut self) {
        self.stop.close();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_thread(
    store: SharedStore,
    command_tx: async_channel::Sender<MprisAction>,
    commands: async_channel::Receiver<MprisAction>,
    stop: async_channel::Receiver<()>,
    wake: WakeHandle,
) {
    let runtime = match io_uring_runtime(4) {
        Ok(runtime) => runtime,
        Err(error) => {
            publish_snapshot(&store, MediaServiceSnapshot::Failed, &wake);
            eprintln!("Tensor Shell could not create its MPRIS io_uring runtime: {error}");
            return;
        }
    };
    runtime.block_on(run_service(store, command_tx, commands, stop, wake));
}

async fn run_service(
    store: SharedStore,
    command_tx: async_channel::Sender<MprisAction>,
    commands: async_channel::Receiver<MprisAction>,
    stop: async_channel::Receiver<()>,
    wake: WakeHandle,
) {
    let Some(connected) = stop_or(&stop, Connection::session_bus()).await else {
        return;
    };
    let mut connection = match connected {
        Ok(connection) => connection,
        Err(error) => {
            publish_snapshot(&store, classify_error(&MprisError::Transport(error)), &wake);
            return;
        }
    };
    let ownership = match stop_or(
        &stop,
        connection.request_name(DESTINATION, RequestNameFlags::DO_NOT_QUEUE),
    )
    .await
    {
        None => return,
        Some(Ok(ownership)) => ownership,
        Some(Err(error)) => {
            publish_snapshot(&store, MediaServiceSnapshot::Failed, &wake);
            eprintln!("Tensor Shell could not own {DESTINATION}: {error}");
            return;
        }
    };
    if !matches!(
        ownership,
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner
    ) {
        publish_snapshot(&store, MediaServiceSnapshot::Failed, &wake);
        eprintln!("Tensor Shell could not own {DESTINATION}: bus returned {ownership:?}");
        return;
    }
    let mut objects = match media_object_server(Arc::clone(&store), command_tx, wake.clone()) {
        Ok(objects) => objects,
        Err(error) => {
            publish_snapshot(&store, MediaServiceSnapshot::Failed, &wake);
            eprintln!("Tensor Shell could not register its media control interface: {error}");
            return;
        }
    };
    let mut active_name = None;

    'monitor: loop {
        let Some(started) = stop_or(&stop, MprisMonitor::start(&mut connection)).await else {
            return;
        };
        let mut monitor = match started {
            Ok(monitor) => monitor,
            Err(error) => {
                publish_snapshot(&store, classify_error(&error), &wake);
                return;
            }
        };
        publish_active(&store, &wake, &monitor, &mut active_name);

        loop {
            let mut received_command = None;
            let mut received_message = None;
            let mut stopped = false;
            {
                let stop_wait = stop.recv().fuse();
                let command_wait = commands.recv().fuse();
                let message_wait = objects.serve_next(&mut connection).fuse();
                pin_mut!(stop_wait, command_wait, message_wait);
                select_biased! {
                    _ = stop_wait => stopped = true,
                    command = command_wait => received_command = Some(command),
                    message = message_wait => received_message = Some(message),
                }
            }
            if stopped {
                return;
            }
            if let Some(command) = received_command {
                let Ok(action) = command else {
                    return;
                };
                let Some(player) = monitor.active_player(active_name.as_deref()) else {
                    publish_action(&store, MediaActionState::Failed(action), &wake);
                    continue;
                };
                let bus_name = player.bus_name().to_owned();
                let Some(result) =
                    stop_or(&stop, monitor.perform(&mut connection, &bus_name, action)).await
                else {
                    return;
                };
                if let Err(error) = result {
                    publish_action(&store, MediaActionState::Failed(action), &wake);
                    eprintln!("Tensor Shell MPRIS {action:?} request failed: {error}");
                    if error.is_service_unavailable() {
                        publish_external_snapshot(&store, MediaServiceSnapshot::Pending, &wake);
                        let Some(closed) = stop_or(&stop, monitor.close(&mut connection)).await
                        else {
                            return;
                        };
                        if let Err(error) = closed {
                            publish_snapshot(
                                &store,
                                classify_error(&MprisError::Transport(error)),
                                &wake,
                            );
                            return;
                        }
                        continue 'monitor;
                    }
                } else {
                    publish_action(&store, MediaActionState::Succeeded(action), &wake);
                }
                continue;
            }

            let message = match received_message.expect("MPRIS select completed one branch") {
                Ok(Some(message)) => message,
                Ok(None) => continue,
                Err(error) => {
                    publish_snapshot(&store, classify_error(&MprisError::Transport(error)), &wake);
                    return;
                }
            };
            match monitor.observe(&message) {
                Ok(MprisMonitorEvent::Changed) => {
                    publish_active(&store, &wake, &monitor, &mut active_name)
                }
                Ok(MprisMonitorEvent::RefreshRequired) => {
                    publish_external_snapshot(&store, MediaServiceSnapshot::Pending, &wake);
                    let Some(closed) = stop_or(&stop, monitor.close(&mut connection)).await else {
                        return;
                    };
                    if let Err(error) = closed {
                        publish_snapshot(
                            &store,
                            classify_error(&MprisError::Transport(error)),
                            &wake,
                        );
                        return;
                    }
                    continue 'monitor;
                }
                Ok(MprisMonitorEvent::Ignored | MprisMonitorEvent::Unchanged) => {}
                Err(error) => {
                    publish_snapshot(&store, classify_error(&error), &wake);
                    let _ = stop_or(&stop, monitor.close(&mut connection)).await;
                    return;
                }
            }
        }
    }
}

fn media_object_server(
    store: SharedStore,
    commands: async_channel::Sender<MprisAction>,
    wake: WakeHandle,
) -> tensor_dbus::Result<ObjectServer> {
    let mut objects = ObjectServer::new();
    for action in [
        MprisAction::Previous,
        MprisAction::PlayPause,
        MprisAction::Next,
    ] {
        let store = Arc::clone(&store);
        let commands = commands.clone();
        let wake = wake.clone();
        objects.register::<(), (), _, _>(
            MEDIA_PATH,
            MEDIA_INTERFACE,
            media_member(action),
            move |()| {
                let result =
                    queue_action(&store, &commands, &wake, action).map_err(media_method_error);
                async move { result }
            },
        )?;
    }
    Ok(objects)
}

fn queue_action(
    store: &SharedStore,
    commands: &async_channel::Sender<MprisAction>,
    wake: &WakeHandle,
    action: MprisAction,
) -> Result<(), MediaServiceError> {
    store
        .lock()
        .map_err(|_| MediaServiceError::Stopped)?
        .begin_action(action)?;
    wake.wake();
    if let Err(error) = commands.try_send(action) {
        publish_action(store, MediaActionState::Failed(action), wake);
        return Err(match error {
            async_channel::TrySendError::Full(_) => {
                MediaServiceError::CommandQueueFull(COMMAND_CAPACITY)
            }
            async_channel::TrySendError::Closed(_) => MediaServiceError::Stopped,
        });
    }
    Ok(())
}

fn media_method_error(error: MediaServiceError) -> MethodError {
    let name = match error {
        MediaServiceError::CommandQueueFull(_) | MediaServiceError::Busy => {
            "org.tensor.Shell1.Error.Busy"
        }
        MediaServiceError::Unavailable => "org.tensor.Shell1.Error.Unavailable",
        MediaServiceError::Unsupported(_) => "org.tensor.Shell1.Error.Unsupported",
        MediaServiceError::Stopped => "org.tensor.Shell1.Error.Stopped",
    };
    MethodError::new(name, error.to_string())
}

fn publish_active(
    store: &SharedStore,
    wake: &WakeHandle,
    monitor: &MprisMonitor,
    active_name: &mut Option<String>,
) {
    let active = monitor.active_player(active_name.as_deref()).cloned();
    *active_name = active.as_ref().map(|player| player.bus_name().to_owned());
    publish_external_snapshot(
        store,
        MediaServiceSnapshot::Ready(active.map(Arc::new)),
        wake,
    );
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

fn classify_error(error: &MprisError) -> MediaServiceSnapshot {
    if error.is_service_unavailable() {
        MediaServiceSnapshot::Unavailable
    } else {
        MediaServiceSnapshot::Failed
    }
}

fn publish_snapshot(store: &SharedStore, snapshot: MediaServiceSnapshot, wake: &WakeHandle) {
    let changed = store
        .lock()
        .map(|mut store| store.publish_snapshot(snapshot))
        .unwrap_or(false);
    if changed {
        wake.wake();
    }
}

fn publish_external_snapshot(
    store: &SharedStore,
    snapshot: MediaServiceSnapshot,
    wake: &WakeHandle,
) {
    let changed = store
        .lock()
        .map(|mut store| store.publish_external_snapshot(snapshot))
        .unwrap_or(false);
    if changed {
        wake.wake();
    }
}

fn publish_action(store: &SharedStore, action: MediaActionState, wake: &WakeHandle) {
    let changed = store
        .lock()
        .map(|mut store| store.publish_action(action))
        .unwrap_or(false);
    if changed {
        wake.wake();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum MediaServiceError {
    #[error("Tensor Shell MPRIS command queue is full (capacity {0})")]
    CommandQueueFull(usize),
    #[error("Tensor Shell MPRIS service has stopped")]
    Stopped,
    #[error("Tensor Shell MPRIS state has no active player")]
    Unavailable,
    #[error("Tensor Shell MPRIS active player does not support {0:?}")]
    Unsupported(MprisAction),
    #[error("Tensor Shell MPRIS already has an action in flight")]
    Busy,
}

#[cfg(test)]
mod tests {
    use tensor_dbus::Error;

    use super::*;

    #[test]
    fn missing_session_bus_is_distinct_from_invalid_player_data() {
        let unavailable = MprisError::Transport(Error::AddressUnavailable("session"));
        assert_eq!(
            classify_error(&unavailable),
            MediaServiceSnapshot::Unavailable
        );
        let failed = MprisError::MissingProperty {
            interface: "org.mpris.MediaPlayer2.Player",
            property: "PlaybackStatus",
        };
        assert_eq!(classify_error(&failed), MediaServiceSnapshot::Failed);
    }
}
