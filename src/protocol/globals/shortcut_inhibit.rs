//! Tensor-owned keyboard-shortcuts-inhibit wire state.

use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};

use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::server::{
    zwp_keyboard_shortcuts_inhibit_manager_v1::{self, ZwpKeyboardShortcutsInhibitManagerV1},
    zwp_keyboard_shortcuts_inhibitor_v1::{self, ZwpKeyboardShortcutsInhibitorV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct ShortcutInhibitProtocol {
    _global: GlobalId,
    /// Tensor exposes one logical seat, so surface identity is the complete
    /// protocol uniqueness key even if a client binds wl_seat more than once.
    inhibitors: HashMap<ObjectId, Weak<ZwpKeyboardShortcutsInhibitorV1>>,
}

impl ShortcutInhibitProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display
                .create_global::<RuntimeState, ZwpKeyboardShortcutsInhibitManagerV1, _>(
                    1,
                    ShortcutInhibitGlobalData,
                ),
            inhibitors: HashMap::new(),
        }
    }

    fn contains(&mut self, surface: &WlSurface) -> bool {
        let key = surface.id();
        let live = self
            .inhibitors
            .get(&key)
            .is_some_and(|inhibitor| inhibitor.upgrade().is_ok());
        if !live {
            self.inhibitors.remove(&key);
        }
        live
    }

    fn insert(&mut self, surface: &WlSurface, inhibitor: &ZwpKeyboardShortcutsInhibitorV1) {
        self.inhibitors.insert(surface.id(), inhibitor.downgrade());
    }

    fn remove(&mut self, surface: &WlSurface, inhibitor: &ZwpKeyboardShortcutsInhibitorV1) {
        let key = surface.id();
        if self
            .inhibitors
            .get(&key)
            .and_then(|resource| resource.upgrade().ok())
            .as_ref()
            == Some(inhibitor)
        {
            self.inhibitors.remove(&key);
        }
    }

    pub(super) fn remove_surface(&mut self, surface: &WlSurface) {
        self.inhibitors.remove(&surface.id());
    }

    pub(super) fn is_active(&self, surface: &WlSurface) -> bool {
        self.inhibitors
            .get(&surface.id())
            .and_then(|inhibitor| inhibitor.upgrade().ok())
            .is_some_and(|inhibitor| {
                inhibitor
                    .data::<ShortcutInhibitorData>()
                    .is_some_and(|data| data.active.load(Ordering::Acquire))
            })
    }

    #[cfg(test)]
    pub(super) fn inhibitor_count(&self) -> usize {
        self.inhibitors.len()
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct ShortcutInhibitGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct ShortcutInhibitManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct ShortcutInhibitorData {
    surface: WlSurface,
    active: AtomicBool,
}

impl GlobalDispatchDelegate<ZwpKeyboardShortcutsInhibitManagerV1, RuntimeState>
    for ShortcutInhibitGlobalData
{
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpKeyboardShortcutsInhibitManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, ShortcutInhibitManagerData);
    }
}

impl DispatchDelegate<ZwpKeyboardShortcutsInhibitManagerV1, RuntimeState>
    for ShortcutInhibitManagerData
{
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &ZwpKeyboardShortcutsInhibitManagerV1,
        request: zwp_keyboard_shortcuts_inhibit_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_keyboard_shortcuts_inhibit_manager_v1::Request::InhibitShortcuts {
                id,
                surface,
                seat,
            } => {
                if !state.seat.owns(&seat) {
                    return;
                }
                if state.protocol_globals.shortcut_inhibit.contains(&surface) {
                    manager.post_error(
                        zwp_keyboard_shortcuts_inhibit_manager_v1::Error::AlreadyInhibited,
                        "shortcuts are already inhibited for this seat and surface",
                    );
                    return;
                }
                let inhibitor = data_init.init(
                    id,
                    ShortcutInhibitorData {
                        surface: surface.clone(),
                        active: AtomicBool::new(true),
                    },
                );
                state
                    .protocol_globals
                    .shortcut_inhibit
                    .insert(&surface, &inhibitor);
                inhibitor.active();
            }
            zwp_keyboard_shortcuts_inhibit_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwpKeyboardShortcutsInhibitorV1, RuntimeState> for ShortcutInhibitorData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _inhibitor: &ZwpKeyboardShortcutsInhibitorV1,
        request: zwp_keyboard_shortcuts_inhibitor_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_keyboard_shortcuts_inhibitor_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        inhibitor: &ZwpKeyboardShortcutsInhibitorV1,
    ) {
        self.active.store(false, Ordering::Release);
        state
            .protocol_globals
            .shortcut_inhibit
            .remove(&self.surface, inhibitor);
    }
}

impl RuntimeState {
    pub(crate) fn shortcuts_inhibited_for(&self, surface: &WlSurface) -> bool {
        self.protocol_globals.shortcut_inhibit.is_active(surface)
    }

    #[cfg(test)]
    pub(crate) fn shortcut_inhibitor_count(&self) -> usize {
        self.protocol_globals.shortcut_inhibit.inhibitor_count()
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ZwpKeyboardShortcutsInhibitManagerV1,
    ShortcutInhibitGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ZwpKeyboardShortcutsInhibitManagerV1,
    ShortcutInhibitManagerData
);
delegate_dispatch!(
    RuntimeState,
    ZwpKeyboardShortcutsInhibitorV1,
    ShortcutInhibitorData
);
