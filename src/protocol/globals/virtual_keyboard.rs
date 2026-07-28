//! Authorized virtual keyboard injection with per-device XKB state.
//!
//! Keymaps are read once into bounded storage, compiled on the compositor
//! thread, and retained as sealed memfds. Key events then switch the active
//! wire keymap by `Arc` identity and do not copy keymap text on the hot path.

use std::{io, os::fd::OwnedFd, sync::Arc};

use rustix::fs::fstat;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::server::{
    zwp_virtual_keyboard_manager_v1::{self, ZwpVirtualKeyboardManagerV1},
    zwp_virtual_keyboard_v1::{self, ZwpVirtualKeyboardV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId},
    protocol::wl_keyboard,
};
use xkbcommon::xkb;

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::seat::KeymapFile,
    seat::ModifiersState,
    serial::next_serial,
    state::RuntimeState,
};

const VERSION: u32 = 1;
const MAX_VIRTUAL_KEYBOARDS: usize = 32;
const MAX_KEYMAP_BYTES: usize = 1 << 20;
const KEY_WORDS: usize = 16;
const KEY_COUNT: usize = KEY_WORDS * u64::BITS as usize;
const CAPACITY_ERROR: u32 = 1;
const INVALID_KEY_ERROR: u32 = 2;

struct VirtualKeymap {
    file: Arc<KeymapFile>,
    state: xkb::State,
}

#[derive(Clone)]
struct WireKeymap {
    file: Arc<KeymapFile>,
    modifiers: ModifiersState,
}

impl VirtualKeymap {
    fn wire(&self) -> WireKeymap {
        WireKeymap {
            file: self.file.clone(),
            modifiers: ModifiersState::from_xkb(&self.state),
        }
    }
}

struct VirtualKeyboard {
    resource: Weak<ZwpVirtualKeyboardV1>,
    keymap: Option<VirtualKeymap>,
    pressed: [u64; KEY_WORDS],
}

pub(crate) struct VirtualKeyboardProtocol {
    _global: GlobalId,
    keyboards: Vec<VirtualKeyboard>,
    key_counts: [u8; KEY_COUNT],
}

