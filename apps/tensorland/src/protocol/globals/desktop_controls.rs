//! Tensor-owned low-frequency desktop control protocols.

mod pointer_warp;
mod system_bell;
mod toplevel_icon;
mod toplevel_tag;

use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
};

use wayland_protocols::xdg::{
    shell::server::xdg_toplevel::XdgToplevel,
    toplevel_icon::v1::server::{
        xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1,
        xdg_toplevel_icon_v1::{self, XdgToplevelIconV1},
    },
};
use wayland_server::{
    DisplayHandle, Resource, Weak,
    backend::{GlobalId, ObjectId},
    protocol::{wl_buffer::WlBuffer, wl_surface::WlSurface},
};

use crate::protocol::state::RuntimeState;

use pointer_warp::PointerWarpGlobalData;
use system_bell::SystemBellGlobalData;
use toplevel_icon::{IconSnapshot, ToplevelIconData, ToplevelIconGlobalData};
use toplevel_tag::ToplevelTagGlobalData;

pub(crate) struct DesktopControls {
    _globals: [GlobalId; 4],
    toplevels: RefCell<HashMap<ObjectId, ToplevelState>>,
    buffer_icons: RefCell<HashMap<ObjectId, HashMap<ObjectId, Weak<XdgToplevelIconV1>>>>,
}

impl DesktopControls {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        use wayland_protocols::{
            wp::pointer_warp::v1::server::wp_pointer_warp_v1::WpPointerWarpV1,
            xdg::{
                system_bell::v1::server::xdg_system_bell_v1::XdgSystemBellV1,
                toplevel_tag::v1::server::xdg_toplevel_tag_manager_v1::XdgToplevelTagManagerV1,
            },
        };

        Self {
            _globals: [
                display.create_global::<RuntimeState, XdgSystemBellV1, _>(1, SystemBellGlobalData),
                display.create_global::<RuntimeState, WpPointerWarpV1, _>(1, PointerWarpGlobalData),
                display.create_global::<RuntimeState, XdgToplevelTagManagerV1, _>(
                    1,
                    ToplevelTagGlobalData,
                ),
                display.create_global::<RuntimeState, XdgToplevelIconManagerV1, _>(
                    1,
                    ToplevelIconGlobalData,
                ),
            ],
            toplevels: RefCell::new(HashMap::new()),
            buffer_icons: RefCell::new(HashMap::new()),
        }
    }

    pub(super) fn set_tag(&self, surface: &WlSurface, tag: String) {
        self.toplevels
            .borrow_mut()
            .entry(surface.id())
            .or_default()
            .tag = Some(tag);
    }

    pub(super) fn set_description(&self, surface: &WlSurface, description: String) {
        self.toplevels
            .borrow_mut()
            .entry(surface.id())
            .or_default()
            .description = Some(description);
    }

    pub(crate) fn remove_surface(&self, surface: &WlSurface) {
        self.toplevels.borrow_mut().remove(&surface.id());
    }

    fn set_pending_icon(&self, surface: &WlSurface, icon: Option<Arc<IconSnapshot>>) -> bool {
        let mut toplevels = self.toplevels.borrow_mut();
        let state = toplevels.entry(surface.id()).or_default();
        state.pending_icon = Some(icon.filter(|icon| !icon.is_empty()));
        let install_hook = !state.icon_commit_hook_installed;
        state.icon_commit_hook_installed = true;
        install_hook
    }

    pub(super) fn commit_icon(&self, surface: &WlSurface) {
        if let Some(state) = self.toplevels.borrow_mut().get_mut(&surface.id()) {
            state.commit_icon();
        }
    }

    pub(super) fn replace_icon_buffer(
        &self,
        icon: &XdgToplevelIconV1,
        buffer: ObjectId,
        replaced: Option<ObjectId>,
    ) {
        let icon_id = icon.id();
        let mut buffer_icons = self.buffer_icons.borrow_mut();
        if let Some(replaced) = replaced {
            remove_buffer_icon(&mut buffer_icons, &replaced, &icon_id);
        }
        buffer_icons
            .entry(buffer)
            .or_default()
            .insert(icon_id, icon.downgrade());
    }

    fn unregister_icon(&self, icon: &XdgToplevelIconV1, data: &ToplevelIconData) {
        let icon_id = icon.id();
        let mut buffer_icons = self.buffer_icons.borrow_mut();
        data.for_each_buffer(|buffer| remove_buffer_icon(&mut buffer_icons, buffer, &icon_id));
    }

    pub(crate) fn shm_buffer_destroyed(&self, buffer: &WlBuffer) {
        let violating_icon = self
            .buffer_icons
            .borrow_mut()
            .remove(&buffer.id())
            .and_then(|icons| icons.into_values().find_map(|icon| icon.upgrade().ok()));
        if let Some(icon) = violating_icon {
            icon.post_error(
                xdg_toplevel_icon_v1::Error::NoBuffer,
                "the icon buffer was destroyed before its xdg_toplevel_icon_v1",
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn toplevel_text(&self, surface: &WlSurface) -> Option<(String, String)> {
        self.toplevels.borrow().get(&surface.id()).map(|state| {
            (
                state.tag.clone().unwrap_or_default(),
                state.description.clone().unwrap_or_default(),
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn toplevel_icon_name(&self, surface: &WlSurface) -> Option<String> {
        self.toplevels
            .borrow()
            .get(&surface.id())?
            .current_icon
            .as_ref()?
            .name()
            .map(str::to_owned)
    }

    #[cfg(test)]
    pub(crate) fn toplevel_icon_buffer_sample(
        &self,
        surface: &WlSurface,
    ) -> Option<(i32, i32, i32, [u8; 4])> {
        self.toplevels
            .borrow()
            .get(&surface.id())?
            .current_icon
            .as_ref()?
            .first_buffer_sample()
    }
}

fn remove_buffer_icon(
    buffer_icons: &mut HashMap<ObjectId, HashMap<ObjectId, Weak<XdgToplevelIconV1>>>,
    buffer: &ObjectId,
    icon: &ObjectId,
) {
    if let Entry::Occupied(mut entry) = buffer_icons.entry(buffer.clone()) {
        entry.get_mut().remove(icon);
        if entry.get().is_empty() {
            entry.remove();
        }
    }
}

#[derive(Debug, Default)]
struct ToplevelState {
    tag: Option<String>,
    description: Option<String>,
    pending_icon: Option<Option<Arc<IconSnapshot>>>,
    current_icon: Option<Arc<IconSnapshot>>,
    icon_commit_hook_installed: bool,
}

impl ToplevelState {
    fn commit_icon(&mut self) {
        if let Some(icon) = self.pending_icon.take() {
            drop(std::mem::replace(&mut self.current_icon, icon));
        }
    }
}

fn toplevel_surface(state: &RuntimeState, toplevel: &XdgToplevel) -> Option<WlSurface> {
    state
        .protocol_globals
        .xdg_shell
        .toplevel(toplevel)
        .map(|surface| surface.wl_surface().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toplevel_text_replaces_without_retaining_history() {
        let mut state = ToplevelState {
            tag: Some("settings".to_owned()),
            ..ToplevelState::default()
        };
        state.tag = Some("composer".to_owned());
        state.description = Some("Compose message".to_owned());

        assert_eq!(state.tag.as_deref(), Some("composer"));
        assert_eq!(state.description.as_deref(), Some("Compose message"));
    }
}
