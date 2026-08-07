use std::{
    collections::VecDeque,
    env,
    future::Future,
    io,
    path::{Path, PathBuf},
    pin::Pin,
};

use compio::{
    io::{AsyncRead, AsyncWriteExt},
    net::UnixStream,
};

use super::{
    CodecError, Command, EventMessage, FrameDecoder, IPC_PROTOCOL_VERSION, Request, Response,
    ResultBody, ServerMessage, encode,
};

const READ_CHUNK_SIZE: usize = 16 * 1024;
const MAX_PENDING_MESSAGES: usize = 256;

type WriteFuture = Pin<Box<dyn Future<Output = io::Result<()>>>>;
type ReadFuture = Pin<Box<dyn Future<Output = (io::Result<usize>, Vec<u8>)>>>;

/// Caller-driven Compio client for Tensorland's bounded request protocol.
///
/// No runtime or task is created internally. Active I/O futures remain owned by
/// the client when an outer call future is dropped, so the next call finishes
/// the same operation instead of losing a partially completed stream frame.
pub struct CompioClient {
    stream: UnixStream,
    next_request_id: u64,
    active_request: Option<u64>,
    abandoned: VecDeque<u64>,
    write: Option<WriteFuture>,
    read: Option<ReadFuture>,
    decoder: FrameDecoder,
    decoded: Vec<ServerMessage>,
    pending: VecDeque<ServerMessage>,
    events: VecDeque<EventMessage>,
    failed: bool,
}

