use bitflags::bitflags;

use crate::{
    ActivationEvent, DndEvent, InputSerial, LayerSurfaceEvent, LogicalPosition, LogicalSize,
    OutputEvent, OutputId, PointerAxisSource, PointerAxisValue, PointerConstraintEvent,
    PointerGestureEvent, RelativePointerEvent, SeatEvent, SuggestedSize, SurfaceId,
    TextInputEvent,
};

bitflags! {
    /// State flags reported by an xdg-toplevel configure.
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
    pub struct ToplevelState: u16 {
        const MAXIMIZED = 1 << 0;
        const FULLSCREEN = 1 << 1;
        const RESIZING = 1 << 2;
        const ACTIVATED = 1 << 3;
        const TILED_LEFT = 1 << 4;
        const TILED_RIGHT = 1 << 5;
        const TILED_TOP = 1 << 6;
        const TILED_BOTTOM = 1 << 7;
        const SUSPENDED = 1 << 8;
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PopupConfigureKind {
    Initial,
    Reactive,
    Reposition { token: u32 },
}

#[derive(Clone, Debug)]
pub enum SurfaceEvent {
    Configure {
        surface: SurfaceId,
        suggested_size: SuggestedSize,
        state: ToplevelState,
        serial: u32,
    },
    PopupConfigure {
        surface: SurfaceId,
        position: LogicalPosition,
        size: LogicalSize,
        serial: u32,
        kind: PopupConfigureKind,
    },
    CloseRequested {
        surface: SurfaceId,
    },
    PopupDone {
        surface: SurfaceId,
    },
    Frame {
        surface: SurfaceId,
        time: u32,
    },
    /// `wp_presentation_feedback.presented` — content became visible to the user.
    Presented {
        surface: SurfaceId,
        /// Presentation-clock timestamp (seconds + nanoseconds).
        tv_sec: u64,
        tv_nsec: u32,
        /// Nominal refresh period in nanoseconds (`0` if unknown).
        refresh_ns: u32,
        /// Presentation sequence when available.
        seq: u64,
        /// `wp_presentation_feedback.kind` bitfield.
        flags: u32,
        /// Output registry name when the compositor reported `sync_output`.
        sync_output: Option<OutputId>,
    },
    /// Content update was discarded without being shown.
    PresentationDiscarded {
        surface: SurfaceId,
    },
    ScaleFactorChanged {
        surface: SurfaceId,
        /// Preferred compositor scale. Fractional values are reported when
        /// wp-fractional-scale-v1 is active for the surface.
        factor: f64,
    },
    /// `wl_surface.enter` — surface is (now) on this output.
    OutputEnter {
        surface: SurfaceId,
        output: OutputId,
    },
    /// `wl_surface.leave` — surface left this output.
    OutputLeave {
        surface: SurfaceId,
        output: OutputId,
    },
}

#[derive(Clone, Debug)]
pub enum PointerEventKind {
    Enter {
        serial: InputSerial,
    },
    Leave,
    Motion {
        time: u32,
    },
    Press {
        time: u32,
        button: u32,
        serial: InputSerial,
    },
    Release {
        time: u32,
        button: u32,
        serial: InputSerial,
    },
    Axis {
        time: u32,
        horizontal: PointerAxisValue,
        vertical: PointerAxisValue,
        source: Option<PointerAxisSource>,
    },
}

#[derive(Clone, Debug)]
pub struct PointerEvent {
    pub surface: SurfaceId,
    pub position: (f64, f64),
    pub kind: PointerEventKind,
    /// Seat that produced this event (`wl_seat` registry id), when known.
    pub seat: Option<crate::SeatId>,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub caps_lock: bool,
    pub logo: bool,
    pub num_lock: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyState {
    Pressed,
    Repeated,
    Released,
}

#[derive(Clone, Debug)]
pub enum KeyboardEvent {
    Enter {
        surface: SurfaceId,
        serial: InputSerial,
        pressed_raw_codes: Vec<u32>,
        /// Seat that owns this keyboard, when known.
        seat: Option<crate::SeatId>,
    },
    Leave {
        surface: SurfaceId,
        seat: Option<crate::SeatId>,
    },
    Key {
        surface: SurfaceId,
        state: KeyState,
        time: u32,
        raw_code: u32,
        keysym: u32,
        text: Option<String>,
        serial: InputSerial,
        seat: Option<crate::SeatId>,
    },
    Modifiers {
        surface: SurfaceId,
        modifiers: Modifiers,
        seat: Option<crate::SeatId>,
    },
}

#[derive(Clone, Debug)]
pub enum TouchEventKind {
    Down {
        time: u32,
        id: i32,
        position: (f64, f64),
        serial: InputSerial,
    },
    Up {
        time: u32,
        id: i32,
        serial: InputSerial,
    },
    Motion {
        time: u32,
        id: i32,
        position: (f64, f64),
    },
    Shape {
        id: i32,
        major: f64,
        minor: f64,
    },
    Orientation {
        id: i32,
        degrees: f64,
    },
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct TouchEvent {
    /// Surface associated with this point or cancellation. This is `None` only
    /// for an unmatched up or a cancellation with no tracked live point.
    pub surface: Option<SurfaceId>,
    pub kind: TouchEventKind,
    /// Seat that owns this touch device, when known.
    pub seat: Option<crate::SeatId>,
}

#[derive(Clone, Debug)]
pub enum Event {
    Surface(SurfaceEvent),
    LayerSurface(LayerSurfaceEvent),
    Output(OutputEvent),
    /// Seat hotplug (`wl_seat` registry add/remove).
    Seat(SeatEvent),
    Activation(ActivationEvent),
    Pointer(PointerEvent),
    PointerGesture(PointerGestureEvent),
    PointerConstraint(PointerConstraintEvent),
    RelativePointer(RelativePointerEvent),
    Keyboard(KeyboardEvent),
    TextInput(TextInputEvent),
    Touch(TouchEvent),
    Dnd(DndEvent),
    /// Linux dmabuf feedback / buffer lifecycle (`zwp_linux_dmabuf_v1`).
    Dmabuf(crate::dmabuf::DmabufEvent),
}

/// Contiguous pending-event storage optimized for append-and-drain batches.
///
/// Used by unit tests and as a pattern for batching; the live runtime maps
/// shell events straight into the caller's `Vec` to avoid an extra hop.
#[cfg(test)]
pub(crate) struct EventBuffer {
    pending: Vec<Event>,
}

#[cfg(test)]
impl EventBuffer {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            pending: Vec::with_capacity(capacity),
        }
    }

    pub(crate) fn push(&mut self, event: Event) {
        self.pending.push(event);
    }

    pub(crate) fn drain_into(&mut self, target: &mut Vec<Event>) {
        target.append(&mut self.pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_buffer_drains_in_order_and_reuses_internal_capacity() {
        let mut events = EventBuffer::with_capacity(4);
        events.push(Event::Touch(TouchEvent {
            surface: None,
            kind: TouchEventKind::Cancelled,
            seat: None,
        }));
        events.push(Event::Touch(TouchEvent {
            surface: Some(SurfaceId(7)),
            kind: TouchEventKind::Cancelled,
            seat: None,
        }));
        let capacity = events.pending.capacity();
        let mut drained = vec![Event::Touch(TouchEvent {
            surface: Some(SurfaceId(1)),
            kind: TouchEventKind::Cancelled,
            seat: None,
        })];

        events.drain_into(&mut drained);

        assert_eq!(drained.len(), 3);
        assert!(matches!(
            drained[0],
            Event::Touch(TouchEvent {
                surface: Some(SurfaceId(1)),
                ..
            })
        ));
        assert!(matches!(
            drained[1],
            Event::Touch(TouchEvent { surface: None, .. })
        ));
        assert!(matches!(
            drained[2],
            Event::Touch(TouchEvent {
                surface: Some(SurfaceId(7)),
                ..
            })
        ));
        assert!(events.pending.is_empty());
        assert_eq!(events.pending.capacity(), capacity);
    }
}
