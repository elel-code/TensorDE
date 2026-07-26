//! Minimal libxkbcommon wrapper for native keyboard text.

use std::os::fd::OwnedFd;

use xkbcommon::xkb;

/// Per-seat xkb state built from a compositor keymap fd.
pub struct NativeXkb {
    state: xkb::State,
}

impl NativeXkb {
    /// Load keymap from a `wl_keyboard.keymap` fd (format text v1).
    ///
    /// # Safety contract
    ///
    /// Callers must pass a valid keymap fd from the compositor. Size is the
    /// byte length from the protocol event (includes trailing NUL).
    pub fn from_fd(fd: OwnedFd, size: u32) -> Option<Self> {
        if size == 0 {
            return None;
        }
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let map_size = size as usize;
        // SAFETY: fd is owned from wl_keyboard.keymap; size matches the event.
        // xkbcommon maps with MAP_PRIVATE (required for wl_keyboard v7+).
        let keymap = unsafe {
            xkb::Keymap::new_from_fd(
                &context,
                fd,
                map_size,
                xkb::KEYMAP_FORMAT_TEXT_V1,
                xkb::COMPILE_NO_FLAGS,
            )
        }
        .ok()
        .flatten()?;
        let state = xkb::State::new(&keymap);
        Some(Self { state })
    }

    pub fn update_mask(
        &mut self,
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    ) {
        self.state
            .update_mask(depressed, latched, locked, 0, 0, group);
    }

    /// Wayland keycode is Linux evdev; xkb expects keycode + 8.
    ///
    /// Updates internal key state on press/release. Returns UTF-8 text only
    /// for presses that produce printable characters.
    pub fn key_event(&mut self, wayland_key: u32, pressed: bool) -> KeyLookup {
        let keycode = xkb::Keycode::new(wayland_key + 8);
        if pressed {
            self.state.update_key(keycode, xkb::KeyDirection::Down);
        } else {
            self.state.update_key(keycode, xkb::KeyDirection::Up);
            return KeyLookup {
                keysym: self.state.key_get_one_sym(keycode).raw(),
                text: None,
            };
        }
        let keysym = self.state.key_get_one_sym(keycode).raw();
        let text = self.state.key_get_utf8(keycode);
        let text = if text.is_empty() { None } else { Some(text) };
        KeyLookup { keysym, text }
    }
}

/// Result of mapping a Wayland key through xkb.
#[derive(Clone, Debug, Default)]
pub struct KeyLookup {
    pub keysym: u32,
    pub text: Option<String>,
}
