//! Eventfd-based wake for the native Compio event loop.
//!
//! Writes are safe from any thread. Readiness is observed on the event-loop
//! thread through a cloned fd registered with Compio's io_uring proactor.

use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};

use rustix::event::{EventfdFlags, eventfd};
use rustix::io::{Errno, read, write};

/// Cross-thread wake via Linux `eventfd`.
#[derive(Debug)]
pub struct EventFdWake {
    fd: OwnedFd,
    closed: AtomicBool,
    pending: AtomicBool,
}

impl EventFdWake {
    pub fn new() -> io::Result<Self> {
        let fd =
            eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK).map_err(io::Error::from)?;
        Ok(Self {
            fd,
            closed: AtomicBool::new(false),
            pending: AtomicBool::new(false),
        })
    }

    pub fn wake(&self) {
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        if self.pending.swap(true, Ordering::AcqRel) {
            return;
        }
        loop {
            match write(self.fd.as_fd(), &1u64.to_ne_bytes()) {
                Ok(_) | Err(Errno::AGAIN) => break,
                Err(Errno::INTR) => continue,
                Err(_) => {
                    self.pending.store(false, Ordering::Release);
                    break;
                }
            }
        }
    }

    /// Drain the eventfd counter after Compio reports the wake fd readable.
    pub fn drain(&self) {
        let mut buf = [0u8; 8];
        loop {
            // Clear before draining so a racing producer writes a new event.
            // If that happens before the fd is empty, loop once more and
            // consume it; if it happens after the final load, readiness stays.
            self.pending.store(false, Ordering::Release);
            loop {
                match read(self.fd.as_fd(), &mut buf) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(Errno::INTR) => continue,
                    // AGAIN / other: drained, would-block, or shutdown race.
                    Err(_) => break,
                }
            }
            if !self.pending.load(Ordering::Acquire) {
                break;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_wakes_share_one_eventfd_notification() {
        let wake = EventFdWake::new().unwrap();
        wake.wake();
        wake.wake();
        wake.wake();

        let mut bytes = [0u8; 8];
        assert_eq!(read(wake.fd.as_fd(), &mut bytes).unwrap(), 8);
        assert_eq!(u64::from_ne_bytes(bytes), 1);
    }

    #[test]
    fn wake_can_signal_again_after_drain() {
        let wake = EventFdWake::new().unwrap();
        wake.wake();
        wake.drain();
        wake.wake();

        let mut bytes = [0u8; 8];
        assert_eq!(read(wake.fd.as_fd(), &mut bytes).unwrap(), 8);
        assert_eq!(u64::from_ne_bytes(bytes), 1);
    }
}
