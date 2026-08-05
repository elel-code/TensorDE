use std::{
    fs,
    io::{BufRead, BufReader},
    os::fd::AsFd,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use compio::{
    driver::{DriverType, ProactorBuilder},
    runtime::RuntimeBuilder,
};
use tensor_dbus::{
    BusAddress, Connection, Error, MatchRule, MethodCall, MethodError, ReleaseNameReply,
    RequestNameFlags, RequestNameReply, reply_method, reply_method_error, reply_method_result,
};

struct PrivateBus {
    child: Child,
    socket: Option<PathBuf>,
    address: String,
}

impl PrivateBus {
    fn start() -> Option<Self> {
        let socket = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "tensor-dbus-test-{}-{}.sock",
                std::process::id(),
                next_bus_id()
            ));
        fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let _ = fs::remove_file(&socket);
        let address = format!("unix:path={}", socket.display());
        Self::launch(address, Some(socket))
    }

    fn start_abstract() -> Option<Self> {
        let address = format!(
            "unix:abstract=tensor-dbus-test-{}-{}",
            std::process::id(),
            next_bus_id()
        );
        Self::launch(address, None)
    }

    fn launch(address: String, socket: Option<PathBuf>) -> Option<Self> {
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
        assert!(announced.trim().starts_with(&address));
        Some(Self {
            child,
            socket,
            address: announced.trim().to_owned(),
        })
    }

    fn guid(&self) -> &str {
        let (_, guid) = self
            .address
            .rsplit_once(",guid=")
            .expect("dbus-daemon must announce its GUID");
        guid
    }

    fn address_with_guid(&self, guid: &str) -> String {
        let (transport, _) = self
            .address
            .rsplit_once(",guid=")
            .expect("dbus-daemon must announce its GUID");
        format!("{transport},guid={guid}")
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(socket) = &self.socket {
            let _ = fs::remove_file(socket);
        }
    }
}

fn next_bus_id() -> u64 {
    static NEXT_BUS: AtomicU64 = AtomicU64::new(1);
    NEXT_BUS.fetch_add(1, Ordering::Relaxed)
}

#[test]
fn compio_client_connects_to_abstract_bus() {
    let Some(bus) = PrivateBus::start_abstract() else {
        eprintln!("skipping private bus test: dbus-daemon is unavailable");
        return;
    };
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let connection = Connection::connect_bus(BusAddress::parse(&bus.address).unwrap())
            .await
            .unwrap();
        assert!(connection.unique_name().unwrap().starts_with(':'));
        assert!(connection.supports_unix_fd());
    });
}

#[test]
fn compio_client_exposes_the_authenticated_server_guid() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping private bus test: dbus-daemon is unavailable");
        return;
    };
    let expected_guid = bus.guid().parse().unwrap();
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let connection = Connection::connect_bus(BusAddress::parse(&bus.address).unwrap())
            .await
            .unwrap();
        assert_eq!(connection.server_guid(), expected_guid);
    });
}

#[test]
fn compio_client_rejects_an_address_with_the_wrong_server_guid() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping private bus test: dbus-daemon is unavailable");
        return;
    };
    let wrong_guid = if bus.guid() == "00000000000000000000000000000000" {
        "11111111111111111111111111111111"
    } else {
        "00000000000000000000000000000000"
    };
    let address = BusAddress::parse(&bus.address_with_guid(wrong_guid)).unwrap();
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let error = match Connection::connect_bus(address).await {
            Ok(_) => panic!("connection unexpectedly accepted the wrong server GUID"),
            Err(error) => error,
        };
        match error {
            Error::Authentication(message) => {
                assert!(message.contains("server GUID mismatch"));
                assert!(message.contains(wrong_guid));
                assert!(message.contains(bus.guid()));
            }
            error => panic!("expected GUID authentication failure, got {error:?}"),
        }
    });
}

#[test]
fn compio_client_falls_back_after_a_server_guid_mismatch() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping private bus test: dbus-daemon is unavailable");
        return;
    };
    let wrong_guid = if bus.guid() == "00000000000000000000000000000000" {
        "11111111111111111111111111111111"
    } else {
        "00000000000000000000000000000000"
    };
    let address = BusAddress::parse(&format!(
        "{};{}",
        bus.address_with_guid(wrong_guid),
        bus.address
    ))
    .unwrap();
    let expected_guid = bus.guid().parse().unwrap();
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let connection = Connection::connect_bus(address).await.unwrap();
        assert_eq!(connection.server_guid(), expected_guid);
    });
}

#[test]
fn compio_client_uses_the_next_reachable_bus_address() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping private bus test: dbus-daemon is unavailable");
        return;
    };
    let missing = std::env::current_dir()
        .unwrap()
        .join("target")
        .join(format!("tensor-dbus-missing-{}.sock", next_bus_id()));
    let address =
        BusAddress::parse(&format!("unix:path={};{}", missing.display(), bus.address)).unwrap();
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let connection = Connection::connect_bus(address).await.unwrap();
        assert!(connection.unique_name().unwrap().starts_with(':'));
    });
}

