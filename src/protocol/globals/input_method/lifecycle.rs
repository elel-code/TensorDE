//! Resource ownership and child-object lifecycle for input-method-v2.

use tracing::info;
use wayland_protocols::wp::text_input::zv3::server::zwp_text_input_v3::ZwpTextInputV3;
use wayland_protocols_misc::zwp_input_method_v2::server::{
    zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2,
    zwp_input_method_v2::ZwpInputMethodV2, zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2,
};
use wayland_server::{
    Resource,
    backend::{ClientId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use super::{
    InputMethodInstance, InputMethodKeyboardGrab, InputMethodProtocol, InputPopupSurface,
    MAX_INPUT_METHOD_KEYBOARD_GRABS, MAX_INPUT_POPUPS, PendingInputMethodState,
    PendingTextInputState, TextInputInstance, TextInputState,
};
use tensor_util::LogicalRect;

impl InputMethodProtocol {
    fn focused_surface(&self) -> Option<WlSurface> {
        self.focused_surface.as_ref()?.upgrade().ok()
    }

    pub(super) fn focused_client(&self) -> Option<ClientId> {
        self.focused_surface()?.client().map(|client| client.id())
    }

    pub(super) fn input_method_resource(&self) -> Option<ZwpInputMethodV2> {
        self.input_method.as_ref()?.resource.upgrade().ok()
    }

    pub(super) fn active_text_input_resource(&self) -> Option<ZwpTextInputV3> {
        let id = self.active_text_input.as_ref()?;
        self.text_inputs.get(id)?.resource.upgrade().ok()
    }

    pub(crate) fn set_focus(&mut self, focus: Option<&WlSurface>) {
        let previous = self.focused_surface();
        if previous.as_ref().map(Resource::id) == focus.map(Resource::id) {
            return;
        }
        if let Some(previous) = previous.as_ref()
            && let Some(previous_client) = previous.client().map(|client| client.id())
        {
            for instance in self.text_inputs.values() {
                if instance.client == previous_client
                    && let Ok(resource) = instance.resource.upgrade()
                {
                    resource.leave(previous);
                }
            }
        }
        if self.active_text_input.take().is_some() {
            self.deactivate_input_method();
        }
        for instance in self.text_inputs.values_mut() {
            instance.pending = PendingTextInputState::default();
            instance.current = TextInputState::default();
        }
        self.focused_surface = focus.map(Resource::downgrade);
        if let Some(focus) = focus {
            let Some(client) = focus.client().map(|client| client.id()) else {
                return;
            };
            for instance in self.text_inputs.values() {
                if instance.client == client
                    && let Ok(resource) = instance.resource.upgrade()
                {
                    resource.enter(focus);
                }
            }
        }
    }

    pub(crate) fn surface_destroyed(&mut self, surface: &WlSurface) {
        if self
            .focused_surface
            .as_ref()
            .is_some_and(|focused| focused.id() == surface.id())
        {
            self.set_focus(None);
        }
        self.popups.retain(|popup| {
            if popup.surface.id() != surface.id() {
                return true;
            }
            if let Ok(role) = popup.role.upgrade() {
                role.post_error(
                    0_u32,
                    "input popup wl_surface was destroyed before its role",
                );
            }
            false
        });
    }

    pub(super) fn register_text_input(&mut self, resource: &ZwpTextInputV3, client: ClientId) {
        let id = resource.id();
        self.text_inputs.insert(
            id,
            TextInputInstance {
                resource: resource.downgrade(),
                client: client.clone(),
                serial: 0,
                pending: PendingTextInputState::default(),
                current: TextInputState::default(),
            },
        );
        if self.focused_client().as_ref() == Some(&client)
            && let Some(surface) = self.focused_surface()
        {
            resource.enter(&surface);
        }
    }

    pub(super) fn remove_text_input(&mut self, resource: &ZwpTextInputV3) {
        let id = resource.id();
        self.text_inputs.remove(&id);
        if self.active_text_input.as_ref() == Some(&id) {
            self.active_text_input = None;
            self.deactivate_input_method();
        }
    }

    pub(super) fn register_input_method(&mut self, resource: &ZwpInputMethodV2) {
        self.cleanup_input_method();
        if self.input_method.is_some() {
            info!("input-method client rejected because the seat already has an owner");
            resource.unavailable();
            self.unavailable_input_methods.insert(resource.id());
            return;
        }
        self.input_method = Some(InputMethodInstance {
            resource: resource.downgrade(),
            serial: 0,
            pending: PendingInputMethodState::default(),
        });
        info!(
            active_text_input = self.active_text_input.is_some(),
            "input-method client registered"
        );
        if self.active_text_input.is_some() {
            self.activate_input_method();
        }
    }

    pub(super) fn remove_input_method(&mut self, resource: &ZwpInputMethodV2) -> bool {
        let id = resource.id();
        self.unavailable_input_methods.remove(&id);
        if self
            .input_method
            .as_ref()
            .is_some_and(|instance| instance.resource.id() == id)
        {
            self.input_method = None;
            self.remove_owner_state(&id);
            info!("input-method client disconnected");
            true
        } else {
            false
        }
    }

    fn cleanup_input_method(&mut self) {
        let stale = self.input_method.as_ref().and_then(|instance| {
            instance
                .resource
                .upgrade()
                .is_err()
                .then(|| instance.resource.id())
        });
        if let Some(id) = stale {
            self.input_method = None;
            self.remove_owner_state(&id);
        }
    }

    fn remove_owner_state(&mut self, owner: &ObjectId) {
        self.popups.retain(|popup| &popup.owner != owner);
        self.keyboard_grabs.retain(|grab| &grab.owner != owner);
        if self.active_keyboard_grab.as_ref().is_some_and(|grab| {
            !self
                .keyboard_grabs
                .iter()
                .any(|item| item.resource.id() == grab.id())
        }) {
            self.active_keyboard_grab = None;
        }
    }

    pub(super) fn input_method_available(&self, resource: &ZwpInputMethodV2) -> bool {
        !self.unavailable_input_methods.contains(&resource.id())
            && self
                .input_method
                .as_ref()
                .is_some_and(|instance| instance.resource.id() == resource.id())
    }

    pub(super) fn register_popup(
        &mut self,
        owner: &ZwpInputMethodV2,
        role: &ZwpInputPopupSurfaceV2,
        surface: WlSurface,
    ) -> bool {
        if !self.input_method_available(owner) {
            return true;
        }
        self.popups.retain(|popup| popup.role.upgrade().is_ok());
        if self.popups.len() == MAX_INPUT_POPUPS {
            return false;
        }
        let active_rectangle = self.active_text_input.as_ref().and_then(|active| {
            self.text_inputs
                .get(active)
                .and_then(|instance| instance.current.cursor_rectangle)
        });
        self.popups.push(InputPopupSurface {
            role: role.downgrade(),
            surface: surface.clone(),
            owner: owner.id(),
        });
        info!(
            surface = surface.id().protocol_id(),
            active_text_input = self.active_text_input.is_some(),
            "input-method popup registered"
        );
        if let Some(rectangle) = active_rectangle {
            role.text_input_rectangle(
                rectangle.loc.x,
                rectangle.loc.y,
                rectangle.size.w,
                rectangle.size.h,
            );
        }
        true
    }

    pub(super) fn remove_popup(&mut self, role: &ZwpInputPopupSurfaceV2) {
        self.popups.retain(|popup| popup.role.id() != role.id());
    }

    pub(super) fn register_keyboard_grab(
        &mut self,
        owner: &ZwpInputMethodV2,
        grab: &ZwpInputMethodKeyboardGrabV2,
    ) -> bool {
        if !self.input_method_available(owner) {
            return true;
        }
        self.keyboard_grabs
            .retain(|current| current.resource.upgrade().is_ok());
        if self.keyboard_grabs.len() == MAX_INPUT_METHOD_KEYBOARD_GRABS {
            return false;
        }
        self.keyboard_grabs.push(InputMethodKeyboardGrab {
            resource: grab.downgrade(),
            owner: owner.id(),
        });
        self.active_keyboard_grab = Some(grab.downgrade());
        info!("input-method keyboard grab registered");
        true
    }

    pub(super) fn remove_keyboard_grab(&mut self, grab: &ZwpInputMethodKeyboardGrabV2) -> bool {
        self.keyboard_grabs
            .retain(|current| current.resource.id() != grab.id());
        if self
            .active_keyboard_grab
            .as_ref()
            .is_some_and(|current| current.id() == grab.id())
        {
            self.active_keyboard_grab = None;
            true
        } else {
            false
        }
    }

    pub(super) fn destroy_children(
        &mut self,
        owner: &ZwpInputMethodV2,
        mut destroy: impl FnMut(ObjectId),
    ) -> bool {
        let owner = owner.id();
        self.popups.retain(|popup| {
            if popup.owner != owner {
                return true;
            }
            if let Ok(role) = popup.role.upgrade() {
                destroy(role.id());
            }
            false
        });
        let active = self.active_keyboard_grab.as_ref().map(|grab| grab.id());
        let mut removed_active_grab = false;
        self.keyboard_grabs.retain(|grab| {
            if grab.owner != owner {
                return true;
            }
            removed_active_grab |= active.as_ref() == Some(&grab.resource.id());
            if let Ok(resource) = grab.resource.upgrade() {
                destroy(resource.id());
            }
            false
        });
        if removed_active_grab {
            self.active_keyboard_grab = None;
        }
        removed_active_grab
    }

    pub(crate) fn keyboard_grab_active(&self) -> bool {
        self.active_keyboard_grab
            .as_ref()
            .is_some_and(|grab| grab.upgrade().is_ok())
    }

    pub(crate) fn keyboard_grab_resource(&self) -> Option<ZwpInputMethodKeyboardGrabV2> {
        self.active_keyboard_grab.as_ref()?.upgrade().ok()
    }

    pub(crate) fn popup_parent(&self, surface: &WlSurface) -> Option<WlSurface> {
        if !self
            .popups
            .iter()
            .any(|popup| popup.surface.id() == surface.id() && popup.role.upgrade().is_ok())
        {
            return None;
        }
        self.focused_surface()
    }

    pub(crate) fn active_popup_context(&self) -> Option<(WlSurface, LogicalRect<i32>)> {
        let active = self.active_text_input.as_ref()?;
        let instance = self.text_inputs.get(active)?;
        Some((
            self.focused_surface()?,
            instance
                .current
                .cursor_rectangle
                .unwrap_or_else(LogicalRect::zero),
        ))
    }

    pub(crate) fn for_each_visible_popup(&self, mut visit: impl FnMut(&InputPopupSurface)) {
        if self.active_text_input.is_none() {
            return;
        }
        for popup in &self.popups {
            if popup.surface.is_alive() && popup.role.upgrade().is_ok() {
                visit(popup);
            }
        }
    }

    pub(crate) fn for_each_popup(&self, mut visit: impl FnMut(&InputPopupSurface)) {
        for popup in &self.popups {
            if popup.surface.is_alive() && popup.role.upgrade().is_ok() {
                visit(popup);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn child_ids(&self) -> Vec<ObjectId> {
        self.popups
            .iter()
            .filter_map(|popup| popup.role.upgrade().ok().map(|role| role.id()))
            .chain(
                self.keyboard_grabs
                    .iter()
                    .filter_map(|grab| grab.resource.upgrade().ok().map(|resource| resource.id())),
            )
            .collect()
    }
}
