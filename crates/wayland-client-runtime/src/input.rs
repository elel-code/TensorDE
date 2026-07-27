// Seat serial types use wayland-client WlSeat.
use wayland_client::protocol::wl_seat::WlSeat;

/// Registry-global identity of a bound `wl_seat`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SeatId(u32);

impl SeatId {
    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }
}

/// Snapshot of a bound seat's identity and devices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeatInfo {
    pub id: SeatId,
    /// Compositor seat name (`wl_seat.name`), when advertised.
    pub name: Option<String>,
    pub has_keyboard: bool,
    pub has_pointer: bool,
    pub has_touch: bool,
}

/// Seat hotplug and capability updates (`wl_seat` lifecycle).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SeatEvent {
    /// A seat was bound (devices may still be empty until capabilities arrive).
    Added(SeatInfo),
    /// Name or device capabilities changed on an existing seat.
    Changed(SeatInfo),
    /// A previously bound seat was removed from the registry.
    Removed(SeatId),
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum CursorIcon {
    ColResize,
    #[default]
    Default,
    Pointer,
    Text,
    /// North (top edge) resize.
    NResize,
    /// South (bottom edge) resize.
    SResize,
    /// East (right edge) resize.
    EResize,
    /// West (left edge) resize.
    WResize,
    /// North-east corner resize.
    NeResize,
    /// North-west corner resize.
    NwResize,
    /// South-east corner resize.
    SeResize,
    /// South-west corner resize.
    SwResize,
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

    /// Seat that issued this serial (must match the grab target seat).
    pub fn seat(&self) -> &WlSeat {
        &self.seat
    }

    /// Raw Wayland serial value.
    pub fn serial(&self) -> u32 {
        self.serial
    }

    pub fn is_popup_grab(&self) -> bool {
        matches!(
            self.source,
            InputSerialSource::PointerPress | InputSerialSource::TouchDown
        )
    }
}
