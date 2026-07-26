//! `wp_security_context_v1` with Compio-completed listener operations.
//!
//! Protocol objects stay on the compositor thread. Each committed listener is
//! transferred to one Compio runtime, which submits both `accept` and a read of
//! the close fd. Only accepted streams and immutable metadata cross back.

use std::{
    cell::Cell,
    io,
    os::{
        fd::OwnedFd,
        unix::net::{UnixListener as StdUnixListener, UnixStream as StdUnixStream},
    },
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};

use compio::{
    io::AsyncRead,
    net::{UnixListener, UnixStream},
    runtime::{JoinHandle as CompioJoinHandle, Runtime, fd::AsyncFd},
};
use futures_util::future::{Either, select};
use rustix::{
    io::fcntl_dupfd_cloexec,
    net::{
        AddressFamily, Shutdown, SocketType, getsockname, shutdown,
        sockopt::{socket_acceptconn, socket_type},
    },
};
use smithay::wayland::{Dispatch2, GlobalDispatch2};
use tensor_protocol::SecurityContextMetadata;
use tensor_runtime::{
    EventfdWake, EventfdWakeError, TrySendError, WakeSink, WorkerBridge, WorkerRx, WorkerTx,
};
use thiserror::Error;
use tracing::{debug, warn};
use wayland_protocols::wp::security_context::v1::server::{
    wp_security_context_manager_v1::{self, WpSecurityContextManagerV1},
    wp_security_context_v1::{self, WpSecurityContextV1},
};
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, New, Resource, backend::GlobalId};

const VERSION: u32 = 1;
const MAX_ACTIVE_LISTENERS: usize = 64;
const MAX_PENDING_LISTENERS: usize = 32;
pub(crate) const MAX_PENDING_SECURITY_CONTEXT_EVENTS: usize = 64;

/// Owns the advertised security-context global.
pub struct SecurityContextManagerState {
    _global: GlobalId,
}

pub struct SecurityContextGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

pub struct SecurityContextManagerUserData;

#[derive(Debug)]
pub struct SecurityContextUserData(Mutex<Option<SecurityContextBuilder>>);

#[derive(Debug)]
struct SecurityContextBuilder {
    listener: StdUnixListener,
    close_fd: OwnedFd,
    sandbox_engine: Option<String>,
    app_id: Option<String>,
    instance_id: Option<String>,
}

/// FDs and value metadata produced by a committed protocol object.
#[derive(Debug)]
pub struct SecurityContextListener {
    listener: StdUnixListener,
    close_fd: OwnedFd,
    context: Arc<SecurityContextMetadata>,
}

pub trait SecurityContextHandler: 'static {
    fn security_context_is_nested(&self, client: &Client) -> bool;
    fn context_created(&mut self, listener: SecurityContextListener);
}

impl crate::protocol::state::RuntimeState {
    pub(crate) fn install_security_context_submitter(
        &mut self,
        submitter: SecurityContextSubmitter,
    ) {
        self.security_context_submitter = Some(submitter);
    }

    pub(crate) fn security_context_submitter(&self) -> Option<&SecurityContextSubmitter> {
        self.security_context_submitter.as_ref()
    }
}

impl SecurityContextManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: wayland_server::GlobalDispatch<WpSecurityContextManagerV1, SecurityContextGlobalData>,
        D: Dispatch<WpSecurityContextManagerV1, SecurityContextManagerUserData>,
        D: Dispatch<WpSecurityContextV1, SecurityContextUserData>,
        D: SecurityContextHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let global = display.create_global::<D, WpSecurityContextManagerV1, _>(
            VERSION,
            SecurityContextGlobalData {
                filter: Box::new(filter),
            },
        );
        Self { _global: global }
    }
}

impl<D> GlobalDispatch2<WpSecurityContextManagerV1, D> for SecurityContextGlobalData
where
    D: Dispatch<WpSecurityContextManagerV1, SecurityContextManagerUserData>,
    D: 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WpSecurityContextManagerV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, SecurityContextManagerUserData);
    }

    fn can_view(&self, client: &Client) -> bool {
        (self.filter)(client)
    }
}

