use std::{
    collections::VecDeque,
    future::Future,
    io,
    path::{Path, PathBuf},
    pin::Pin,
};

use compio::{
    io::{AsyncRead, AsyncWriteExt},
    net::UnixStream,
};

use crate::{FrameDecoder, GreetdProtocolError, Request, Response, SensitiveFrame, encode_request};

const READ_CHUNK_SIZE: usize = 16 * 1024;
const MAX_PENDING_RESPONSES: usize = 8;

type WriteFuture = Pin<Box<dyn Future<Output = io::Result<()>>>>;
type ReadFuture = Pin<Box<dyn Future<Output = (io::Result<usize>, Vec<u8>)>>>;

/// Caller-driven Compio transport for one greetd authentication connection.
pub struct GreetdClient {
    stream: UnixStream,
    write: Option<WriteFuture>,
    read: Option<ReadFuture>,
    decoder: FrameDecoder,
    decoded: Vec<Response>,
    pending: VecDeque<Response>,
    exchange_active: bool,
    failed: bool,
}

impl GreetdClient {
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self, GreetdClientError> {
        let path = path.as_ref();
        let stream =
            UnixStream::connect(path)
                .await
                .map_err(|source| GreetdClientError::Connect {
                    path: path.to_owned(),
                    source,
                })?;
        Ok(Self::from_stream(stream))
    }

    pub fn from_stream(stream: UnixStream) -> Self {
        Self {
            stream,
            write: None,
            read: None,
            decoder: FrameDecoder::new(),
            decoded: Vec::with_capacity(2),
            pending: VecDeque::with_capacity(2),
            exchange_active: false,
            failed: false,
        }
    }

    pub fn is_usable(&self) -> bool {
        !self.failed && !self.exchange_active
    }

    pub async fn create_session(&mut self, username: &str) -> Result<Response, GreetdClientError> {
        self.exchange(Request::CreateSession { username }).await
    }

    pub async fn post_auth_message_response(
        &mut self,
        response: Option<&str>,
    ) -> Result<Response, GreetdClientError> {
        self.exchange(Request::PostAuthMessageResponse { response })
            .await
    }

    pub async fn start_session(
        &mut self,
        command: &[String],
        environment: &[String],
    ) -> Result<Response, GreetdClientError> {
        self.exchange(Request::StartSession {
            cmd: command,
            env: environment,
        })
        .await
    }

    pub async fn cancel_session(&mut self) -> Result<Response, GreetdClientError> {
        self.exchange(Request::CancelSession).await
    }

    async fn exchange(&mut self, request: Request<'_>) -> Result<Response, GreetdClientError> {
        self.ensure_usable()?;
        let frame = encode_request(&request)?;
        self.exchange_active = true;
        let result = self.exchange_frame(frame).await;
        self.exchange_active = false;
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    async fn exchange_frame(
        &mut self,
        frame: SensitiveFrame,
    ) -> Result<Response, GreetdClientError> {
        self.finish_write().await?;
        let mut stream = self.stream.clone();
        self.write = Some(Box::pin(async move {
            let compio::BufResult(result, frame) = stream.write_all(frame).await;
            drop(frame);
            result
        }));
        self.finish_write().await?;
        self.next_response().await
    }

    async fn finish_write(&mut self) -> Result<(), GreetdClientError> {
        let Some(write) = &mut self.write else {
            return Ok(());
        };
        let result = write.await;
        self.write = None;
        result.map_err(GreetdClientError::Io)
    }

    async fn next_response(&mut self) -> Result<Response, GreetdClientError> {
        loop {
            if let Some(response) = self.pending.pop_front() {
                return Ok(response);
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
            let read = result.map_err(GreetdClientError::Io)?;
            if read == 0 {
                return Err(GreetdClientError::UnexpectedEof);
            }
            self.decoder
                .push::<Response>(&buffer[..read])
                .map(|decoded| self.decoded = decoded)?;
            if self.pending.len().saturating_add(self.decoded.len()) > MAX_PENDING_RESPONSES {
                return Err(GreetdClientError::PendingResponsesFull {
                    limit: MAX_PENDING_RESPONSES,
                });
            }
            self.pending.extend(self.decoded.drain(..));
        }
    }

    fn ensure_usable(&mut self) -> Result<(), GreetdClientError> {
        if self.exchange_active {
            self.failed = true;
            Err(GreetdClientError::InterruptedExchange)
        } else if self.failed {
            Err(GreetdClientError::ConnectionUnusable)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GreetdClientError {
    #[error("failed to connect to greetd socket {path}: {source}")]
    Connect { path: PathBuf, source: io::Error },
    #[error("greetd I/O failed: {0}")]
    Io(io::Error),
    #[error(transparent)]
    Protocol(#[from] GreetdProtocolError),
    #[error("greetd closed before returning a response")]
    UnexpectedEof,
    #[error("greetd response queue reached its {limit}-response limit")]
    PendingResponsesFull { limit: usize },
    #[error("a cancelled greetd exchange left protocol state indeterminate")]
    InterruptedExchange,
    #[error("greetd connection is unusable after a terminal error")]
    ConnectionUnusable,
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        os::unix::net::UnixListener,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    fn socket_path() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "tensor-greeter-client-{}-{}.sock",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn write_response(stream: &mut impl Write, response: &[u8]) {
        stream
            .write_all(&(response.len() as u32).to_le_bytes())
            .unwrap();
        stream.write_all(response).unwrap();
    }

    #[test]
    fn compio_transport_completes_a_greetd_authentication_sequence() {
        let path = socket_path();
        let _ = fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut decoder = FrameDecoder::new();
            let mut bytes = [0_u8; 4096];
            for response in [
                br#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password:"}"#.as_slice(),
                br#"{"type":"success"}"#.as_slice(),
                br#"{"type":"success"}"#.as_slice(),
            ] {
                loop {
                    let read = stream.read(&mut bytes).unwrap();
                    if !decoder
                        .push::<serde_json::Value>(&bytes[..read])
                        .unwrap()
                        .is_empty()
                    {
                        break;
                    }
                }
                write_response(&mut stream, response);
            }
        });

        tensor_runtime::io_uring_runtime(8)
            .unwrap()
            .block_on(async {
                let mut client = GreetdClient::connect(&path).await.unwrap();
                assert!(matches!(
                    client.create_session("tensor").await.unwrap(),
                    Response::AuthMessage {
                        auth_message_type: crate::AuthMessageType::Secret,
                        ..
                    }
                ));
                assert_eq!(
                    client
                        .post_auth_message_response(Some("test-password"))
                        .await
                        .unwrap(),
                    Response::Success
                );
                assert_eq!(
                    client
                        .start_session(&["tensor-session".into()], &[])
                        .await
                        .unwrap(),
                    Response::Success
                );
                assert!(client.is_usable());
            });
        server.join().unwrap();
        fs::remove_file(path).unwrap();
    }
}
