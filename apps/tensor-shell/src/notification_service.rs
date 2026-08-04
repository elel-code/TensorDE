use std::{
    collections::HashMap,
    num::NonZeroU64,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use async_io::Timer;
use futures_lite::future::block_on;
use zbus::{fdo, object_server::SignalEmitter, zvariant::OwnedValue};

use crate::{
    CloseReason, ClosedNotification, NotificationAction, NotificationId, NotificationRequest,
    NotificationStore, NotificationTimeout, NotificationUrgency,
};

const BUS_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
const SIGNAL_CAPACITY: usize = 128;

pub(crate) type SharedNotificationStore = Arc<Mutex<NotificationStore>>;

pub(crate) struct NotificationServiceHandle {
    store: SharedNotificationStore,
    clock: Instant,
    closed_tx: SyncSender<ClosedNotification>,
    stop_tx: mpsc::Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl NotificationServiceHandle {
    pub(crate) fn start(store: SharedNotificationStore) -> Result<Self, NotificationServiceError> {
        let clock = Instant::now();
        let (closed_tx, closed_rx) = mpsc::sync_channel(SIGNAL_CAPACITY);
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let service_store = Arc::clone(&store);
        let join = thread::Builder::new()
            .name("tensor-shell-notifications".into())
            .spawn(move || {
                let result = block_on(run_service(
                    service_store,
                    clock,
                    closed_rx,
                    stop_rx,
                    ready_tx,
                ));
                if let Err(error) = result {
                    eprintln!("Tensor Shell notification service stopped: {error}");
                }
            })
            .map_err(NotificationServiceError::Spawn)?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                store,
                clock,
                closed_tx,
                stop_tx,
                join: Some(join),
            }),
            Ok(Err(message)) => {
                let _ = join.join();
                Err(NotificationServiceError::Start(message))
            }
            Err(_) => {
                let _ = join.join();
                Err(NotificationServiceError::StartupDisconnected)
            }
        }
    }

    pub(crate) fn store(&self) -> &SharedNotificationStore {
        &self.store
    }

    pub(crate) fn now_ms(&self) -> u64 {
        u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    pub(crate) fn emit_closed(
        &self,
        notification: ClosedNotification,
    ) -> Result<(), NotificationServiceError> {
        self.closed_tx
            .try_send(notification)
            .map_err(|error| match error {
                TrySendError::Full(_) => NotificationServiceError::SignalQueueFull,
                TrySendError::Disconnected(_) => NotificationServiceError::ServiceStopped,
            })
    }
}

