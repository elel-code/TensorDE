//! Tensor-owned Linux input values.
//!
//! Tensor's native libinput adapter and Wayland virtual-input protocols convert
//! source events into these plain values. Policy and the event bus never see
//! libinput or Wayland objects.
//!
//! # Performance
//!
//! - Samples are `Copy` and small enough for the fixed event rings.
//! - Pointer motion coalesces in the event queue (last sample wins).
//! - No allocation on the input path inside this crate.

mod capability;
mod event;
mod sample;

pub use capability::{DeviceCapabilities, DeviceGroupId, DeviceId, TabletToolId};
pub use event::{
    AbsoluteMotionEvent, AxisDirection, BackendInputEvent, DeviceChange, DeviceEvent,
    KeyboardEvent, PointerAxisEvent, PointerButtonEvent, PointerGestureEvent, RelativeMotionEvent,
    TabletPadDescriptor, TabletPadEvent, TabletPadGroupDescriptor, TabletPadRingEvent,
    TabletPadStripEvent, TabletToolAxesEvent, TabletToolButtonEvent, TabletToolCapabilities,
    TabletToolDescriptor, TabletToolProximityEvent, TabletToolTipEvent, TabletToolType,
};
pub use sample::{
    AxisSource, ButtonCode, KeyCode, KeyState, PointerAxis, PointerButton, PointerMotion, Sample,
    TimeNs,
};
