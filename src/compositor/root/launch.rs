//! Transitional launch-completion adapter.
//!
//! The worker payload lives in `WorkerBridge`. This module consumes only the
//! notification emitted after a Compio eventfd read completes, drains bounded
//! value outcomes, and injects their stable result into `tensor-event`.

use tensor_runtime::WorkerRx;
use tracing::{info, warn};

use crate::{
    protocol::RuntimeState,
    spawn::{LaunchOutcome, MAX_PENDING_LAUNCHES},
};

pub(super) fn drain_launch_outcomes(outcomes: &WorkerRx<LaunchOutcome>, state: &mut RuntimeState) {
    outcomes.drain(MAX_PENDING_LAUNCHES, |outcome| {
        handle_launch_outcome(outcome, state);
    });
}

fn handle_launch_outcome(outcome: LaunchOutcome, state: &mut RuntimeState) {
    let bus = match outcome.result() {
        Ok(process) => {
            info!(
                request_id = outcome.id(),
                program = ?outcome.program(),
                pid = process.pid(),
                strategy = process.strategy().name(),
                "application launch completed"
            );
            tensor_event::Event::Launch(tensor_event::LaunchOutcome::Started {
                request: outcome.id(),
                pid: process.pid(),
            })
        }
        Err(error) => {
            warn!(
                request_id = outcome.id(),
                program = ?outcome.program(),
                %error,
                "application launch failed"
            );
            tensor_event::Event::Launch(tensor_event::LaunchOutcome::Failed {
                request: outcome.id(),
            })
        }
    };
    let _ = state.push_event(bus);
}
