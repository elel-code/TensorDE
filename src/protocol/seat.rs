//! Tensor-owned input state.
//!
//! This is compositor-thread state, not a readiness source. Backend samples
//! have already completed before they reach it, and protocol delivery is a
//! direct value-to-wire operation.

use std::io;

use tensor_util::LogicalPoint;
use wayland_server::{Resource, protocol::wl_surface::WlSurface};
use xkbcommon::xkb;

use super::serial::Serial;
use super::{
    focus::{install_keyboard_focus_hook, remove_keyboard_focus_hook},
    state::RuntimeState,
};

const KEY_WORDS: usize = 16;
const BUTTON_WORDS: usize = 16;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SerializedModifiers {
    pub(crate) depressed: u32,
    pub(crate) latched: u32,
    pub(crate) locked: u32,
    pub(crate) layout: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ModifiersState {
    pub(crate) ctrl: bool,
    pub(crate) alt: bool,
    pub(crate) shift: bool,
    pub(crate) logo: bool,
    pub(crate) serialized: SerializedModifiers,
}

impl ModifiersState {
    fn from_xkb(state: &xkb::State) -> Self {
        Self {
            ctrl: state.mod_name_is_active(&xkb::MOD_NAME_CTRL, xkb::STATE_MODS_EFFECTIVE),
            alt: state.mod_name_is_active(&xkb::MOD_NAME_ALT, xkb::STATE_MODS_EFFECTIVE),
            shift: state.mod_name_is_active(&xkb::MOD_NAME_SHIFT, xkb::STATE_MODS_EFFECTIVE),
            logo: state.mod_name_is_active(&xkb::MOD_NAME_LOGO, xkb::STATE_MODS_EFFECTIVE),
            serialized: SerializedModifiers {
                depressed: state.serialize_mods(xkb::STATE_MODS_DEPRESSED),
                latched: state.serialize_mods(xkb::STATE_MODS_LATCHED),
                locked: state.serialize_mods(xkb::STATE_MODS_LOCKED),
                layout: state.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE),
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct KeyUpdate {
    pub(crate) evdev_key: u32,
    pub(crate) keysym: u32,
    pub(crate) pressed: bool,
    pub(crate) modifiers: ModifiersState,
    pub(crate) modifiers_changed: bool,
    pub(crate) transition: bool,
}

pub(crate) struct KeyboardState {
    state: xkb::State,
    keymap: String,
    pressed: [u64; KEY_WORDS],
    forwarded: [u64; KEY_WORDS],
    modifiers: ModifiersState,
    focus: Option<WlSurface>,
    last_press_serial: Option<Serial>,
}

impl KeyboardState {
    fn new() -> io::Result<Self> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            "",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .ok_or_else(|| io::Error::other("failed to compile the default XKB keymap"))?;
        let state = xkb::State::new(&keymap);
        let modifiers = ModifiersState::from_xkb(&state);
        Ok(Self {
            state,
            keymap: keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1),
            pressed: [0; KEY_WORDS],
            forwarded: [0; KEY_WORDS],
            modifiers,
            focus: None,
            last_press_serial: None,
        })
    }

    fn update(&mut self, evdev_key: u32, pressed: bool, serial: Serial) -> KeyUpdate {
        let transition = set_bit(&mut self.pressed, evdev_key, pressed);
        let keycode = xkb::Keycode::new(evdev_key.saturating_add(8));
        let keysym = self.state.key_get_one_sym(keycode).raw();
        let old_modifiers = self.modifiers;
        if transition {
            self.state.update_key(
                keycode,
                if pressed {
                    xkb::KeyDirection::Down
                } else {
                    xkb::KeyDirection::Up
                },
            );
            self.modifiers = ModifiersState::from_xkb(&self.state);
            if pressed {
                self.last_press_serial = Some(serial);
            }
        }
        KeyUpdate {
            evdev_key,
            keysym,
            pressed,
            modifiers: self.modifiers,
            modifiers_changed: old_modifiers.serialized != self.modifiers.serialized,
            transition,
        }
    }

    fn pressed_wire_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.pressed_count() * size_of::<u32>());
        for (word_index, word) in self.pressed.iter().copied().enumerate() {
            let mut remaining = word;
            while remaining != 0 {
                let bit = remaining.trailing_zeros() as usize;
                let key = (word_index * u64::BITS as usize + bit) as u32;
                bytes.extend_from_slice(&key.to_ne_bytes());
                remaining &= remaining - 1;
            }
        }
        bytes
    }

    fn pressed_count(&self) -> usize {
        self.pressed
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PointerGrabStart {
    pub(crate) serial: Serial,
    pub(crate) focus: Option<WlSurface>,
    pub(crate) origin: LogicalPoint<f64>,
}

pub(crate) struct PointerState {
    location: LogicalPoint<f64>,
    focus: Option<WlSurface>,
    focus_origin: LogicalPoint<f64>,
    buttons: [u64; BUTTON_WORDS],
    grab_start: Option<PointerGrabStart>,
}

impl Default for PointerState {
    fn default() -> Self {
        Self {
            location: (0.0, 0.0).into(),
            focus: None,
            focus_origin: (0.0, 0.0).into(),
            buttons: [0; BUTTON_WORDS],
            grab_start: None,
        }
    }
}

impl PointerState {
    fn set_button(&mut self, button: u32, pressed: bool, serial: Serial) -> bool {
        let transition = set_bit(&mut self.buttons, button, pressed);
        if !transition {
            return false;
        }
        if pressed && self.grab_start.is_none() {
            self.grab_start = Some(PointerGrabStart {
                serial,
                focus: self.focus.clone(),
                origin: self.focus_origin,
            });
        } else if !pressed && self.buttons.iter().all(|word| *word == 0) {
            self.grab_start = None;
        }
        true
    }
}

#[derive(Default)]
pub(crate) struct InputSeat {
    keyboard: Option<KeyboardState>,
    pointer: Option<PointerState>,
    touch: bool,
}

impl InputSeat {
    pub(crate) fn enable_keyboard(&mut self) -> io::Result<&str> {
        if self.keyboard.is_none() {
            self.keyboard = Some(KeyboardState::new()?);
        }
        Ok(&self.keyboard.as_ref().unwrap().keymap)
    }

    pub(crate) fn disable_keyboard(&mut self) {
        self.keyboard = None;
    }

    pub(crate) const fn keyboard_enabled(&self) -> bool {
        self.keyboard.is_some()
    }

    pub(crate) fn keyboard_focus(&self) -> Option<&WlSurface> {
        self.keyboard.as_ref()?.focus.as_ref()
    }

    pub(crate) fn set_keyboard_focus(&mut self, focus: Option<WlSurface>) -> Option<WlSurface> {
        let keyboard = self.keyboard.as_mut()?;
        if keyboard.focus == focus {
            return None;
        }
        std::mem::replace(&mut keyboard.focus, focus)
    }

    pub(crate) fn update_key(
        &mut self,
        evdev_key: u32,
        pressed: bool,
        serial: Serial,
    ) -> Option<KeyUpdate> {
        Some(self.keyboard.as_mut()?.update(evdev_key, pressed, serial))
    }

    pub(crate) fn set_key_forwarded(&mut self, key: u32, forwarded: bool) {
        if let Some(keyboard) = self.keyboard.as_mut() {
            set_bit(&mut keyboard.forwarded, key, forwarded);
        }
    }

    pub(crate) fn key_was_forwarded(&self, key: u32) -> bool {
        self.keyboard
            .as_ref()
            .is_some_and(|keyboard| bit_is_set(&keyboard.forwarded, key))
    }

    pub(crate) fn keyboard_modifiers(&self) -> ModifiersState {
        self.keyboard
            .as_ref()
            .map(|keyboard| keyboard.modifiers)
            .unwrap_or_default()
    }

    pub(crate) fn pressed_key_bytes(&self) -> Vec<u8> {
        self.keyboard
            .as_ref()
            .map(KeyboardState::pressed_wire_bytes)
            .unwrap_or_default()
    }

    pub(crate) fn keyboard_has_serial(&self, serial: Serial) -> bool {
        self.keyboard
            .as_ref()
            .is_some_and(|keyboard| keyboard.last_press_serial == Some(serial))
    }

    pub(crate) fn enable_pointer(&mut self) {
        self.pointer.get_or_insert_with(PointerState::default);
    }

    pub(crate) fn disable_pointer(&mut self) {
        self.pointer = None;
    }

    pub(crate) const fn pointer_enabled(&self) -> bool {
        self.pointer.is_some()
    }

    pub(crate) fn pointer_location(&self) -> Option<LogicalPoint<f64>> {
        Some(self.pointer.as_ref()?.location)
    }

    pub(crate) fn set_pointer_location(&mut self, location: LogicalPoint<f64>) {
        if let Some(pointer) = self.pointer.as_mut() {
            pointer.location = location;
        }
    }

    pub(crate) fn pointer_focus(&self) -> Option<&WlSurface> {
        self.pointer.as_ref()?.focus.as_ref()
    }

    pub(crate) fn pointer_focus_owned(&self) -> Option<WlSurface> {
        self.pointer_focus().cloned()
    }

    pub(crate) fn replace_pointer_focus(
        &mut self,
        focus: Option<(WlSurface, LogicalPoint<f64>)>,
    ) -> Option<WlSurface> {
        let pointer = self.pointer.as_mut()?;
        let (focus, origin) = focus
            .map(|(surface, origin)| (Some(surface), origin))
            .unwrap_or((None, pointer.focus_origin));
        pointer.focus_origin = origin;
        std::mem::replace(&mut pointer.focus, focus)
    }

    pub(crate) fn update_pointer_origin(&mut self, origin: LogicalPoint<f64>) {
        if let Some(pointer) = self.pointer.as_mut() {
            pointer.focus_origin = origin;
        }
    }

    pub(crate) fn set_button(&mut self, button: u32, pressed: bool, serial: Serial) -> bool {
        self.pointer
            .as_mut()
            .is_some_and(|pointer| pointer.set_button(button, pressed, serial))
    }

    pub(crate) fn pointer_is_grabbed(&self) -> bool {
        self.pointer
            .as_ref()
            .is_some_and(|pointer| pointer.grab_start.is_some())
    }

    pub(crate) fn pointer_grab_start(&self) -> Option<&PointerGrabStart> {
        self.pointer.as_ref()?.grab_start.as_ref()
    }

    pub(crate) fn pointer_has_serial(&self, serial: Serial) -> bool {
        self.pointer_grab_start()
            .is_some_and(|start| start.serial == serial)
    }

    pub(crate) fn clear_pointer_grab(&mut self) {
        if let Some(pointer) = self.pointer.as_mut() {
            pointer.buttons = [0; BUTTON_WORDS];
            pointer.grab_start = None;
        }
    }

    pub(crate) fn set_touch_enabled(&mut self, enabled: bool) {
        self.touch = enabled;
    }

    pub(crate) const fn touch_enabled(&self) -> bool {
        self.touch
    }

    pub(crate) fn surface_destroyed(&mut self, surface: &WlSurface) {
        let object = surface.id();
        if self
            .keyboard_focus()
            .is_some_and(|focus| focus.id() == object)
            && let Some(keyboard) = self.keyboard.as_mut()
        {
            keyboard.focus = None;
        }
        if let Some(pointer) = self.pointer.as_mut() {
            if pointer
                .focus
                .as_ref()
                .is_some_and(|focus| focus.id() == object)
            {
                pointer.focus = None;
            }
            if pointer
                .grab_start
                .as_ref()
                .and_then(|start| start.focus.as_ref())
                .is_some_and(|focus| focus.id() == object)
            {
                pointer.grab_start = None;
                pointer.buttons = [0; BUTTON_WORDS];
            }
        }
    }
}

impl RuntimeState {
    pub(crate) fn set_keyboard_focus(&mut self, focus: Option<WlSurface>, serial: Serial) {
        if !self.input_seat.keyboard_enabled() || self.input_seat.keyboard_focus() == focus.as_ref()
        {
            return;
        }
        let old = self.input_seat.set_keyboard_focus(focus.clone());
        if let Some(old) = old {
            self.protocol_globals.seat.keyboard_leave(&old, serial);
            remove_keyboard_focus_hook(&old);
        }
        let focus = focus.filter(Resource::is_alive);
        if let Some(surface) = focus.as_ref() {
            self.protocol_globals.seat.keyboard_enter(
                surface,
                self.input_seat.pressed_key_bytes(),
                serial,
            );
            self.protocol_globals
                .seat
                .modifiers(self.input_seat.keyboard_modifiers(), serial);
            install_keyboard_focus_hook(surface);
        } else if self.input_seat.keyboard_focus().is_some() {
            self.input_seat.set_keyboard_focus(None);
        }
        self.protocol_globals
            .activation
            .sync_keyboard_focus(focus.as_ref());
        self.protocol_globals
            .selection
            .set_focus(focus.and_then(|surface| surface.client().map(|client| client.id())));
    }
}

fn bit_is_set<const N: usize>(bits: &[u64; N], value: u32) -> bool {
    let index = value as usize / u64::BITS as usize;
    let bit = value % u64::BITS;
    bits.get(index)
        .is_some_and(|word| word & (1_u64 << bit) != 0)
}

fn set_bit<const N: usize>(bits: &mut [u64; N], value: u32, enabled: bool) -> bool {
    let index = value as usize / u64::BITS as usize;
    let bit = value % u64::BITS;
    let Some(word) = bits.get_mut(index) else {
        return false;
    };
    let mask = 1_u64 << bit;
    let was_enabled = *word & mask != 0;
    if enabled {
        *word |= mask;
    } else {
        *word &= !mask;
    }
    was_enabled != enabled
}