impl VirtualKeyboardProtocol {
    pub(crate) fn new<F>(display: &DisplayHandle, filter: F) -> Self
    where
        F: for<'client> Fn(&'client Client) -> bool + Send + Sync + 'static,
    {
        Self {
            _global: display.create_global::<RuntimeState, ZwpVirtualKeyboardManagerV1, _>(
                VERSION,
                GlobalData {
                    filter: Box::new(filter),
                },
            ),
            keyboards: Vec::with_capacity(MAX_VIRTUAL_KEYBOARDS),
            key_counts: [0; KEY_COUNT],
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.keyboards.is_empty()
    }

    fn register(&mut self, resource: &ZwpVirtualKeyboardV1) -> bool {
        self.keyboards
            .retain(|keyboard| keyboard.resource.upgrade().is_ok());
        if self.keyboards.len() == MAX_VIRTUAL_KEYBOARDS {
            return false;
        }
        self.keyboards.push(VirtualKeyboard {
            resource: resource.downgrade(),
            keymap: None,
            pressed: [0; KEY_WORDS],
        });
        true
    }

    fn keyboard_mut(&mut self, resource: &ZwpVirtualKeyboardV1) -> Option<&mut VirtualKeyboard> {
        let id = resource.id();
        self.keyboards
            .iter_mut()
            .find(|keyboard| keyboard.resource.id() == id)
    }

    fn replace_keymap(
        &mut self,
        resource: &ZwpVirtualKeyboardV1,
        keymap: VirtualKeymap,
    ) -> Option<(Option<WireKeymap>, [u64; KEY_WORDS])> {
        let index = self
            .keyboards
            .iter()
            .position(|keyboard| keyboard.resource.id() == resource.id())?;
        let old_file = self.keyboards[index]
            .keymap
            .as_ref()
            .map(VirtualKeymap::wire);
        let pressed = std::mem::take(&mut self.keyboards[index].pressed);
        let releases = self.release_counts(pressed);
        self.keyboards[index].keymap = Some(keymap);
        Some((old_file, releases))
    }

    fn update_key(
        &mut self,
        resource: &ZwpVirtualKeyboardV1,
        key: u32,
        pressed: bool,
    ) -> Result<(WireKeymap, bool), KeyFailure> {
        let key = usize::try_from(key)
            .ok()
            .filter(|key| *key < KEY_COUNT)
            .ok_or(KeyFailure::InvalidKey)?;
        let index = self
            .keyboards
            .iter()
            .position(|keyboard| keyboard.resource.id() == resource.id())
            .ok_or(KeyFailure::Missing)?;
        let keymap = self.keyboards[index]
            .keymap
            .as_ref()
            .ok_or(KeyFailure::NoKeymap)?
            .wire();
        let word = key / u64::BITS as usize;
        let mask = 1_u64 << (key % u64::BITS as usize);
        let was_pressed = self.keyboards[index].pressed[word] & mask != 0;
        if was_pressed == pressed {
            return Ok((keymap, false));
        }
        if pressed {
            self.keyboards[index].pressed[word] |= mask;
            let count = &mut self.key_counts[key];
            let emit = *count == 0;
            *count = count.saturating_add(1);
            Ok((keymap, emit))
        } else {
            self.keyboards[index].pressed[word] &= !mask;
            let count = &mut self.key_counts[key];
            *count = count.saturating_sub(1);
            Ok((keymap, *count == 0))
        }
    }

    fn update_modifiers(
        &mut self,
        resource: &ZwpVirtualKeyboardV1,
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    ) -> Result<WireKeymap, KeyFailure> {
        let keyboard = self.keyboard_mut(resource).ok_or(KeyFailure::Missing)?;
        let keymap = keyboard.keymap.as_mut().ok_or(KeyFailure::NoKeymap)?;
        keymap
            .state
            .update_mask(depressed, latched, locked, 0, 0, group);
        Ok(keymap.wire())
    }

    fn remove(
        &mut self,
        resource: &ZwpVirtualKeyboardV1,
    ) -> Option<(Option<WireKeymap>, [u64; KEY_WORDS])> {
        let index = self
            .keyboards
            .iter()
            .position(|keyboard| keyboard.resource.id() == resource.id())?;
        let keyboard = self.keyboards.swap_remove(index);
        let file = keyboard.keymap.as_ref().map(VirtualKeymap::wire);
        let releases = self.release_counts(keyboard.pressed);
        Some((file, releases))
    }

    fn release_counts(&mut self, pressed: [u64; KEY_WORDS]) -> [u64; KEY_WORDS] {
        let mut releases = [0; KEY_WORDS];
        for (word_index, word) in pressed.into_iter().enumerate() {
            let mut remaining = word;
            while remaining != 0 {
                let bit = remaining.trailing_zeros() as usize;
                let key = word_index * u64::BITS as usize + bit;
                self.key_counts[key] = self.key_counts[key].saturating_sub(1);
                if self.key_counts[key] == 0 {
                    releases[word_index] |= 1_u64 << bit;
                }
                remaining &= remaining - 1;
            }
        }
        releases
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyFailure {
    Missing,
    NoKeymap,
    InvalidKey,
}

struct GlobalData {
    filter: Box<dyn for<'client> Fn(&'client Client) -> bool + Send + Sync>,
}

#[derive(Clone, Copy, Debug)]
struct ManagerData;

#[derive(Clone, Copy, Debug)]
struct KeyboardData;

impl GlobalDispatchDelegate<ZwpVirtualKeyboardManagerV1, RuntimeState> for GlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpVirtualKeyboardManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, ManagerData);
    }

    fn can_view(&self, client: &Client) -> bool {
        (self.filter)(client)
    }
}

