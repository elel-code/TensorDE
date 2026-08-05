use std::{
    collections::HashMap,
    num::NonZeroU64,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::Instant,
};

use futures_util::{FutureExt, pin_mut, select_biased};
#[cfg(test)]
use tensor_dbus::zvariant::Value;
use tensor_dbus::{
    BusAddress, Connection, MethodError, MethodResult, ObjectServer, RequestNameFlags,
    RequestNameReply, zvariant::OwnedValue,
};
use tensor_runtime::io_uring_runtime;

use crate::{
    CloseReason, ClosedNotification, NotificationAction, NotificationId, NotificationRequest,
    NotificationStore, NotificationTimeout, NotificationUrgency,
};

const BUS_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
const INTERFACE: &str = "org.freedesktop.Notifications";
const SIGNAL_CAPACITY: usize = 128;

type NotifyBody = (
    String,
    u32,
    String,
    String,
    String,
    Vec<String>,
    HashMap<String, OwnedValue>,
    i32,
);

pub(crate) type SharedNotificationStore = Arc<Mutex<NotificationStore>>;

pub(crate) struct NotificationServiceHandle {
    store: SharedNotificationStore,
    clock: Instant,
    commands: async_channel::Sender<ServiceCommand>,
    join: Option<JoinHandle<()>>,
}

impl NotificationServiceHandle {
    pub(crate) fn start(store: SharedNotificationStore) -> Result<Self, NotificationServiceError> {
        Self::start_with_address(store, None)
    }

    fn start_with_address(
        store: SharedNotificationStore,
        address: Option<BusAddress>,
    ) -> Result<Self, NotificationServiceError> {
        let clock = Instant::now();
        let (commands, command_rx) = async_channel::bounded(SIGNAL_CAPACITY);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let service_store = Arc::clone(&store);
        let join = thread::Builder::new()
            .name("tensor-shell-notifications".into())
            .spawn(move || run_thread(service_store, clock, command_rx, ready_tx, address))
            .map_err(NotificationServiceError::Spawn)?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                store,
                clock,
                commands,
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
        elapsed_ms(self.clock)
    }

    pub(crate) fn emit_closed(
        &self,
        notification: ClosedNotification,
    ) -> Result<(), NotificationServiceError> {
        self.commands
            .try_send(ServiceCommand::Closed(notification))
            .map_err(|error| match error {
                async_channel::TrySendError::Full(_) => NotificationServiceError::SignalQueueFull,
                async_channel::TrySendError::Closed(_) => NotificationServiceError::ServiceStopped,
            })
    }
}