impl Drop for NotificationServiceHandle {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NotificationServiceError {
    #[error("failed to spawn Tensor Shell notification service: {0}")]
    Spawn(std::io::Error),
    #[error("failed to start Tensor Shell notification service: {0}")]
    Start(String),
    #[error("Tensor Shell notification service stopped during startup")]
    StartupDisconnected,
    #[error("Tensor Shell notification signal queue is full")]
    SignalQueueFull,
    #[error("Tensor Shell notification service has stopped")]
    ServiceStopped,
}

async fn run_service(
    store: SharedNotificationStore,
    clock: Instant,
    closed_rx: Receiver<ClosedNotification>,
    stop_rx: Receiver<()>,
    ready_tx: SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let builder = zbus::connection::Builder::session()
        .map_err(|error| format!("connect to session D-Bus: {error}"))?
        .name(BUS_NAME)
        .map_err(|error| format!("request {BUS_NAME}: {error}"))?
        .allow_name_replacements(true)
        .replace_existing_names(true)
        .serve_at(OBJECT_PATH, NotificationDbus { store, clock })
        .map_err(|error| format!("register {OBJECT_PATH}: {error}"))?;
    let connection = match builder.build().await {
        Ok(connection) => connection,
        Err(error) => {
            let message = format!("build D-Bus service: {error}");
            let _ = ready_tx.send(Err(message.clone()));
            return Err(message);
        }
    };
    let interface = connection
        .object_server()
        .interface::<_, NotificationDbus>(OBJECT_PATH)
        .await
        .map_err(|error| format!("resolve notification interface: {error}"))?;
    let emitter = interface.signal_emitter().clone();
    if ready_tx.send(Ok(())).is_err() {
        return Ok(());
    }

    loop {
        match stop_rx.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => return Ok(()),
            Err(TryRecvError::Empty) => {}
        }
        loop {
            match closed_rx.try_recv() {
                Ok(closed) => NotificationDbus::notification_closed(
                    &emitter,
                    closed.id.get(),
                    close_reason_code(closed.reason),
                )
                .await
                .map_err(|error| format!("emit NotificationClosed: {error}"))?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        Timer::after(Duration::from_millis(20)).await;
    }
}

struct NotificationDbus {
    store: SharedNotificationStore,
    clock: Instant,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl NotificationDbus {
    #[zbus(name = "GetCapabilities")]
    fn get_capabilities(&self) -> Vec<&str> {
        vec!["actions", "body"]
    }

    #[zbus(name = "GetServerInformation")]
    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        ("Tensor Shell", "TensorDE", env!("CARGO_PKG_VERSION"), "1.2")
    }

    #[zbus(name = "Notify")]
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> fdo::Result<u32> {
        let request = notification_request(WireNotification {
            app_name,
            replaces_id,
            app_icon,
            summary,
            body,
            actions,
            hints: &hints,
            expire_timeout,
        });
        let mut store = self
            .store
            .lock()
            .map_err(|_| fdo::Error::Failed("notification store lock poisoned".into()))?;
        Ok(store.notify(request, elapsed_ms(self.clock)).get())
    }

    #[zbus(name = "CloseNotification")]
    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let Some(id) = NotificationId::from_raw(id) else {
            return Ok(());
        };
        let closed = self
            .store
            .lock()
            .map_err(|_| fdo::Error::Failed("notification store lock poisoned".into()))?
            .close(id, CloseReason::ClosedByApplication, elapsed_ms(self.clock));
        if let Some(closed) = closed {
            Self::notification_closed(&emitter, closed.id.get(), close_reason_code(closed.reason))
                .await
                .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        }
        Ok(())
    }

    #[zbus(signal, name = "NotificationClosed")]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "ActionInvoked")]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

struct WireNotification<'a> {
    app_name: String,
    replaces_id: u32,
    app_icon: String,
    summary: String,
    body: String,
    actions: Vec<String>,
    hints: &'a HashMap<String, OwnedValue>,
    expire_timeout: i32,
}

fn notification_request(wire: WireNotification<'_>) -> NotificationRequest {
    NotificationRequest {
        replaces: NotificationId::from_raw(wire.replaces_id),
        app_name: wire.app_name,
        desktop_entry: hint_string(wire.hints, "desktop-entry"),
        summary: wire.summary,
        body: wire.body,
        icon: (!wire.app_icon.is_empty()).then_some(wire.app_icon),
        urgency: match hint_u8(wire.hints, "urgency") {
            Some(0) => NotificationUrgency::Low,
            Some(2) => NotificationUrgency::Critical,
            _ => NotificationUrgency::Normal,
        },
        timeout: match wire.expire_timeout {
            value if value < 0 => NotificationTimeout::Default,
            0 => NotificationTimeout::Never,
            value => NotificationTimeout::Milliseconds(NonZeroU64::new(value as u64).unwrap()),
        },
        transient: hint_bool(wire.hints, "transient").unwrap_or(false),
        actions: wire
            .actions
            .chunks_exact(2)
            .map(|pair| NotificationAction {
                key: pair[0].clone(),
                label: pair[1].clone(),
            })
            .collect(),
    }
}

fn hint_u8(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<u8> {
    hints
        .get(key)
        .and_then(|value| u8::try_from(value.clone()).ok())
}

fn hint_bool(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    hints
        .get(key)
        .and_then(|value| bool::try_from(value.clone()).ok())
}

fn hint_string(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    hints
        .get(key)
        .and_then(|value| String::try_from(value.clone()).ok())
        .filter(|value| !value.is_empty())
}

fn elapsed_ms(clock: Instant) -> u64 {
    u64::try_from(clock.elapsed().as_millis()).unwrap_or(u64::MAX)
}

const fn close_reason_code(reason: CloseReason) -> u32 {
    match reason {
        CloseReason::Expired => 1,
        CloseReason::DismissedByUser => 2,
        CloseReason::ClosedByApplication => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    #[test]
    fn wire_request_maps_replacement_hints_timeout_and_action_pairs() {
        let mut hints = HashMap::new();
        hints.insert(
            "urgency".into(),
            OwnedValue::try_from(Value::new(2_u8)).unwrap(),
        );
        hints.insert(
            "transient".into(),
            OwnedValue::try_from(Value::new(true)).unwrap(),
        );
        hints.insert(
            "desktop-entry".into(),
            OwnedValue::try_from(Value::new("org.tensor.Mail")).unwrap(),
        );
        let request = notification_request(WireNotification {
            app_name: "Mail".into(),
            replaces_id: 7,
            app_icon: "mail-unread".into(),
            summary: "Subject".into(),
            body: "Body".into(),
            actions: vec!["open".into(), "Open".into(), "dangling".into()],
            hints: &hints,
            expire_timeout: 2_500,
        });
        assert_eq!(request.replaces.unwrap().get(), 7);
        assert_eq!(request.urgency, NotificationUrgency::Critical);
        assert!(request.transient);
        assert_eq!(request.desktop_entry.as_deref(), Some("org.tensor.Mail"));
        assert_eq!(request.actions.len(), 1);
        assert_eq!(
            request.timeout,
            NotificationTimeout::Milliseconds(NonZeroU64::new(2_500).unwrap())
        );
    }

    #[test]
    fn wire_timeout_values_follow_freedesktop_semantics() {
        let hints = HashMap::new();
        let default = notification_request(WireNotification {
            app_name: String::new(),
            replaces_id: 0,
            app_icon: String::new(),
            summary: String::new(),
            body: String::new(),
            actions: vec![],
            hints: &hints,
            expire_timeout: -1,
        });
        let never = notification_request(WireNotification {
            app_name: String::new(),
            replaces_id: 0,
            app_icon: String::new(),
            summary: String::new(),
            body: String::new(),
            actions: vec![],
            hints: &hints,
            expire_timeout: 0,
        });
        assert_eq!(default.timeout, NotificationTimeout::Default);
        assert_eq!(never.timeout, NotificationTimeout::Never);
    }

    #[test]
    fn private_session_bus_round_trip_updates_the_store() {
        if std::env::var_os("TENSOR_SHELL_DBUS_TEST").is_none() {
            return;
        }
        let store = Arc::new(Mutex::new(NotificationStore::default()));
        let service = NotificationServiceHandle::start(Arc::clone(&store)).unwrap();
        block_on(async {
            let connection = zbus::Connection::session().await.unwrap();
            let proxy = zbus::Proxy::new(
                &connection,
                BUS_NAME,
                OBJECT_PATH,
                "org.freedesktop.Notifications",
            )
            .await
            .unwrap();
            let capabilities: Vec<String> = proxy.call("GetCapabilities", &()).await.unwrap();
            assert_eq!(capabilities, ["actions", "body"]);
            let id: u32 = proxy
                .call(
                    "Notify",
                    &(
                        "Mail",
                        0_u32,
                        "mail-unread",
                        "Subject",
                        "Body",
                        Vec::<String>::new(),
                        HashMap::<String, OwnedValue>::new(),
                        -1_i32,
                    ),
                )
                .await
                .unwrap();
            let id = NotificationId::from_raw(id).unwrap();
            assert_eq!(store.lock().unwrap().active(id).unwrap().summary, "Subject");
            let _: () = proxy.call("CloseNotification", &(id.get(),)).await.unwrap();
            assert!(store.lock().unwrap().active(id).is_none());
        });
        drop(service);
    }
}
