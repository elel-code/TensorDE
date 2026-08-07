use std::{
    cell::Cell,
    collections::HashMap,
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use compio::{
    driver::{DriverType, ProactorBuilder},
    runtime::RuntimeBuilder,
};
use tensor_dbus::{
    BusAddress, Connection, ObjectServer, PropertyChangeMode, RequestNameFlags, RequestNameReply,
    freedesktop::network_manager::{
        Connectivity, DESTINATION, NetworkManagerMonitor, NetworkManagerMonitorEvent, NetworkState,
        PrimaryConnectionKind, ROOT_INTERFACE, ROOT_PATH, set_wireless_enabled,
        wifi::{
            ACCESS_POINT_INTERFACE, DEVICE_INTERFACE, NetworkManagerDetailsMonitor,
            WIRELESS_INTERFACE, request_scan,
        },
    },
};

#[test]
fn network_manager_monitor_and_wireless_set_interoperate_on_a_private_bus() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping NetworkManager integration test: dbus-daemon is unavailable");
        return;
    };
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let address = BusAddress::parse(&bus.address).unwrap();
        let mut service = Connection::connect_bus(address.clone()).await.unwrap();
        assert_eq!(
            service
                .request_name(DESTINATION, RequestNameFlags::default())
                .await
                .unwrap(),
            RequestNameReply::PrimaryOwner
        );

        let wireless = Rc::new(Cell::new(true));
        let service_wireless = Rc::clone(&wireless);
        let server = compio::runtime::spawn(async move {
            let mut objects = network_manager_object(service_wireless);
            let mut served = 0;
            while served < 2 {
                if objects.serve_next(&mut service).await.unwrap().is_none() {
                    served += 1;
                }
            }
        });

        let mut client = Connection::connect_bus(address).await.unwrap();
        let mut monitor = NetworkManagerMonitor::start(&mut client).await.unwrap();
        assert!(monitor.snapshot().effective_wireless_enabled());
        assert_eq!(monitor.snapshot().state(), NetworkState::ConnectedGlobal);
        assert_eq!(monitor.snapshot().connectivity(), Connectivity::Full);
        assert_eq!(
            monitor.snapshot().primary_connection(),
            PrimaryConnectionKind::Wifi
        );

        set_wireless_enabled(&mut client, false).await.unwrap();
        let signal = loop {
            let message = client.receive().await.unwrap();
            if message.member() == Some("PropertiesChanged") {
                break message;
            }
        };
        assert_eq!(
            monitor.observe(&signal).unwrap(),
            NetworkManagerMonitorEvent::Changed
        );
        assert!(!monitor.snapshot().wireless_enabled());
        assert!(!wireless.get());
        monitor.close(&mut client).await.unwrap();
        server.await.unwrap();
    });
}

fn network_manager_object(wireless: Rc<Cell<bool>>) -> ObjectServer {
    let mut objects = ObjectServer::new();
    objects
        .register_read_only_property::<bool, _, _>(
            ROOT_PATH,
            ROOT_INTERFACE,
            "NetworkingEnabled",
            PropertyChangeMode::Value,
            || async { Ok(true) },
        )
        .unwrap();
    let get_wireless = Rc::clone(&wireless);
    let set_wireless = Rc::clone(&wireless);
    objects
        .register_property::<bool, _, _, _, _, _>(
            ROOT_PATH,
            ROOT_INTERFACE,
            "WirelessEnabled",
            PropertyChangeMode::Value,
            move || {
                let enabled = get_wireless.get();
                async move { Ok(enabled) }
            },
            move |enabled| {
                set_wireless.set(enabled);
                async { Ok(()) }
            },
        )
        .unwrap();
    objects
        .register_read_only_property::<bool, _, _>(
            ROOT_PATH,
            ROOT_INTERFACE,
            "WirelessHardwareEnabled",
            PropertyChangeMode::Value,
            || async { Ok(true) },
        )
        .unwrap();
    objects
        .register_read_only_property::<u32, _, _>(
            ROOT_PATH,
            ROOT_INTERFACE,
            "State",
            PropertyChangeMode::Value,
            || async { Ok(70) },
        )
        .unwrap();
    objects
        .register_read_only_property::<u32, _, _>(
            ROOT_PATH,
            ROOT_INTERFACE,
            "Connectivity",
            PropertyChangeMode::Value,
            || async { Ok(4) },
        )
        .unwrap();
    objects
        .register_read_only_property::<String, _, _>(
            ROOT_PATH,
            ROOT_INTERFACE,
            "PrimaryConnectionType",
            PropertyChangeMode::Value,
            || async { Ok("802-11-wireless".to_owned()) },
        )
        .unwrap();
    objects
}