#[test]
fn compio_client_completes_bus_lifecycle() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping private bus test: dbus-daemon is unavailable");
        return;
    };
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let mut connection = Connection::connect_bus(BusAddress::parse(&bus.address).unwrap())
            .await
            .unwrap();
        assert!(connection.unique_name().unwrap().starts_with(':'));

        let names: Vec<String> = connection
            .call(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "ListNames",
                &(),
            )
            .await
            .unwrap();
        assert!(
            names
                .iter()
                .any(|name| Some(name.as_str()) == connection.unique_name())
        );

        assert_eq!(
            connection
                .request_name("org.tensor.PrivateTest", RequestNameFlags::default())
                .await
                .unwrap(),
            RequestNameReply::PrimaryOwner
        );
        assert_eq!(
            connection
                .release_name("org.tensor.PrivateTest")
                .await
                .unwrap(),
            ReleaseNameReply::Released
        );
    });
}

#[test]
fn compio_connection_can_serve_methods_and_standard_errors() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping private bus test: dbus-daemon is unavailable");
        return;
    };
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let address = BusAddress::parse(&bus.address).unwrap();
        let mut server_connection = Connection::connect_bus(address).await.unwrap();
        assert_eq!(
            server_connection
                .request_name("org.tensor.TestService", RequestNameFlags::default())
                .await
                .unwrap(),
            RequestNameReply::PrimaryOwner
        );
        let server = compio::runtime::spawn(async move {
            let mut served = 0;
            while served < 11 {
                let message = server_connection.receive().await.unwrap();
                let Some(call) = MethodCall::new(message) else {
                    continue;
                };
                assert_eq!(call.path(), "/org/tensor/TestService");
                assert_eq!(call.interface(), Some("org.tensor.Test"));

                match call.member() {
                    "Echo" => {
                        let body: String = call.body().unwrap();
                        reply_method(&mut server_connection, &call, &format!("reply:{body}"))
                            .await
                            .unwrap();
                    }
                    "ParseString" => {
                        let result = call.body::<String>().map(|body| format!("parsed:{body}"));
                        reply_method_result(&mut server_connection, &call, result)
                            .await
                            .unwrap();
                    }
                    "NoReply" => {
                        assert!(!call.expects_reply());
                        reply_method(&mut server_connection, &call, &"ignored")
                            .await
                            .unwrap();
                    }
                    member => {
                        reply_method_error(
                            &mut server_connection,
                            &call,
                            MethodError::unknown_method(format!("unknown method {member}")),
                        )
                        .await
                        .unwrap();
                    }
                }
                served += 1;
            }
        });

        let mut client = Connection::connect_bus(BusAddress::parse(&bus.address).unwrap())
            .await
            .unwrap();
        let mut proxy = tensor_dbus::Proxy::new(
            &mut client,
            Some("org.tensor.TestService"),
            "/org/tensor/TestService",
            Some("org.tensor.Test"),
        )
        .unwrap();
        let first = proxy
            .send_call::<_, String>("Echo", &"first")
            .await
            .unwrap();
        let second = proxy
            .send_call::<_, String>("Echo", &"second")
            .await
            .unwrap();
        assert!(first.serial() < second.serial());
        assert_eq!(proxy.wait(second).await.unwrap(), "reply:second");
        assert_eq!(proxy.wait(first).await.unwrap(), "reply:first");

        let queued_abandoned = proxy
            .send_call::<_, String>("Echo", &"queued-abandoned")
            .await
            .unwrap();
        let queued_after = proxy
            .send_call::<_, String>("Echo", &"queued-after")
            .await
            .unwrap();
        assert_eq!(
            proxy.wait(queued_after).await.unwrap(),
            "reply:queued-after"
        );
        proxy.abandon(queued_abandoned).unwrap();

        let abandoned = proxy
            .send_call::<_, String>("Echo", &"abandoned")
            .await
            .unwrap();
        proxy.abandon(abandoned).unwrap();
        let after_abandoned = proxy
            .send_call::<_, String>("Echo", &"after-abandoned")
            .await
            .unwrap();
        assert_eq!(
            proxy.wait(after_abandoned).await.unwrap(),
            "reply:after-abandoned"
        );

        let receive_abandoned = proxy
            .send_call::<_, String>("Echo", &"receive-abandoned")
            .await
            .unwrap();
        let receive_abandoned_serial = receive_abandoned.serial();
        proxy.abandon(receive_abandoned).unwrap();
        let receive_after = proxy
            .send_call::<_, String>("Echo", &"receive-after")
            .await
            .unwrap();
        drop(proxy);
        let receive_after_reply = loop {
            let message = client.receive().await.unwrap();
            assert_ne!(
                message.reply_serial(),
                Some(receive_abandoned_serial),
                "receive exposed an explicitly abandoned reply"
            );
            if receive_after.matches(&message) {
                break message;
            }
        };
        assert_eq!(
            receive_after.decode(receive_after_reply).unwrap(),
            "reply:receive-after"
        );
        let mut proxy = tensor_dbus::Proxy::new(
            &mut client,
            Some("org.tensor.TestService"),
            "/org/tensor/TestService",
            Some("org.tensor.Test"),
        )
        .unwrap();

        proxy.send_no_reply("NoReply", &()).await.unwrap();

        let invalid_args = proxy
            .call::<_, String>("ParseString", &42_u32)
            .await
            .unwrap_err();
        match invalid_args {
            Error::Method { name, message } => {
                assert_eq!(name, "org.freedesktop.DBus.Error.InvalidArgs");
                assert!(!message.is_empty());
            }
            error => panic!("expected InvalidArgs method error, got {error:?}"),
        }

        let unknown_method = proxy.call::<_, ()>("Missing", &()).await.unwrap_err();
        match unknown_method {
            Error::Method { name, message } => {
                assert_eq!(name, "org.freedesktop.DBus.Error.UnknownMethod");
                assert_eq!(message, "unknown method Missing");
            }
            error => panic!("expected UnknownMethod method error, got {error:?}"),
        }
        server.await.unwrap();
    });
}