impl Drop for NotificationServiceHandle {
    fn drop(&mut self) {
        self.commands.close();
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

#[derive(Clone, Copy, Debug)]
enum ServiceCommand {
    Closed(ClosedNotification),
}

fn run_thread(
    store: SharedNotificationStore,
    clock: Instant,
    commands: async_channel::Receiver<ServiceCommand>,
    ready: mpsc::SyncSender<Result<(), String>>,
    address: Option<BusAddress>,
) {
    let runtime = match io_uring_runtime(4) {
        Ok(runtime) => runtime,
        Err(error) => {
            let message = format!("create Compio io_uring runtime: {error}");
            let _ = ready.send(Err(message));
            return;
        }
    };
    if let Err(error) = runtime.block_on(run_service(store, clock, commands, ready, address)) {
        eprintln!("Tensor Shell notification service stopped: {error}");
    }
}

async fn run_service(
    store: SharedNotificationStore,
    clock: Instant,
    commands: async_channel::Receiver<ServiceCommand>,
    ready: mpsc::SyncSender<Result<(), String>>,
    address: Option<BusAddress>,
) -> Result<(), String> {
    let connected = match address {
        Some(address) => Connection::connect_bus(address).await,
        None => Connection::session_bus().await,
    };
    let mut connection = match connected {
        Ok(connection) => connection,
        Err(error) => return startup_error(ready, format!("connect to session D-Bus: {error}")),
    };
    let flags = RequestNameFlags::ALLOW_REPLACEMENT | RequestNameFlags::REPLACE_EXISTING;
    let ownership = match connection.request_name(BUS_NAME, flags).await {
        Ok(ownership) => ownership,
        Err(error) => return startup_error(ready, format!("request {BUS_NAME}: {error}")),
    };
    if !matches!(
        ownership,
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner
    ) {
        return startup_error(
            ready,
            format!("request {BUS_NAME}: bus returned ownership result {ownership:?}"),
        );
    }
    let mut objects = match notification_object_server(Arc::clone(&store), clock) {
        Ok(objects) => objects,
        Err(error) => {
            return startup_error(ready, format!("register notification objects: {error}"));
        }
    };
    if ready.send(Ok(())).is_err() {
        return Ok(());
    }

    loop {
        let mut received_message = None;
        let mut received_command = None;
        {
            let message = objects.serve_next(&mut connection).fuse();
            let command = commands.recv().fuse();
            pin_mut!(message, command);
            select_biased! {
                command = command => received_command = Some(command),
                message = message => received_message = Some(message),
            }
        }
        if let Some(command) = received_command {
            match command {
                Ok(ServiceCommand::Closed(closed)) => {
                    emit_closed(&mut connection, closed)
                        .await
                        .map_err(|error| format!("emit NotificationClosed: {error}"))?;
                }
                Err(_) => return Ok(()),
            }
        } else {
            match received_message.expect("select completed one service event") {
                Ok(message) => drop(message),
                Err(error) => return Err(format!("serve D-Bus message: {error}")),
            }
        }
    }
}

fn notification_object_server(
    store: SharedNotificationStore,
    clock: Instant,
) -> tensor_dbus::Result<ObjectServer> {
    let mut objects = ObjectServer::new();
    objects.register::<(), Vec<String>, _, _>(
        OBJECT_PATH,
        INTERFACE,
        "GetCapabilities",
        |()| async { Ok(vec!["actions".to_owned(), "body".to_owned()]) },
    )?;
    objects.register::<(), (String, String, String, String), _, _>(
        OBJECT_PATH,
        INTERFACE,
        "GetServerInformation",
        |()| async {
            Ok((
                "Tensor Shell".to_owned(),
                "TensorDE".to_owned(),
                env!("CARGO_PKG_VERSION").to_owned(),
                "1.2".to_owned(),
            ))
        },
    )?;
    let notify_store = Arc::clone(&store);
    objects.register::<NotifyBody, u32, _, _>(OBJECT_PATH, INTERFACE, "Notify", move |body| {
        let result = notify(&notify_store, clock, body);
        async move { result }
    })?;
    objects.register_with_connection::<u32, (), _>(
        OBJECT_PATH,
        INTERFACE,
        "CloseNotification",
        async move |connection: &mut Connection, _context, id| {
            if let Some(closed) = close_notification(&store, clock, id)? {
                emit_closed(connection, closed)
                    .await
                    .map_err(|error| MethodError::failed(error.to_string()))?;
            }
            Ok(())
        },
    )?;
    objects.register_signal::<(u32, u32)>(OBJECT_PATH, INTERFACE, "NotificationClosed")?;
    objects.register_signal::<(u32, String)>(OBJECT_PATH, INTERFACE, "ActionInvoked")?;
    Ok(objects)
}

fn notify(store: &SharedNotificationStore, clock: Instant, body: NotifyBody) -> MethodResult<u32> {
    let (app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout) = body;
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
    store
        .lock()
        .map(|mut store| store.notify(request, elapsed_ms(clock)).get())
        .map_err(|_| MethodError::failed("notification store lock poisoned"))
}

fn close_notification(
    store: &SharedNotificationStore,
    clock: Instant,
    id: u32,
) -> MethodResult<Option<ClosedNotification>> {
    store
        .lock()
        .map(|mut store| {
            NotificationId::from_raw(id)
                .and_then(|id| store.close(id, CloseReason::ClosedByApplication, elapsed_ms(clock)))
        })
        .map_err(|_| MethodError::failed("notification store lock poisoned"))
}

async fn emit_closed(
    connection: &mut Connection,
    closed: ClosedNotification,
) -> tensor_dbus::Result<()> {
    connection
        .emit_signal(
            OBJECT_PATH,
            INTERFACE,
            "NotificationClosed",
            &(closed.id.get(), close_reason_code(closed.reason)),
        )
        .await
}

fn startup_error(
    ready: mpsc::SyncSender<Result<(), String>>,
    message: String,
) -> Result<(), String> {
    let _ = ready.send(Err(message.clone()));
    Err(message)
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
    use std::{
        fs,
        io::{BufRead, BufReader},
        path::PathBuf,
        process::{Child, Command, Stdio},
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    struct PrivateBus {
        child: Child,
        socket: PathBuf,
        address: BusAddress,
    }

    impl PrivateBus {
        fn start() -> Option<Self> {
            static NEXT_BUS: AtomicU64 = AtomicU64::new(1);
            let socket = std::env::current_dir()
                .unwrap()
                .join("target")
                .join(format!(
                    "tensor-shell-dbus-{}-{}.sock",
                    std::process::id(),
                    NEXT_BUS.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(socket.parent().unwrap()).unwrap();
            let _ = fs::remove_file(&socket);
            let address_text = format!("unix:path={}", socket.display());
            let mut child = match Command::new("dbus-daemon")
                .args([
                    "--session",
                    "--nofork",
                    "--nopidfile",
                    "--print-address=1",
                    "--address",
                    &address_text,
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
            {
                Ok(child) => child,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
                Err(error) => panic!("failed to start private dbus-daemon: {error}"),
            };
            let mut announced = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut announced)
                .expect("dbus-daemon did not announce its address");
            assert!(announced.trim().starts_with(&address_text));
            Some(Self {
                child,
                socket,
                address: BusAddress::parse(&address_text).unwrap(),
            })
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = fs::remove_file(&self.socket);
        }
    }

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
        let Some(bus) = PrivateBus::start() else {
            return;
        };
        let store = Arc::new(Mutex::new(NotificationStore::default()));
        let service = NotificationServiceHandle::start_with_address(
            Arc::clone(&store),
            Some(bus.address.clone()),
        )
        .unwrap();
        io_uring_runtime(4).unwrap().block_on(async {
            let mut connection = Connection::connect_bus(bus.address.clone()).await.unwrap();
            let mut proxy = tensor_dbus::Proxy::new(
                &mut connection,
                Some(BUS_NAME),
                OBJECT_PATH,
                Some(INTERFACE),
            )
            .unwrap();
            let capabilities: Vec<String> = proxy.call("GetCapabilities", &()).await.unwrap();
            assert_eq!(capabilities, ["actions", "body"]);
            let information: (String, String, String, String) =
                proxy.call("GetServerInformation", &()).await.unwrap();
            assert_eq!(information.0, "Tensor Shell");
            assert_eq!(information.1, "TensorDE");
            assert_eq!(information.2, env!("CARGO_PKG_VERSION"));
            assert_eq!(information.3, "1.2");
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
            let closed = proxy.subscribe("NotificationClosed").await.unwrap();
            let _: () = proxy.call("CloseNotification", &id.get()).await.unwrap();
            assert!(store.lock().unwrap().active(id).is_none());
            let mut signals = proxy.signal_stream(closed);
            let signal = signals.next().await.unwrap();
            assert_eq!(signal.body::<(u32, u32)>().unwrap(), (id.get(), 3));
            let _ = signals.close().await.unwrap();
            drop(proxy);

            let mut introspectable = tensor_dbus::Proxy::new(
                &mut connection,
                Some(BUS_NAME),
                OBJECT_PATH,
                Some("org.freedesktop.DBus.Introspectable"),
            )
            .unwrap();
            let xml: String = introspectable.call("Introspect", &()).await.unwrap();
            for name in [
                "GetCapabilities",
                "GetServerInformation",
                "Notify",
                "CloseNotification",
                "NotificationClosed",
                "ActionInvoked",
            ] {
                assert!(xml.contains(name), "missing {name} from {xml}");
            }
            assert!(!xml.contains("GetMachineId"));
        });
        drop(service);
    }
}
