use std::{
    cell::Cell,
    collections::HashMap,
    fs,
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use compio::{
    driver::{DriverType, ProactorBuilder},
    net::{UnixListener, UnixStream},
    runtime::RuntimeBuilder,
};
use tensor_dbus::{
    Connection, ConnectionMode, Error, Guid, MachineId, ObjectServer, PropertyChangeMode, Proxy,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

fn socket_path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    std::env::current_dir()
        .unwrap()
        .join("target")
        .join(format!(
            "tensor-dbus-object-server-{}-{}.sock",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

fn runtime() -> compio::runtime::Runtime {
    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    builder.build().expect("io_uring runtime is required")
}

fn method_error(error: Error) -> (String, String) {
    match error {
        Error::Method { name, message } => (name, message),
        error => panic!("expected a remote method error, got {error:?}"),
    }
}

#[test]
fn object_server_routes_typed_peer_methods_and_standard_interfaces() {
    let path = socket_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = fs::remove_file(&path);
    let guid = Guid::generate().unwrap();

    runtime().block_on(async {
        let listener = UnixListener::bind(&path).await.unwrap();
        let server = compio::runtime::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut connection = Connection::accept_peer(stream, guid).await.unwrap();
            let calls = Rc::new(Cell::new(0_u32));
            let machine_id: MachineId = "0123456789abcdef0123456789abcdef".parse().unwrap();
            let mut objects = ObjectServer::new();
            let echo_calls = Rc::clone(&calls);
            objects
                .register::<String, String, _, _>(
                    "/org/tensor/Test",
                    "org.tensor.Test",
                    "Echo",
                    move |value| {
                        echo_calls.set(echo_calls.get() + 1);
                        async move { Ok(format!("echo:{value}")) }
                    },
                )
                .unwrap();
            objects
                .register::<(u32, u32), u32, _, _>(
                    "/org/tensor/Test",
                    "org.tensor.Test",
                    "Add",
                    |(left, right)| async move { Ok(left + right) },
                )
                .unwrap();
            objects
                .register::<String, String, _, _>(
                    "/org/tensor/Test",
                    "org.tensor.Other",
                    "Echo",
                    |value| async move { Ok(format!("other:{value}")) },
                )
                .unwrap();
            objects
                .register::<(), (), _, _>(
                    "/org/tensor/Test",
                    "org.tensor.Test",
                    "Notify",
                    |()| async move { Ok(()) },
                )
                .unwrap();
            objects
                .register_with_context::<(), u32, _, _>(
                    "/org/tensor/Test",
                    "org.tensor.Test",
                    "PeerUid",
                    |context, ()| async move {
                        assert_eq!(context.mode(), ConnectionMode::Peer);
                        assert!(context.sender().is_none());
                        Ok(context.peer_credentials().unwrap().user_id)
                    },
                )
                .unwrap();

            objects
                .register_with_connection::<u32, u32, _>(
                    "/org/tensor/Test",
                    "org.tensor.Test",
                    "SignalThenDouble",
                    async |connection: &mut Connection,
                           context: tensor_dbus::MethodContext,
                           value| {
                        assert_eq!(context.mode(), ConnectionMode::Peer);
                        connection
                            .emit_signal("/org/tensor/Test", "org.tensor.Test", "Progress", &value)
                            .await
                            .map_err(|error| tensor_dbus::MethodError::failed(error.to_string()))?;
                        Ok(value * 2)
                    },
                )
                .unwrap();
            objects.set_machine_id(machine_id);

            for _ in 0..16 {
                assert!(objects.serve_next(&mut connection).await.unwrap().is_none());
            }
            assert_eq!(calls.get(), 1);
        });

        let stream = UnixStream::connect(&path).await.unwrap();
        let mut connection = Connection::connect_peer(stream, Some(guid)).await.unwrap();
        let mut proxy = Proxy::new(
            &mut connection,
            None::<&str>,
            "/org/tensor/Test",
            Some("org.tensor.Test"),
        )
        .unwrap();
        assert_eq!(
            proxy.call::<_, String>("Echo", &"hello").await.unwrap(),
            "echo:hello"
        );

        let (name, _) = method_error(proxy.call::<_, u32>("Add", &"wrong").await.unwrap_err());
        assert_eq!(name, "org.freedesktop.DBus.Error.InvalidArgs");
        drop(proxy);

        let mut unknown_interface = Proxy::new(
            &mut connection,
            None::<&str>,
            "/org/tensor/Test",
            Some("org.tensor.Missing"),
        )
        .unwrap();
        let (name, _) = method_error(
            unknown_interface
                .call::<_, ()>("Missing", &())
                .await
                .unwrap_err(),
        );
        assert_eq!(name, "org.freedesktop.DBus.Error.UnknownInterface");
        drop(unknown_interface);

        let mut absent_properties = Proxy::new(
            &mut connection,
            None::<&str>,
            "/org/tensor/Test",
            Some("org.freedesktop.DBus.Properties"),
        )
        .unwrap();
        let (name, _) = method_error(
            absent_properties
                .call::<_, HashMap<String, OwnedValue>>("GetAll", &"org.tensor.Test")
                .await
                .unwrap_err(),
        );
        assert_eq!(name, "org.freedesktop.DBus.Error.UnknownInterface");
        drop(absent_properties);

        let mut proxy = Proxy::new(
            &mut connection,
            None::<&str>,
            "/org/tensor/Test",
            Some("org.tensor.Test"),
        )
        .unwrap();
        let (name, _) = method_error(proxy.call::<_, ()>("Missing", &()).await.unwrap_err());
        assert_eq!(name, "org.freedesktop.DBus.Error.UnknownMethod");
        drop(proxy);

        let mut unknown_object = Proxy::new(
            &mut connection,
            None::<&str>,
            "/org/tensor/Missing",
            Some("org.tensor.Test"),
        )
        .unwrap();
        let (name, _) = method_error(
            unknown_object
                .call::<_, ()>("Missing", &())
                .await
                .unwrap_err(),
        );
        assert_eq!(name, "org.freedesktop.DBus.Error.UnknownObject");
        drop(unknown_object);

        let mut no_interface = Proxy::new(
            &mut connection,
            None::<&str>,
            "/org/tensor/Test",
            None::<&str>,
        )
        .unwrap();
        assert_eq!(
            no_interface
                .call::<_, u32>("Add", &(7_u32, 8_u32))
                .await
                .unwrap(),
            15
        );
        let (name, message) = method_error(
            no_interface
                .call::<_, String>("Echo", &"ambiguous")
                .await
                .unwrap_err(),
        );
        assert_eq!(name, "org.freedesktop.DBus.Error.UnknownMethod");
        assert!(message.contains("unambiguous interface"));
        drop(no_interface);

        let mut introspectable = Proxy::new(
            &mut connection,
            None::<&str>,
            "/org/tensor/Test",
            Some("org.freedesktop.DBus.Introspectable"),
        )
        .unwrap();
        let xml: String = introspectable.call("Introspect", &()).await.unwrap();
        assert!(xml.contains("GetMachineId"));
        assert!(xml.contains("<interface name=\"org.tensor.Test\">"));
        assert!(xml.contains("<method name=\"Add\">"));
        assert!(xml.contains("<arg type=\"u\" direction=\"in\"/>"));
        assert!(xml.contains("org.freedesktop.DBus.Peer"));
        drop(introspectable);

        let mut root = Proxy::new(
            &mut connection,
            None::<&str>,
            "/",
            Some("org.freedesktop.DBus.Introspectable"),
        )
        .unwrap();
        let xml: String = root.call("Introspect", &()).await.unwrap();
        assert!(xml.contains("<node name=\"org\"/>"));
        drop(root);

        let mut org = Proxy::new(
            &mut connection,
            None::<&str>,
            "/org",
            Some("org.freedesktop.DBus.Introspectable"),
        )
        .unwrap();
        let xml: String = org.call("Introspect", &()).await.unwrap();
        assert!(xml.contains("<node name=\"tensor\"/>"));
        drop(org);

        let mut peer = Proxy::new(
            &mut connection,
            None::<&str>,
            "/org/tensor/Test",
            Some("org.freedesktop.DBus.Peer"),
        )
        .unwrap();
        peer.call::<_, ()>("Ping", &()).await.unwrap();
        let machine_id: String = peer.call("GetMachineId", &()).await.unwrap();
        assert_eq!(machine_id, "0123456789abcdef0123456789abcdef");
        drop(peer);

        let mut proxy = Proxy::new(
            &mut connection,
            None::<&str>,
            "/org/tensor/Test",
            Some("org.tensor.Test"),
        )
        .unwrap();
        let uid: u32 = proxy.call("PeerUid", &()).await.unwrap();
        assert_eq!(uid, rustix::process::getuid().as_raw());
        let pending = proxy
            .send_call::<_, u32>("SignalThenDouble", &21_u32)
            .await
            .unwrap();
        drop(proxy);
        let progress = connection.receive().await.unwrap();
        assert_eq!(progress.member(), Some("Progress"));
        assert_eq!(progress.body::<u32>().unwrap(), 21);
        assert_eq!(pending.wait(&mut connection).await.unwrap(), 42);
        connection
            .send_no_reply(
                None,
                "/org/tensor/Test",
                Some("org.tensor.Test"),
                "Notify",
                &(),
            )
            .await
            .unwrap();
        server.await.unwrap();
    });
    fs::remove_file(path).unwrap();
}

#[test]
fn object_server_rejects_duplicate_and_reserved_registration() {
    let mut objects = ObjectServer::new();
    objects
        .register::<(), (), _, _>("/org/tensor/Test", "org.tensor.Test", "Ping", |()| async {
            Ok(())
        })
        .unwrap();
    assert!(!objects.contains_object("/"));
    assert!(!objects.contains_object("/org"));
    assert!(objects.contains_introspection_path("/"));
    assert!(objects.contains_introspection_path("/org"));
    assert!(matches!(
        objects.register::<(), (), _, _>(
            "/org/tensor/Test",
            "org.tensor.Test",
            "Ping",
            |()| async { Ok(()) }
        ),
        Err(Error::DuplicateMethod { .. })
    ));
    objects
        .register_signal::<(String, u32)>("/org/tensor/Test", "org.tensor.Test", "Changed")
        .unwrap();
    assert!(matches!(
        objects.register_signal::<(String, u32)>("/org/tensor/Test", "org.tensor.Test", "Changed",),
        Err(Error::DuplicateSignal { .. })
    ));
    assert!(matches!(
        objects.register::<(), (), _, _>(
            "/org/tensor/Test",
            "org.freedesktop.DBus.Peer",
            "Ping",
            |()| async { Ok(()) }
        ),
        Err(Error::ReservedInterface(_))
    ));
    assert!(matches!(
        objects
            .register::<(), (), _, _>("invalid", "org.tensor.Test", "Ping", |()| async { Ok(()) }),
        Err(Error::InvalidName { .. })
    ));

    objects
        .register::<(), (), _, _>("/org/tensor/Child", "org.tensor.Child", "Run", |()| async {
            Ok(())
        })
        .unwrap();
    objects
        .register::<(), (), _, _>(
            "/org/tensor/Child/Grandchild",
            "org.tensor.Grandchild",
            "Run",
            |()| async { Ok(()) },
        )
        .unwrap();
    assert!(
        objects
            .unregister_interface("/org/tensor/Child", "org.tensor.Child")
            .unwrap()
    );
    assert!(
        !objects
            .unregister_interface("/org/tensor/Child", "org.tensor.Child")
            .unwrap()
    );
    assert_eq!(
        objects
            .unregister_object("/org/tensor/Child/Grandchild")
            .unwrap(),
        ["org.tensor.Grandchild"]
    );
    assert!(!objects.contains_object("/org/tensor/Child/Grandchild"));
    assert!(objects.unregister_object("/org/tensor/Test").unwrap().len() == 1);
    assert!(!objects.contains_introspection_path("/org/tensor/Test"));
    assert!(!objects.contains_introspection_path("/"));
}

#[test]
fn object_server_serves_properties_object_manager_and_change_signals() {
    const OBJECT: &str = "/org/tensor/Managed";
    const INTERFACE: &str = "org.tensor.Managed";
    const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
    const INTROSPECTABLE: &str = "org.freedesktop.DBus.Introspectable";
    const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

    type PropertyMap = HashMap<String, OwnedValue>;
    type InterfaceMap = HashMap<String, PropertyMap>;
    type ManagedObjects = HashMap<OwnedObjectPath, InterfaceMap>;

    let path = socket_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = fs::remove_file(&path);
    let guid = Guid::generate().unwrap();

    runtime().block_on(async {
        let listener = UnixListener::bind(&path).await.unwrap();
        let server = compio::runtime::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut connection = Connection::accept_peer(stream, guid).await.unwrap();
            let level = Rc::new(Cell::new(7_u32));
            let mut objects = ObjectServer::new();
            objects.enable_object_manager("/org/tensor").unwrap();
            objects
                .register_signal::<(String, u32)>(OBJECT, INTERFACE, "LevelChanged")
                .unwrap();
            objects
                .register_read_only_property::<String, _, _>(
                    OBJECT,
                    INTERFACE,
                    "Version",
                    PropertyChangeMode::Invalidates,
                    || async { Ok("1.0".to_owned()) },
                )
                .unwrap();
            objects
                .register_read_only_property::<String, _, _>(
                    OBJECT,
                    INTERFACE,
                    "Build",
                    PropertyChangeMode::Const,
                    || async { Ok("release".to_owned()) },
                )
                .unwrap();
            objects
                .register_read_only_property::<bool, _, _>(
                    OBJECT,
                    INTERFACE,
                    "Private",
                    PropertyChangeMode::Silent,
                    || async { Ok(true) },
                )
                .unwrap();
            let get_level = Rc::clone(&level);
            let set_level = Rc::clone(&level);
            objects
                .register_property::<u32, _, _, _, _, _>(
                    OBJECT,
                    INTERFACE,
                    "Level",
                    PropertyChangeMode::Value,
                    move || {
                        let value = get_level.get();
                        async move { Ok(value) }
                    },
                    move |value| {
                        set_level.set(value.min(40));
                        async { Ok(()) }
                    },
                )
                .unwrap();

            for _ in 0..10 {
                assert!(objects.serve_next(&mut connection).await.unwrap().is_none());
            }
            assert_eq!(level.get(), 40);
            objects
                .emit_properties_changed(
                    &mut connection,
                    OBJECT,
                    INTERFACE,
                    &["Level", "Version", "Build", "Private"],
                )
                .await
                .unwrap();
            objects
                .emit_interfaces_added(&mut connection, "/org/tensor", OBJECT)
                .await
                .unwrap();
            assert_eq!(objects.unregister_object(OBJECT).unwrap(), [INTERFACE]);
            objects
                .emit_interfaces_removed(&mut connection, "/org/tensor", OBJECT, &[INTERFACE])
                .await
                .unwrap();
        });

        let stream = UnixStream::connect(&path).await.unwrap();
        let mut connection = Connection::connect_peer(stream, Some(guid)).await.unwrap();
        let mut properties =
            Proxy::new(&mut connection, None::<&str>, OBJECT, Some(PROPERTIES)).unwrap();
        let version: OwnedValue = properties
            .call("Get", &(INTERFACE, "Version"))
            .await
            .unwrap();
        assert_eq!(String::try_from(version).unwrap(), "1.0");

        let value = Value::new(42_u32).try_into_owned().unwrap();
        properties
            .call::<_, ()>("Set", &(INTERFACE, "Level", value))
            .await
            .unwrap();
        let level: OwnedValue = properties.call("Get", &(INTERFACE, "Level")).await.unwrap();
        assert_eq!(u32::try_from(level).unwrap(), 40);

        let all: PropertyMap = properties.call("GetAll", &INTERFACE).await.unwrap();
        assert_eq!(
            u32::try_from(all["Level"].try_clone().unwrap()).unwrap(),
            40
        );
        assert_eq!(
            String::try_from(all["Version"].try_clone().unwrap()).unwrap(),
            "1.0"
        );
        let missing: PropertyMap = properties
            .call("GetAll", &"org.tensor.Missing")
            .await
            .unwrap();
        assert!(missing.is_empty());

        let read_only = Value::new("2.0").try_into_owned().unwrap();
        let (name, _) = method_error(
            properties
                .call::<_, ()>("Set", &(INTERFACE, "Version", read_only))
                .await
                .unwrap_err(),
        );
        assert_eq!(name, "org.freedesktop.DBus.Error.PropertyReadOnly");

        let wrong_type = Value::new("high").try_into_owned().unwrap();
        let (name, _) = method_error(
            properties
                .call::<_, ()>("Set", &(INTERFACE, "Level", wrong_type))
                .await
                .unwrap_err(),
        );
        assert_eq!(name, "org.freedesktop.DBus.Error.InvalidArgs");
        drop(properties);

        let set_changed = connection.receive().await.unwrap();
        assert_eq!(set_changed.interface(), Some(PROPERTIES));
        assert_eq!(set_changed.member(), Some("PropertiesChanged"));
        let (interface, values, invalidated): (String, PropertyMap, Vec<String>) =
            set_changed.body().unwrap();
        assert_eq!(interface, INTERFACE);
        assert_eq!(
            u32::try_from(values["Level"].try_clone().unwrap()).unwrap(),
            40
        );
        assert!(invalidated.is_empty());

        let mut introspection =
            Proxy::new(&mut connection, None::<&str>, OBJECT, Some(INTROSPECTABLE)).unwrap();
        let xml: String = introspection.call("Introspect", &()).await.unwrap();
        assert!(xml.contains("<property name=\"Level\" type=\"u\" access=\"readwrite\">"));
        assert!(xml.contains("<property name=\"Version\" type=\"s\" access=\"read\">"));
        assert!(xml.contains("org.freedesktop.DBus.Property.EmitsChangedSignal\" value=\"true\""));
        assert!(
            xml.contains(
                "org.freedesktop.DBus.Property.EmitsChangedSignal\" value=\"invalidates\""
            )
        );
        assert!(xml.contains("org.freedesktop.DBus.Property.EmitsChangedSignal\" value=\"const\""));
        assert!(xml.contains("org.freedesktop.DBus.Property.EmitsChangedSignal\" value=\"false\""));
        assert!(xml.contains(
            "<signal name=\"LevelChanged\">\n      <arg type=\"s\"/>\n      <arg type=\"u\"/>"
        ));
        assert!(xml.contains(PROPERTIES));
        assert!(!xml.contains("GetMachineId"));
        drop(introspection);

        let mut root_introspection = Proxy::new(
            &mut connection,
            None::<&str>,
            "/org/tensor",
            Some(INTROSPECTABLE),
        )
        .unwrap();
        let xml: String = root_introspection.call("Introspect", &()).await.unwrap();
        assert!(xml.contains("<node name=\"Managed\"/>"));
        assert!(!xml.contains("<node name=\"Managed/"));
        drop(root_introspection);

        let mut manager =
            Proxy::new(&mut connection, None::<&str>, "/org/tensor", None::<&str>).unwrap();
        let managed: ManagedObjects = manager.call("GetManagedObjects", &()).await.unwrap();
        assert_eq!(managed.len(), 1);
        let object_path = OwnedObjectPath::try_from(OBJECT).unwrap();
        assert_eq!(
            u32::try_from(
                managed[&object_path][INTERFACE]["Level"]
                    .try_clone()
                    .unwrap()
            )
            .unwrap(),
            40
        );
        drop(manager);

        let changed = connection.receive().await.unwrap();
        assert_eq!(changed.interface(), Some(PROPERTIES));
        assert_eq!(changed.member(), Some("PropertiesChanged"));
        let (interface, values, invalidated): (String, PropertyMap, Vec<String>) =
            changed.body().unwrap();
        assert_eq!(interface, INTERFACE);
        assert_eq!(
            u32::try_from(values["Level"].try_clone().unwrap()).unwrap(),
            40
        );
        assert_eq!(invalidated, ["Version"]);

        let added = connection.receive().await.unwrap();
        assert_eq!(added.interface(), Some(OBJECT_MANAGER));
        assert_eq!(added.member(), Some("InterfacesAdded"));
        let (added_path, interfaces): (OwnedObjectPath, InterfaceMap) = added.body().unwrap();
        assert_eq!(added_path.as_str(), OBJECT);
        assert_eq!(
            String::try_from(interfaces[INTERFACE]["Version"].try_clone().unwrap()).unwrap(),
            "1.0"
        );

        let removed = connection.receive().await.unwrap();
        assert_eq!(removed.interface(), Some(OBJECT_MANAGER));
        assert_eq!(removed.member(), Some("InterfacesRemoved"));
        let (removed_path, interfaces): (OwnedObjectPath, Vec<String>) = removed.body().unwrap();
        assert_eq!(removed_path.as_str(), OBJECT);
        assert_eq!(interfaces, [INTERFACE]);
        server.await.unwrap();
    });
    fs::remove_file(path).unwrap();
}
