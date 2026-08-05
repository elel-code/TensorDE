use std::{
    ffi::OsString,
    fs,
    os::{fd::AsFd, unix::ffi::OsStringExt},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use compio::{
    driver::{DriverType, ProactorBuilder},
    net::{UnixListener, UnixStream},
    runtime::RuntimeBuilder,
};
use tensor_dbus::{
    Connection, ConnectionMode, Error, Guid, MethodCall, PeerListener, RequestNameFlags,
    reply_method, zvariant::DynamicType,
};

fn socket_path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    std::env::current_dir()
        .unwrap()
        .join("target")
        .join(format!(
            "tensor-dbus-peer-{}-{}.sock",
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

fn abstract_socket_path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let name = format!(
        "tensor-dbus-peer-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let mut bytes = Vec::with_capacity(name.len() + 1);
    bytes.push(0);
    bytes.extend_from_slice(name.as_bytes());
    PathBuf::from(OsString::from_vec(bytes))
}

#[test]
fn compio_peer_connections_call_methods_without_a_bus() {
    let path = socket_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = fs::remove_file(&path);
    let server_guid = Guid::generate().unwrap();

    runtime().block_on(async {
        let listener = UnixListener::bind(&path).await.unwrap();
        let server = compio::runtime::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut connection = Connection::accept_peer(stream, server_guid).await.unwrap();
            assert_eq!(connection.mode(), ConnectionMode::Peer);
            assert_eq!(connection.server_guid(), server_guid);
            assert!(connection.unique_name().is_none());
            assert_eq!(
                connection.peer_credentials().unwrap().user_id,
                rustix::process::getuid().as_raw()
            );
            assert!(connection.supports_unix_fd());
            assert!(matches!(
                connection
                    .request_name("org.tensor.Peer", RequestNameFlags::default())
                    .await,
                Err(Error::BusOperationOnPeer)
            ));

            let message = connection.receive().await.unwrap();
            let call = MethodCall::new(message).unwrap();
            assert_eq!(call.path(), "/org/tensor/Peer");
            assert_eq!(call.interface(), None);
            assert_eq!(call.member(), "Echo");
            assert_eq!(call.sender(), None);
            let value: String = call.body().unwrap();
            reply_method(&mut connection, &call, &format!("peer:{value}"))
                .await
                .unwrap();

            let message = connection.receive().await.unwrap();
            let call = MethodCall::new(message).unwrap();
            let dynamic = call.message().body_dynamic().unwrap();
            reply_method(&mut connection, &call, &dynamic)
                .await
                .unwrap();
        });

        let stream = UnixStream::connect(&path).await.unwrap();
        let mut client = Connection::connect_peer(stream, Some(server_guid))
            .await
            .unwrap();
        assert_eq!(client.mode(), ConnectionMode::Peer);
        assert_eq!(client.server_guid(), server_guid);
        assert!(client.unique_name().is_none());
        assert!(client.peer_credentials().is_none());
        let reply: String = client
            .call(None, "/org/tensor/Peer", None, "Echo", &"hello")
            .await
            .unwrap();
        assert_eq!(reply, "peer:hello");
        let dynamic: (u32, String) = client
            .call(
                None,
                "/org/tensor/Peer",
                None,
                "Dynamic",
                &(17_u32, "runtime"),
            )
            .await
            .unwrap();
        assert_eq!(dynamic, (17, "runtime".to_owned()));
        server.await.unwrap();
    });
    fs::remove_file(path).unwrap();
}

#[test]
fn compio_peer_connections_exchange_unix_fds() {
    let path = socket_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = fs::remove_file(&path);
    let server_guid = Guid::generate().unwrap();

    runtime().block_on(async {
        let listener = UnixListener::bind(&path).await.unwrap();
        let server = compio::runtime::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut connection = Connection::accept_peer(stream, server_guid).await.unwrap();
            let message = connection.receive().await.unwrap();
            let call = MethodCall::new(message).unwrap();
            let received: (
                tensor_dbus::zvariant::OwnedFd,
                tensor_dbus::zvariant::OwnedFd,
            ) = call.body().unwrap();
            for fd in [&received.0, &received.1] {
                assert!(
                    rustix::io::fcntl_getfd(fd.as_fd())
                        .unwrap()
                        .contains(rustix::io::FdFlags::CLOEXEC)
                );
            }
            reply_method(
                &mut connection,
                &call,
                &(
                    tensor_dbus::zvariant::Fd::from(&received.0),
                    tensor_dbus::zvariant::Fd::from(&received.1),
                ),
            )
            .await
            .unwrap();
        });

        let stream = UnixStream::connect(&path).await.unwrap();
        let mut client = Connection::connect_peer(stream, Some(server_guid))
            .await
            .unwrap();
        let sources = [
            fs::File::open("/dev/null").unwrap(),
            fs::File::open("/dev/null").unwrap(),
        ];
        let returned: (
            tensor_dbus::zvariant::OwnedFd,
            tensor_dbus::zvariant::OwnedFd,
        ) = client
            .call(
                None,
                "/org/tensor/Peer",
                Some("org.tensor.Peer"),
                "ExchangeFd",
                &(
                    tensor_dbus::zvariant::Fd::from(&sources[0]),
                    tensor_dbus::zvariant::Fd::from(&sources[1]),
                ),
            )
            .await
            .unwrap();
        for fd in [&returned.0, &returned.1] {
            assert!(
                rustix::io::fcntl_getfd(fd.as_fd())
                    .unwrap()
                    .contains(rustix::io::FdFlags::CLOEXEC)
            );
        }
        server.await.unwrap();
    });
    fs::remove_file(path).unwrap();
}