impl<D> Dispatch2<WpSecurityContextManagerV1, D> for SecurityContextManagerUserData
where
    D: Dispatch<WpSecurityContextV1, SecurityContextUserData>,
    D: SecurityContextHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        client: &Client,
        manager: &WpSecurityContextManagerV1,
        request: wp_security_context_manager_v1::Request,
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wp_security_context_manager_v1::Request::CreateListener {
                id,
                listen_fd,
                close_fd,
            } => {
                if state.security_context_is_nested(client) {
                    manager.post_error(
                        wp_security_context_manager_v1::Error::Nested,
                        "nested security contexts are forbidden",
                    );
                    return;
                }
                if !is_valid_listener(&listen_fd) {
                    manager.post_error(
                        wp_security_context_manager_v1::Error::InvalidListenFd,
                        "listen_fd is not a listening Unix stream socket",
                    );
                    return;
                }
                data_init.init(
                    id,
                    SecurityContextUserData(Mutex::new(Some(SecurityContextBuilder {
                        listener: StdUnixListener::from(listen_fd),
                        close_fd,
                        sandbox_engine: None,
                        app_id: None,
                        instance_id: None,
                    }))),
                );
            }
            wp_security_context_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl<D> Dispatch2<WpSecurityContextV1, D> for SecurityContextUserData
where
    D: SecurityContextHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        context: &WpSecurityContextV1,
        request: wp_security_context_v1::Request,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        let mut builder = self.0.lock().unwrap();
        if matches!(request, wp_security_context_v1::Request::Destroy) {
            return;
        }
        if builder.is_none() {
            context.post_error(
                wp_security_context_v1::Error::AlreadyUsed,
                "security context has already been committed",
            );
            return;
        }

        match request {
            wp_security_context_v1::Request::SetSandboxEngine { name } => {
                set_once(
                    context,
                    &mut builder.as_mut().unwrap().sandbox_engine,
                    name,
                    "sandbox engine",
                );
            }
            wp_security_context_v1::Request::SetAppId { app_id } => {
                set_once(
                    context,
                    &mut builder.as_mut().unwrap().app_id,
                    app_id,
                    "application id",
                );
            }
            wp_security_context_v1::Request::SetInstanceId { instance_id } => {
                set_once(
                    context,
                    &mut builder.as_mut().unwrap().instance_id,
                    instance_id,
                    "instance id",
                );
            }
            wp_security_context_v1::Request::Commit => {
                let builder = builder.take().expect("builder was checked");
                state.context_created(SecurityContextListener {
                    listener: builder.listener,
                    close_fd: builder.close_fd,
                    context: Arc::new(SecurityContextMetadata::new(
                        builder.sandbox_engine,
                        builder.app_id,
                        builder.instance_id,
                    )),
                });
            }
            _ => unreachable!(),
        }
    }
}

fn set_once(
    resource: &WpSecurityContextV1,
    slot: &mut Option<String>,
    value: String,
    field: &'static str,
) {
    if slot.is_some() {
        resource.post_error(
            wp_security_context_v1::Error::AlreadySet,
            format!("security context already has a {field}"),
        );
        return;
    }
    *slot = Some(value);
}

fn is_valid_listener(fd: &OwnedFd) -> bool {
    socket_type(fd).is_ok_and(|kind| kind == SocketType::STREAM)
        && socket_acceptconn(fd).unwrap_or(false)
        && getsockname(fd).is_ok_and(|address| address.address_family() == AddressFamily::UNIX)
}

#[derive(Clone)]
pub(crate) struct SecurityContextSubmitter {
    requests: WorkerTx<SecurityContextListener>,
}

impl SecurityContextSubmitter {
    pub(crate) fn submit(&self, listener: SecurityContextListener) -> Result<(), TrySendError> {
        self.requests.try_send(listener)
    }
}

/// Values returned by submitted security-context listener operations.
pub(crate) enum SecurityContextEvent {
    ListenerReady(Arc<SecurityContextMetadata>),
    Accepted {
        stream: StdUnixStream,
        context: Arc<SecurityContextMetadata>,
    },
    ListenerClosed(Arc<SecurityContextMetadata>),
    ListenerFailed {
        context: Arc<SecurityContextMetadata>,
        message: String,
    },
    RuntimeFailed(String),
}

/// Owns one Compio runtime for all security-context accept and close operations.
pub(crate) struct SecurityContextRuntime {
    submitter: SecurityContextSubmitter,
    wake: Arc<EventfdWake>,
    stopping: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl SecurityContextRuntime {
    pub(crate) fn start(
        events: WorkerTx<SecurityContextEvent>,
    ) -> Result<Self, SecurityContextRuntimeError> {
        let wake = Arc::new(EventfdWake::new()?);
        let (requests, pending) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_LISTENERS,
            Arc::clone(&wake) as Arc<dyn WakeSink>,
        );
        let submitter = SecurityContextSubmitter { requests };
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_wake = Arc::clone(&wake);
        let thread_stopping = Arc::clone(&stopping);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("tensor-security-context-completions".to_owned())
            .spawn(move || {
                let runtime = match Runtime::new() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(SecurityContextRuntimeError::Runtime(error)));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let mut completion = match thread_wake.completion_reader() {
                        Ok(completion) => completion,
                        Err(error) => {
                            let _ =
                                ready_tx.send(Err(SecurityContextRuntimeError::AttachWake(error)));
                            return;
                        }
                    };
                    if ready_tx.send(Ok(())).is_err() {
                        return;
                    }
                    run_completion_loop(&mut completion, pending, events, thread_stopping).await;
                });
            })
            .map_err(SecurityContextRuntimeError::Spawn)?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                submitter,
                wake,
                stopping,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(SecurityContextRuntimeError::StartupDisconnected)
            }
        }
    }

    pub(crate) fn submitter(&self) -> SecurityContextSubmitter {
        self.submitter.clone()
    }
}

