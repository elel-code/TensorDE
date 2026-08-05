// Derived from Smithay's popup topology rules at commit c0aa71d.
// Smithay's copyright notice and MIT terms are in LICENSES/Smithay-MIT.txt.

use std::sync::{Arc, Mutex};

use thiserror::Error;
use wayland_protocols::xdg::shell::server::xdg_popup::XdgPopup;
use wayland_server::{Resource, Weak, protocol::wl_surface::WlSurface};

use crate::protocol::serial::Serial;

use super::registry::PopupKind;

#[derive(Debug, Error)]
pub(crate) enum PopupGrabError {
    #[error("popup resource is no longer alive")]
    DeadResource,
    #[error("the parent popup was already dismissed")]
    ParentDismissed,
    #[error("popup is not a child of the topmost grabbed popup")]
    NotTheTopmostPopup,
}

#[derive(Debug, Default)]
struct PopupGrabInternal {
    serial: Option<Serial>,
    active: Vec<GrabPopup>,
    dismissed: Vec<GrabPopup>,
}

#[derive(Clone, Debug)]
struct GrabPopup {
    surface: WlSurface,
    role: Weak<XdgPopup>,
}

impl GrabPopup {
    fn new(popup: &PopupKind) -> Self {
        Self {
            surface: popup.wl_surface().clone(),
            role: popup.0.xdg_popup().downgrade(),
        }
    }

    fn alive(&self) -> bool {
        self.surface.is_alive() && self.role.upgrade().is_ok()
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct PopupGrabInner(Arc<Mutex<PopupGrabInternal>>);

impl PopupGrabInner {
    pub(super) fn has_any_grabs(&self) -> bool {
        let state = self.0.lock().unwrap();
        !state.active.is_empty() || !state.dismissed.is_empty()
    }

    pub(super) fn has_active_grabs(&self) -> bool {
        self.0.lock().unwrap().active.iter().any(GrabPopup::alive)
    }

    fn current_grab(&self) -> Option<WlSurface> {
        self.0
            .lock()
            .unwrap()
            .active
            .iter()
            .rev()
            .find(|popup| popup.alive())
            .map(|popup| popup.surface.clone())
    }

    pub(super) fn cleanup(&self) {
        let mut state = self.0.lock().unwrap();
        let mut index = 0;
        while index < state.active.len() {
            if state.active[index].alive() {
                index += 1;
            } else {
                let popup = state.active.remove(index);
                state.dismissed.push(popup);
            }
        }
        state.dismissed.retain(GrabPopup::alive);
    }

    pub(super) fn grab(
        &self,
        popup: &PopupKind,
        serial: Serial,
    ) -> Result<Option<Serial>, PopupGrabError> {
        let parent = popup.parent().ok_or(PopupGrabError::DeadResource)?;
        self.cleanup();
        let mut state = self.0.lock().unwrap();
        if let Some(current) = state.active.iter().rev().find(|popup| popup.alive())
            && current.surface != parent
        {
            if state.dismissed.iter().any(|popup| popup.surface == parent) {
                return Err(PopupGrabError::ParentDismissed);
            }
            return Err(PopupGrabError::NotTheTopmostPopup);
        }
        state.active.push(GrabPopup::new(popup));
        Ok(state.serial.replace(serial))
    }

    fn ungrab(&self) -> Option<WlSurface> {
        let mut state = self.0.lock().unwrap();
        let dismissed = state.active.first().map(|popup| popup.surface.clone());
        let active = std::mem::take(&mut state.active);
        state.dismissed.extend(active);
        dismissed
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PopupGrab {
    root: WlSurface,
    serial: Serial,
    previous_serial: Option<Serial>,
    popups: PopupGrabInner,
}

impl PopupGrab {
    pub(super) fn new(
        popups: PopupGrabInner,
        root: WlSurface,
        serial: Serial,
        previous_serial: Option<Serial>,
    ) -> Self {
        Self {
            root,
            serial,
            previous_serial,
            popups,
        }
    }

    pub(crate) const fn serial(&self) -> Serial {
        self.serial
    }

    pub(crate) const fn previous_serial(&self) -> Option<Serial> {
        self.previous_serial
    }

    pub(crate) fn has_ended(&self) -> bool {
        !self.root.is_alive() || !self.popups.has_active_grabs()
    }

    pub(crate) fn current_grab(&self) -> WlSurface {
        self.popups
            .current_grab()
            .unwrap_or_else(|| self.root.clone())
    }

    pub(crate) fn allows(&self, surface: &WlSurface) -> bool {
        self.current_grab().id().same_client_as(&surface.id())
    }

    pub(crate) fn ungrab(&self) -> Option<(WlSurface, WlSurface)> {
        self.popups
            .ungrab()
            .map(|dismissed| (self.root.clone(), dismissed))
    }
}
