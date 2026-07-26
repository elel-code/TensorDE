//! Main-thread libseat owner driven by one-shot Compio fd completions.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd},
    path::Path,
    rc::Rc,
};

use libseat::{Seat, SeatEvent};
use rustix::{fs::OFlags, io::Errno, io::fcntl_dupfd_cloexec};
use smithay::backend::session::{AsErrno, Session};
use tensor_host::SessionEvent;
use thiserror::Error;

const MAX_PENDING_SIGNALS: usize = 8;
const MAX_DISPATCHES_PER_COMPLETION: usize = 64;
const MAX_EVENTS_PER_COMPLETION: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SeatSignal {
    Enable,
    Disable,
}

#[derive(Debug)]
struct SignalQueue {
    pending: VecDeque<SeatSignal>,
    overflowed: bool,
}

impl SignalQueue {
    fn new() -> Self {
        Self {
            pending: VecDeque::with_capacity(MAX_PENDING_SIGNALS),
            overflowed: false,
        }
    }

    fn push(&mut self, signal: SeatSignal) {
        if self.pending.len() == MAX_PENDING_SIGNALS {
            self.overflowed = true;
        } else {
            self.pending.push_back(signal);
        }
    }

    fn drain(&mut self) -> Result<Vec<SeatSignal>, SeatSessionError> {
        if std::mem::take(&mut self.overflowed) {
            self.pending.clear();
            return Err(SeatSessionError::SignalOverflow);
        }
        Ok(self.pending.drain(..).collect())
    }
}

#[derive(Debug)]
struct SeatState {
    seat: RefCell<Seat>,
    seat_name: String,
    completion_fd: OwnedFd,
    active: Cell<bool>,
    devices: RefCell<HashMap<RawFd, libseat::Device>>,
    signals: Rc<RefCell<SignalQueue>>,
}

/// Cloneable main-thread session handle shared by DRM and libinput adapters.
#[derive(Clone, Debug)]
pub(super) struct SeatSession {
    state: Rc<SeatState>,
}

impl SeatSession {
    pub(super) fn new() -> Result<Self, SeatSessionError> {
        let signals = Rc::new(RefCell::new(SignalQueue::new()));
        let callback_signals = Rc::clone(&signals);
        let mut seat = Seat::open(move |_seat, event| {
            let signal = match event {
                SeatEvent::Enable => SeatSignal::Enable,
                SeatEvent::Disable => SeatSignal::Disable,
            };
            callback_signals.borrow_mut().push(signal);
        })
        .map_err(|error| SeatSessionError::Open(errno(error)))?;

        seat.dispatch(0)
            .map_err(|error| SeatSessionError::Dispatch(errno(error)))?;
        let mut active = false;
        for signal in signals.borrow_mut().drain()? {
            match signal {
                SeatSignal::Enable => active = true,
                SeatSignal::Disable => {
                    active = false;
                    seat.disable()
                        .map_err(|error| SeatSessionError::Disable(errno(error)))?;
                }
            }
        }

        let seat_name = seat.name().to_owned();
        let completion_fd = fcntl_dupfd_cloexec(
            seat.get_fd()
                .map_err(|error| SeatSessionError::CompletionFd(errno(error)))?,
            0,
        )
        .map_err(SeatSessionError::DuplicateCompletionFd)?;
        Ok(Self {
            state: Rc::new(SeatState {
                seat: RefCell::new(seat),
                seat_name,
                completion_fd,
                active: Cell::new(active),
                devices: RefCell::new(HashMap::new()),
                signals,
            }),
        })
    }

    pub(super) fn drain(&mut self) -> Result<Vec<SessionEvent>, SeatSessionError> {
        let mut events = Vec::with_capacity(2);
        for _ in 0..MAX_DISPATCHES_PER_COMPLETION {
            let dispatched = self
                .state
                .seat
                .borrow_mut()
                .dispatch(0)
                .map_err(|error| SeatSessionError::Dispatch(errno(error)))?;
            self.drain_signals(&mut events)?;
            if dispatched == 0 {
                return Ok(events);
            }
        }
        tracing::warn!(
            limit = MAX_DISPATCHES_PER_COMPLETION,
            "libseat completion hit its nonblocking dispatch budget"
        );
        Ok(events)
    }

