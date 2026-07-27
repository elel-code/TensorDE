//! Small helpers shared by category mappers.

use crate::event::Modifiers;
use crate::native::shell::NativeShellEvent;

/// Extract UTF-8 / keysym from a native key event without building a full
/// [`crate::KeyboardEvent`].
pub fn map_native_key_text(
    event: &NativeShellEvent,
) -> Option<(u32, u32, bool, Option<&str>)> {
    match event {
        NativeShellEvent::SeatKeyboardKey {
            key,
            pressed,
            keysym,
            text,
            ..
        } => Some((*key, *keysym, *pressed, text.as_deref())),
        _ => None,
    }
}

/// Decode a Wayland/XKB modifier mask into public [`Modifiers`].
///
/// Bits follow the common libxkbcommon core mod indices (Shift/Caps/Ctrl/Alt/Num/Logo).
pub(crate) fn modifiers_from_xkb_mask(mask: u32) -> Modifiers {
    const SHIFT: u32 = 1 << 0;
    const CAPS: u32 = 1 << 1;
    const CTRL: u32 = 1 << 2;
    const ALT: u32 = 1 << 3;
    const NUM: u32 = 1 << 4;
    const LOGO: u32 = 1 << 6;
    Modifiers {
        shift: mask & SHIFT != 0,
        caps_lock: mask & CAPS != 0,
        ctrl: mask & CTRL != 0,
        alt: mask & ALT != 0,
        num_lock: mask & NUM != 0,
        logo: mask & LOGO != 0,
    }
}

/// Convenience: whether the event is a press that produced printable text.
pub fn native_key_text_pressed(event: &NativeShellEvent) -> Option<&str> {
    match event {
        NativeShellEvent::SeatKeyboardKey {
            pressed: true,
            text: Some(text),
            ..
        } if !text.is_empty() => Some(text.as_str()),
        _ => None,
    }
}
