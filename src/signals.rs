//! Process-signal ownership for the compositor runtime.
//!
//! The termination signals are blocked before Tensor can create worker threads,
//! then consumed by a submitted Compio read on `signalfd`. Child applications
//! must get their ordinary signal mask back before `exec`.

use std::io::{self, Write};

mod runtime;

pub(crate) use runtime::{
    MAX_PENDING_SIGNAL_EVENTS, SignalEvent, SignalRuntime, SignalRuntimeError, TerminationSignal,
};

/// Block termination signals before any compositor-owned threads can inherit an
/// unmasked signal disposition.
pub(crate) fn block_early() -> io::Result<()> {
    platform::block_early()
}

/// Restore an empty signal mask in a child immediately before it `exec`s.
pub(crate) fn unblock_all_for_child() -> io::Result<()> {
    platform::unblock_all_for_child()
}

/// Report a completed termination signal before the compositor stops.
pub(crate) fn report_termination(signal: TerminationSignal) {
    let line = format!("tensor: quitting due to receiving signal {signal:?}\n");
    let stderr = io::stderr();
    let mut handle = stderr.lock();
    let _ = handle.write_all(line.as_bytes());
    let _ = handle.flush();
    tracing::info!(?signal, "stopping compositor after termination signal");
}

#[cfg(target_os = "linux")]
pub(super) fn termination_signal_set() -> io::Result<libc::sigset_t> {
    platform::termination_signal_set()
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod platform {
    use std::{io, mem};

    pub(super) fn block_early() -> io::Result<()> {
        set_signal_mask(&termination_signal_set()?)
    }

    pub(super) fn unblock_all_for_child() -> io::Result<()> {
        set_signal_mask(&empty_signal_set()?)
    }

    fn empty_signal_set() -> io::Result<libc::sigset_t> {
        let mut set = mem::MaybeUninit::uninit();
        if unsafe { libc::sigemptyset(set.as_mut_ptr()) } == 0 {
            Ok(unsafe { set.assume_init() })
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn termination_signal_set() -> io::Result<libc::sigset_t> {
        let mut set = empty_signal_set()?;
        unsafe {
            add_signal(&mut set, libc::SIGINT)?;
            add_signal(&mut set, libc::SIGTERM)?;
            add_signal(&mut set, libc::SIGHUP)?;
        }
        Ok(set)
    }

    // SAFETY: callers pass one of libc's valid signal constants.
    unsafe fn add_signal(set: &mut libc::sigset_t, signal: libc::c_int) -> io::Result<()> {
        if unsafe { libc::sigaddset(set, signal) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn set_signal_mask(set: &libc::sigset_t) -> io::Result<()> {
        let result = unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, set, std::ptr::null_mut()) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(result))
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use std::io;

    pub(super) fn block_early() -> io::Result<()> {
        Ok(())
    }

    pub(super) fn unblock_all_for_child() -> io::Result<()> {
        Ok(())
    }
}