    fn drain_signals(&self, events: &mut Vec<SessionEvent>) -> Result<(), SeatSessionError> {
        for signal in self.state.signals.borrow_mut().drain()? {
            if events.len() == MAX_EVENTS_PER_COMPLETION {
                return Err(SeatSessionError::EventOverflow);
            }
            match signal {
                SeatSignal::Enable => {
                    self.state.active.set(true);
                    events.push(SessionEvent::Activated);
                }
                SeatSignal::Disable => {
                    self.state.active.set(false);
                    self.state
                        .seat
                        .borrow_mut()
                        .disable()
                        .map_err(|error| SeatSessionError::Disable(errno(error)))?;
                    events.push(SessionEvent::Paused);
                }
            }
        }
        Ok(())
    }
}

impl AsFd for SeatSession {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.state.completion_fd.as_fd()
    }
}

impl Session for SeatSession {
    type Error = SeatSessionError;

    fn open(&mut self, path: &Path, _flags: OFlags) -> Result<OwnedFd, Self::Error> {
        let device = self
            .state
            .seat
            .borrow_mut()
            .open_device(&path)
            .map_err(|error| SeatSessionError::OpenDevice(errno(error)))?;
        let raw_fd = device.as_fd().as_raw_fd();
        self.state.devices.borrow_mut().insert(raw_fd, device);

        Ok(own_libseat_device_fd(raw_fd))
    }

    fn close(&mut self, fd: OwnedFd) -> Result<(), Self::Error> {
        let raw_fd = fd.as_raw_fd();
        if let Some(device) = self.state.devices.borrow_mut().remove(&raw_fd) {
            self.state
                .seat
                .borrow_mut()
                .close_device(device)
                .map_err(|error| SeatSessionError::CloseDevice(errno(error)))?;
        }
        Ok(())
    }

    fn change_vt(&mut self, vt: i32) -> Result<(), Self::Error> {
        self.state
            .seat
            .borrow_mut()
            .switch_session(vt)
            .map_err(|error| SeatSessionError::ChangeVt(errno(error)))
    }

    fn is_active(&self) -> bool {
        self.state.active.get()
    }

    fn seat(&self) -> String {
        self.state.seat_name.clone()
    }
}

#[derive(Debug, Error)]
pub(super) enum SeatSessionError {
    #[error("failed to open the libseat session: {0}")]
    Open(Errno),
    #[error("failed to query the libseat completion fd: {0}")]
    CompletionFd(Errno),
    #[error("failed to duplicate the libseat completion fd: {0}")]
    DuplicateCompletionFd(Errno),
    #[error("failed to dispatch libseat events: {0}")]
    Dispatch(Errno),
    #[error("failed to acknowledge a disabled libseat session: {0}")]
    Disable(Errno),
    #[error("failed to open a libseat device: {0}")]
    OpenDevice(Errno),
    #[error("failed to close a libseat device: {0}")]
    CloseDevice(Errno),
    #[error("failed to change the libseat VT: {0}")]
    ChangeVt(Errno),
    #[error("libseat callback queue exceeded its fixed capacity")]
    SignalOverflow,
    #[error("libseat completion produced too many lifecycle events")]
    EventOverflow,
}

impl AsErrno for SeatSessionError {
    fn as_errno(&self) -> Option<i32> {
        match self {
            Self::Open(error)
            | Self::CompletionFd(error)
            | Self::DuplicateCompletionFd(error)
            | Self::Dispatch(error)
            | Self::Disable(error)
            | Self::OpenDevice(error)
            | Self::CloseDevice(error)
            | Self::ChangeVt(error) => Some(error.raw_os_error()),
            Self::SignalOverflow | Self::EventOverflow => None,
        }
    }
}

fn errno(error: impl Into<i32>) -> Errno {
    Errno::from_raw_os_error(error.into())
}

#[allow(unsafe_code)]
fn own_libseat_device_fd(raw_fd: RawFd) -> OwnedFd {
    // `libseat::Device` deliberately does not close this valid fd on drop and
    // provides no safe ownership conversion. The matching `OwnedFd` is kept
    // alive until `Session::close`, after `libseat_close_device` is called.
    unsafe { OwnedFd::from_raw_fd(raw_fd) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_queue_preserves_lifecycle_order() {
        let mut queue = SignalQueue::new();
        queue.push(SeatSignal::Enable);
        queue.push(SeatSignal::Disable);

        assert_eq!(
            queue.drain().unwrap(),
            vec![SeatSignal::Enable, SeatSignal::Disable]
        );
    }

    #[test]
    fn signal_queue_fails_closed_on_overflow() {
        let mut queue = SignalQueue::new();
        for _ in 0..=MAX_PENDING_SIGNALS {
            queue.push(SeatSignal::Enable);
        }

        assert!(matches!(
            queue.drain(),
            Err(SeatSessionError::SignalOverflow)
        ));
        assert!(queue.drain().unwrap().is_empty());
    }
}
