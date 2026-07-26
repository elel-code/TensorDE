//! Compositor-side handling for completed GPU sync-file waits.

use std::{cell::RefCell, rc::Rc};

use calloop::LoopSignal;
use tensor_runtime::WorkerRx;
use tracing::error;

use crate::{
    protocol::RuntimeState,
    render::{GpuFenceEvent, MAX_PENDING_GPU_FENCES},
};

pub(super) fn drain_gpu_fence_events(
    events: &WorkerRx<GpuFenceEvent>,
    state: &mut RuntimeState,
    stop_signal: &LoopSignal,
    runtime_failure: &Rc<RefCell<Option<String>>>,
) {
    events.drain(MAX_PENDING_GPU_FENCES, |event| match event {
        GpuFenceEvent::Signaled {
            output,
            timeline_value,
        } => {
            if let Err(message) = state.handle_gpu_fence_completion(output, timeline_value) {
                fail(message, stop_signal, runtime_failure);
            }
        }
        GpuFenceEvent::WaitFailed {
            output,
            timeline_value,
            message,
        } => fail(
            format!(
                "GPU SYNC_FD wait failed for output {output:?} timeline {timeline_value}: {message}"
            ),
            stop_signal,
            runtime_failure,
        ),
        GpuFenceEvent::RuntimeFailed(message) => fail(
            format!("GPU fence completion runtime failed: {message}"),
            stop_signal,
            runtime_failure,
        ),
    });
}

fn fail(message: String, stop_signal: &LoopSignal, runtime_failure: &Rc<RefCell<Option<String>>>) {
    error!(%message);
    runtime_failure
        .borrow_mut()
        .replace(format!("GPU fence: {message}"));
    stop_signal.stop();
}
