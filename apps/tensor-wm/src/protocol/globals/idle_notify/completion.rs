use std::{io, os::fd::OwnedFd};

use tracing::warn;

use crate::protocol::state::RuntimeState;

pub(super) struct IdleTimerCompletion {
    pub(super) rearm: bool,
    pub(super) error: Option<String>,
}

impl IdleTimerCompletion {
    pub(super) fn healthy() -> Self {
        Self {
            rearm: true,
            error: None,
        }
    }

    pub(super) fn failed() -> Self {
        Self {
            rearm: false,
            error: None,
        }
    }

    pub(super) fn with_error(error: String) -> Self {
        Self {
            rearm: false,
            error: Some(error),
        }
    }
}

impl RuntimeState {
    pub(crate) fn duplicate_idle_timer_fd(&self) -> io::Result<Option<OwnedFd>> {
        self.protocol_globals.idle_notify.duplicate_timer_fd()
    }

    pub(crate) fn complete_idle_timer(&mut self) -> bool {
        let completion = self.protocol_globals.idle_notify.complete_timer();
        if let Some(error) = completion.error {
            warn!(%error, "idle-notify timerfd completion could not be consumed");
        }
        completion.rearm
    }

    pub(crate) fn idle_timer_completion_failed(&mut self, error: &io::Error) {
        warn!(%error, "idle-notify io_uring operation failed");
        self.protocol_globals.idle_notify.fail_timer();
    }

    pub(crate) fn notify_idle_activity(&mut self) {
        self.protocol_globals.idle_notify.notify_activity();
    }
}
