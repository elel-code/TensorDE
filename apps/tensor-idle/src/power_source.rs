use std::{
    future::Future,
    io,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use futures_util::{FutureExt, pin_mut, select_biased};
use tensor_dbus::{
    Connection,
    freedesktop::upower::{UPowerError, UPowerMonitor, UPowerMonitorEvent},
};
use wayland_client_runtime::WakeHandle;

use crate::PowerSource;

const RETRY_DELAY: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PowerSourceStatus {
    #[default]
    Pending,
    Ready(PowerSource),
    Unavailable,
    Failed,
}

#[derive(Debug, Default)]
struct PowerSourceStore {
    generation: u64,
    status: PowerSourceStatus,
}

impl PowerSourceStore {
    fn publish(&mut self, status: PowerSourceStatus) -> bool {
        if self.status == status {
            return false;
        }
        self.status = status;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    const fn read(&self) -> (u64, PowerSourceStatus) {
        (self.generation, self.status)
    }
}

type SharedStore = Arc<Mutex<PowerSourceStore>>;

/// Application-owned UPower task whose Compio runtime stays off the Wayland thread.
///
/// The shared state is a single coalesced snapshot. Every generation change wakes
/// the Wayland completion loop, so repeated battery telemetry cannot grow a queue.
pub struct PowerSourceService {
    store: SharedStore,
    stop: async_channel::Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl PowerSourceService {
    pub fn start(wake: WakeHandle) -> Result<Self, PowerSourceServiceError> {
        let store = Arc::new(Mutex::new(PowerSourceStore::default()));
        let (stop, stop_rx) = async_channel::bounded(1);
        let worker_store = Arc::clone(&store);
        let join = thread::Builder::new()
            .name("tensor-idle-power".to_owned())
            .spawn(move || run_thread(worker_store, wake, stop_rx))
            .map_err(PowerSourceServiceError::Spawn)?;
        Ok(Self {
            store,
            stop,
            join: Some(join),
        })
    }

    pub fn read(&self) -> (u64, PowerSourceStatus) {
        self.store
            .lock()
            .map(|store| store.read())
            .unwrap_or((u64::MAX, PowerSourceStatus::Failed))
    }
}

impl Drop for PowerSourceService {
    fn drop(&mut self) {
        self.stop.close();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_thread(store: SharedStore, wake: WakeHandle, stop: async_channel::Receiver<()>) {
    let runtime = match tensor_runtime::io_uring_runtime(8) {
        Ok(runtime) => runtime,
        Err(_) => {
            publish(&store, &wake, PowerSourceStatus::Failed);
            return;
        }
    };
    runtime.block_on(run_service(store, wake, stop));
}

async fn run_service(store: SharedStore, wake: WakeHandle, stop: async_channel::Receiver<()>) {
    loop {
        let Some(connected) = stop_or(&stop, Connection::system_bus()).await else {
            return;
        };
        let mut connection = match connected {
            Ok(connection) => connection,
            Err(error) => {
                publish(
                    &store,
                    &wake,
                    classify_error(&UPowerError::Transport(error)),
                );
                if !retry_or_stop(&stop).await {
                    return;
                }
                continue;
            }
        };

        'monitor: loop {
            let Some(started) = stop_or(&stop, UPowerMonitor::start(&mut connection)).await else {
                return;
            };
            let mut monitor = match started {
                Ok(monitor) => monitor,
                Err(error) => {
                    publish(&store, &wake, classify_error(&error));
                    break 'monitor;
                }
            };
            publish(
                &store,
                &wake,
                PowerSourceStatus::Ready(map_source(monitor.snapshot().source())),
            );

            loop {
                let Some(received) = stop_or(&stop, connection.receive()).await else {
                    return;
                };
                let message = match received {
                    Ok(message) => message,
                    Err(error) => {
                        publish(
                            &store,
                            &wake,
                            classify_error(&UPowerError::Transport(error)),
                        );
                        break 'monitor;
                    }
                };
                match monitor.observe(&message) {
                    Ok(UPowerMonitorEvent::Changed) => publish(
                        &store,
                        &wake,
                        PowerSourceStatus::Ready(map_source(monitor.snapshot().source())),
                    ),
                    Ok(UPowerMonitorEvent::RefreshRequired | UPowerMonitorEvent::OwnerChanged) => {
                        publish(&store, &wake, PowerSourceStatus::Pending);
                        let Some(closed) = stop_or(&stop, monitor.close(&mut connection)).await
                        else {
                            return;
                        };
                        if let Err(error) = closed {
                            publish(
                                &store,
                                &wake,
                                classify_error(&UPowerError::Transport(error)),
                            );
                            break 'monitor;
                        }
                        continue 'monitor;
                    }
                    Ok(UPowerMonitorEvent::Ignored | UPowerMonitorEvent::Unchanged) => {}
                    Err(error) => {
                        publish(&store, &wake, classify_error(&error));
                        break 'monitor;
                    }
                }
            }
        }

        if !retry_or_stop(&stop).await {
            return;
        }
    }
}

async fn retry_or_stop(stop: &async_channel::Receiver<()>) -> bool {
    stop_or(stop, compio::runtime::time::sleep(RETRY_DELAY))
        .await
        .is_some()
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

const fn map_source(source: tensor_dbus::freedesktop::upower::PowerSource) -> PowerSource {
    match source {
        tensor_dbus::freedesktop::upower::PowerSource::Ac => PowerSource::Ac,
        tensor_dbus::freedesktop::upower::PowerSource::Battery => PowerSource::Battery,
    }
}

fn classify_error(error: &UPowerError) -> PowerSourceStatus {
    if error.is_service_unavailable() {
        PowerSourceStatus::Unavailable
    } else {
        PowerSourceStatus::Failed
    }
}

fn publish(store: &SharedStore, wake: &WakeHandle, status: PowerSourceStatus) {
    let changed = store
        .lock()
        .map(|mut store| store.publish(status))
        .unwrap_or(false);
    if changed {
        wake.wake();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PowerSourceServiceError {
    #[error("failed to start Tensor Idle's UPower task: {0}")]
    Spawn(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tensor_dbus::Error;

    #[test]
    fn store_coalesces_repeated_sources_and_keeps_latest_generation() {
        let mut store = PowerSourceStore::default();
        assert!(store.publish(PowerSourceStatus::Ready(PowerSource::Ac)));
        assert!(!store.publish(PowerSourceStatus::Ready(PowerSource::Ac)));
        assert!(store.publish(PowerSourceStatus::Ready(PowerSource::Battery)));
        assert_eq!(
            store.read(),
            (2, PowerSourceStatus::Ready(PowerSource::Battery))
        );
    }

    #[test]
    fn unavailable_provider_is_distinct_from_invalid_service_data() {
        let unavailable = UPowerError::Transport(Error::Method {
            name: "org.freedesktop.DBus.Error.ServiceUnknown".to_owned(),
            message: "not installed".to_owned(),
        });
        assert_eq!(classify_error(&unavailable), PowerSourceStatus::Unavailable);

        let invalid = UPowerError::MissingProperty {
            interface: "org.freedesktop.UPower.Device",
            property: "Percentage",
        };
        assert_eq!(classify_error(&invalid), PowerSourceStatus::Failed);
    }
}