#[test]
fn network_manager_details_pipeline_reads_wifi_and_scan_is_typed() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping NetworkManager Wi-Fi integration test: dbus-daemon is unavailable");
        return;
    };
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let address = BusAddress::parse(&bus.address).unwrap();
        let mut service = Connection::connect_bus(address.clone()).await.unwrap();
        assert_eq!(
            service
                .request_name(DESTINATION, RequestNameFlags::default())
                .await
                .unwrap(),
            RequestNameReply::PrimaryOwner
        );
        let scanned = Rc::new(Cell::new(false));
        let server_scanned = Rc::clone(&scanned);
        let server = compio::runtime::spawn(async move {
            let mut objects = network_manager_details_object(server_scanned);
            let mut served = 0;
            while served < 7 {
                if objects.serve_next(&mut service).await.unwrap().is_none() {
                    served += 1;
                }
            }
        });

        let mut client = Connection::connect_bus(address).await.unwrap();
        let monitor = NetworkManagerDetailsMonitor::start(&mut client)
            .await
            .unwrap();
        let snapshot = monitor.snapshot();
        assert_eq!(snapshot.wifi().devices().len(), 1);
        assert_eq!(snapshot.wifi().access_points().len(), 1);
        let device = &snapshot.wifi().devices()[0];
        assert_eq!(device.interface_name(), "wlan0");
        assert_eq!(device.active_access_point().unwrap().as_str(), AP_PATH);
        let point = snapshot.wifi().active_access_point().unwrap();
        assert_eq!(point.ssid(), &[b'T', 0xff, b'N']);
        assert_eq!(point.ssid_display(), "T\u{fffd}N");
        assert!(point.secured());
        assert!(point.enterprise());
        request_scan(&mut client, device.path()).await.unwrap();
        assert!(scanned.get());
        monitor.close(&mut client).await.unwrap();
        server.await.unwrap();
    });
}

const DEVICE_PATH: &str = "/org/freedesktop/NetworkManager/Devices/1";
const AP_PATH: &str = "/org/freedesktop/NetworkManager/AccessPoint/1";

