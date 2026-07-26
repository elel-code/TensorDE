//! Libinput/Smithay → `tensor_input::Sample` conversion (adapter edge only).
//!
//! Policy and the event bus never see libinput types. Keep this module as the
//! only place that maps Smithay input events into value samples.

use smithay::backend::input::{
    self as input_backend, ButtonState, InputBackend, KeyState as SmithayKeyState,
    KeyboardKeyEvent, PointerButtonEvent,
};
use tensor_host::AxisSource;
use tensor_input::Sample;

/// Linux keycode from a Smithay keyboard event (xkb keycode raw).
#[inline]
pub(super) fn key_sample<B, E>(event: &E) -> Sample
where
    B: InputBackend,
    E: KeyboardKeyEvent<B>,
{
    let pressed = event.state() == SmithayKeyState::Pressed;
    let time_ns = u64::from(input_backend::Event::time_msec(event)).saturating_mul(1_000_000);
    Sample::key(event.key_code().raw(), pressed, time_ns)
}

/// Button sample from a Smithay pointer button event.
#[inline]
pub(super) fn button_sample<B, E>(event: &E) -> Sample
where
    B: InputBackend,
    E: PointerButtonEvent<B>,
{
    let pressed = event.state() == ButtonState::Pressed;
    let time_ns = u64::from(input_backend::Event::time_msec(event)).saturating_mul(1_000_000);
    Sample::pointer_button(event.button_code(), pressed, time_ns)
}

/// Axis sample when either axis reports a non-zero amount.
#[inline]
pub(super) fn axis_sample_if_nonzero(
    horizontal: f64,
    vertical: f64,
    time_msec: u32,
) -> Option<Sample> {
    if horizontal == 0.0 && vertical == 0.0 {
        return None;
    }
    Some(Sample::pointer_axis(
        horizontal,
        vertical,
        u64::from(time_msec).saturating_mul(1_000_000),
        AxisSource::Unknown,
    ))
}

/// Motion sample in logical compositor coordinates.
#[inline]
pub(super) fn motion_sample(x: f64, y: f64, time_msec: u32) -> Sample {
    Sample::pointer_motion(x, y, u64::from(time_msec).saturating_mul(1_000_000))
}
