//! Wayland protocol adapters for backend-independent scene events.

mod pointer;

use crate::engine::scene::{SceneEvent, SceneEventQueue};

#[derive(Debug, Default)]
pub(super) struct NativeWaylandEventSource {
    pending: Vec<SceneEvent>,
    last_pointer_time_millis: u32,
}

impl NativeWaylandEventSource {
    pub(super) fn push_pointer_event(
        &mut self,
        surface_id: u64,
        surface_size: [u32; 2],
        event: &smithay_client_toolkit::seat::pointer::PointerEvent,
    ) {
        let event = pointer::scene_pointer_event(
            event,
            surface_id,
            surface_size,
            &mut self.last_pointer_time_millis,
        );
        self.pending.push(SceneEvent::Pointer(event));
    }

    pub(super) fn publish_to(&mut self, queue: &mut SceneEventQueue) {
        for event in self.pending.drain(..) {
            queue.publish(event);
        }
    }
}
