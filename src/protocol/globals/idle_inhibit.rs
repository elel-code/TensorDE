//! Tensor-owned idle-inhibit wire state with exact inhibitor multiplicity.

use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};

use wayland_protocols::wp::idle_inhibit::zv1::server::{
    zwp_idle_inhibit_manager_v1::{self, ZwpIdleInhibitManagerV1},
    zwp_idle_inhibitor_v1::{self, ZwpIdleInhibitorV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct IdleInhibitProtocol {
    _global: GlobalId,
    surfaces: HashMap<ObjectId, usize>,
}

impl IdleInhibitProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, ZwpIdleInhibitManagerV1, _>(
                1,
                IdleInhibitGlobalData,
            ),
            surfaces: HashMap::new(),
        }
    }

    /// Returns whether aggregate inhibition changed from false to true.
    pub(super) fn add(&mut self, surface: &WlSurface) -> bool {
        let was_empty = self.surfaces.is_empty();
        let count = self.surfaces.entry(surface.id()).or_default();
        *count = count.saturating_add(1);
        was_empty
    }

    /// Returns whether aggregate inhibition changed from true to false.
    pub(super) fn remove(&mut self, surface: &WlSurface) -> bool {
        let Some(count) = self.surfaces.get_mut(&surface.id()) else {
            return false;
        };
        *count -= 1;
        if *count == 0 {
            self.surfaces.remove(&surface.id());
        }
        self.surfaces.is_empty()
    }

    /// A destroyed surface cannot inhibit even if its inhibitor object lingers.
    pub(super) fn remove_surface(&mut self, surface: &WlSurface) -> bool {
        self.surfaces.remove(&surface.id()).is_some() && self.surfaces.is_empty()
    }

    #[cfg(test)]
    pub(super) fn inhibitor_count(&self) -> usize {
        self.surfaces.values().copied().sum()
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct IdleInhibitGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct IdleInhibitManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct IdleInhibitorData {
    surface: WlSurface,
    active: AtomicBool,
}

impl GlobalDispatchDelegate<ZwpIdleInhibitManagerV1, RuntimeState> for IdleInhibitGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpIdleInhibitManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, IdleInhibitManagerData);
    }
}

impl DispatchDelegate<ZwpIdleInhibitManagerV1, RuntimeState> for IdleInhibitManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _manager: &ZwpIdleInhibitManagerV1,
        request: zwp_idle_inhibit_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_idle_inhibit_manager_v1::Request::CreateInhibitor { id, surface } => {
                state.add_idle_inhibitor(&surface);
                data_init.init(
                    id,
                    IdleInhibitorData {
                        surface,
                        active: AtomicBool::new(true),
                    },
                );
            }
            zwp_idle_inhibit_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwpIdleInhibitorV1, RuntimeState> for IdleInhibitorData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _inhibitor: &ZwpIdleInhibitorV1,
        request: zwp_idle_inhibitor_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_idle_inhibitor_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        _resource: &ZwpIdleInhibitorV1,
    ) {
        if self.active.swap(false, Ordering::AcqRel) {
            state.remove_idle_inhibitor(&self.surface);
        }
    }
}

impl RuntimeState {
    fn add_idle_inhibitor(&mut self, surface: &WlSurface) {
        if self.protocol_globals.idle_inhibit.add(surface) {
            self.protocol_globals.idle_notifier.set_is_inhibited(true);
        }
    }

    fn remove_idle_inhibitor(&mut self, surface: &WlSurface) {
        if self.protocol_globals.idle_inhibit.remove(surface) {
            self.protocol_globals.idle_notifier.set_is_inhibited(false);
        }
    }

    #[cfg(test)]
    pub(crate) fn idle_inhibitor_count(&self) -> usize {
        self.protocol_globals.idle_inhibit.inhibitor_count()
    }
}

delegate_global_dispatch!(RuntimeState, ZwpIdleInhibitManagerV1, IdleInhibitGlobalData);
delegate_dispatch!(
    RuntimeState,
    ZwpIdleInhibitManagerV1,
    IdleInhibitManagerData
);
delegate_dispatch!(RuntimeState, ZwpIdleInhibitorV1, IdleInhibitorData);
