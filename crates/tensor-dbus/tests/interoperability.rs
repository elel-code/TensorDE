use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
};

use compio::{
    driver::{DriverType, ProactorBuilder},
    runtime::RuntimeBuilder,
};
use tensor_dbus::{
    BusAddress, Connection, MachineId, MessageKind, MethodCall, ObjectServer, PropertyChangeMode,
    RequestNameFlags, RequestNameReply,
};

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
            .join(format!("tensor-dbus-interop-{}.sock", std::process::id()));
        fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let _ = fs::remove_file(&socket);
        let requested = format!("unix:path={}", socket.display());
        let mut child = match Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--nopidfile",
                "--print-address=1",
                "--address",
                &requested,
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
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .expect("dbus-daemon did not announce its address");
        Some(Self {
            child,
            socket,
            address: address.trim().to_owned(),
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
fn object_server_interoperates_with_gdbus_client_and_introspection() {
    if Command::new("gdbus")
        .arg("help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        eprintln!("skipping interoperability test: gdbus is unavailable");
        return;
    }
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping interoperability test: dbus-daemon is unavailable");
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
        assert_eq!(
            connection
                .request_name("org.tensor.Interop", RequestNameFlags::DO_NOT_QUEUE)
                .await
                .unwrap(),
            RequestNameReply::PrimaryOwner
        );
        let machine_id: MachineId = "0123456789abcdef0123456789abcdef".parse().unwrap();
        let mut objects = ObjectServer::with_machine_id(machine_id);
        objects
            .register::<String, String, _, _>(
                "/org/tensor/Interop",
                "org.tensor.Interop",
                "Echo",
                |value| async move { Ok(format!("interop:{value}")) },
            )
            .unwrap();
        objects
            .register_signal::<(String, u32)>(
                "/org/tensor/Interop",
                "org.tensor.Interop",
                "Changed",
            )
            .unwrap();
        objects
            .register_read_only_property::<String, _, _>(
                "/org/tensor/Interop",
                "org.tensor.Interop",
                "Version",
                PropertyChangeMode::Const,
                || async { Ok("1".to_owned()) },
            )
            .unwrap();

        let address = bus.address.clone();
        let client = std::thread::spawn(move || run_gdbus(&address));
        let mut echoed = false;
        while !echoed {
            let message = connection.receive().await.unwrap();
            if message.kind() != MessageKind::MethodCall {
                assert_eq!(message.kind(), MessageKind::Signal);
                continue;
            }
            let call = MethodCall::new(message).expect("message kind was checked");
            echoed = call.member() == "Echo";
            objects.dispatch(&mut connection, &call).await.unwrap();
        }
        client.join().unwrap();
    });
}

fn run_gdbus(address: &str) {
    let introspect = Command::new("gdbus")
        .args([
            "introspect",
            "--address",
            address,
            "--dest",
            "org.tensor.Interop",
            "--object-path",
            "/org/tensor/Interop",
            "--xml",
        ])
        .output()
        .unwrap();
    assert!(
        introspect.status.success(),
        "{}",
        String::from_utf8_lossy(&introspect.stderr)
    );
    let xml = String::from_utf8(introspect.stdout).unwrap();
    assert!(xml.contains("interface name=\"org.tensor.Interop\""));
    assert!(xml.contains("method name=\"Echo\""));
    assert!(xml.contains("signal name=\"Changed\""));
    assert!(xml.contains("property name=\"Version\""));
    assert!(xml.contains("org.freedesktop.DBus.Property.EmitsChangedSignal"));
    assert!(xml.contains("value=\"const\""));

    let call = Command::new("gdbus")
        .args([
            "call",
            "--address",
            address,
            "--dest",
            "org.tensor.Interop",
            "--object-path",
            "/org/tensor/Interop",
            "--method",
            "org.tensor.Interop.Echo",
            "hello",
        ])
        .output()
        .unwrap();
    assert!(
        call.status.success(),
        "{}",
        String::from_utf8_lossy(&call.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&call.stdout).trim(),
        "('interop:hello',)"
    );
}
