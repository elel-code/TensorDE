//! Direct `wayland-client` connection — **protocol I/O only**, no event-loop
//! executor. Callers integrate with Compio (`feature = "compio"`), calloop,
//! poll, or any other readiness source using [`NativeConnection::as_fd`].

use std::fmt;
use std::os::fd::{AsFd, BorrowedFd};

use wayland_client::Connection;
use wayland_client::backend::WaylandError;

/// Errors from the native Wayland connection / protocol path.
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

/// Owned Wayland display connection (no async runtime dependency).
///
/// # Integrating with an external event loop
///
/// 1. Register [`Self::as_fd`] for readability.
/// 2. On readable: [`Self::flush`], then the shell's `try_read_and_dispatch`
///    / `dispatch_pending`.
/// 3. After sending requests: [`Self::flush`] again.
pub struct NativeConnection {
    connection: Connection,
}

impl NativeConnection {
    /// Connect using `WAYLAND_DISPLAY` / `WAYLAND_SOCKET` environment rules.
    pub fn connect_to_env() -> Result<Self, NativeError> {
        let connection = Connection::connect_to_env()
            .map_err(|error| NativeError::Connect(error.to_string()))?;
        Self::from_connection(connection)
    }

    pub fn from_connection(connection: Connection) -> Result<Self, NativeError> {
        Ok(Self { connection })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.connection.as_fd()
    }

    /// Flush outgoing requests to the compositor.
    pub fn flush(&self) -> Result<(), NativeError> {
        self.connection.flush()?;
        Ok(())
    }

    /// Blocking roundtrip (sync helper for init / tests).
    pub fn roundtrip(&self) -> Result<usize, NativeError> {
        Ok(self.connection.roundtrip()?)
    }
}

impl AsFd for NativeConnection {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.connection.as_fd()
    }
}
