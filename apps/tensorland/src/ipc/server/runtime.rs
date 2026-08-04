//! Compio completion runtime for the Tensor Unix IPC socket.
//!
//! The listener and every client operation are submitted to Compio. Decoded
//! value requests cross a bounded bridge to the compositor; connection tasks
//! await a oneshot response without blocking the runtime thread.

use std::{
    io,
    os::unix::net::UnixListener as StdUnixListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
};

use compio::{
    BufResult,
    io::{AsyncRead, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};
use futures_channel::{mpsc, oneshot};
use futures_util::{
    StreamExt,
    future::{Either, select},
};
use rustix::{net::sockopt::socket_peercred, process::geteuid};
use tensor_runtime::{EventfdWake, TrySendError, WakeSink, WorkerTx, io_uring_runtime};
use tracing::warn;

use super::{IpcError, IpcReply};
use crate::ipc::{
    Command, EventMessage, FrameDecoder, IpcSubscriptionSink, Request, Response, ServerMessage,
    encode, subscription_channel,
};

const READ_BUFFER_SIZE: usize = 16 * 1024;
const MAX_IPC_CONNECTIONS: usize = 64;
pub(crate) const MAX_PENDING_IPC_REQUESTS: usize = 128;
pub(crate) const MAX_PENDING_IPC_CONTROL_EVENTS: usize = 1;

/// Values crossing from the IPC completion runtime to the compositor thread.
pub(crate) struct IpcEvent {
    pub(crate) request: Request,
    pub(crate) respond_to: oneshot::Sender<IpcReply>,
    pub(crate) subscription: Option<IpcSubscriptionSink>,
}

/// Critical completion state has a reserved slot separate from client load.
pub(crate) enum IpcControlEvent {
    ShutdownFlushed,
    RuntimeFailed(String),
}

/// Owns the Compio thread that submits IPC accept/read/write operations.
pub(crate) struct IpcRuntime {
    stop: Arc<EventfdWake>,
    join: Option<JoinHandle<()>>,
}

impl IpcRuntime {
    pub(super) fn start(
        listener: StdUnixListener,
        requests: WorkerTx<IpcEvent>,
        control: WorkerTx<IpcControlEvent>,
    ) -> Result<Self, IpcError> {
        let stop = Arc::new(EventfdWake::new()?);
        let thread_stop = Arc::clone(&stop);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("tensor-ipc-completions".to_owned())
            .spawn(move || {
                let runtime = match io_uring_runtime(MAX_IPC_CONNECTIONS + 2) {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(IpcError::Runtime(error)));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let listener = match UnixListener::from_std(listener) {
                        Ok(listener) => listener,
                        Err(error) => {
                            let _ = ready_tx.send(Err(IpcError::AttachListener(error)));
                            return;
                        }
                    };
                    let mut stop_completion = match thread_stop.completion_reader() {
                        Ok(completion) => completion,
                        Err(error) => {
                            let _ = ready_tx.send(Err(IpcError::AttachStop(error)));
                            return;
                        }
                    };
                    let accept_requests = requests.clone();
                    let accept_control = control.clone();
                    let accept_task = compio::runtime::spawn(async move {
                        accept_loop(listener, accept_requests, accept_control).await;
                    });
                    if ready_tx.send(Ok(())).is_err() {
                        return;
                    }
                    let _ = stop_completion.completed().await;
                    drop(accept_task);
                });
            })
            .map_err(IpcError::RuntimeThread)?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                stop,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(IpcError::RuntimeStartupDisconnected)
            }
        }
    }
}