impl Drop for SecurityContextRuntime {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        self.wake.wake();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

async fn run_completion_loop(
    completion: &mut tensor_runtime::EventfdCompletion,
    pending: WorkerRx<SecurityContextListener>,
    events: WorkerTx<SecurityContextEvent>,
    stopping: Arc<AtomicBool>,
) {
    let active = Rc::new(Cell::new(0usize));
    let mut tasks = Vec::with_capacity(MAX_ACTIVE_LISTENERS);
    loop {
        if let Err(error) = completion.completed().await {
            let _ = events.try_send(SecurityContextEvent::RuntimeFailed(error.to_string()));
            stop_listener_tasks(tasks).await;
            return;
        }
        if stopping.load(Ordering::Acquire) {
            stop_listener_tasks(tasks).await;
            return;
        }
        tasks.retain(|task: &ActiveListenerTask| !task.join.is_finished());
        while let Some(listener) = pending.try_recv() {
            if active.get() >= MAX_ACTIVE_LISTENERS {
                let _ = events.try_send(SecurityContextEvent::ListenerFailed {
                    context: listener.context,
                    message: format!("active listener limit {MAX_ACTIVE_LISTENERS} reached"),
                });
                continue;
            }
            let stop = match EventfdWake::new() {
                Ok(stop) => Arc::new(stop),
                Err(error) => {
                    emit_failure(&events, listener.context, error);
                    continue;
                }
            };
            active.set(active.get() + 1);
            let slot = ListenerSlot::new(Rc::clone(&active));
            let listener_events = events.clone();
            let listener_stop = Arc::clone(&stop);
            let join = compio::runtime::spawn(async move {
                let _slot = slot;
                run_listener(listener, listener_events, listener_stop).await;
            });
            tasks.push(ActiveListenerTask { stop, join });
        }
    }
}

struct ActiveListenerTask {
    stop: Arc<EventfdWake>,
    join: CompioJoinHandle<()>,
}

async fn stop_listener_tasks(tasks: Vec<ActiveListenerTask>) {
    for task in &tasks {
        task.stop.wake();
    }
    for task in tasks {
        let _ = task.join.await;
    }
}

struct ListenerSlot {
    active: Rc<Cell<usize>>,
}

impl ListenerSlot {
    fn new(active: Rc<Cell<usize>>) -> Self {
        Self { active }
    }
}

impl Drop for ListenerSlot {
    fn drop(&mut self) {
        self.active.set(self.active.get().saturating_sub(1));
    }
}

async fn run_listener(
    listener: SecurityContextListener,
    events: WorkerTx<SecurityContextEvent>,
    runtime_stop: Arc<EventfdWake>,
) {
    let SecurityContextListener {
        listener,
        close_fd,
        context,
    } = listener;
    let listener = match UnixListener::from_std(listener) {
        Ok(listener) => listener,
        Err(error) => {
            emit_failure(&events, context, error);
            return;
        }
    };
    let close_fd = match AsyncFd::new(close_fd) {
        Ok(close_fd) => close_fd,
        Err(error) => {
            emit_failure(&events, context, error);
            return;
        }
    };
    let mut stop_completion = match runtime_stop.completion_reader() {
        Ok(completion) => completion,
        Err(error) => {
            emit_failure(&events, context, error);
            return;
        }
    };
    if events
        .try_send(SecurityContextEvent::ListenerReady(Arc::clone(&context)))
        .is_err()
    {
        warn!(?context, "security-context ready completion was dropped");
    }

    let protocol = compio::runtime::spawn(wait_for_protocol_close(
        listener.clone(),
        close_fd,
        Arc::clone(&context),
        events.clone(),
    ));
    let stop = compio::runtime::spawn(async move { stop_completion.completed().await });
    match select(protocol, stop).await {
        Either::Left((result, stop)) => {
            let _ = stop.cancel().await;
            let close_result = listener.close().await;
            match result {
                Ok(Ok(ListenerCompletion::Closed)) if close_result.is_ok() => {
                    let _ = events.try_send(SecurityContextEvent::ListenerClosed(context));
                }
                Ok(Err(error)) => emit_failure(&events, context, error),
                Err(error) => emit_failure(&events, context, error),
                Ok(Ok(ListenerCompletion::Closed)) => {
                    emit_failure(&events, context, close_result.unwrap_err());
                }
            }
        }
        Either::Right((result, protocol)) => {
            let _ = shutdown(&listener, Shutdown::Both);
            let _ = protocol.cancel().await;
            let close_result = listener.close().await;
            match result {
                Ok(Ok(_)) if close_result.is_ok() => {}
                Ok(Err(error)) => emit_failure(&events, context, error),
                Err(error) => emit_failure(&events, context, error),
                Ok(Ok(_)) => emit_failure(&events, context, close_result.unwrap_err()),
            }
        }
    }
}

enum ListenerCompletion {
    Closed,
}

async fn wait_for_protocol_close(
    listener: UnixListener,
    close_fd: AsyncFd<OwnedFd>,
    context: Arc<SecurityContextMetadata>,
    events: WorkerTx<SecurityContextEvent>,
) -> io::Result<ListenerCompletion> {
    let accepts = compio::runtime::spawn(accept_loop(listener.clone(), context, events));
    let close = compio::runtime::spawn(wait_for_close(close_fd));
    match select(accepts, close).await {
        Either::Left((result, close)) => {
            let _ = close.cancel().await;
            match result {
                Ok(Ok(())) => Err(io::Error::other("security-context accept loop stopped")),
                Ok(Err(error)) => Err(error),
                Err(error) => Err(io::Error::from(error)),
            }
        }
        Either::Right((result, accepts)) => {
            let _ = shutdown(&listener, Shutdown::Both);
            let _ = accepts.cancel().await;
            match result {
                Ok(Ok(())) => Ok(ListenerCompletion::Closed),
                Ok(Err(error)) => Err(error),
                Err(error) => Err(io::Error::from(error)),
            }
        }
    }
}

async fn accept_loop(
    listener: UnixListener,
    context: Arc<SecurityContextMetadata>,
    events: WorkerTx<SecurityContextEvent>,
) -> io::Result<()> {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let stream = duplicate_stream(&stream)?;
                if events
                    .try_send(SecurityContextEvent::Accepted {
                        stream,
                        context: Arc::clone(&context),
                    })
                    .is_err()
                {
                    warn!(?context, "security-context accepted stream was dropped");
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

fn duplicate_stream(stream: &UnixStream) -> io::Result<StdUnixStream> {
    fcntl_dupfd_cloexec(stream, 0)
        .map(StdUnixStream::from)
        .map_err(io::Error::from)
}

async fn wait_for_close(mut close_fd: AsyncFd<OwnedFd>) -> io::Result<()> {
    loop {
        let compio::BufResult(result, _) = close_fd.read([0u8; 8]).await;
        match result {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

fn emit_failure(
    events: &WorkerTx<SecurityContextEvent>,
    context: Arc<SecurityContextMetadata>,
    error: impl std::fmt::Display,
) {
    let message = error.to_string();
    if events
        .try_send(SecurityContextEvent::ListenerFailed {
            context: Arc::clone(&context),
            message: message.clone(),
        })
        .is_err()
    {
        warn!(?context, %message, "security-context failure completion was dropped");
    }
}

pub(crate) fn drain_security_context_events(
    events: &WorkerRx<SecurityContextEvent>,
    state: &mut crate::protocol::state::RuntimeState,
) -> Result<(), String> {
    while let Some(event) = events.try_recv() {
        match event {
            SecurityContextEvent::ListenerReady(context) => {
                debug!(?context, "security-context listener ready");
            }
            SecurityContextEvent::Accepted { stream, context } => {
                let client_data = crate::protocol::state::WaylandClientState {
                    security_context: Some(Arc::clone(&context)),
                    ..Default::default()
                };
                if let Err(error) = state
                    .display_handle
                    .insert_client(stream, Arc::new(client_data))
                {
                    warn!(%error, ?context, "failed to insert sandboxed Wayland client");
                }
            }
            SecurityContextEvent::ListenerClosed(context) => {
                debug!(?context, "security-context listener closed");
            }
            SecurityContextEvent::ListenerFailed { context, message } => {
                warn!(?context, %message, "security-context listener failed");
            }
            SecurityContextEvent::RuntimeFailed(message) => return Err(message),
        }
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum SecurityContextRuntimeError {
    #[error(transparent)]
    Wake(#[from] EventfdWakeError),
    #[error("failed to spawn the security-context completion thread: {0}")]
    Spawn(#[source] io::Error),
    #[error("failed to initialize the security-context Compio runtime: {0}")]
    Runtime(#[source] io::Error),
    #[error("failed to attach the security-context command wake: {0}")]
    AttachWake(#[source] io::Error),
    #[error("security-context runtime stopped during initialization")]
    StartupDisconnected,
}

#[cfg(test)]
mod runtime_tests;
