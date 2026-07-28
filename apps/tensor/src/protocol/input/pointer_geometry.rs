//! Pointer / keyboard key helpers (value-only; no seat objects).

use tensor_util::{LogicalPoint, LogicalRect};
use xkbcommon::xkb::keysyms;

pub(super) fn center_pointer_location(bounds: LogicalRect<i32>) -> LogicalPoint<f64> {
    let min_x = f64::from(bounds.loc.x);
    let min_y = f64::from(bounds.loc.y);
    let max_x = min_x + f64::from(bounds.size.w.saturating_sub(1));
    let max_y = min_y + f64::from(bounds.size.h.saturating_sub(1));
    ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0).into()
}

pub(super) fn sanitize_relative_pointer_delta(delta: LogicalPoint<f64>) -> LogicalPoint<f64> {
    (
        if delta.x.is_finite() { delta.x } else { 0.0 },
        if delta.y.is_finite() { delta.y } else { 0.0 },
    )
        .into()
}

pub(super) fn replace_non_finite_pointer_location(
    location: LogicalPoint<f64>,
    fallback: LogicalPoint<f64>,
) -> LogicalPoint<f64> {
    (
        if location.x.is_finite() {
            location.x
        } else {
            fallback.x
        },
        if location.y.is_finite() {
            location.y
        } else {
            fallback.y
        },
    )
        .into()
}

pub(crate) fn constrain_pointer_location(
    location: LogicalPoint<f64>,
    bounds: LogicalRect<i32>,
) -> LogicalPoint<f64> {
    let min_x = f64::from(bounds.loc.x);
    let min_y = f64::from(bounds.loc.y);
    let max_x = min_x + f64::from(bounds.size.w.saturating_sub(1));
    let max_y = min_y + f64::from(bounds.size.h.saturating_sub(1));
    (
        constrain_pointer_coordinate(location.x, min_x, max_x),
        constrain_pointer_coordinate(location.y, min_y, max_y),
    )
        .into()
}

pub(super) fn constrain_pointer_coordinate(value: f64, min: f64, max: f64) -> f64 {
    if value.is_nan() || value == f64::NEG_INFINITY {
        min
    } else if value == f64::INFINITY {
        max
    } else {
        value.clamp(min, max)
    }
}

pub(super) fn virtual_terminal_for_keysym(keysym: u32) -> Option<i32> {
    (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12)
        .contains(&keysym)
        .then(|| (keysym - keysyms::KEY_XF86Switch_VT_1 + 1) as i32)
}

/// Super+1..9 → workspace index 0..8 (zero-based host id).
pub(super) fn workspace_index_for_keysym(keysym: u32) -> Option<u32> {
    match keysym {
        keysyms::KEY_1 => Some(0),
        keysyms::KEY_2 => Some(1),
        keysyms::KEY_3 => Some(2),
        keysyms::KEY_4 => Some(3),
        keysyms::KEY_5 => Some(4),
        keysyms::KEY_6 => Some(5),
        keysyms::KEY_7 => Some(6),
        keysyms::KEY_8 => Some(7),
        keysyms::KEY_9 => Some(8),
        _ => None,
    }
}
