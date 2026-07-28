//! Direct `wayland-client` connection — **protocol I/O only**, no event-loop
//! executor. Callers integrate with any readiness source using
//! [`NativeConnection::as_fd`] (a plain non-blocking display socket).

use std::cell::Cell;
use std::fmt;
use std::os::fd::{AsFd, BorrowedFd};

use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
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
/// 2. On readable: [`Self::flush_if_needed`] (or [`Self::flush`]), then the
///    shell's `try_read_and_dispatch` / `dispatch_pending`.
/// 3. After batching protocol requests: [`Self::mark_dirty`]; the next pump
///    step flushes. Call [`Self::flush`] only when the compositor must see
///    requests before a subsequent blocking step (e.g. clipboard pipe I/O).
pub struct NativeConnection {
    connection: Connection,
    /// Set when requests were queued and a later [`Self::flush_if_needed`] is due.
    needs_flush: Cell<bool>,
}

impl NativeConnection {
    /// Connect using `WAYLAND_DISPLAY` / `WAYLAND_SOCKET` environment rules.
    pub fn connect_to_env() -> Result<Self, NativeError> {
        let connection = Connection::connect_to_env()
            .map_err(|error| NativeError::Connect(error.to_string()))?;
        Self::from_connection(connection)
    }

    pub fn from_connection(connection: Connection) -> Result<Self, NativeError> {
        // Protocol reads use the classic prepare_read/read path and must not
        // block the thread. Ensure O_NONBLOCK regardless of compositor/lib.
        ensure_nonblocking(connection.as_fd())?;
        Ok(Self {
            connection,
            needs_flush: Cell::new(false),
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.connection.as_fd()
    }

    /// Record that protocol requests were queued and need a later flush.
    ///
    /// Prefer this from shell API methods; the display pump coalesces many
    /// marks into a single write via [`Self::flush_if_needed`].
    #[inline]
    pub fn mark_dirty(&self) {
        self.needs_flush.set(true);
    }

    /// Whether a flush is pending.
    #[inline]
    pub fn needs_flush(&self) -> bool {
        self.needs_flush.get()
    }

    /// Flush outgoing requests only if [`Self::mark_dirty`] was called.
    pub fn flush_if_needed(&self) -> Result<(), NativeError> {
        if self.needs_flush.get() {
            self.flush()
        } else {
            Ok(())
        }
    }

    /// Always flush outgoing requests to the compositor and clear the dirty flag.
    pub fn flush(&self) -> Result<(), NativeError> {
        self.connection.flush()?;
        self.needs_flush.set(false);
        Ok(())
    }

    /// Blocking roundtrip (sync helper for init / tests).
    pub fn roundtrip(&self) -> Result<usize, NativeError> {
        // Roundtrip always writes; clear dirty regardless of prior mark.
        let n = self.connection.roundtrip()?;
        self.needs_flush.set(false);
        Ok(n)
    }
}

impl AsFd for NativeConnection {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.connection.as_fd()
    }
}

fn ensure_nonblocking(fd: BorrowedFd<'_>) -> Result<(), NativeError> {
    let flags = fcntl_getfl(fd).map_err(io_from_errno)?;
    if flags.contains(OFlags::NONBLOCK) {
        return Ok(());
    }
    fcntl_setfl(fd, flags | OFlags::NONBLOCK).map_err(io_from_errno)?;
    Ok(())
}

fn io_from_errno(err: rustix::io::Errno) -> NativeError {
    NativeError::from(std::io::Error::from(err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_dirty_coalesces_until_flush() {
        let Ok(conn) = NativeConnection::connect_to_env() else {
            return;
        };
        assert!(!conn.needs_flush());
        conn.mark_dirty();
        conn.mark_dirty();
        assert!(conn.needs_flush());
        conn.flush_if_needed().expect("flush dirty");
        assert!(!conn.needs_flush());
        // Clean flush is a no-op.
        conn.flush_if_needed().expect("noop");
        assert!(!conn.needs_flush());
        // Explicit flush always clears.
        conn.mark_dirty();
        conn.flush().expect("force flush");
        assert!(!conn.needs_flush());
    }
}
