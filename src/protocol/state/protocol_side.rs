//! Side tables for protocol globals that keep compositor-owned identity.

use std::collections::HashMap;

use smithay::wayland::session_lock::LockSurface;
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use super::capture::CaptureSessions;
use crate::protocol::globals::foreign_toplevel::ForeignToplevelHandle;

/// Side tables for protocols that keep compositor-owned identity beyond a global.
#[derive(Default)]
pub(crate) struct ProtocolSideState {
    pub(crate) foreign_toplevels: HashMap<ObjectKey, ForeignToplevelHandle>,
    pub(crate) session_lock: Option<SessionLockState>,
    pub(crate) capture: CaptureSessions,
}

/// Hashable key for live Wayland surfaces.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ObjectKey(u32);

impl ObjectKey {
    pub(crate) fn from_surface(surface: &WlSurface) -> Self {
        Self(surface.id().protocol_id())
    }

    #[cfg(test)]
    pub(crate) const fn from_protocol_id(id: u32) -> Self {
        Self(id)
    }
}

pub(crate) struct SessionLockState {
    pub(crate) surfaces: HashMap<String, LockSurface>,
}
