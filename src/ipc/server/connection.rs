use std::{
    cell::RefCell,
    collections::VecDeque,
    io::{self, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    rc::Rc,
};

use smithay::reexports::calloop::{
    Interest, LoopHandle, LoopSignal, Mode, PostAction,
    generic::{Generic, NoIoDrop},
};
use smithay::reexports::rustix::{net::sockopt::socket_peercred, process::geteuid};
use tracing::warn;

use super::{super::codec, super::message::Request, IpcReply};

type Handler = Rc<RefCell<dyn FnMut(Request) -> IpcReply>>;

const READ_BUFFER_SIZE: usize = 16 * 1024;
const MAX_PENDING_BYTES: usize = 4 * codec::MAX_FRAME_SIZE;

pub(super) fn register_listener<H>(
    handle: &LoopHandle<'static, ()>,
    listener: UnixListener,
    handler: H,
) -> Result<(), String>
where
    H: FnMut(Request) -> IpcReply + 'static,
{
    let handler: Handler = Rc::new(RefCell::new(handler));
    let listener_handle = handle.clone();
    handle
        .insert_source(
            Generic::new(listener, Interest::READ, Mode::Level),
            move |_, source, _| {
                let listener = source_mut(source);
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let handler = handler.clone();
                            if let Err(error) = register_client(&listener_handle, stream, handler) {
                                warn!(%error, "failed to register IPC client");
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => return Err(error),
                    }
                }
                Ok(PostAction::Continue)
            },
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn register_client(
    handle: &LoopHandle<'static, ()>,
    stream: UnixStream,
    handler: Handler,
) -> Result<(), String> {
    verify_peer(&stream).map_err(|error| format!("IPC peer credentials rejected: {error}"))?;
    stream
        .set_nonblocking(true)
        .map_err(|error| format!("failed to configure IPC client: {error}"))?;
    let state = Rc::new(RefCell::new(ClientState::default()));
    let client_state = state.clone();
    let client_handler = handler.clone();
    let client_handle = handle.clone();
    handle
        .insert_source(
            Generic::new(stream, Interest::READ, Mode::Level),
            move |_, source, _| on_readable(&client_handle, &client_state, &client_handler, source),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
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

#[derive(Default)]
struct ClientState {
    decoder: codec::FrameDecoder,
    pending: VecDeque<u8>,
    writer_active: bool,
    stop_after_flush: Option<LoopSignal>,
}

fn on_readable(
    handle: &LoopHandle<'static, ()>,
    state: &Rc<RefCell<ClientState>>,
    handler: &Handler,
    source: &mut smithay::reexports::calloop::generic::NoIoDrop<UnixStream>,
) -> Result<PostAction, io::Error> {
    let stream = source_mut(source);
    let mut read_buffer = [0; READ_BUFFER_SIZE];
    let mut should_remove = false;

    for _ in 0..64 {
        match stream.read(&mut read_buffer) {
            Ok(0) => {
                should_remove = true;
                break;
            }
            Ok(read) => {
                let requests = {
                    let mut state = state.borrow_mut();
                    match state.decoder.push::<Request>(&read_buffer[..read]) {
                        Ok(requests) => requests,
                        Err(error) => {
                            warn!(%error, "closing malformed IPC client");
                            if let Some(signal) = state.stop_after_flush.take() {
                                signal.stop();
                            }
                            return Ok(PostAction::Remove);
                        }
                    }
                };
                for request in requests {
                    let reply = (handler.borrow_mut())(request);
                    let frame = match codec::encode(&reply.response) {
                        Ok(frame) => frame,
                        Err(error) => {
                            warn!(%error, "failed to encode IPC response");
                            if let Some(signal) = reply.stop_after_flush {
                                signal.stop();
                            } else {
                                stop_pending_action(state);
                            }
                            return Ok(PostAction::Remove);
                        }
                    };
                    let mut state = state.borrow_mut();
                    if state.pending.len().saturating_add(frame.len()) > MAX_PENDING_BYTES {
                        warn!(
                            limit = MAX_PENDING_BYTES,
                            "closing IPC client with an oversized response queue"
                        );
                        let stop_signal = reply
                            .stop_after_flush
                            .or_else(|| state.stop_after_flush.take());
                        if let Some(signal) = stop_signal {
                            signal.stop();
                        }
                        return Ok(PostAction::Remove);
                    }
                    state.pending.extend(frame);
                    if let Some(signal) = reply.stop_after_flush {
                        state.stop_after_flush = Some(signal);
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                warn!(%error, "closing unreadable IPC client");
                stop_pending_action(state);
                return Ok(PostAction::Remove);
            }
        }
    }

    let should_start_writer = {
        let state = state.borrow();
        !state.pending.is_empty() && !state.writer_active
    };
    if should_start_writer {
        let writer = match stream.try_clone() {
            Ok(writer) => writer,
            Err(error) => {
                warn!(%error, "failed to clone IPC stream for response");
                stop_pending_action(state);
                return Ok(PostAction::Remove);
            }
        };
        state.borrow_mut().writer_active = true;
        let writer_state = state.clone();
        if let Err(error) = handle.insert_source(
            Generic::new(writer, Interest::WRITE, Mode::Level),
            move |_, source, _| on_writable(&writer_state, source),
        ) {
            if let Some(signal) = state.borrow_mut().stop_after_flush.take() {
                signal.stop();
            }
            state.borrow_mut().writer_active = false;
            warn!(%error, "failed to register IPC response writer");
            return Ok(PostAction::Remove);
        }
    }

    Ok(if should_remove {
        PostAction::Remove
    } else {
        PostAction::Continue
    })
}

fn stop_pending_action(state: &Rc<RefCell<ClientState>>) {
    if let Some(signal) = state.borrow_mut().stop_after_flush.take() {
        signal.stop();
    }
}

fn on_writable(
    state: &Rc<RefCell<ClientState>>,
    source: &mut smithay::reexports::calloop::generic::NoIoDrop<UnixStream>,
) -> Result<PostAction, io::Error> {
    let stream = source_mut(source);
    let mut state = state.borrow_mut();
    while !state.pending.is_empty() {
        let (front, back) = state.pending.as_slices();
        let bytes = if front.is_empty() { back } else { front };
        match stream.write(bytes) {
            Ok(0) => {
                state.writer_active = false;
                if let Some(signal) = state.stop_after_flush.take() {
                    signal.stop();
                }
                return Ok(PostAction::Remove);
            }
            Ok(written) => {
                state.pending.drain(..written);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                return Ok(PostAction::Continue);
            }
            Err(error) => {
                state.writer_active = false;
                if let Some(signal) = state.stop_after_flush.take() {
                    signal.stop();
                }
                warn!(%error, "closing unwritable IPC client");
                return Ok(PostAction::Remove);
            }
        }
    }
    state.writer_active = false;
    if let Some(signal) = state.stop_after_flush.take() {
        signal.stop();
    }
    Ok(PostAction::Remove)
}

#[allow(unsafe_code)]
fn source_mut<T>(source: &mut NoIoDrop<T>) -> &mut T {
    unsafe { source.get_mut() }
}