impl DispatchDelegate<ZwpVirtualKeyboardManagerV1, RuntimeState> for ManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &ZwpVirtualKeyboardManagerV1,
        request: zwp_virtual_keyboard_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_virtual_keyboard_manager_v1::Request::CreateVirtualKeyboard { seat, id } => {
                if !state.protocol_globals.seat.owns(&seat) {
                    resource.post_error(
                        zwp_virtual_keyboard_manager_v1::Error::Unauthorized,
                        "seat is not owned by Tensor",
                    );
                    return;
                }
                let keyboard = data_init.init(id, KeyboardData);
                if !state.protocol_globals.virtual_keyboard.register(&keyboard) {
                    resource.post_error(CAPACITY_ERROR, "virtual-keyboard capacity exceeded");
                    return;
                }
                state.reconcile_virtual_keyboard_capability();
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwpVirtualKeyboardV1, RuntimeState> for KeyboardData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &ZwpVirtualKeyboardV1,
        request: zwp_virtual_keyboard_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_virtual_keyboard_v1::Request::Keymap { format, fd, size } => {
                let Ok(keymap) = read_keymap(format, fd, size) else {
                    resource.post_error(
                        zwp_virtual_keyboard_v1::Error::NoKeymap,
                        "invalid or oversized XKB keymap",
                    );
                    return;
                };
                if let Some((file, releases)) = state
                    .protocol_globals
                    .virtual_keyboard
                    .replace_keymap(resource, keymap)
                {
                    state.release_virtual_keys(file, releases);
                }
            }
            zwp_virtual_keyboard_v1::Request::Key {
                time,
                key,
                state: key_state,
            } => {
                if key_state > 1 {
                    resource.post_error(INVALID_KEY_ERROR, "invalid virtual-keyboard key state");
                    return;
                }
                match state.protocol_globals.virtual_keyboard.update_key(
                    resource,
                    key,
                    key_state == wl_keyboard::KeyState::Pressed as u32,
                ) {
                    Ok((keymap, true)) => {
                        state.forward_virtual_key(keymap, key, key_state != 0, time)
                    }
                    Ok((_, false)) | Err(KeyFailure::Missing) => {}
                    Err(KeyFailure::NoKeymap) => resource.post_error(
                        zwp_virtual_keyboard_v1::Error::NoKeymap,
                        "key sent before keymap",
                    ),
                    Err(KeyFailure::InvalidKey) => {
                        resource.post_error(INVALID_KEY_ERROR, "virtual keycode exceeds seat range")
                    }
                }
            }
            zwp_virtual_keyboard_v1::Request::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            } => match state.protocol_globals.virtual_keyboard.update_modifiers(
                resource,
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            ) {
                Ok(keymap) => state.forward_virtual_modifiers(keymap),
                Err(KeyFailure::Missing) | Err(KeyFailure::InvalidKey) => {}
                Err(KeyFailure::NoKeymap) => resource.post_error(
                    zwp_virtual_keyboard_v1::Error::NoKeymap,
                    "modifiers sent before keymap",
                ),
            },
            zwp_virtual_keyboard_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &ZwpVirtualKeyboardV1,
    ) {
        if let Some((file, releases)) = state.protocol_globals.virtual_keyboard.remove(resource) {
            state.release_virtual_keys(file, releases);
            state.reconcile_virtual_keyboard_capability();
            state.restore_physical_keymap();
        }
    }
}

impl RuntimeState {
    fn reconcile_virtual_keyboard_capability(&mut self) {
        #[cfg(feature = "tty")]
        self.reconcile_seat_capabilities();
        #[cfg(not(feature = "tty"))]
        {
            let active = self.protocol_globals.virtual_keyboard.is_active();
            if active && !self.input_seat.keyboard_enabled() {
                if let Ok(keymap) = self.input_seat.enable_keyboard().map(ToOwned::to_owned) {
                    let _ = self
                        .protocol_globals
                        .seat
                        .set_keyboard_enabled(true, Some(&keymap));
                }
            } else if !active && self.input_seat.keyboard_enabled() {
                self.set_keyboard_focus(None, next_serial());
                self.input_seat.disable_keyboard();
                let _ = self.protocol_globals.seat.set_keyboard_enabled(false, None);
                self.protocol_globals.activation.sync_keyboard_focus(None);
            }
        }
    }