#[test]
fn compio_connection_passes_unix_fds_in_both_directions() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping private bus test: dbus-daemon is unavailable");
        return;
    };
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let address = BusAddress::parse(&bus.address).unwrap();
        let mut server_connection = Connection::connect_bus(address).await.unwrap();
        assert!(server_connection.supports_unix_fd());
        assert_eq!(
            server_connection
                .request_name("org.tensor.FdService", RequestNameFlags::default())
                .await
                .unwrap(),
            RequestNameReply::PrimaryOwner
        );
        let server = compio::runtime::spawn(async move {
            let call = loop {
                let message = server_connection.receive().await.unwrap();
                if message.member() == Some("ExchangeFd") {
                    break message;
                }
            };
            assert_eq!(call.unix_fd_count(), 3);
            let received: (
                tensor_dbus::zvariant::OwnedFd,
                tensor_dbus::zvariant::OwnedFd,
                tensor_dbus::zvariant::OwnedFd,
            ) = call.body().unwrap();
            for fd in [&received.0, &received.1, &received.2] {
                assert!(
                    rustix::io::fcntl_getfd(fd.as_fd())
                        .unwrap()
                        .contains(rustix::io::FdFlags::CLOEXEC)
                );
            }
            server_connection
                .reply(
                    &call,
                    &(
                        tensor_dbus::zvariant::Fd::from(&received.0),
                        tensor_dbus::zvariant::Fd::from(&received.1),
                        tensor_dbus::zvariant::Fd::from(&received.2),
                    ),
                )
                .await
                .unwrap();
        });

        let mut client = Connection::connect_bus(BusAddress::parse(&bus.address).unwrap())
            .await
            .unwrap();
        assert!(client.supports_unix_fd());
        let sources = [
            fs::File::open("/dev/null").unwrap(),
            fs::File::open("/dev/null").unwrap(),
            fs::File::open("/dev/null").unwrap(),
        ];
        let returned: (
            tensor_dbus::zvariant::OwnedFd,
            tensor_dbus::zvariant::OwnedFd,
            tensor_dbus::zvariant::OwnedFd,
        ) = client
            .call(
                Some("org.tensor.FdService"),
                "/org/tensor/FdService",
                Some("org.tensor.Fd"),
                "ExchangeFd",
                &(
                    tensor_dbus::zvariant::Fd::from(&sources[0]),
                    tensor_dbus::zvariant::Fd::from(&sources[1]),
                    tensor_dbus::zvariant::Fd::from(&sources[2]),
                ),
            )
            .await
            .unwrap();
        for fd in [&returned.0, &returned.1, &returned.2] {
            assert!(
                rustix::io::fcntl_getfd(fd.as_fd())
                    .unwrap()
                    .contains(rustix::io::FdFlags::CLOEXEC)
            );
        }
        server.await.unwrap();
    });
}

