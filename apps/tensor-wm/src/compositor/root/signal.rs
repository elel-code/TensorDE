//! Compositor-side handling for completed signalfd reads.

use std::{cell::RefCell, rc::Rc};

use tensor_runtime::{RuntimeStop, WorkerRx};
use tracing::error;

use crate::signals::{MAX_PENDING_SIGNAL_EVENTS, SignalEvent};

pub(super) fn drain_signal_events(
    events: &WorkerRx<SignalEvent>,
    stop_signal: &RuntimeStop,
    runtime_failure: &Rc<RefCell<Option<String>>>,
) {
    events.drain(MAX_PENDING_SIGNAL_EVENTS, |event| match event {
        SignalEvent::Termination(signal) => {
            crate::signals::report_termination(signal);
            stop_signal.stop();
        }
        SignalEvent::RuntimeFailed(message) => {
            error!(%message, "signal completion runtime failed");
            runtime_failure
                .borrow_mut()
                .replace(format!("signal: {message}"));
            stop_signal.stop();
        }
    });
}
