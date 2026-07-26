//! Compio (io_uring completion) readiness for Wayland-related file descriptors.
//!
//! [`DisplayReadiness`] wraps [`compio::runtime::fd::PollFd`] so waits complete
//! through the Compio proactor rather than a blocking `poll(2)` loop.

use std::io;
use std::os::fd::AsFd;
use std::task::{Context, Poll};

use compio::runtime::fd::PollFd;

/// Compio readiness handle for a cloned fd (display socket or eventfd).
#[derive(Debug)]
pub struct DisplayReadiness {
    poll: PollFd<std::os::fd::OwnedFd>,
}

impl DisplayReadiness {
    /// Duplicate `source`'s fd and attach it to Compio's readiness proactor.
    pub fn from_as_fd(source: impl AsFd) -> io::Result<Self> {
        let owned = source.as_fd().try_clone_to_owned()?;
        Ok(Self {
            poll: PollFd::new(owned)?,
        })
    }

    /// Wait until the fd is readable (messages available, hangup, or eventfd).
    ///
    /// Must be called from a Compio runtime context (`block_on` / task).
    pub async fn wait_readable(&self) -> io::Result<()> {
        self.poll.read_ready().await
    }

    /// Non-async poll for racing multiple readiness futures.
    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll.poll_read_ready(cx)
    }

    /// Wait until the fd is writable (for future non-blocking flush).
    pub async fn wait_writable(&self) -> io::Result<()> {
        self.poll.write_ready().await
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
    fn display_readiness_wakes_when_peer_writes() {
        let (mut writer, reader) = UnixStream::pair().expect("socketpair");
        writer.set_nonblocking(true).unwrap();
        reader.set_nonblocking(true).unwrap();

        let readiness = DisplayReadiness::from_as_fd(&reader).expect("poll fd");

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
    fn from_as_fd_accepts_valid_stream() {
        let (a, _b) = UnixStream::pair().unwrap();
        DisplayReadiness::from_as_fd(&a).expect("valid fd");
    }
}