    fn activate_injected_keymap(&mut self, file: Arc<KeymapFile>, modifiers: ModifiersState) {
        let serial = next_serial();
        let keymap_changed = self.protocol_globals.seat.activate_keymap(file);
        if keymap_changed
            && let Some(grab) = self.protocol_globals.input_method.keyboard_grab_resource()
        {
            self.protocol_globals
                .seat
                .initialize_input_method_grab(&grab);
        }
        if keymap_changed || self.protocol_globals.seat.keyboard_modifiers() != modifiers {
            self.protocol_globals.seat.modifiers(modifiers, serial);
        }
    }

    fn forward_virtual_key(&mut self, keymap: WireKeymap, key: u32, pressed: bool, time: u32) {
        self.activate_injected_keymap(keymap.file, keymap.modifiers);
        self.protocol_globals
            .seat
            .key(key, pressed, next_serial(), time);
        self.notify_idle_activity();
    }

    fn forward_virtual_modifiers(&mut self, keymap: WireKeymap) {
        self.activate_injected_keymap(keymap.file, keymap.modifiers);
        self.notify_idle_activity();
    }

    fn release_virtual_keys(&mut self, keymap: Option<WireKeymap>, releases: [u64; KEY_WORDS]) {
        let Some(keymap) = keymap else {
            return;
        };
        self.activate_injected_keymap(keymap.file, keymap.modifiers);
        for (word_index, word) in releases.into_iter().enumerate() {
            let mut remaining = word;
            while remaining != 0 {
                let bit = remaining.trailing_zeros() as usize;
                let key = (word_index * u64::BITS as usize + bit) as u32;
                self.protocol_globals.seat.key(key, false, next_serial(), 0);
                remaining &= remaining - 1;
            }
        }
    }

    fn restore_physical_keymap(&mut self) {
        if self.protocol_globals.seat.activate_default_keymap() {
            if let Some(grab) = self.protocol_globals.input_method.keyboard_grab_resource() {
                self.protocol_globals
                    .seat
                    .initialize_input_method_grab(&grab);
            }
            self.protocol_globals
                .seat
                .modifiers(self.input_seat.keyboard_modifiers(), next_serial());
        }
    }
}

fn read_keymap(format: u32, fd: OwnedFd, size: u32) -> io::Result<VirtualKeymap> {
    let size = usize::try_from(size).map_err(|_| io::ErrorKind::InvalidInput)?;
    if format != wl_keyboard::KeymapFormat::XkbV1 as u32
        || size == 0
        || size > MAX_KEYMAP_BYTES
        || usize::try_from(fstat(&fd)?.st_size)
            .ok()
            .is_none_or(|actual| actual < size)
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut bytes = vec![0; size];
    let mut read = 0;
    while read < size {
        match rustix::io::pread(&fd, &mut bytes[read..], read as u64) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(count) => read += count,
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
    let text = String::from_utf8(bytes).map_err(|_| io::ErrorKind::InvalidData)?;
    let file = Arc::new(KeymapFile::new(&text)?);
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let keymap = xkb::Keymap::new_from_string(
        &context,
        text,
        xkb::KEYMAP_FORMAT_TEXT_V1,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .ok_or(io::ErrorKind::InvalidData)?;
    Ok(VirtualKeymap {
        file,
        state: xkb::State::new(&keymap),
    })
}

delegate_global_dispatch!(RuntimeState, ZwpVirtualKeyboardManagerV1, GlobalData);
delegate_dispatch!(RuntimeState, ZwpVirtualKeyboardManagerV1, ManagerData);
delegate_dispatch!(RuntimeState, ZwpVirtualKeyboardV1, KeyboardData);

#[cfg(test)]
mod tests;
