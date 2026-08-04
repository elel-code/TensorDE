use std::{
    os::linux::net::SocketAddrExt,
    os::unix::net::{SocketAddr, UnixStream as StdUnixStream},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use rustix::pipe::pipe;

use super::*;

static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);

struct TestListener {
    address: SocketAddr,
    listener: Option<StdUnixListener>,
}

impl TestListener {
    fn new() -> Self {
        let ordinal = NEXT_SOCKET.fetch_add(1, Ordering::Relaxed);
        let name = format!("tensor-security-context-{}-{ordinal}", std::process::id());
        let address = SocketAddr::from_abstract_name(name).unwrap();
        let listener = StdUnixListener::bind_addr(&address).unwrap();
        Self {
            address,
            listener: Some(listener),
        }
    }

    fn request(
        &mut self,
        close_fd: OwnedFd,
        context: Arc<SecurityContextMetadata>,
    ) -> SecurityContextListener {
        SecurityContextListener {
            listener: self.listener.take().unwrap(),
            close_fd,
            context,
        }
    }
}

fn metadata() -> Arc<SecurityContextMetadata> {
    Arc::new(SecurityContextMetadata::new(
        Some("org.flatpak".to_owned()),
        Some("org.tensor.Test".to_owned()),
        None,
    ))
}

fn ready(events: &WorkerRx<SecurityContextEvent>) {
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(2)).unwrap(),
        SecurityContextEvent::ListenerReady(_)
    ));
}

#[test]
fn accept_is_published_only_after_its_completion() {
    let (event_tx, events) = WorkerBridge::bounded(8);
    let runtime = SecurityContextRuntime::start(event_tx).unwrap();
    let mut listener = TestListener::new();
    let (close_fd, close_writer) = pipe().unwrap();
    let expected = metadata();
    runtime
        .submitter()
        .submit(listener.request(close_fd, Arc::clone(&expected)))
        .unwrap();
    ready(&events);

    assert!(events.recv_timeout(Duration::from_millis(30)).is_err());
    let _client = StdUnixStream::connect_addr(&listener.address).unwrap();
    match events.recv_timeout(Duration::from_secs(2)).unwrap() {
        SecurityContextEvent::Accepted { context, .. } => assert_eq!(context, expected),
        _ => panic!("expected an accepted-stream completion"),
    }
    drop(close_writer);
}

#[test]
fn close_fd_completion_cancels_the_pending_accept() {
    let (event_tx, events) = WorkerBridge::bounded(8);
    let runtime = SecurityContextRuntime::start(event_tx).unwrap();
    let mut listener = TestListener::new();
    let (close_fd, close_writer) = pipe().unwrap();
    runtime
        .submitter()
        .submit(listener.request(close_fd, metadata()))
        .unwrap();
    ready(&events);

    drop(close_writer);
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(2)).unwrap(),
        SecurityContextEvent::ListenerClosed(_)
    ));
    assert!(StdUnixStream::connect_addr(&listener.address).is_err());
}

#[test]
fn runtime_shutdown_cancels_submitted_listener_operations() {
    let (event_tx, events) = WorkerBridge::bounded(8);
    let runtime = SecurityContextRuntime::start(event_tx).unwrap();
    let mut listener = TestListener::new();
    let (close_fd, _close_writer) = pipe().unwrap();
    runtime
        .submitter()
        .submit(listener.request(close_fd, metadata()))
        .unwrap();
    ready(&events);

    drop(runtime);
    assert!(StdUnixStream::connect_addr(&listener.address).is_err());
}

#[test]
fn listener_validation_rejects_non_listening_fds() {
    let (stream, _peer) = StdUnixStream::pair().unwrap();
    let fd = OwnedFd::from(stream);
    assert!(!is_valid_listener(&fd));
}