impl CompioClient {
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let path = path.as_ref();
        let stream = UnixStream::connect(path)
            .await
            .map_err(|source| ClientError::Connect {
                path: path.to_owned(),
                source,
            })?;
        Ok(Self::from_stream(stream))
    }

    pub async fn connect_default() -> Result<Self, ClientError> {
        Self::connect(default_socket_path()).await
    }

    pub fn from_stream(stream: UnixStream) -> Self {
        Self {
            stream,
            next_request_id: 1,
            active_request: None,
            abandoned: VecDeque::with_capacity(4),
            write: None,
            read: None,
            decoder: FrameDecoder::new(),
            decoded: Vec::with_capacity(4),
            pending: VecDeque::with_capacity(8),
            events: VecDeque::with_capacity(8),
            failed: false,
        }
    }

    pub fn is_usable(&self) -> bool {
        !self.failed
    }

    pub async fn call(&mut self, command: Command) -> Result<ResultBody, ClientError> {
        self.ensure_usable()?;
        self.recover_cancelled_call().await?;
        let request_id = self.take_request_id()?;
        self.active_request = Some(request_id);
        let result = self.call_active(request_id, command).await;
        self.active_request = None;
        result
    }

    async fn call_active(
        &mut self,
        request_id: u64,
        command: Command,
    ) -> Result<ResultBody, ClientError> {
        self.write_request(Request::new(request_id, command))
            .await?;
        loop {
            match self.next_message().await? {
                ServerMessage::Event(_) => {
                    unreachable!("next_message routes events before returning")
                }
                ServerMessage::Response(response) => {
                    if self.take_abandoned(response.request_id) {
                        continue;
                    }
                    let result = validate_response(response, request_id);
                    if matches!(
                        result,
                        Err(ClientError::ResponseVersion(_) | ClientError::RequestId { .. })
                    ) {
                        self.failed = true;
                    }
                    return result;
                }
            }
        }
    }

    async fn recover_cancelled_call(&mut self) -> Result<(), ClientError> {
        let Some(request_id) = self.active_request.take() else {
            return Ok(());
        };
        self.finish_write().await?;
        if self.abandoned.len() == MAX_PENDING_MESSAGES {
            return Err(self.fail(ClientError::AbandonedRequestsFull {
                limit: MAX_PENDING_MESSAGES,
            }));
        }
        self.abandoned.push_back(request_id);
        Ok(())
    }

    pub fn drain_events(&mut self) -> impl Iterator<Item = EventMessage> + '_ {
        self.events.drain(..)
    }

    async fn write_request(&mut self, request: Request) -> Result<(), ClientError> {
        self.finish_write().await?;
        let frame = encode(&request)?;
        let mut stream = self.stream.clone();
        self.write = Some(Box::pin(async move {
            let compio::BufResult(result, _) = stream.write_all(frame).await;
            result
        }));
        self.finish_write().await
    }

    async fn finish_write(&mut self) -> Result<(), ClientError> {
        let Some(write) = &mut self.write else {
            return Ok(());
        };
        let result = write.await;
        self.write = None;
        result.map_err(|source| self.fail(ClientError::Io(source)))
    }

    async fn next_message(&mut self) -> Result<ServerMessage, ClientError> {
        loop {
            if let Some(message) = self.pending.pop_front() {
                return Ok(message);
            }
            if self.read.is_none() {
                let mut stream = self.stream.clone();
                self.read = Some(Box::pin(async move {
                    let compio::BufResult(result, buffer) =
                        stream.read(Vec::with_capacity(READ_CHUNK_SIZE)).await;
                    (result, buffer)
                }));
            }
            let (result, buffer) = self.read.as_mut().unwrap().await;
            self.read = None;
            let read = result.map_err(|source| self.fail(ClientError::Io(source)))?;
            if read == 0 {
                return Err(self.fail(ClientError::UnexpectedEof));
            }
            self.decoder
                .push_into::<ServerMessage>(&buffer[..read], &mut self.decoded)
                .map_err(|error| self.fail(ClientError::Codec(error)))?;
            if self
                .pending
                .len()
                .saturating_add(self.events.len())
                .saturating_add(self.decoded.len())
                > MAX_PENDING_MESSAGES
            {
                return Err(self.fail(ClientError::PendingMessagesFull {
                    limit: MAX_PENDING_MESSAGES,
                }));
            }
            let mut decoded = std::mem::take(&mut self.decoded);
            for message in decoded.drain(..) {
                match message {
                    ServerMessage::Event(event) => self.queue_event(event)?,
                    response @ ServerMessage::Response(_) => self.pending.push_back(response),
                }
            }
            self.decoded = decoded;
        }
    }

    fn queue_event(&mut self, event: EventMessage) -> Result<(), ClientError> {
        if event.version != IPC_PROTOCOL_VERSION {
            return Err(self.fail(ClientError::EventVersion(event.version)));
        }
        if self.events.len() == MAX_PENDING_MESSAGES {
            return Err(self.fail(ClientError::PendingMessagesFull {
                limit: MAX_PENDING_MESSAGES,
            }));
        }
        self.events.push_back(event);
        Ok(())
    }

    fn take_request_id(&mut self) -> Result<u64, ClientError> {
        let request_id = self.next_request_id;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(|| self.fail(ClientError::RequestIdExhausted))?;
        Ok(request_id)
    }

    fn take_abandoned(&mut self, request_id: u64) -> bool {
        let Some(index) = self
            .abandoned
            .iter()
            .position(|candidate| *candidate == request_id)
        else {
            return false;
        };
        self.abandoned.remove(index);
        true
    }

    fn ensure_usable(&self) -> Result<(), ClientError> {
        if self.failed {
            Err(ClientError::ConnectionUnusable)
        } else {
            Ok(())
        }
    }

    fn fail(&mut self, error: ClientError) -> ClientError {
        self.failed = true;
        error
    }
}

pub fn default_socket_path() -> PathBuf {
    env::var_os("TENSOR_IPC_SOCKET")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("XDG_RUNTIME_DIR")
                .map(|directory| PathBuf::from(directory).join("tensor.sock"))
        })
        .unwrap_or_else(|| PathBuf::from("/tmp/tensor.sock"))
}

