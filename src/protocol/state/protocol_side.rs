//! Side tables for protocol globals that keep compositor-owned identity.

use std::collections::{HashMap, HashSet};

use smithay::{
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    wayland::{
        foreign_toplevel_list::ForeignToplevelHandle,
        keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitor, session_lock::LockSurface,
    },
};

/// Side tables for protocols that keep compositor-owned identity beyond a global.
#[derive(Default)]
pub(crate) struct ProtocolSideState {
    pub(crate) idle_inhibitors: HashSet<WlSurface>,
    pub(crate) shortcut_inhibitors: HashMap<ObjectKey, KeyboardShortcutsInhibitor>,
    pub(crate) foreign_toplevels: HashMap<ObjectKey, ForeignToplevelHandle>,
    pub(crate) session_lock: Option<SessionLockState>,
}

/// Hashable key for live Wayland surfaces.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ObjectKey(u32);

impl ObjectKey {
    pub(crate) fn from_surface(surface: &WlSurface) -> Self {
        use smithay::reexports::wayland_server::Resource;
        Self(surface.id().protocol_id())
    }
}

pub(crate) struct SessionLockState {
    pub(crate) surfaces: HashMap<String, LockSurface>,
}