#[test]
fn dynamic_body_owns_unix_fds_after_the_source_message_is_dropped() {
    let path = socket_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = fs::remove_file(&path);
    let server_guid = Guid::generate().unwrap();

    runtime().block_on(async {
        let listener = UnixListener::bind(&path).await.unwrap();
        let server = compio::runtime::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut connection = Connection::accept_peer(stream, server_guid).await.unwrap();
            let message = connection.receive().await.unwrap();
            let dynamic = message.body_dynamic().unwrap();
            assert_eq!(dynamic.signature().to_string_no_parens(), "h");
            drop(message);

            connection
                .emit_signal("/org/tensor/Peer", "org.tensor.Peer", "DynamicFd", &dynamic)
                .await
                .unwrap();
        });

        let stream = UnixStream::connect(&path).await.unwrap();
        let mut client = Connection::connect_peer(stream, Some(server_guid))
            .await
            .unwrap();
        let source = fs::File::open("/dev/null").unwrap();
        client
            .send_no_reply(
                None,
                "/org/tensor/Peer",
                Some("org.tensor.Peer"),
                "ForwardDynamicFd",
                &tensor_dbus::zvariant::Fd::from(&source),
            )
            .await
            .unwrap();

        let signal = client.receive().await.unwrap();
        assert_eq!(signal.member(), Some("DynamicFd"));
        let returned: tensor_dbus::zvariant::OwnedFd = signal.body().unwrap();
        assert!(
            rustix::io::fcntl_getfd(returned.as_fd())
                .unwrap()
                .contains(rustix::io::FdFlags::CLOEXEC)
        );
        server.await.unwrap();
    });
    fs::remove_file(path).unwrap();
}

#[test]
fn compio_peer_client_rejects_the_wrong_server_guid() {
    let path = socket_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = fs::remove_file(&path);
    let server_guid = Guid::generate().unwrap();
    let wrong_guid = Guid::generate().unwrap();

    runtime().block_on(async {
        let listener = UnixListener::bind(&path).await.unwrap();
        let server = compio::runtime::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ = Connection::accept_peer(stream, server_guid).await;
        });
        let stream = UnixStream::connect(&path).await.unwrap();
        let error = match Connection::connect_peer(stream, Some(wrong_guid)).await {
            Ok(_) => panic!("peer unexpectedly accepted the wrong GUID"),
            Err(error) => error,
        };
        assert!(
            matches!(error, Error::Authentication(message) if message.contains("GUID mismatch"))
        );
        server.await.unwrap();
    });
    fs::remove_file(path).unwrap();
}

#[test]
fn peer_listener_keeps_accept_independent_from_slow_authentication() {
    let path = socket_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = fs::remove_file(&path);
    let server_guid = Guid::generate().unwrap();

    runtime().block_on(async {
        let listener = PeerListener::bind(&path, server_guid).await.unwrap();
        assert_eq!(listener.server_guid(), server_guid);
        let slow_client = UnixStream::connect(&path).await.unwrap();
        let first = listener.accept().await.unwrap();
        assert_eq!(
            first.credentials().user_id,
            rustix::process::getuid().as_raw()
        );

        let client_path = path.clone();
        let second_client = compio::runtime::spawn(async move {
            let stream = UnixStream::connect(&client_path).await.unwrap();
            Connection::connect_peer(stream, Some(server_guid))
                .await
                .unwrap()
        });
        let second = listener.accept().await.unwrap();
        let second_server = second.authenticate().await.unwrap();
        let second_client = second_client.await.unwrap();
        assert_eq!(second_server.mode(), ConnectionMode::Peer);
        assert_eq!(second_client.mode(), ConnectionMode::Peer);

        drop(slow_client);
        assert!(first.authenticate().await.is_err());
    });
    fs::remove_file(path).unwrap();
}

#[test]
fn peer_listener_accepts_linux_abstract_unix_connections() {
    let path = abstract_socket_path();
    let server_guid = Guid::generate().unwrap();

    runtime().block_on(async {
        let listener = PeerListener::bind(&path, server_guid).await.unwrap();
        let server = compio::runtime::spawn(async move {
            let mut connection = listener.accept_authenticated().await.unwrap();
            let call = MethodCall::new(connection.receive().await.unwrap()).unwrap();
            let value: u32 = call.body().unwrap();
            reply_method(&mut connection, &call, &(value + 1))
                .await
                .unwrap();
        });

        let stream = UnixStream::connect(&path).await.unwrap();
        let mut client = Connection::connect_peer(stream, Some(server_guid))
            .await
            .unwrap();
        let value: u32 = client
            .call(
                None,
                "/org/tensor/Peer",
                Some("org.tensor.Peer"),
                "Increment",
                &41_u32,
            )
            .await
            .unwrap();
        assert_eq!(value, 42);
        server.await.unwrap();
    });
}