fn network_manager_details_object(scanned: Rc<Cell<bool>>) -> ObjectServer {
    let mut objects = network_manager_object(Rc::new(Cell::new(true)));
    objects
        .register::<(), Vec<zvariant::OwnedObjectPath>, _, _>(
            ROOT_PATH,
            ROOT_INTERFACE,
            "GetDevices",
            |_| async {
                Ok(vec![
                    zvariant::OwnedObjectPath::try_from(DEVICE_PATH).unwrap(),
                ])
            },
        )
        .unwrap();
    for (name, value) in [("Interface", "wlan0"), ("HwAddress", "02:00:00:00:00:02")] {
        let value = value.to_owned();
        objects
            .register_read_only_property::<String, _, _>(
                DEVICE_PATH,
                DEVICE_INTERFACE,
                name,
                PropertyChangeMode::Value,
                move || {
                    let value = value.clone();
                    async move { Ok(value) }
                },
            )
            .unwrap();
    }
    objects
        .register_read_only_property::<u32, _, _>(
            DEVICE_PATH,
            DEVICE_INTERFACE,
            "DeviceType",
            PropertyChangeMode::Value,
            || async { Ok(2) },
        )
        .unwrap();
    objects
        .register_read_only_property::<u32, _, _>(
            DEVICE_PATH,
            DEVICE_INTERFACE,
            "State",
            PropertyChangeMode::Value,
            || async { Ok(100) },
        )
        .unwrap();
    objects
        .register_read_only_property::<zvariant::OwnedObjectPath, _, _>(
            DEVICE_PATH,
            WIRELESS_INTERFACE,
            "ActiveAccessPoint",
            PropertyChangeMode::Value,
            || async { Ok(zvariant::OwnedObjectPath::try_from(AP_PATH).unwrap()) },
        )
        .unwrap();
    let access_point = zvariant::OwnedObjectPath::try_from(AP_PATH).unwrap();
    objects
        .register::<(), Vec<zvariant::OwnedObjectPath>, _, _>(
            DEVICE_PATH,
            WIRELESS_INTERFACE,
            "GetAllAccessPoints",
            move |()| {
                let access_point = access_point.clone();
                async move { Ok(vec![access_point]) }
            },
        )
        .unwrap();
    let scan = Rc::clone(&scanned);
    objects
        .register::<HashMap<String, zvariant::OwnedValue>, (), _, _>(
            DEVICE_PATH,
            WIRELESS_INTERFACE,
            "RequestScan",
            move |options| {
                assert!(options.is_empty());
                scan.set(true);
                async { Ok(()) }
            },
        )
        .unwrap();
    objects
        .register_read_only_property::<String, _, _>(
            AP_PATH,
            ACCESS_POINT_INTERFACE,
            "HwAddress",
            PropertyChangeMode::Value,
            || async { Ok("02:00:00:00:00:01".to_owned()) },
        )
        .unwrap();
    objects
        .register_read_only_property::<Vec<u8>, _, _>(
            AP_PATH,
            ACCESS_POINT_INTERFACE,
            "Ssid",
            PropertyChangeMode::Value,
            || async { Ok(vec![b'T', 0xff, b'N']) },
        )
        .unwrap();
    for (name, value) in [("Flags", 1_u32), ("WpaFlags", 0_u32), ("RsnFlags", 512_u32)] {
        objects
            .register_read_only_property::<u32, _, _>(
                AP_PATH,
                ACCESS_POINT_INTERFACE,
                name,
                PropertyChangeMode::Value,
                move || async move { Ok(value) },
            )
            .unwrap();
    }
    objects
        .register_read_only_property::<u8, _, _>(
            AP_PATH,
            ACCESS_POINT_INTERFACE,
            "Strength",
            PropertyChangeMode::Value,
            || async { Ok(83) },
        )
        .unwrap();
    for (name, value) in [
        ("Frequency", 5_180_u32),
        ("MaxBitrate", 866_700_u32),
        ("Mode", 2_u32),
    ] {
        objects
            .register_read_only_property::<u32, _, _>(
                AP_PATH,
                ACCESS_POINT_INTERFACE,
                name,
                PropertyChangeMode::Value,
                move || async move { Ok(value) },
            )
            .unwrap();
    }
    objects
}

struct PrivateBus {
    child: Child,
    socket: PathBuf,
    address: String,
}

impl PrivateBus {
    fn start() -> Option<Self> {
        let socket = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "tensor-dbus-network-test-{}-{}.sock",
                std::process::id(),
                next_bus_id()
            ));
        fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let _ = fs::remove_file(&socket);
        let address = format!("unix:path={}", socket.display());
        let mut child = match Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--nopidfile",
                "--print-address=1",
                "--address",
                &address,
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
        Some(Self {
            child,
            socket,
            address: announced.trim().to_owned(),
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

fn next_bus_id() -> u64 {
    static NEXT_BUS: AtomicU64 = AtomicU64::new(1);
    NEXT_BUS.fetch_add(1, Ordering::Relaxed)
}
