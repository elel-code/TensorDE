//! Eventfd-based wake for the native Compio event loop.
//!
//! Writes are safe from any thread. Readiness is observed on the event-loop
//! thread through a cloned fd registered with Compio's io_uring proactor.

use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};

/// Cross-thread wake via Linux `eventfd`.
#[derive(Debug)]
pub struct EventFdWake {
    fd: OwnedFd,
    closed: AtomicBool,
}

impl EventFdWake {
    pub fn new() -> io::Result<Self> {
        // SAFETY: eventfd is a pure syscall; we take ownership of the returned fd.
        let raw = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: raw is a valid eventfd we own exclusively.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        Ok(Self {
            fd,
            closed: AtomicBool::new(false),
        })
    }

    pub fn wake(&self) {
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        let one: u64 = 1;
        // Ignore EAGAIN (counter already non-zero) and other races on shutdown.
        let _ = nix_write_u64(self.fd.as_raw_fd(), one);
    }

    /// Drain the eventfd counter after Compio reports the wake fd readable.
    pub fn drain(&self) {
        let mut buf = [0u8; 8];
        loop {
            // SAFETY: read into stack buffer of exact eventfd size.
            let n = unsafe {
                libc::read(
                    self.fd.as_raw_fd(),
                    buf.as_mut_ptr().cast(),
                    buf.len(),
                )
            };
            if n < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::WouldBlock {
                    break;
                }
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }
            if n == 0 {
                break;
            }
        }
    }

    pub fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl AsFd for EventFdWake {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

fn nix_write_u64(fd: RawFd, value: u64) -> io::Result<()> {
    let bytes = value.to_ne_bytes();
    // SAFETY: write of 8 bytes to eventfd.
    let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
    if n < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

// OwnedFd::from_raw_fd requires this import in scope for the unsafe block above.
use std::os::fd::FromRawFd;
