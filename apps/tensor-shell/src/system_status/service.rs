use std::{
    future::Future,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use futures_util::{FutureExt, pin_mut, select_biased};
use tensor_dbus::{
    Connection,
    freedesktop::upower::{UPowerError, UPowerMonitor, UPowerMonitorEvent},
};
use tensor_runtime::io_uring_runtime;

use super::{PowerServiceSnapshot, PowerServiceStore};

type SharedStore = Arc<Mutex<PowerServiceStore>>;

pub(crate) struct PowerServiceHandle {
    store: SharedStore,
    stop: async_channel::Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl PowerServiceHandle {
    pub(crate) fn start() -> Self {
        let store = Arc::new(Mutex::new(PowerServiceStore::default()));
        let (stop, stop_rx) = async_channel::bounded(1);
        let worker_store = Arc::clone(&store);
        let join = match thread::Builder::new()
            .name("tensor-shell-power".into())
            .spawn(move || run_thread(worker_store, stop_rx))
        {
            Ok(join) => Some(join),
            Err(error) => {
                publish(&store, PowerServiceSnapshot::Failed);
                eprintln!("Tensor Shell could not start its UPower adapter: {error}");
                None
            }
        };
        Self { store, stop, join }
    }

    pub(crate) fn read(&self) -> (u64, PowerServiceSnapshot) {
        self.store
            .lock()
            .map(|store| store.read())
            .unwrap_or((u64::MAX, PowerServiceSnapshot::Failed))
    }
}

impl Drop for PowerServiceHandle {
    fn drop(&mut self) {
        self.stop.close();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_thread(store: SharedStore, stop: async_channel::Receiver<()>) {
    let runtime = match io_uring_runtime(4) {
        Ok(runtime) => runtime,
        Err(error) => {
            publish(&store, PowerServiceSnapshot::Failed);
            eprintln!("Tensor Shell could not create its UPower io_uring runtime: {error}");
            return;
        }
    };
    runtime.block_on(run_service(store, stop));
}

async fn run_service(store: SharedStore, stop: async_channel::Receiver<()>) {
    let Some(connection) = stop_or(&stop, Connection::system_bus()).await else {
        return;
    };
    let mut connection = match connection {
        Ok(connection) => connection,
        Err(error) => {
            publish(&store, classify_error(&UPowerError::Transport(error)));
            return;
        }
    };

    'monitor: loop {
        let Some(started) = stop_or(&stop, UPowerMonitor::start(&mut connection)).await else {
            return;
        };
        let mut monitor = match started {
            Ok(monitor) => monitor,
            Err(error) => {
                publish(&store, classify_error(&error));
                return;
            }
        };
        publish(
            &store,
            PowerServiceSnapshot::Ready(monitor.snapshot().clone()),
        );

        loop {
            let Some(received) = stop_or(&stop, connection.receive()).await else {
                return;
            };
            let message = match received {
                Ok(message) => message,
                Err(error) => {
                    publish(&store, classify_error(&UPowerError::Transport(error)));
                    return;
                }
            };
            match monitor.observe(&message) {
                Ok(UPowerMonitorEvent::Changed) => publish(
                    &store,
                    PowerServiceSnapshot::Ready(monitor.snapshot().clone()),
                ),
                Ok(UPowerMonitorEvent::RefreshRequired | UPowerMonitorEvent::OwnerChanged) => {
                    publish(&store, PowerServiceSnapshot::Pending);
                    let Some(closed) = stop_or(&stop, monitor.close(&mut connection)).await else {
                        return;
                    };
                    if let Err(error) = closed {
                        publish(&store, classify_error(&UPowerError::Transport(error)));
                        return;
                    }
                    continue 'monitor;
                }
                Ok(UPowerMonitorEvent::Ignored | UPowerMonitorEvent::Unchanged) => {}
                Err(error) => {
                    publish(&store, classify_error(&error));
                    return;
                }
            }
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

fn classify_error(error: &UPowerError) -> PowerServiceSnapshot {
    if error.is_service_unavailable() {
        PowerServiceSnapshot::Unavailable
    } else {
        PowerServiceSnapshot::Failed
    }
}

fn publish(store: &SharedStore, snapshot: PowerServiceSnapshot) {
    if let Ok(mut store) = store.lock() {
        store.publish(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tensor_dbus::Error;

    #[test]
    fn missing_provider_is_not_reported_as_a_decode_failure() {
        let unavailable = UPowerError::Transport(Error::Method {
            name: "org.freedesktop.DBus.Error.ServiceUnknown".to_owned(),
            message: "not installed".to_owned(),
        });
        assert_eq!(
            classify_error(&unavailable),
            PowerServiceSnapshot::Unavailable
        );

        let failed = UPowerError::MissingProperty {
            interface: "org.freedesktop.UPower.Device",
            property: "Percentage",
        };
        assert_eq!(classify_error(&failed), PowerServiceSnapshot::Failed);
    }
}