#[test]
fn signal_rules_route_by_unique_sender_and_can_be_removed() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping private bus test: dbus-daemon is unavailable");
        return;
    };
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let address = BusAddress::parse(&bus.address).unwrap();
        let mut sender = Connection::connect_bus(address.clone()).await.unwrap();
        let other = Connection::connect_bus(address.clone()).await.unwrap();
        let mut receiver = Connection::connect_bus(address).await.unwrap();
        let mut rule = MatchRule::signal(
            sender.unique_name(),
            Some("/org/tensor/Signals"),
            Some("org.tensor.Signals"),
            Some("Changed"),
        )
        .unwrap();
        let wrong_sender = MatchRule::signal(
            other.unique_name(),
            Some("/org/tensor/Signals"),
            Some("org.tensor.Signals"),
            Some("Changed"),
        )
        .unwrap();
        receiver.add_match(&mut rule).await.unwrap();

        sender
            .emit_signal_to(
                receiver.unique_name().unwrap(),
                "/org/tensor/Signals",
                "org.tensor.Signals",
                "Changed",
                &42_u32,
            )
            .await
            .unwrap();
        let signal = loop {
            let message = receiver.receive().await.unwrap();
            if message.path() == Some("/org/tensor/Signals")
                && message.interface() == Some("org.tensor.Signals")
                && message.member() == Some("Changed")
            {
                break message;
            }
        };
        assert!(rule.matches(&signal));
        assert!(!wrong_sender.matches(&signal));
        assert_eq!(signal.body::<u32>().unwrap(), 42);

        receiver.remove_match(&rule).await.unwrap();
    });
}

#[test]
fn extended_signal_rules_interoperate_with_the_bus() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping private bus test: dbus-daemon is unavailable");
        return;
    };
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let address = BusAddress::parse(&bus.address).unwrap();
        let mut sender = Connection::connect_bus(address.clone()).await.unwrap();
        let mut receiver = Connection::connect_bus(address).await.unwrap();
        let mut rule = MatchRule::signal(
            sender.unique_name(),
            None,
            Some("org.tensor.Signals"),
            Some("Changed"),
        )
        .unwrap()
        .path_namespace("/org/tensor")
        .unwrap()
        .destination(receiver.unique_name().unwrap())
        .unwrap()
        .arg(2, "exact")
        .unwrap()
        .arg_path(1, "/org/tensor/")
        .unwrap()
        .arg0_namespace("org.tensor")
        .unwrap();
        receiver.add_match(&mut rule).await.unwrap();

        sender
            .emit_signal_to(
                receiver.unique_name().unwrap(),
                "/org/tensor/Signals/Child",
                "org.tensor.Signals",
                "Changed",
                &(
                    "org.tensor.Service",
                    zvariant::ObjectPath::from_static_str_unchecked("/org/tensor/Object"),
                    "exact",
                ),
            )
            .await
            .unwrap();
        let signal = loop {
            let message = receiver.receive().await.unwrap();
            if message.member() == Some("Changed") {
                break message;
            }
        };
        assert!(rule.matches(&signal));
        receiver.remove_match(&rule).await.unwrap();
    });
}

#[test]
fn well_known_signal_rule_tracks_name_owner_handoffs() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping private bus test: dbus-daemon is unavailable");
        return;
    };
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build().expect("io_uring runtime is required");

    runtime.block_on(async {
        let address = BusAddress::parse(&bus.address).unwrap();
        let mut first_owner = Connection::connect_bus(address.clone()).await.unwrap();
        let mut second_owner = Connection::connect_bus(address.clone()).await.unwrap();
        let mut receiver = Connection::connect_bus(address).await.unwrap();
        assert_eq!(
            first_owner
                .request_name("org.tensor.SignalService", RequestNameFlags::default())
                .await
                .unwrap(),
            RequestNameReply::PrimaryOwner
        );
        let mut proxy = tensor_dbus::Proxy::new(
            &mut receiver,
            Some("org.tensor.SignalService"),
            "/org/tensor/Signals",
            Some("org.tensor.Signals"),
        )
        .unwrap();
        let rule = proxy.subscribe("Changed").await.unwrap();
        let mut signals = proxy.signal_stream(rule);

        first_owner
            .emit_signal(
                "/org/tensor/Signals",
                "org.tensor.Signals",
                "Changed",
                &1_u32,
            )
            .await
            .unwrap();
        let first = signals.next().await.unwrap();
        assert_eq!(first.sender(), first_owner.unique_name());
        assert_eq!(first.body::<u32>().unwrap(), 1);

        assert_eq!(
            first_owner
                .release_name("org.tensor.SignalService")
                .await
                .unwrap(),
            ReleaseNameReply::Released
        );
        assert_eq!(
            second_owner
                .request_name("org.tensor.SignalService", RequestNameFlags::default())
                .await
                .unwrap(),
            RequestNameReply::PrimaryOwner
        );
        second_owner
            .emit_signal(
                "/org/tensor/Signals",
                "org.tensor.Signals",
                "Changed",
                &2_u32,
            )
            .await
            .unwrap();
        let second = signals.next().await.unwrap();
        assert_eq!(second.sender(), second_owner.unique_name());
        assert_eq!(second.body::<u32>().unwrap(), 2);

        signals.close().await.unwrap();
    });
}
