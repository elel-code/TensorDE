use std::{
    fs, io,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
};

use thiserror::Error;

#[allow(dead_code)]
pub struct IpcServer {
    listener: UnixListener,
    path: PathBuf,
    identity: SocketIdentity,
}

#[allow(dead_code)]
impl IpcServer {
    pub fn bind(path: impl Into<PathBuf>) -> Result<Self, IpcError> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(IpcError::EmptyPath);
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent).map_err(IpcError::CreateParent)?;
            }
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

#[allow(dead_code)]
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_identity_is_stable_for_an_owned_path() {
        let path = PathBuf::from(format!("target/tensor-identity-{}", std::process::id()));
        fs::write(&path, b"identity").unwrap();

        let first = SocketIdentity::read(&path).unwrap();
        let second = SocketIdentity::read(&path).unwrap();

        assert_eq!(first, second);
        fs::remove_file(path).unwrap();
    }
}
