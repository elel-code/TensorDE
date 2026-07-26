use std::{
    fs, io,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
};

use calloop::{LoopHandle, LoopSignal};
use thiserror::Error;

use super::message::{Request, Response};

mod connection;

pub(crate) struct IpcReply {
    pub(crate) response: Response,
    pub(crate) stop_after_flush: Option<LoopSignal>,
}

impl IpcReply {
    pub(crate) fn new(response: Response) -> Self {
        Self {
            response,
            stop_after_flush: None,
        }
    }

    pub(crate) fn stop_after_flush(response: Response, signal: LoopSignal) -> Self {
        Self {
            response,
            stop_after_flush: Some(signal),
        }
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

    pub fn accept(&self) -> Result<Option<UnixStream>, IpcError> {
        match self.listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(true)
                    .map_err(IpcError::Nonblocking)?;
                Ok(Some(stream))
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(IpcError::Accept(error)),
        }
    }

    pub(crate) fn register<T, H>(
        &self,
        handle: &LoopHandle<'static, T>,
        handler: H,
    ) -> Result<(), IpcError>
    where
        T: 'static,
        H: FnMut(Request, &mut T) -> IpcReply + 'static,
    {
        let listener = self.listener.try_clone().map_err(IpcError::CloneListener)?;
        connection::register_listener(handle, listener, handler).map_err(IpcError::Source)
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
    #[error("failed to accept IPC connection: {0}")]
    Accept(io::Error),
    #[error("failed to clone IPC listener: {0}")]
    CloneListener(io::Error),
    #[error("failed to register IPC source: {0}")]
    Source(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{Command, FrameDecoder, Request, Response, ResultBody, encode};
    use calloop::EventLoop;
    use std::io::{Read, Write};
    use std::time::Duration;

    #[test]
    fn socket_identity_is_stable_for_an_owned_path() {
        let path = PathBuf::from(format!("target/tensor-identity-{}", std::process::id()));
        fs::write(&path, b"identity").unwrap();

        let first = SocketIdentity::read(&path).unwrap();
        let second = SocketIdentity::read(&path).unwrap();

        assert_eq!(first, second);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn registered_server_processes_multiple_requests_on_one_connection() {
        let path = PathBuf::from(format!("target/tensor-ipc-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        let server = IpcServer::bind(&path).unwrap();
        let mut event_loop = EventLoop::<()>::try_new().unwrap();
        server
            .register(&event_loop.handle(), |request, _| {
                let result = match request.command {
                    Command::Ping => ResultBody::Pong,
                    _ => ResultBody::Accepted,
                };
                IpcReply::new(Response::new(request.request_id, result))
            })
            .unwrap();

        let mut client = UnixStream::connect(&path).unwrap();
        let mut outgoing = encode(&Request::new(1, Command::Ping)).unwrap();
        outgoing.extend(encode(&Request::new(2, Command::GetState)).unwrap());
        client.write_all(&outgoing).unwrap();
        client.set_nonblocking(true).unwrap();

        let mut decoder = FrameDecoder::new();
        let mut received = Vec::new();
        let mut buffer = [0; 4096];
        for _ in 0..32 {
            event_loop
                .dispatch(Duration::from_millis(2), &mut ())
                .unwrap();
            match client.read(&mut buffer) {
                Ok(read) => received.extend(decoder.push::<Response>(&buffer[..read]).unwrap()),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("IPC client read failed: {error}"),
            }
            if received.len() == 2 {
                break;
            }
        }

        assert_eq!(received.len(), 2);
        assert_eq!(received[0].request_id, 1);
        assert_eq!(received[1].request_id, 2);
        drop(client);
        drop(event_loop);
        drop(server);
        assert!(!path.exists());
    }

    #[test]
    fn shutdown_signal_follows_the_accepted_response() {
        let path = PathBuf::from(format!("target/tensor-ipc-shutdown-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        let server = IpcServer::bind(&path).unwrap();
        let mut event_loop = EventLoop::<()>::try_new().unwrap();
        let stop_signal = event_loop.get_signal();
        server
            .register(&event_loop.handle(), move |request, _| {
                IpcReply::stop_after_flush(
                    Response::new(request.request_id, ResultBody::Accepted),
                    stop_signal.clone(),
                )
            })
            .unwrap();

        let mut client = UnixStream::connect(&path).unwrap();
        client
            .write_all(&encode(&Request::new(3, Command::Quit)).unwrap())
            .unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();

        let fallback_signal = event_loop.get_signal();
        let dispatches = std::rc::Rc::new(std::cell::Cell::new(0));
        let callback_dispatches = dispatches.clone();
        event_loop
            .run(Some(Duration::from_millis(10)), &mut (), move |_| {
                let next = callback_dispatches.get() + 1;
                callback_dispatches.set(next);
                if next == 10 {
                    fallback_signal.stop();
                }
            })
            .unwrap();

        let mut buffer = [0; 4096];
        let read = client.read(&mut buffer).unwrap();
        let responses = FrameDecoder::new()
            .push::<Response>(&buffer[..read])
            .unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, 3);
        assert!(matches!(responses[0].result, ResultBody::Accepted));
        drop(client);
        drop(event_loop);
        drop(server);
        assert!(!path.exists());
    }
}