impl Drop for IpcRuntime {
    fn drop(&mut self) {
        self.stop.wake();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

async fn accept_loop(
    listener: UnixListener,
    requests: WorkerTx<IpcEvent>,
    control: WorkerTx<IpcControlEvent>,
) {
    let active_connections = Arc::new(AtomicUsize::new(0));
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                if let Err(error) = verify_peer(&stream) {
                    warn!(%error, "IPC peer credentials rejected");
                    continue;
                }
                let Some(connection_slot) = ConnectionSlot::acquire(&active_connections) else {
                    warn!(limit = MAX_IPC_CONNECTIONS, "IPC connection limit reached");
                    continue;
                };
                let client_requests = requests.clone();
                let client_control = control.clone();
                compio::runtime::spawn(async move {
                    let _connection_slot = connection_slot;
                    if let Err(error) = handle_client(stream, client_requests, client_control).await
                    {
                        warn!(%error, "IPC completion connection closed");
                    }
                })
                .detach();
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                let _ = control.try_send(IpcControlEvent::RuntimeFailed(error.to_string()));
                return;
            }
        }
    }
}

struct ConnectionSlot {
    active: Arc<AtomicUsize>,
}

impl ConnectionSlot {
    fn acquire(active: &Arc<AtomicUsize>) -> Option<Self> {
        active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_IPC_CONNECTIONS).then_some(count + 1)
            })
            .ok()
            .map(|_| Self {
                active: Arc::clone(active),
            })
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

fn verify_peer(stream: &UnixStream) -> io::Result<()> {
    let credentials = socket_peercred(stream)?;
    let current_uid = geteuid();
    if credentials.uid != current_uid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "peer uid {} does not match compositor uid {current_uid}",
                credentials.uid
            ),
        ));
    }
    Ok(())
}

async fn handle_client(
    mut stream: UnixStream,
    requests: WorkerTx<IpcEvent>,
    control: WorkerTx<IpcControlEvent>,
) -> io::Result<()> {
    let mut decoder = FrameDecoder::new();
    loop {
        let BufResult(result, buffer) = stream.read(Vec::with_capacity(READ_BUFFER_SIZE)).await;
        let read = result?;
        if read == 0 {
            return Ok(());
        }
        let decoded_requests = decoder
            .push::<Request>(&buffer[..read])
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut pending = Vec::with_capacity(decoded_requests.len());
        let request_count = decoded_requests.len();
        let batch_ends_on_frame_boundary = decoder.buffered_bytes() == 0;
        let mut bridge_stopped = false;
        for (index, request) in decoded_requests.into_iter().enumerate() {
            let request_id = request.request_id;
            let closes_batch = index + 1 == request_count;
            if bridge_stopped {
                pending.push(unavailable_reply(request_id, closes_batch));
                continue;
            }
            let (subscription, subscription_events) = if closes_batch
                && batch_ends_on_frame_boundary
                && let Command::Subscribe { events } = &request.command
            {
                let (sink, receiver) = subscription_channel(events.clone());
                (Some(sink), Some(receiver))
            } else {
                (None, None)
            };
            let (respond_to, response) = oneshot::channel();
            match requests.try_send(IpcEvent {
                request,
                respond_to,
                subscription,
            }) {
                Ok(()) => pending.push(PendingIpcReply::Compositor {
                    request_id,
                    response,
                    subscription_events,
                }),
                Err(TrySendError::Full) => pending.push(PendingIpcReply::Ready {
                    reply: IpcReply::new(Response::error(
                        request_id,
                        "queue_full",
                        "compositor IPC request queue is full",
                    )),
                    close_after_flush: false,
                }),
                Err(TrySendError::Disconnected) => {
                    bridge_stopped = true;
                    pending.push(unavailable_reply(request_id, closes_batch));
                }
            }
        }
        let pending_count = pending.len();
        let mut compositor_stopped = false;
        let mut subscription_events = None;
        for (index, pending_reply) in pending.into_iter().enumerate() {
            let closes_batch = index + 1 == pending_count;
            let request_id = pending_reply.request_id();
            let (reply, close_after_flush, resolved_subscription) = if compositor_stopped {
                (unavailable_response(request_id), closes_batch, None)
            } else {
                match pending_reply.resolve().await {
                    Ok(resolved) => resolved,
                    Err(request_id) => {
                        compositor_stopped = true;
                        (unavailable_response(request_id), closes_batch, None)
                    }
                }
            };
            let subscription_accepted = reply.is_accepted();
            let stop_after_flush = reply.should_stop_after_flush();
            let frame = match encode(&ServerMessage::Response(reply.response)) {
                Ok(frame) => frame,
                Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
            };
            let BufResult(result, _) = stream.write_all(frame).await;
            result?;
            if subscription_accepted {
                subscription_events = resolved_subscription;
            }
            notify_shutdown_if_needed(stop_after_flush, &control);
            if close_after_flush {
                return Ok(());
            }
        }
        if let Some(events) = subscription_events {
            return stream_subscription_events(stream, events).await;
        }
    }
}

