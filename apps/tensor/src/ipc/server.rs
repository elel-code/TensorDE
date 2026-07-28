use std::{
    fs, io,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::UnixListener,
    },
    path::{Path, PathBuf},
};

use tensor_runtime::{EventfdWakeError, WorkerTx};
use thiserror::Error;

use super::message::Response;

mod runtime;

pub(crate) use runtime::{
    IpcControlEvent, IpcEvent, IpcRuntime, MAX_PENDING_IPC_CONTROL_EVENTS, MAX_PENDING_IPC_REQUESTS,
};

#[derive(Debug)]
pub(crate) struct IpcReply {
    pub(crate) response: Response,
    stop_after_flush: bool,
}

impl IpcReply {
    pub(crate) fn new(response: Response) -> Self {
        Self {
            response,
            stop_after_flush: false,
        }
    }

    pub(crate) fn stop_after_flush(response: Response) -> Self {
        Self {
            response,
            stop_after_flush: true,
        }
    }

    pub(super) const fn should_stop_after_flush(&self) -> bool {
        self.stop_after_flush
    }
}

pub struct IpcServer {
    listener: UnixListener,
    path: PathBuf,
    identity: SocketIdentity,
}

impl IpcServer {
    pub fn bind(path: impl Into<PathBuf>) -> Result<Self, IpcError> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(IpcError::EmptyPath);
        }
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            fs::create_dir_all(parent).map_err(IpcError::CreateParent)?;
        }

        let listener = UnixListener::bind(&path).map_err(IpcError::Bind)?;
        let mut permissions = fs::metadata(&path)
            .map_err(IpcError::Identity)?
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&path, permissions).map_err(IpcError::Permissions)?;
        listener
            .set_nonblocking(true)
            .map_err(IpcError::Nonblocking)?;
        let identity = SocketIdentity::read(&path).map_err(IpcError::Identity)?;
        Ok(Self {
            listener,
            path,
            identity,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn start(
        &self,
        requests: WorkerTx<IpcEvent>,
        control: WorkerTx<IpcControlEvent>,
    ) -> Result<IpcRuntime, IpcError> {
        let listener = self.listener.try_clone().map_err(IpcError::CloneListener)?;
        IpcRuntime::start(listener, requests, control)
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let Ok(identity) = SocketIdentity::read(&self.path) else {
            return;
        };
        if identity == self.identity {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SocketIdentity {
    device: u64,
    inode: u64,
}

impl SocketIdentity {
    fn read(path: &Path) -> io::Result<Self> {
        let metadata = fs::metadata(path)?;
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("IPC socket path is empty")]
    EmptyPath,
    #[error("failed to create IPC socket parent directory: {0}")]
    CreateParent(io::Error),
    #[error("failed to bind IPC socket: {0}")]
    Bind(io::Error),
    #[error("failed to configure IPC socket: {0}")]
    Nonblocking(io::Error),
    #[error("failed to set IPC socket permissions: {0}")]
    Permissions(io::Error),
    #[error("failed to inspect IPC socket: {0}")]
    Identity(io::Error),
    #[error("failed to clone IPC listener: {0}")]
    CloneListener(io::Error),
    #[error(transparent)]
    StopWake(#[from] EventfdWakeError),
    #[error("failed to spawn IPC completion runtime: {0}")]
    RuntimeThread(io::Error),
    #[error("failed to initialize IPC Compio runtime: {0}")]
    Runtime(io::Error),
    #[error("failed to attach IPC listener to Compio: {0}")]
    AttachListener(io::Error),
    #[error("failed to attach IPC stop eventfd to Compio: {0}")]
    AttachStop(io::Error),
    #[error("IPC completion runtime stopped during initialization")]
    RuntimeStartupDisconnected,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{Command, FrameDecoder, Request, Response, ResultBody, encode};
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
        time::Duration,
    };
    use tensor_runtime::WorkerBridge;

    #[test]
    fn socket_identity_is_stable_for_an_owned_path() {
        let path = std::env::temp_dir().join(format!(
            "tensor-identity-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_file(&path);
        fs::write(&path, b"identity").unwrap();

        let first = SocketIdentity::read(&path).unwrap();
        let second = SocketIdentity::read(&path).unwrap();

        assert_eq!(first, second);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn completion_runtime_processes_multiple_requests_on_one_connection() {
        let path =
            std::env::temp_dir().join(format!("tensor-ipc-{}-{}", std::process::id(), line!()));
        let _ = fs::remove_file(&path);
        let server = IpcServer::bind(&path).unwrap();
        let (requests, received_requests) = WorkerBridge::bounded(MAX_PENDING_IPC_REQUESTS);
        let (control, _) = WorkerBridge::bounded(MAX_PENDING_IPC_CONTROL_EVENTS);
        let runtime = server.start(requests, control).unwrap();

        let mut client = UnixStream::connect(&path).unwrap();
        let mut outgoing = encode(&Request::new(1, Command::Ping)).unwrap();
        outgoing.extend(encode(&Request::new(2, Command::GetState)).unwrap());
        client.write_all(&outgoing).unwrap();

        for expected_id in [1, 2] {
            let event = received_requests
                .recv_timeout(Duration::from_secs(1))
                .expect("Compio IPC completion");
            let IpcEvent {
                request,
                respond_to,
            } = event;
            assert_eq!(request.request_id, expected_id);
            let result = match request.command {
                Command::Ping => ResultBody::Pong,
                _ => ResultBody::Accepted,
            };
            respond_to
                .send(IpcReply::new(Response::new(request.request_id, result)))
                .expect("client waits for response");
        }

        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut decoder = FrameDecoder::new();
        let mut received = Vec::new();
        let mut buffer = [0; 4096];
        while received.len() < 2 {
            let read = client.read(&mut buffer).expect("IPC response completion");
            received.extend(decoder.push::<Response>(&buffer[..read]).unwrap());
            if received.len() == 2 {
                break;
            }
        }

        assert_eq!(received.len(), 2);
        assert_eq!(received[0].request_id, 1);
        assert_eq!(received[1].request_id, 2);
        drop(client);
        drop(runtime);
        drop(server);
        assert!(!path.exists());
    }

    #[test]
    fn shutdown_signal_follows_the_accepted_response() {
        let path = PathBuf::from(format!("target/tensor-ipc-shutdown-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        let server = IpcServer::bind(&path).unwrap();
        let (requests, received_requests) = WorkerBridge::bounded(MAX_PENDING_IPC_REQUESTS);
        let (control, received_control) = WorkerBridge::bounded(MAX_PENDING_IPC_CONTROL_EVENTS);
        let runtime = server.start(requests, control).unwrap();

        let mut client = UnixStream::connect(&path).unwrap();
        client
            .write_all(&encode(&Request::new(3, Command::Quit)).unwrap())
            .unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();

        let event = received_requests
            .recv_timeout(Duration::from_secs(1))
            .expect("Compio IPC request completion");
        let IpcEvent {
            request,
            respond_to,
        } = event;
        respond_to
            .send(IpcReply::stop_after_flush(Response::new(
                request.request_id,
                ResultBody::Accepted,
            )))
            .expect("client waits for response");

        let mut buffer = [0; 4096];
        let read = client.read(&mut buffer).unwrap();
        let responses = FrameDecoder::new()
            .push::<Response>(&buffer[..read])
            .unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, 3);
        assert!(matches!(responses[0].result, ResultBody::Accepted));
        assert!(matches!(
            received_control.recv_timeout(Duration::from_secs(1)),
            Ok(IpcControlEvent::ShutdownFlushed)
        ));
        drop(client);
        drop(runtime);
        drop(server);
        assert!(!path.exists());
    }
}
