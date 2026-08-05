use std::{
    io,
    os::fd::{AsFd, OwnedFd},
    sync::Arc,
};

use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
use wayland_protocols_misc::zwp_input_method_v2::server::zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2;
use wayland_server::{
    Resource,
    backend::ClientId,
    protocol::{
        wl_keyboard::{self, WlKeyboard},
        wl_surface::WlSurface,
    },
};

use super::{SeatProtocol, remove_resource};
use crate::protocol::{seat::ModifiersState, serial::Serial, state::RuntimeState};

#[derive(Debug)]
pub(in crate::protocol::globals) struct KeymapFile {
    fd: OwnedFd,
    nul_terminated_size: u32,
}

impl KeymapFile {
    pub(in crate::protocol::globals) fn new(keymap: &str) -> io::Result<Self> {
        let size = keymap
            .len()
            .checked_add(1)
            .and_then(|size| u32::try_from(size).ok())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "XKB keymap is too large")
            })?;
        let fd = memfd_create(
            "tensor-keymap",
            MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
        )?;
        write_all_at(&fd, 0, keymap.as_bytes())?;
        write_all_at(&fd, keymap.len() as u64, &[0])?;
        fcntl_add_seals(
            &fd,
            SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE | SealFlags::SEAL,
        )?;
        Ok(Self {
            fd,
            nul_terminated_size: size,
        })
    }

    fn send(&self, keyboard: &WlKeyboard) {
        keyboard.keymap(
            wl_keyboard::KeymapFormat::XkbV1,
            self.fd.as_fd(),
            self.nul_terminated_size,
        );
    }

    fn send_input_method(&self, keyboard: &ZwpInputMethodKeyboardGrabV2) {
        // Input-method clients parse this event as a sized XKB buffer, unlike
        // wl_keyboard consumers that expect a C string. Keep one sealed memfd
        // but exclude its trailing NUL from this wire view. Fcitx rejects that
        // terminator as an extra token when it is included in `size`.
        keyboard.keymap(
            wl_keyboard::KeymapFormat::XkbV1,
            self.fd.as_fd(),
            self.nul_terminated_size - 1,
        );
    }
}

fn write_all_at(fd: &impl AsFd, mut offset: u64, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        match rustix::io::pwrite(fd, bytes, offset) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(written) => {
                bytes = &bytes[written..];
                offset += written as u64;
            }
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) struct KeyboardSnapshot {
    surface: WlSurface,
    serial: Serial,
    pressed: Vec<u8>,
    modifiers: ModifiersState,
}

impl SeatProtocol {
    pub(crate) fn initialize_input_method_grab(&self, keyboard: &ZwpInputMethodKeyboardGrabV2) {
        if self.keyboard_enabled
            && let Some(keymap) = &self.keymap
        {
            keymap.send_input_method(keyboard);
        }
        keyboard.repeat_info(self.repeat_rate.max(0), self.repeat_delay.max(0));
    }

    pub(crate) fn set_keyboard_enabled(
        &mut self,
        enabled: bool,
        keymap: Option<&str>,
    ) -> io::Result<()> {
        if let Some(keymap) = keymap {
            let file = Arc::new(KeymapFile::new(keymap)?);
            self.default_keymap = Some(file.clone());
            self.activate_keymap(file);
        }
        if self.keyboard_enabled != enabled {
            self.keyboard_enabled = enabled;
            if !enabled {
                self.keyboard_focus = None;
                self.keyboard_focus_surface = None;
                self.keyboard_enter_serial = None;
            }
            self.send_capabilities();
        }
        Ok(())
    }

