//! Value-only input layer for Tensor.
//!
//! Device adapters (libinput via Smithay today, native later) convert OS events
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
    KeyboardEvent, PointerAxisEvent, PointerButtonEvent, RelativeMotionEvent,
};
pub use sample::{ButtonCode, KeyCode, Sample, TimeNs};
