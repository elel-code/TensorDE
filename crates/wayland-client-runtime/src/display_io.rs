//! Compio adapter: wait until a **non-blocking** fd is readable/writable.
//!
//! # What this is (and is not)
//!
//! - **Protocol I/O** uses ordinary Wayland fds (`NativeShell::display_fd`).
//!   Reads go through `wayland-client` (`prepare_read` / `read`), never through
//!   this type.
//! - **This module** only answers: “when may we try that read?” under Compio’s
//!   completion model (io_uring). Internally that is `compio::runtime::fd::PollFd`
//!   submitting a one-shot readiness op — **not** a blocking `poll(2)` loop.
//!
//! External runtimes never need this: register `display_fd()` yourself, then
//! call `try_read_and_dispatch`.
//!
//! Only built with `feature = "compio"`.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::task::{Context, Poll};

use compio::runtime::fd::PollFd;

/// Long-lived Compio readiness watch on a cloned non-blocking fd.
///
/// Construct once per fd (display socket, wake eventfd, …) and reuse across
/// wait cycles. Cloning the source fd is intentional: the original stays with
/// the protocol stack for `read`/`write`; this clone is only registered with
/// the proactor for readiness completions.
#[derive(Debug)]
pub struct CompioFdReady {
    poll: PollFd<OwnedFd>,
}

impl CompioFdReady {
    /// Watch `source` for readiness. The fd should already be **non-blocking**.
    pub fn watch(source: impl AsFd) -> io::Result<Self> {
        let owned = source.as_fd().try_clone_to_owned()?;
        Ok(Self {
            poll: PollFd::new(owned)?,
        })
    }

    /// Wait until the fd is readable (data, hangup, or eventfd counter).
    ///
    /// Must run on a Compio executor.
    pub async fn wait_readable(&self) -> io::Result<()> {
        self.poll.read_ready().await
    }

    /// Poll-style API for racing several readiness futures (display vs wake).
    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll.poll_read_ready(cx)
    }

    pub async fn wait_writable(&self) -> io::Result<()> {
        self.poll.write_ready().await
    }
}

/// Historical name used by Fika’s runtime facade.
pub type DisplayReadiness = CompioFdReady;

impl CompioFdReady {
    /// Alias for [`Self::watch`] (older call sites).
    pub fn from_as_fd(source: impl AsFd) -> io::Result<Self> {
        Self::watch(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn readiness_wakes_when_peer_writes() {
        let (mut writer, reader) = UnixStream::pair().expect("socketpair");
        writer.set_nonblocking(true).unwrap();
        reader.set_nonblocking(true).unwrap();

        let readiness = CompioFdReady::watch(&reader).expect("watch");

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            let _ = writer.write_all(b"w");
        });

        compio::runtime::Runtime::new()
            .expect("compio runtime")
            .block_on(async move {
                readiness.wait_readable().await.expect("readable");
            });
    }

    #[test]
    fn watch_accepts_valid_stream() {
        let (a, _b) = UnixStream::pair().unwrap();
        a.set_nonblocking(true).unwrap();
        CompioFdReady::watch(&a).expect("valid fd");
    }
}
