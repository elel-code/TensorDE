//! Shared Wayland events to backend-independent scene events.

mod pointer;

use wayland_client_runtime::{NativeShellEvent, NativeSurfaceId};

use crate::engine::scene::{SceneEvent, SceneEventQueue};

#[derive(Debug, Default)]
pub(super) struct NativeWaylandEventSource {
    pending: Vec<SceneEvent>,
    pointer: pointer::PointerState,
}

impl NativeWaylandEventSource {
    pub(super) fn push_native_event(
        &mut self,
        surface: NativeSurfaceId,
        surface_protocol_id: u32,
        surface_size: (u32, u32),
        event: &NativeShellEvent,
    ) {
        if let Some(event) = pointer::scene_pointer_event(
            event,
            surface,
            u64::from(surface_protocol_id),
            [surface_size.0, surface_size.1],
            &mut self.pointer,
        ) {
            self.pending.push(SceneEvent::Pointer(event));
        }
    }

    pub(super) fn publish_to(&mut self, queue: &mut SceneEventQueue) {
        for event in self.pending.drain(..) {
            queue.publish(event);
        }
    }

    pub(super) fn discard_pending(&mut self) {
        self.pending.clear();
    }
}
