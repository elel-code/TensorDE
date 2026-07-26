// Same type as smithay_client_toolkit reexports (both are wayland-client 0.31).
use wayland_client::protocol::wl_seat::WlSeat;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum CursorIcon {
    ColResize,
    #[default]
    Default,
    Pointer,
    Text,
}

/// The kind of input event that produced a Wayland serial.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InputSerialSource {
    PointerEnter,
    PointerPress,
    PointerRelease,
    PointerGestureBegin,
    PointerGestureEnd,
    KeyboardEnter,
    KeyboardKey,
    TouchDown,
    TouchUp,
}

/// An opaque seat-scoped Wayland input serial.
///
/// Keeping the seat with the serial prevents callers from accidentally pairing
/// a serial with an unrelated seat. Popup grabs accept only press/down serials.
#[derive(Clone, Debug)]
pub struct InputSerial {
    pub(crate) seat: WlSeat,
    pub(crate) serial: u32,
    source: InputSerialSource,
}

impl InputSerial {
    pub(crate) fn new(seat: WlSeat, serial: u32, source: InputSerialSource) -> Self {
        Self {
            seat,
            serial,
            source,
        }
    }

    pub fn source(&self) -> InputSerialSource {
        self.source
    }

    pub fn is_popup_grab(&self) -> bool {
        matches!(
            self.source,
            InputSerialSource::PointerPress | InputSerialSource::TouchDown
        )
    }
}