async fn stream_subscription_events(
    stream: UnixStream,
    events: mpsc::Receiver<EventMessage>,
) -> io::Result<()> {
    let (reader, writer) = stream.into_split();
    let peer = Box::pin(wait_for_subscription_peer(reader));
    let events = Box::pin(write_subscription_events(writer, events));
    match select(peer, events).await {
        Either::Left((result, _)) | Either::Right((result, _)) => result,
    }
}

async fn wait_for_subscription_peer(mut stream: UnixStream) -> io::Result<()> {
    let BufResult(result, _) = stream.read(Vec::with_capacity(1)).await;
    match result? {
        0 => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC subscription connections are receive-only after acceptance",
        )),
    }
}

async fn write_subscription_events(
    mut stream: UnixStream,
    mut events: mpsc::Receiver<EventMessage>,
) -> io::Result<()> {
    while let Some(event) = events.next().await {
        let frame = encode(&ServerMessage::Event(event))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let BufResult(result, _) = stream.write_all(frame).await;
        result?;
    }
    Ok(())
}

fn unavailable_reply(request_id: u64, close_after_flush: bool) -> PendingIpcReply {
    PendingIpcReply::Ready {
        reply: unavailable_response(request_id),
        close_after_flush,
    }
}

fn unavailable_response(request_id: u64) -> IpcReply {
    IpcReply::new(Response::error(
        request_id,
        "service_unavailable",
        "compositor IPC service has stopped",
    ))
}

enum PendingIpcReply {
    Compositor {
        request_id: u64,
        response: oneshot::Receiver<IpcReply>,
        subscription_events: Option<mpsc::Receiver<EventMessage>>,
    },
    Ready {
        reply: IpcReply,
        close_after_flush: bool,
    },
}

impl PendingIpcReply {
    fn request_id(&self) -> u64 {
        match self {
            Self::Compositor { request_id, .. } => *request_id,
            Self::Ready { reply, .. } => reply.response.request_id,
        }
    }

    async fn resolve(self) -> Result<(IpcReply, bool, Option<mpsc::Receiver<EventMessage>>), u64> {
        match self {
            Self::Compositor {
                request_id,
                response,
                subscription_events,
            } => response
                .await
                .map(|reply| (reply, false, subscription_events))
                .map_err(|_| request_id),
            Self::Ready {
                reply,
                close_after_flush,
            } => Ok((reply, close_after_flush, None)),
        }
    }
}

fn notify_shutdown_if_needed(stop_after_flush: bool, control: &WorkerTx<IpcControlEvent>) {
    if stop_after_flush {
        // Capacity one intentionally coalesces concurrent quit completions.
        let _ = control.try_send(IpcControlEvent::ShutdownFlushed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_slots_enforce_and_release_the_fixed_limit() {
        let active = Arc::new(AtomicUsize::new(0));
        let slots = (0..MAX_IPC_CONNECTIONS)
            .map(|_| ConnectionSlot::acquire(&active).expect("slot within limit"))
            .collect::<Vec<_>>();

        assert!(ConnectionSlot::acquire(&active).is_none());
        drop(slots);
        assert!(ConnectionSlot::acquire(&active).is_some());
    }
}
