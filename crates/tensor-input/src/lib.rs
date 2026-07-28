//! Value-only input layer for Tensor.
//!
//! Tensor's native libinput adapter converts OS events
//! into [`Sample`] values. Policy and the event bus never see libinput objects.
//!
//! # Performance
//!
//! - Samples are `Copy` and small enough for the fixed event rings.
//! - Pointer motion coalesces in `tensor-event` (last sample wins).
//! - No allocation on the input path inside this crate.

mod capability;
mod event;
mod sample;

pub use capability::{DeviceCapabilities, DeviceId};
pub use event::{
    AbsoluteMotionEvent, AxisDirection, BackendInputEvent, DeviceChange, DeviceEvent,
    KeyboardEvent, PointerAxisEvent, PointerButtonEvent, PointerGestureEvent, RelativeMotionEvent,
};
pub use sample::{ButtonCode, KeyCode, Sample, TimeNs};
