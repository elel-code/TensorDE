//! Tensor-owned stable xdg-shell wire and role state.
//!
//! Tensor owns the resource indices, role lifecycle, configure queues, popup topology, and
//! client/server double-buffered state directly.

mod lifecycle;
mod positioner;
mod surface;
mod wire;

use std::collections::{HashMap, HashSet};

pub(in crate::protocol) use positioner::PositionerState;
pub(crate) use surface::{Popup, PopupParent, Toplevel};
use wayland_protocols::xdg::shell::server::{
    xdg_popup::XdgPopup, xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel, xdg_wm_base::XdgWmBase,
};
use wayland_server::{
    DisplayHandle, Resource,
    backend::{GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::state::RuntimeState;

pub(in crate::protocol) const XDG_TOPLEVEL_ROLE: &str = "xdg_toplevel";
pub(in crate::protocol) const XDG_POPUP_ROLE: &str = "xdg_popup";
const XDG_SHELL_VERSION: u32 = 7;

#[derive(Clone, Debug, Eq, PartialEq)]
enum SurfaceRole {
    Toplevel(ObjectId),
    Popup(ObjectId),
}

#[derive(Debug)]
struct SurfaceEntry {
    wl_surface: WlSurface,
    base: ObjectId,
    role: Option<SurfaceRole>,
}

#[derive(Debug, Default)]
struct BaseEntry {
    surfaces: HashSet<ObjectId>,
}

/// Exact compositor-thread ownership for every live xdg-shell resource.
pub(crate) struct XdgShellProtocol {
    _global: GlobalId,
    bases: HashMap<ObjectId, BaseEntry>,
    surfaces: HashMap<ObjectId, SurfaceEntry>,
    surface_index: HashMap<ObjectId, ObjectId>,
    toplevels: HashMap<ObjectId, Toplevel>,
    popups: HashMap<ObjectId, Popup>,
}

impl XdgShellProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, XdgWmBase, _>(
                XDG_SHELL_VERSION,
                wire::XdgShellGlobalData,
            ),
            bases: HashMap::new(),
            surfaces: HashMap::new(),
            surface_index: HashMap::new(),
            toplevels: HashMap::new(),
            popups: HashMap::new(),
        }
    }

    fn insert_base(&mut self, base: &XdgWmBase) {
        assert!(
            self.bases.insert(base.id(), BaseEntry::default()).is_none(),
            "xdg_wm_base resource registered twice"
        );
    }

    fn base_has_surfaces(&self, base: &XdgWmBase) -> bool {
        self.bases
            .get(&base.id())
            .is_some_and(|entry| !entry.surfaces.is_empty())
    }

    fn remove_base(&mut self, base: &XdgWmBase) {
        self.bases.remove(&base.id());
    }

    fn insert_surface(
        &mut self,
        xdg_surface: &XdgSurface,
        wl_surface: WlSurface,
        base: &XdgWmBase,
    ) -> bool {
        if self.surface_index.contains_key(&wl_surface.id()) {
            return false;
        }
        let id = xdg_surface.id();
        let wl_id = wl_surface.id();
        let base_id = base.id();
        let Some(base_entry) = self.bases.get_mut(&base_id) else {
            return false;
        };
        base_entry.surfaces.insert(id.clone());
        self.surface_index.insert(wl_id, id.clone());
        assert!(
            self.surfaces
                .insert(
                    id,
                    SurfaceEntry {
                        wl_surface,
                        base: base_id,
                        role: None,
                    },
                )
                .is_none(),
            "xdg_surface resource registered twice"
        );
        true
    }

    fn surface_has_role(&self, surface: &XdgSurface) -> bool {
        self.surfaces
            .get(&surface.id())
            .is_some_and(|entry| entry.role.is_some())
    }

    fn insert_toplevel(&mut self, surface: &XdgSurface, toplevel: Toplevel) -> bool {
        let Some(entry) = self.surfaces.get_mut(&surface.id()) else {
            return false;
        };
        if entry.role.is_some() {
            return false;
        }
        let id = toplevel.protocol_id();
        entry.role = Some(SurfaceRole::Toplevel(id.clone()));
        assert!(self.toplevels.insert(id, toplevel).is_none());
        true
    }

    fn insert_popup(&mut self, surface: &XdgSurface, popup: Popup) -> bool {
        let Some(entry) = self.surfaces.get_mut(&surface.id()) else {
            return false;
        };
        if entry.role.is_some() {
            return false;
        }
        let id = popup.protocol_id();
        entry.role = Some(SurfaceRole::Popup(id.clone()));
        assert!(self.popups.insert(id, popup).is_none());
        true
    }

    pub(in crate::protocol) fn toplevel(&self, resource: &XdgToplevel) -> Option<Toplevel> {
        self.toplevels.get(&resource.id()).cloned()
    }

    pub(in crate::protocol) fn popup(&self, resource: &XdgPopup) -> Option<Popup> {
        self.popups.get(&resource.id()).cloned()
    }

    pub(in crate::protocol) fn toplevel_for_surface(
        &self,
        surface: &WlSurface,
    ) -> Option<Toplevel> {
        let xdg = self.surface_index.get(&surface.id())?;
        let SurfaceRole::Toplevel(role) = self.surfaces.get(xdg)?.role.as_ref()? else {
            return None;
        };
        self.toplevels.get(role).cloned()
    }

    pub(in crate::protocol) fn popup_for_surface(&self, surface: &WlSurface) -> Option<Popup> {
        let xdg = self.surface_index.get(&surface.id())?;
        let SurfaceRole::Popup(role) = self.surfaces.get(xdg)?.role.as_ref()? else {
            return None;
        };
        self.popups.get(role).cloned()
    }

    fn parent_for_surface(&self, surface: &XdgSurface) -> Option<PopupParent> {
        match self.surfaces.get(&surface.id())?.role.as_ref()? {
            SurfaceRole::Toplevel(id) => self
                .toplevels
                .get(id)
                .map(|toplevel| PopupParent::Surface(toplevel.wl_surface().clone())),
            SurfaceRole::Popup(id) => self.popups.get(id).cloned().map(PopupParent::Popup),
        }
    }

    fn remove_toplevel(&mut self, resource: &XdgToplevel) -> Option<Toplevel> {
        let id = resource.id();
        let toplevel = self.toplevels.remove(&id)?;
        toplevel.mark_destroyed();
        if let Some(xdg) = self.surface_index.get(&toplevel.wl_surface().id())
            && let Some(entry) = self.surfaces.get_mut(xdg)
            && entry.role.as_ref() == Some(&SurfaceRole::Toplevel(id))
        {
            entry.role = None;
        }
        Some(toplevel)
    }

    fn remove_popup(&mut self, resource: &XdgPopup) -> Option<Popup> {
        let id = resource.id();
        let popup = self.popups.remove(&id)?;
        popup.mark_destroyed();
        if let Some(xdg) = self.surface_index.get(&popup.wl_surface().id())
            && let Some(entry) = self.surfaces.get_mut(xdg)
            && entry.role.as_ref() == Some(&SurfaceRole::Popup(id))
        {
            entry.role = None;
        }
        Some(popup)
    }

    fn remove_surface_resource(&mut self, resource: &XdgSurface) {
        self.remove_surface_id(&resource.id());
    }

    pub(in crate::protocol) fn remove_wl_surface(&mut self, surface: &WlSurface) {
        if let Some(id) = self.surface_index.get(&surface.id()).cloned() {
            self.remove_surface_id(&id);
        }
    }

    fn remove_surface_id(&mut self, id: &ObjectId) {
        let Some(entry) = self.surfaces.remove(id) else {
            return;
        };
        self.surface_index.remove(&entry.wl_surface.id());
        if let Some(base) = self.bases.get_mut(&entry.base) {
            base.surfaces.remove(id);
        }
        match entry.role {
            Some(SurfaceRole::Toplevel(role)) => {
                if let Some(toplevel) = self.toplevels.remove(&role) {
                    toplevel.mark_destroyed();
                }
            }
            Some(SurfaceRole::Popup(role)) => {
                if let Some(popup) = self.popups.remove(&role) {
                    popup.mark_destroyed();
                }
            }
            None => {}
        }
    }

    pub(in crate::protocol) fn is_toplevel(&self, surface: &WlSurface) -> bool {
        self.toplevel_for_surface(surface).is_some()
    }
}
