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
pub(super) fn termination_signal_set() -> rustix::runtime::KernelSigSet {
    platform::termination_signal_set()
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod platform {
    use std::io;

    use rustix::{
        process::Signal,
        runtime::{How, KernelSigSet, kernel_sigprocmask},
    };

    pub(super) fn block_early() -> io::Result<()> {
        set_signal_mask(&termination_signal_set())
    }

    pub(super) fn unblock_all_for_child() -> io::Result<()> {
        set_signal_mask(&KernelSigSet::empty())
    }

    pub(super) fn termination_signal_set() -> KernelSigSet {
        let mut set = KernelSigSet::empty();
        set.insert(Signal::INT);
        set.insert(Signal::TERM);
        set.insert(Signal::HUP);
        set
    }

    fn set_signal_mask(set: &KernelSigSet) -> io::Result<()> {
        // These are ordinary application signals, never runtime-reserved RT signals.
        unsafe { kernel_sigprocmask(How::SETMASK, Some(set)) }
            .map(drop)
            .map_err(io::Error::from)
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