    pub(in crate::protocol::globals) fn activate_keymap(
        &mut self,
        keymap: Arc<KeymapFile>,
    ) -> bool {
        if self
            .keymap
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &keymap))
        {
            return false;
        }
        for keyboards in self.keyboards.values() {
            for keyboard in keyboards {
                keymap.send(keyboard);
                if keyboard.version() >= 4 {
                    keyboard.repeat_info(self.repeat_rate, self.repeat_delay);
                }
            }
        }
        self.keymap = Some(keymap);
        true
    }

    pub(crate) fn activate_default_keymap(&mut self) -> bool {
        let Some(keymap) = self.default_keymap.clone() else {
            return false;
        };
        self.activate_keymap(keymap)
    }

    pub(crate) fn keyboard_enter(&mut self, surface: &WlSurface, pressed: Vec<u8>, serial: Serial) {
        let Some(client) = surface.client() else {
            return;
        };
        let client_id = client.id();
        self.keyboard_focus = Some(client_id.clone());
        self.keyboard_focus_surface = Some(surface.downgrade());
        self.keyboard_enter_serial = Some(serial);
        let Some(keyboards) = self.keyboards.get(&client_id) else {
            return;
        };
        let Some((last, preceding)) = keyboards.split_last() else {
            return;
        };
        for keyboard in preceding {
            keyboard.enter(serial.into(), surface, pressed.clone());
        }
        last.enter(serial.into(), surface, pressed);
    }

    pub(crate) fn keyboard_leave(&mut self, surface: &WlSurface, serial: Serial) {
        let focus = self.keyboard_focus.take();
        self.keyboard_focus_surface = None;
        self.keyboard_enter_serial = None;
        if !surface.is_alive() {
            return;
        }
        let Some(client) = focus else {
            return;
        };
        if let Some(keyboards) = self.keyboards.get(&client) {
            for keyboard in keyboards {
                keyboard.leave(serial.into(), surface);
            }
        }
    }

    pub(crate) fn key(&self, key: u32, pressed: bool, serial: Serial, time: u32) {
        let Some(client) = self.keyboard_focus.as_ref() else {
            return;
        };
        let wire_state = if pressed {
            wl_keyboard::KeyState::Pressed
        } else {
            wl_keyboard::KeyState::Released
        };
        if let Some(keyboards) = self.keyboards.get(client) {
            for keyboard in keyboards {
                keyboard.key(serial.into(), time, key, wire_state);
            }
        }
    }

    pub(crate) fn modifiers(&mut self, modifiers: ModifiersState, serial: Serial) {
        self.keyboard_modifiers = modifiers;
        let Some(client) = self.keyboard_focus.as_ref() else {
            return;
        };
        let modifiers = modifiers.serialized;
        if let Some(keyboards) = self.keyboards.get(client) {
            for keyboard in keyboards {
                keyboard.modifiers(
                    serial.into(),
                    modifiers.depressed,
                    modifiers.latched,
                    modifiers.locked,
                    modifiers.layout,
                );
            }
        }
    }

    pub(super) fn insert_keyboard(
        &mut self,
        client: ClientId,
        keyboard: WlKeyboard,
        snapshot: Option<KeyboardSnapshot>,
    ) {
        if self.keyboard_enabled
            && let Some(keymap) = &self.keymap
        {
            keymap.send(&keyboard);
            if keyboard.version() >= 4 {
                keyboard.repeat_info(self.repeat_rate, self.repeat_delay);
            }
        }
        if let Some(snapshot) = snapshot {
            keyboard.enter(snapshot.serial.into(), &snapshot.surface, snapshot.pressed);
            let modifiers = snapshot.modifiers.serialized;
            keyboard.modifiers(
                snapshot.serial.into(),
                modifiers.depressed,
                modifiers.latched,
                modifiers.locked,
                modifiers.layout,
            );
        }
        self.keyboards.entry(client).or_default().push(keyboard);
    }

    pub(super) fn remove_keyboard(&mut self, client: &ClientId, keyboard: &WlKeyboard) {
        remove_resource(&mut self.keyboards, client, keyboard);
    }

    pub(crate) fn keyboard_focus_surface(&self) -> Option<WlSurface> {
        self.keyboard_focus_surface.as_ref()?.upgrade().ok()
    }

    pub(crate) const fn keyboard_modifiers(&self) -> ModifiersState {
        self.keyboard_modifiers
    }
}

impl RuntimeState {
    pub(super) fn keyboard_snapshot(&self, client: &ClientId) -> Option<KeyboardSnapshot> {
        let surface = self.protocol_globals.seat.keyboard_focus_surface()?;
        if surface.client()?.id() != *client {
            return None;
        }
        let serial = self.protocol_globals.seat.keyboard_enter_serial()?;
        Some(KeyboardSnapshot {
            surface,
            serial,
            pressed: self.input_seat.pressed_key_bytes(),
            modifiers: self.protocol_globals.seat.keyboard_modifiers(),
        })
    }
}
