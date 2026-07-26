//! Compio-backed readiness for the Wayland display file descriptor.
//!
//! Phase 1 of the SCTK → native migration: wait for display readability on the
//! Compio executor without calloop. Protocol dispatch still uses the existing
//! runtime path until Phase 2 replaces SCTK handlers.
//!
//! The readiness object holds a **duplicated** fd so it does not take ownership
//! of the live [`wayland_client::Connection`].

use std::io;
use std::os::fd::{AsFd, OwnedFd};

use compio::runtime::fd::PollFd;

/// Compio poll handle for `wl_display` readability (and future write flush).
#[derive(Debug)]
pub struct DisplayReadiness {
    poll: PollFd<OwnedFd>,
}

impl DisplayReadiness {
    /// Duplicate `source`'s fd and attach it to Compio's readiness poller.
    pub fn from_as_fd(source: impl AsFd) -> io::Result<Self> {
        let owned = source.as_fd().try_clone_to_owned()?;
        Ok(Self {
            poll: PollFd::new(owned)?,
        })
    }

    /// Wait until the display fd is readable (messages available or hangup).
    ///
    /// Must be called from a Compio runtime context (`block_on` / task).
    pub async fn wait_readable(&self) -> io::Result<()> {
        self.poll.read_ready().await
    }

    /// Wait until the display fd is writable (for future non-blocking flush).
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
