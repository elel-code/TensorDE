use std::{
    future::Future,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use futures_util::{FutureExt, pin_mut, select_biased};
use tensor_dbus::{
    Connection, MatchRule,
    freedesktop::network_manager::{
        DESTINATION, NetworkManagerError, PROPERTIES_INTERFACE, ROOT_PATH, set_wireless_enabled,
        wifi::{
            NetworkManagerDetailsError, NetworkManagerDetailsEvent, NetworkManagerDetailsMonitor,
        },
    },
};
use tensor_runtime::io_uring_runtime;
use wayland_client_runtime::WakeHandle;

use super::{NetworkActionState, NetworkServiceSnapshot, NetworkServiceStore};

const COMMAND_CAPACITY: usize = 1;

type SharedStore = Arc<Mutex<NetworkServiceStore>>;

pub(crate) struct NetworkServiceHandle {
    store: SharedStore,
    commands: async_channel::Sender<bool>,
    stop: async_channel::Sender<()>,
    join: Option<JoinHandle<()>>,
    wake: WakeHandle,
}

impl NetworkServiceHandle {
    pub(crate) fn start(wake: WakeHandle) -> Self {
        let store = Arc::new(Mutex::new(NetworkServiceStore::default()));
        let (commands, command_rx) = async_channel::bounded(COMMAND_CAPACITY);
        let (stop, stop_rx) = async_channel::bounded(1);
        let worker_store = Arc::clone(&store);
        let worker_wake = wake.clone();
        let join = match thread::Builder::new()
            .name("tensor-shell-network".into())
            .spawn(move || run_thread(worker_store, command_rx, stop_rx, worker_wake))
        {
            Ok(join) => Some(join),
            Err(error) => {
                publish_snapshot(&store, NetworkServiceSnapshot::Failed, &wake);
                eprintln!("Tensor Shell could not start its NetworkManager adapter: {error}");
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

    pub(crate) fn read(&self) -> (u64, NetworkServiceSnapshot, NetworkActionState) {
        self.store.lock().map(|store| store.read()).unwrap_or((
            u64::MAX,
            NetworkServiceSnapshot::Failed,
            NetworkActionState::Idle,
        ))
    }

    pub(crate) fn request_toggle(&self) -> Result<(), NetworkServiceError> {
        let target = self
            .store
            .lock()
            .map_err(|_| NetworkServiceError::Stopped)?
            .begin_toggle()?;
        self.wake.wake();
        if let Err(error) = self.commands.try_send(target) {
            publish_action(&self.store, NetworkActionState::Failed(target), &self.wake);
            return Err(match error {
                async_channel::TrySendError::Full(_) => {
                    NetworkServiceError::CommandQueueFull(COMMAND_CAPACITY)
                }
                async_channel::TrySendError::Closed(_) => NetworkServiceError::Stopped,
            });
        }
        Ok(())
    }
}

impl Drop for NetworkServiceHandle {
    fn drop(&mut self) {
        self.stop.close();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_thread(
    store: SharedStore,
    commands: async_channel::Receiver<bool>,
    stop: async_channel::Receiver<()>,
    wake: WakeHandle,
) {
    let runtime = match io_uring_runtime(4) {
        Ok(runtime) => runtime,
        Err(error) => {
            publish_snapshot(&store, NetworkServiceSnapshot::Failed, &wake);
            eprintln!("Tensor Shell could not create its NetworkManager io_uring runtime: {error}");
            return;
        }
    };
    runtime.block_on(run_service(store, commands, stop, wake));
}

async fn run_service(
    store: SharedStore,
    commands: async_channel::Receiver<bool>,
    stop: async_channel::Receiver<()>,
    wake: WakeHandle,
) {
    let Some(connected) = stop_or(&stop, Connection::system_bus()).await else {
        return;
    };
    let mut connection = match connected {
        Ok(connection) => connection,
        Err(error) => {
            publish_snapshot(
                &store,
                classify_error(&NetworkManagerDetailsError::Transport(error)),
                &wake,
            );
            return;
        }
    };

    'monitor: loop {
        let Some(started) =
            stop_or(&stop, NetworkManagerDetailsMonitor::start(&mut connection)).await
        else {
            return;
        };
        let mut monitor = match started {
            Ok(monitor) => monitor,
            Err(error) if error.is_service_unavailable() => {
                publish_snapshot(&store, NetworkServiceSnapshot::Unavailable, &wake);
                match wait_for_owner(&stop, &mut connection).await {
                    Ok(true) => continue 'monitor,
                    Ok(false) => return,
                    Err(error) => {
                        publish_snapshot(&store, classify_root_error(&error), &wake);
                        return;
                    }
                }
            }
            Err(error) => {
                publish_snapshot(&store, classify_error(&error), &wake);
                return;
            }
        };
        publish_snapshot(
            &store,
            NetworkServiceSnapshot::Ready(monitor.snapshot().clone()),
            &wake,
        );
        publish_action(&store, NetworkActionState::Idle, &wake);

        loop {
            let mut received_command = None;
            let mut received_message = None;
            let mut stopped = false;
            {
                let stop_wait = stop.recv().fuse();
                let command_wait = commands.recv().fuse();
                let message_wait = connection.receive().fuse();
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
                let Ok(enabled) = command else {
                    return;
                };
                let Some(result) =
                    stop_or(&stop, set_wireless_enabled(&mut connection, enabled)).await
                else {
                    return;
                };
                if let Err(error) = result {
                    publish_action(&store, NetworkActionState::Failed(enabled), &wake);
                    if error.is_service_unavailable() {
                        publish_snapshot(&store, NetworkServiceSnapshot::Pending, &wake);
                        let _ = stop_or(&stop, monitor.close(&mut connection)).await;
                        continue 'monitor;
                    }
                    eprintln!(
                        "Tensor Shell NetworkManager wireless={enabled} request failed: {error}"
                    );
                    continue;
                }
                match stop_or(&stop, monitor.refresh(&mut connection)).await {
                    None => return,
                    Some(Ok(NetworkManagerDetailsEvent::Changed)) => publish_snapshot(
                        &store,
                        NetworkServiceSnapshot::Ready(monitor.snapshot().clone()),
                        &wake,
                    ),
                    Some(Ok(NetworkManagerDetailsEvent::Unchanged)) => {}
                    Some(Ok(
                        NetworkManagerDetailsEvent::Ignored
                        | NetworkManagerDetailsEvent::RefreshRequired
                        | NetworkManagerDetailsEvent::OwnerChanged,
                    )) => unreachable!("NetworkManager refresh returns only changed or unchanged"),
                    Some(Err(error)) => {
                        publish_action(&store, NetworkActionState::Failed(enabled), &wake);
                        publish_snapshot(&store, classify_error(&error), &wake);
                        return;
                    }
                }
                publish_action(&store, NetworkActionState::Succeeded(enabled), &wake);
                continue;
            }

            let message =
                match received_message.expect("NetworkManager select completed one branch") {
                    Ok(message) => message,
                    Err(error) => {
                        publish_snapshot(
                            &store,
                            classify_error(&NetworkManagerDetailsError::Transport(error)),
                            &wake,
                        );
                        return;
                    }
                };
            match monitor.observe(&message) {
                Ok(NetworkManagerDetailsEvent::Changed) => publish_external_snapshot(
                    &store,
                    NetworkServiceSnapshot::Ready(monitor.snapshot().clone()),
                    &wake,
                ),
                Ok(NetworkManagerDetailsEvent::RefreshRequired) => {
                    match stop_or(&stop, monitor.refresh(&mut connection)).await {
                        None => return,
                        Some(Ok(NetworkManagerDetailsEvent::Changed)) => publish_external_snapshot(
                            &store,
                            NetworkServiceSnapshot::Ready(monitor.snapshot().clone()),
                            &wake,
                        ),
                        Some(Ok(NetworkManagerDetailsEvent::Unchanged)) => {}
                        Some(Ok(_)) => {
                            unreachable!("NetworkManager refresh returns only changed or unchanged")
                        }
                        Some(Err(error)) => {
                            publish_snapshot(&store, classify_error(&error), &wake);
                            return;
                        }
                    }
                }
                Ok(NetworkManagerDetailsEvent::OwnerChanged) => {
                    publish_snapshot(&store, NetworkServiceSnapshot::Pending, &wake);
                    let Some(closed) = stop_or(&stop, monitor.close(&mut connection)).await else {
                        return;
                    };
                    if let Err(error) = closed {
                        publish_snapshot(
                            &store,
                            classify_error(&NetworkManagerDetailsError::Transport(error)),
                            &wake,
                        );
                        return;
                    }
                    continue 'monitor;
                }
                Ok(NetworkManagerDetailsEvent::Ignored | NetworkManagerDetailsEvent::Unchanged) => {
                }
                Err(error) => {
                    publish_snapshot(&store, classify_error(&error), &wake);
                    let _ = stop_or(&stop, monitor.close(&mut connection)).await;
                    return;
                }
            }
        }
    }
}

async fn wait_for_owner(
    stop: &async_channel::Receiver<()>,
    connection: &mut Connection,
) -> Result<bool, NetworkManagerError> {
    let mut rule = MatchRule::signal(
        Some(DESTINATION),
        Some(ROOT_PATH),
        Some(PROPERTIES_INTERFACE),
        Some("PropertiesChanged"),
    )?;
    let Some(added) = stop_or(stop, connection.add_match(&mut rule)).await else {
        return Ok(false);
    };
    added?;
    if rule.sender_available() {
        let Some(removed) = stop_or(stop, connection.remove_match(&rule)).await else {
            return Ok(false);
        };
        removed?;
        return Ok(true);
    }

    loop {
        let Some(received) = stop_or(stop, connection.receive()).await else {
            return Ok(false);
        };
        let message = received?;
        if rule.observe(&message)? && rule.sender_available() {
            let Some(removed) = stop_or(stop, connection.remove_match(&rule)).await else {
                return Ok(false);
            };
            removed?;
            return Ok(true);
        }
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

fn classify_error(error: &NetworkManagerDetailsError) -> NetworkServiceSnapshot {
    if error.is_service_unavailable() {
        NetworkServiceSnapshot::Unavailable
    } else {
        NetworkServiceSnapshot::Failed
    }
}

fn classify_root_error(error: &NetworkManagerError) -> NetworkServiceSnapshot {
    if error.is_service_unavailable() {
        NetworkServiceSnapshot::Unavailable
    } else {
        NetworkServiceSnapshot::Failed
    }
}

fn publish_snapshot(store: &SharedStore, snapshot: NetworkServiceSnapshot, wake: &WakeHandle) {
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
    snapshot: NetworkServiceSnapshot,
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

fn publish_action(store: &SharedStore, action: NetworkActionState, wake: &WakeHandle) {
    let changed = store
        .lock()
        .map(|mut store| store.publish_action(action))
        .unwrap_or(false);
    if changed {
        wake.wake();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NetworkServiceError {
    #[error("Tensor Shell NetworkManager command queue is full (capacity {0})")]
    CommandQueueFull(usize),
    #[error("Tensor Shell NetworkManager service has stopped")]
    Stopped,
    #[error("Tensor Shell NetworkManager state is not ready")]
    Unavailable,
    #[error("Tensor Shell wireless radio is unavailable")]
    WirelessUnavailable,
    #[error("Tensor Shell wireless radio already has an action in flight")]
    Busy,
}

#[cfg(test)]
mod tests {
    use tensor_dbus::Error;

    use super::*;

    #[test]
    fn missing_provider_is_distinct_from_invalid_network_state() {
        let unavailable = NetworkManagerDetailsError::Transport(Error::Method {
            name: "org.freedesktop.DBus.Error.ServiceUnknown".to_owned(),
            message: "not installed".to_owned(),
        });
        assert_eq!(
            classify_error(&unavailable),
            NetworkServiceSnapshot::Unavailable
        );
        let failed = NetworkManagerDetailsError::MissingProperty {
            interface: "org.freedesktop.NetworkManager",
            property: "Connectivity",
        };
        assert_eq!(classify_error(&failed), NetworkServiceSnapshot::Failed);
    }
}
