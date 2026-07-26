//! Direct `wayland-client` connection owned by the native backend.

use std::fmt;
use std::os::fd::AsFd;

use wayland_client::Connection;
use wayland_client::backend::WaylandError;

use crate::display_io::DisplayReadiness;

/// Errors from the native Wayland connection path.
#[derive(Debug)]
pub enum NativeError {
    Connect(String),
    Registry(String),
    Io(String),
    Protocol(String),
}

impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(msg) => write!(f, "native wayland connect: {msg}"),
            Self::Registry(msg) => write!(f, "native wayland registry: {msg}"),
            Self::Io(msg) => write!(f, "native wayland I/O: {msg}"),
            Self::Protocol(msg) => write!(f, "native wayland protocol: {msg}"),
        }
    }
}

impl std::error::Error for NativeError {}

impl From<WaylandError> for NativeError {
    fn from(error: WaylandError) -> Self {
        Self::Protocol(error.to_string())
    }
}

impl From<std::io::Error> for NativeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<wayland_client::DispatchError> for NativeError {
    fn from(error: wayland_client::DispatchError) -> Self {
        Self::Protocol(error.to_string())
    }
}

/// Native connection + Compio display readiness (io_uring completion).
///
/// Readiness is awaited on Compio; protocol reads use
/// `Connection::prepare_read` / `read`.
pub struct NativeConnection {
    connection: Connection,
    readiness: DisplayReadiness,
}

impl NativeConnection {
    /// Connect using `WAYLAND_DISPLAY` / `WAYLAND_SOCKET` environment rules.
    pub fn connect_to_env() -> Result<Self, NativeError> {
        let connection = Connection::connect_to_env()
            .map_err(|error| NativeError::Connect(error.to_string()))?;
        Self::from_connection(connection)
    }

    pub fn from_connection(connection: Connection) -> Result<Self, NativeError> {
        let readiness = DisplayReadiness::from_as_fd(&connection)?;
        Ok(Self {
            connection,
            readiness,
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.connection.as_fd()
    }

    /// Flush outgoing requests to the compositor.
    pub fn flush(&self) -> Result<(), NativeError> {
        self.connection.flush()?;
        Ok(())
    }

    /// Await display readability on the Compio executor.
    pub async fn wait_readable(&self) -> Result<(), NativeError> {
        self.readiness.wait_readable().await?;
        Ok(())
    }

    /// Blocking roundtrip (sync helper for init / tests).
    pub fn roundtrip(&self) -> Result<usize, NativeError> {
        Ok(self.connection.roundtrip()?)
    }
}
