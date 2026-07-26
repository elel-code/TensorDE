//! Process-signal ownership for the compositor event loop.
//!
//! The termination signals are blocked before Tensor can create worker threads,
//! then consumed through calloop's signalfd source. Child applications must get
//! their ordinary signal mask back before `exec`.

use std::io;

use calloop::{Error, LoopHandle, LoopSignal};

/// Block termination signals before any compositor-owned threads can inherit an
/// unmasked signal disposition.
pub(crate) fn block_early() -> io::Result<()> {
    platform::block_early()
}

/// Stop the event loop after an orderly compositor termination signal.
pub(crate) fn install<D: 'static>(
    handle: &LoopHandle<'static, D>,
    stop_signal: LoopSignal,
) -> Result<(), Error> {
    platform::install(handle, stop_signal)
}

/// Restore an empty signal mask in a child immediately before it `exec`s.
pub(crate) fn unblock_all_for_child() -> io::Result<()> {
    platform::unblock_all_for_child()
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod platform {
    use std::{
        io::{self, Write},
        mem,
    };

    use calloop::{
        Error, LoopHandle, LoopSignal,
        signals::{Signal, Signals},
    };
    use tracing::info;

    pub(super) fn block_early() -> io::Result<()> {
        set_signal_mask(&termination_signal_set()?)
    }

    pub(super) fn install<D: 'static>(
        handle: &LoopHandle<'static, D>,
        stop_signal: LoopSignal,
    ) -> Result<(), Error> {
        let signals = Signals::new(&[Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP])?;
        handle
            .insert_source(signals, move |event, _, _| {
                // Niri-style: keep the handler tiny (log + stop). Also write to
                // stderr so a SIGTERM is visible even when the asynchronous file
                // drain has not yet flushed its queue.
                let signal = event.signal();
                write_signal_notice(signal);
                info!(
                    signal = ?signal,
                    "stopping compositor after termination signal"
                );
                stop_signal.stop();
            })
            .map_err(Error::from)?;
        Ok(())
    }

    fn write_signal_notice(signal: Signal) {
        let line = format!("tensor: quitting due to receiving signal {signal:?}\n");
        let stderr = io::stderr();
        let mut handle = stderr.lock();
        let _ = handle.write_all(line.as_bytes());
        let _ = handle.flush();
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

    fn termination_signal_set() -> io::Result<libc::sigset_t> {
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
        if unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, set, std::ptr::null_mut()) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use std::io;

    use calloop::{Error, LoopHandle, LoopSignal};

    pub(super) fn block_early() -> io::Result<()> {
        Ok(())
    }

    pub(super) fn install<D: 'static>(
        _handle: &LoopHandle<'static, D>,
        _stop_signal: LoopSignal,
    ) -> Result<(), Error> {
        Ok(())
    }

    pub(super) fn unblock_all_for_child() -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use calloop::EventLoop;

    use super::*;

    #[test]
    fn termination_source_registers_without_runtime_state() {
        let event_loop = EventLoop::<()>::try_new().unwrap();
        install(&event_loop.handle(), event_loop.get_signal()).unwrap();
    }
}
