//! Eventfd-based wake for the native Compio event loop.
//!
//! Writes are safe from any thread. Readiness is observed on the event-loop
//! thread through a cloned fd registered with Compio's io_uring proactor.

use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};

use rustix::event::{eventfd, EventfdFlags};
use rustix::io::{read, write, Errno};

/// Cross-thread wake via Linux `eventfd`.
#[derive(Debug)]
pub struct EventFdWake {
    fd: OwnedFd,
    closed: AtomicBool,
}

impl EventFdWake {
    pub fn new() -> io::Result<Self> {
        let fd = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)
            .map_err(io::Error::from)?;
        Ok(Self {
            fd,
            closed: AtomicBool::new(false),
        })
    }

    pub fn wake(&self) {
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        // Ignore EAGAIN (counter already non-zero) and other races on shutdown.
        let _ = write(self.fd.as_fd(), &1u64.to_ne_bytes());
    }

    /// Drain the eventfd counter after Compio reports the wake fd readable.
    pub fn drain(&self) {
        let mut buf = [0u8; 8];
        loop {
            match read(self.fd.as_fd(), &mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(Errno::INTR) => continue,
                // AGAIN / other: drained, would-block, or shutdown race.
                Err(_) => break,
            }
        }
    }
}

impl AsFd for EventFdWake {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl AsRawFd for EventFdWake {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
