//! Main-thread libseat owner driven by one-shot Compio fd completions.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd},
    path::Path,
    rc::Rc,
};

use libseat::{Seat, SeatEvent};
use rustix::{fs::OFlags, io::Errno, io::fcntl_dupfd_cloexec};
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
    pending: [Option<SeatSignal>; MAX_PENDING_SIGNALS],
    head: usize,
    len: usize,
    overflowed: bool,
}

impl SignalQueue {
    fn new() -> Self {
        Self {
            pending: [None; MAX_PENDING_SIGNALS],
            head: 0,
            len: 0,
            overflowed: false,
        }
    }

    fn push(&mut self, signal: SeatSignal) {
        if self.len == MAX_PENDING_SIGNALS {
            self.overflowed = true;
        } else {
            let index = (self.head + self.len) % MAX_PENDING_SIGNALS;
            self.pending[index] = Some(signal);
            self.len += 1;
        }
    }

    fn pop_front(&mut self) -> Result<Option<SeatSignal>, SeatSessionError> {
        if std::mem::take(&mut self.overflowed) {
            self.clear();
            return Err(SeatSessionError::SignalOverflow);
        }
        if self.len == 0 {
            return Ok(None);
        }
        let signal = self.pending[self.head]
            .take()
            .expect("occupied libseat signal slot");
        self.head = (self.head + 1) % MAX_PENDING_SIGNALS;
        self.len -= 1;
        Ok(Some(signal))
    }

    fn clear(&mut self) {
        while self.len > 0 {
            self.pending[self.head] = None;
            self.head = (self.head + 1) % MAX_PENDING_SIGNALS;
            self.len -= 1;
        }
        self.head = 0;
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
    dispatches_in_completion: usize,
    events_in_completion: usize,
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
        while let Some(signal) = signals.borrow_mut().pop_front()? {
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
            dispatches_in_completion: 0,
            events_in_completion: 0,
        })
    }

    pub(super) fn begin_drain(&mut self) {
        self.dispatches_in_completion = 0;
        self.events_in_completion = 0;
    }

    pub(super) fn next_event(&mut self) -> Result<Option<SessionEvent>, SeatSessionError> {
        if let Some(event) = self.pop_signal_event()? {
            return Ok(Some(event));
        }
        while self.dispatches_in_completion < MAX_DISPATCHES_PER_COMPLETION {
            let dispatched = self
                .state
                .seat
                .borrow_mut()
                .dispatch(0)
                .map_err(|error| SeatSessionError::Dispatch(errno(error)))?;
            self.dispatches_in_completion += 1;
            if let Some(event) = self.pop_signal_event()? {
                return Ok(Some(event));
            }
            if dispatched == 0 {
                return Ok(None);
            }
        }
        tracing::warn!(
            limit = MAX_DISPATCHES_PER_COMPLETION,
            "libseat completion hit its nonblocking dispatch budget"
        );
        Ok(None)
    }

    fn pop_signal_event(&mut self) -> Result<Option<SessionEvent>, SeatSessionError> {
        let Some(signal) = self.state.signals.borrow_mut().pop_front()? else {
            return Ok(None);
        };
        if self.events_in_completion == MAX_EVENTS_PER_COMPLETION {
            self.state.signals.borrow_mut().clear();
            return Err(SeatSessionError::EventOverflow);
        }
        self.events_in_completion += 1;
        match signal {
            SeatSignal::Enable => {
                self.state.active.set(true);
                Ok(Some(SessionEvent::Activated))
            }
            SeatSignal::Disable => {
                self.state.active.set(false);
                self.state
                    .seat
                    .borrow_mut()
                    .disable()
                    .map_err(|error| SeatSessionError::Disable(errno(error)))?;
                Ok(Some(SessionEvent::Paused))
            }
        }
    }

    pub(super) fn open(
        &mut self,
        path: &Path,
        _flags: OFlags,
    ) -> Result<OwnedFd, SeatSessionError> {
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

    pub(super) fn close(&mut self, fd: OwnedFd) -> Result<(), SeatSessionError> {
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

    pub(super) fn change_vt(&mut self, vt: i32) -> Result<(), SeatSessionError> {
        self.state
            .seat
            .borrow_mut()
            .switch_session(vt)
            .map_err(|error| SeatSessionError::ChangeVt(errno(error)))
    }

    pub(super) fn is_active(&self) -> bool {
        self.state.active.get()
    }

    pub(super) fn seat(&self) -> String {
        self.state.seat_name.clone()
    }
}

impl AsFd for SeatSession {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.state.completion_fd.as_fd()
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

impl SeatSessionError {
    pub(super) fn errno(&self) -> i32 {
        match self {
            Self::Open(error)
            | Self::CompletionFd(error)
            | Self::DuplicateCompletionFd(error)
            | Self::Dispatch(error)
            | Self::Disable(error)
            | Self::OpenDevice(error)
            | Self::CloseDevice(error)
            | Self::ChangeVt(error) => error.raw_os_error(),
            Self::SignalOverflow | Self::EventOverflow => Errno::PERM.raw_os_error(),
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
    // alive until `SeatSession::close`, after `libseat_close_device` is called.
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

        assert_eq!(queue.pop_front().unwrap(), Some(SeatSignal::Enable));
        assert_eq!(queue.pop_front().unwrap(), Some(SeatSignal::Disable));
        assert_eq!(queue.pop_front().unwrap(), None);
    }

    #[test]
    fn signal_queue_fails_closed_on_overflow() {
        let mut queue = SignalQueue::new();
        for _ in 0..=MAX_PENDING_SIGNALS {
            queue.push(SeatSignal::Enable);
        }

        assert!(matches!(
            queue.pop_front(),
            Err(SeatSessionError::SignalOverflow)
        ));
        assert_eq!(queue.pop_front().unwrap(), None);
    }

    #[test]
    fn signal_queue_reuses_consumed_slots_without_reordering() {
        let mut queue = SignalQueue::new();
        for _ in 0..MAX_PENDING_SIGNALS {
            queue.push(SeatSignal::Enable);
        }
        for _ in 0..3 {
            assert_eq!(queue.pop_front().unwrap(), Some(SeatSignal::Enable));
        }
        queue.push(SeatSignal::Disable);
        queue.push(SeatSignal::Disable);
        queue.push(SeatSignal::Disable);

        for _ in 0..MAX_PENDING_SIGNALS - 3 {
            assert_eq!(queue.pop_front().unwrap(), Some(SeatSignal::Enable));
        }
        for _ in 0..3 {
            assert_eq!(queue.pop_front().unwrap(), Some(SeatSignal::Disable));
        }
        assert_eq!(queue.pop_front().unwrap(), None);
    }
}
