//! Compio completion service for blocked process termination signals.

use std::io;

use tensor_runtime::EventfdWakeError;
use thiserror::Error;

pub(crate) const MAX_PENDING_SIGNAL_EVENTS: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminationSignal {
    Hangup,
    Interrupt,
    Terminate,
}

impl TerminationSignal {
    #[cfg(target_os = "linux")]
    fn from_raw(raw: i32) -> Option<Self> {
        if raw == rustix::process::Signal::HUP.as_raw() {
            Some(Self::Hangup)
        } else if raw == rustix::process::Signal::INT.as_raw() {
            Some(Self::Interrupt)
        } else if raw == rustix::process::Signal::TERM.as_raw() {
            Some(Self::Terminate)
        } else {
            None
        }
    }
}

pub(crate) enum SignalEvent {
    Termination(TerminationSignal),
    RuntimeFailed(String),
}

#[derive(Debug, Error)]
pub enum SignalRuntimeError {
    #[error(transparent)]
    StopWake(#[from] EventfdWakeError),
    #[error("failed to spawn signal completion runtime: {0}")]
    RuntimeThread(io::Error),
    #[error("failed to initialize signal Compio runtime: {0}")]
    Runtime(io::Error),
    #[error("failed to create termination signalfd: {0}")]
    CreateSignalFd(io::Error),
    #[error("failed to attach termination signalfd to Compio: {0}")]
    AttachSignal(io::Error),
    #[error("failed to attach signal stop eventfd to Compio: {0}")]
    AttachStop(io::Error),
    #[error("signal completion runtime stopped during initialization")]
    RuntimeStartupDisconnected,
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod platform {
    use std::{
        ffi::c_int,
        os::fd::{FromRawFd, OwnedFd},
        sync::Arc,
        thread::{self, JoinHandle},
    };

    use compio::{io::AsyncReadExt, runtime::fd::AsyncFd};
    use rustix::{fs::OFlags, runtime::KernelSigSet};
    use tensor_runtime::{EventfdWake, WakeSink, WorkerTx, io_uring_runtime};

    use super::{SignalEvent, SignalRuntimeError, TerminationSignal};

    // `signalfd_siginfo` is a fixed 128-byte Linux userspace ABI record.
    const SIGNAL_INFO_SIZE: usize = 128;
    const SIGNAL_RUNTIME_OPS: usize = 2;

    unsafe extern "C" {
        #[link_name = "signalfd"]
        fn create_linux_signal_fd(fd: c_int, mask: *const KernelSigSet, flags: c_int) -> c_int;
    }

    pub(crate) struct SignalRuntime {
        stop: Arc<EventfdWake>,
        join: Option<JoinHandle<()>>,
    }

    impl SignalRuntime {
        pub(crate) fn start(events: WorkerTx<SignalEvent>) -> Result<Self, SignalRuntimeError> {
            let stop = Arc::new(EventfdWake::new()?);
            let thread_stop = Arc::clone(&stop);
            let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
            let join = thread::Builder::new()
                .name("tensor-signal-completions".to_owned())
                .spawn(move || {
                    let runtime = match io_uring_runtime(SIGNAL_RUNTIME_OPS) {
                        Ok(runtime) => runtime,
                        Err(error) => {
                            let _ = ready_tx.send(Err(SignalRuntimeError::Runtime(error)));
                            return;
                        }
                    };
                    runtime.block_on(async move {
                        let signal_fd = match create_signal_fd() {
                            Ok(fd) => fd,
                            Err(error) => {
                                let _ =
                                    ready_tx.send(Err(SignalRuntimeError::CreateSignalFd(error)));
                                return;
                            }
                        };
                        let signal_fd = match AsyncFd::new(signal_fd) {
                            Ok(fd) => fd,
                            Err(error) => {
                                let _ = ready_tx.send(Err(SignalRuntimeError::AttachSignal(error)));
                                return;
                            }
                        };
                        let mut stop_completion = match thread_stop.completion_reader() {
                            Ok(completion) => completion,
                            Err(error) => {
                                let _ = ready_tx.send(Err(SignalRuntimeError::AttachStop(error)));
                                return;
                            }
                        };
                        let signal_task = compio::runtime::spawn(read_one(signal_fd, events));
                        if ready_tx.send(Ok(())).is_err() {
                            return;
                        }
                        let _ = stop_completion.completed().await;
                        drop(signal_task);
                    });
                })
                .map_err(SignalRuntimeError::RuntimeThread)?;

            match ready_rx.recv() {
                Ok(Ok(())) => Ok(Self {
                    stop,
                    join: Some(join),
                }),
                Ok(Err(error)) => {
                    let _ = join.join();
                    Err(error)
                }
                Err(_) => {
                    let _ = join.join();
                    Err(SignalRuntimeError::RuntimeStartupDisconnected)
                }
            }
        }
    }

    impl Drop for SignalRuntime {
        fn drop(&mut self) {
            self.stop.wake();
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    async fn read_one(mut signal_fd: AsyncFd<OwnedFd>, events: WorkerTx<SignalEvent>) {
        loop {
            match read_signal(&mut signal_fd).await {
                Ok(Some(signal)) => {
                    let _ = events.try_send(SignalEvent::Termination(signal));
                    return;
                }
                Ok(None) => continue,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    let _ = events.try_send(SignalEvent::RuntimeFailed(error.to_string()));
                    return;
                }
            }
        }
    }

    async fn read_signal(
        signal_fd: &mut AsyncFd<OwnedFd>,
    ) -> std::io::Result<Option<TerminationSignal>> {
        let compio::BufResult(result, bytes) = signal_fd.read_exact([0u8; SIGNAL_INFO_SIZE]).await;
        result?;
        let raw = u32::from_ne_bytes(bytes[..4].try_into().expect("four-byte signal field"));
        Ok(TerminationSignal::from_raw(raw as i32))
    }

    fn create_signal_fd() -> std::io::Result<OwnedFd> {
        let set = crate::signals::termination_signal_set();
        let flags = (OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as c_int;
        // Rustix intentionally does not expose signalfd construction. Its
        // KernelSigSet is layout-compatible with the kernel prefix consumed
        // by the C ABI wrapper; all later I/O is a submitted Compio operation.
        let raw = unsafe { create_linux_signal_fd(-1, &set, flags) };
        if raw < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { OwnedFd::from_raw_fd(raw) })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tensor_runtime::WorkerBridge;

        #[test]
        fn runtime_starts_with_submitted_signal_and_stop_reads() {
            crate::signals::block_early().unwrap();
            let (events, _received) = WorkerBridge::bounded(1);
            let runtime = SignalRuntime::start(events).unwrap();
            drop(runtime);
            crate::signals::unblock_all_for_child().unwrap();
        }

        #[test]
        fn submitted_signalfd_read_completes_with_a_value() {
            crate::signals::block_early().unwrap();
            let runtime = io_uring_runtime(SIGNAL_RUNTIME_OPS).unwrap();
            let received = runtime.block_on(async {
                let mut signal_fd = AsyncFd::new(create_signal_fd().unwrap()).unwrap();
                let target = rustix::thread::gettid();
                let sender = std::thread::spawn(move || unsafe {
                    rustix::runtime::tkill(target, rustix::process::Signal::TERM).unwrap();
                });
                let signal = read_signal(&mut signal_fd).await.unwrap();
                sender.join().unwrap();
                signal
            });
            crate::signals::unblock_all_for_child().unwrap();

            assert_eq!(received, Some(TerminationSignal::Terminate));
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use tensor_runtime::WorkerTx;

    use super::{SignalEvent, SignalRuntimeError};

    pub(crate) struct SignalRuntime;

    impl SignalRuntime {
        pub(crate) fn start(_events: WorkerTx<SignalEvent>) -> Result<Self, SignalRuntimeError> {
            Ok(Self)
        }
    }
}

pub(crate) use platform::SignalRuntime;