fn validate_response(response: Response, request_id: u64) -> Result<ResultBody, ClientError> {
    if response.version != IPC_PROTOCOL_VERSION {
        return Err(ClientError::ResponseVersion(response.version));
    }
    if response.request_id != request_id {
        return Err(ClientError::RequestId {
            expected: request_id,
            actual: response.request_id,
        });
    }
    match response.result {
        ResultBody::Error(error) => Err(ClientError::Server {
            code: error.code,
            message: error.message,
        }),
        result => Ok(result),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("failed to connect to Tensor IPC socket {path}: {source}")]
    Connect { path: PathBuf, source: io::Error },
    #[error("Tensor IPC I/O failed: {0}")]
    Io(io::Error),
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error("Tensor IPC closed before returning a response")]
    UnexpectedEof,
    #[error("Tensor IPC connection is unusable after a terminal error")]
    ConnectionUnusable,
    #[error("Tensor IPC request ID space is exhausted")]
    RequestIdExhausted,
    #[error("Tensor IPC abandoned-request registry reached its {limit}-request limit")]
    AbandonedRequestsFull { limit: usize },
    #[error("Tensor returned protocol version {0}; expected {IPC_PROTOCOL_VERSION}")]
    ResponseVersion(u16),
    #[error("Tensor event used protocol version {0}; expected {IPC_PROTOCOL_VERSION}")]
    EventVersion(u16),
    #[error("Tensor returned request ID {actual}; expected {expected}")]
    RequestId { expected: u64, actual: u64 },
    #[error("Tensor IPC pending-message queue reached its {limit}-message limit")]
    PendingMessagesFull { limit: usize },
    #[error("Tensor IPC error {code}: {message}")]
    Server { code: String, message: String },
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        os::unix::net::UnixListener,
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };

    use super::*;

    fn socket_path() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "tensor-ipc-client-{}-{}.sock",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn compio_client_reuses_one_connection_and_checks_responses() {
        let path = socket_path();
        let _ = fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut decoder = FrameDecoder::new();
            let mut bytes = [0_u8; 4096];
            for _ in 0..2 {
                let request = loop {
                    let read = stream.read(&mut bytes).unwrap();
                    let mut requests = decoder.push::<Request>(&bytes[..read]).unwrap();
                    if let Some(request) = requests.pop() {
                        break request;
                    }
                };
                let result = match request.command {
                    Command::Ping => ResultBody::Pong,
                    Command::Spawn { .. } => ResultBody::Accepted,
                    _ => unreachable!(),
                };
                stream
                    .write_all(
                        &encode(&ServerMessage::Response(Response::new(
                            request.request_id,
                            result,
                        )))
                        .unwrap(),
                    )
                    .unwrap();
            }
        });

        tensor_runtime::io_uring_runtime(8)
            .unwrap()
            .block_on(async {
                let mut client = CompioClient::connect(&path).await.unwrap();
                assert!(matches!(
                    client.call(Command::Ping).await.unwrap(),
                    ResultBody::Pong
                ));
                assert!(matches!(
                    client
                        .call(Command::Spawn {
                            argv: vec!["foot".into()],
                            cwd: None,
                        })
                        .await
                        .unwrap(),
                    ResultBody::Accepted
                ));
                assert!(client.is_usable());
            });
        server.join().unwrap();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn cancelled_call_response_is_discarded_before_the_next_reply() {
        let path = socket_path();
        let _ = fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut decoder = FrameDecoder::new();
            let mut bytes = [0_u8; 4096];
            let requests = (0..2)
                .map(|_| {
                    loop {
                        let read = stream.read(&mut bytes).unwrap();
                        let mut requests = decoder.push::<Request>(&bytes[..read]).unwrap();
                        if let Some(request) = requests.pop() {
                            break request;
                        }
                    }
                })
                .collect::<Vec<_>>();
            let mut replies = encode(&ServerMessage::Response(Response::new(
                requests[0].request_id,
                ResultBody::Pong,
            )))
            .unwrap();
            replies.extend(
                encode(&ServerMessage::Response(Response::new(
                    requests[1].request_id,
                    ResultBody::Accepted,
                )))
                .unwrap(),
            );
            stream.write_all(&replies).unwrap();
        });

        tensor_runtime::io_uring_runtime(8)
            .unwrap()
            .block_on(async {
                let mut client = CompioClient::connect(&path).await.unwrap();
                let cancelled = compio::runtime::time::timeout(
                    Duration::from_millis(10),
                    client.call(Command::Ping),
                )
                .await;
                assert!(cancelled.is_err());
                assert!(matches!(
                    client
                        .call(Command::Spawn {
                            argv: vec!["foot".into()],
                            cwd: None,
                        })
                        .await
                        .unwrap(),
                    ResultBody::Accepted
                ));
                assert!(client.is_usable());
            });
        server.join().unwrap();
        fs::remove_file(path).unwrap();
    }
}
