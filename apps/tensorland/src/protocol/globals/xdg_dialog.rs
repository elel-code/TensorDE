//! Tensor-owned xdg-dialog wire state and dialog placement policy.

use std::collections::HashMap;

use tensor_util::Size;
use wayland_protocols::xdg::{
    dialog::v1::server::{
        xdg_dialog_v1::{self, XdgDialogV1},
        xdg_wm_dialog_v1::{self, XdgWmDialogV1},
    },
    shell::server::xdg_toplevel::XdgToplevel,
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
};

use super::xdg_shell::Toplevel;
use crate::{
    ecs::ViewPlacement,
    protocol::{
        dispatch::{
            DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
        },
        state::RuntimeState,
    },
};

const DEFAULT_DIALOG_SIZE: Size = Size::new(480, 320);

#[derive(Debug)]
struct DialogEntry {
    resource: Weak<XdgDialogV1>,
    modal: bool,
}

pub(crate) struct XdgDialogProtocol {
    _global: GlobalId,
    dialogs: HashMap<ObjectId, DialogEntry>,
}

impl XdgDialogProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display
                .create_global::<RuntimeState, XdgWmDialogV1, _>(1, XdgDialogGlobalData),
            dialogs: HashMap::new(),
        }
    }

    fn contains(&self, toplevel: &XdgToplevel) -> bool {
        self.dialogs
            .get(&toplevel.id())
            .is_some_and(|entry| entry.resource.upgrade().is_ok())
    }

    pub(in crate::protocol) fn is_modal(&self, toplevel: &XdgToplevel) -> bool {
        self.dialogs
            .get(&toplevel.id())
            .is_some_and(|entry| entry.modal && entry.resource.upgrade().is_ok())
    }

    fn insert(&mut self, toplevel: &XdgToplevel, dialog: &XdgDialogV1) {
        self.dialogs.insert(
            toplevel.id(),
            DialogEntry {
                resource: dialog.downgrade(),
                modal: false,
            },
        );
    }

    fn set_modal(&mut self, toplevel: &XdgToplevel, dialog: &XdgDialogV1, modal: bool) -> bool {
        let Some(entry) = self.dialogs.get_mut(&toplevel.id()) else {
            return false;
        };
        if entry.resource.upgrade().ok().as_ref() != Some(dialog) {
            return false;
        }
        let changed = entry.modal != modal;
        entry.modal = modal;
        changed
    }

    fn remove(&mut self, toplevel: &XdgToplevel, dialog: &XdgDialogV1) -> bool {
        let key = toplevel.id();
        let matches = self
            .dialogs
            .get(&key)
            .and_then(|entry| entry.resource.upgrade().ok())
            .as_ref()
            == Some(dialog);
        if matches {
            self.dialogs.remove(&key);
        }
        matches
    }

    pub(super) fn toplevel_destroyed(&mut self, toplevel: &XdgToplevel) {
        // xdg-dialog specifies that the ancillary object becomes inert. Do not
        // turn normal xdg_toplevel teardown into a protocol error.
        self.dialogs.remove(&toplevel.id());
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct XdgDialogGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct XdgDialogManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct XdgDialogData {
    toplevel: XdgToplevel,
}

impl GlobalDispatchDelegate<XdgWmDialogV1, RuntimeState> for XdgDialogGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<XdgWmDialogV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, XdgDialogManagerData);
    }
}

impl DispatchDelegate<XdgWmDialogV1, RuntimeState> for XdgDialogManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &XdgWmDialogV1,
        request: xdg_wm_dialog_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_wm_dialog_v1::Request::GetXdgDialog { id, toplevel } => {
                let shell_toplevel = state
                    .protocol_globals
                    .xdg_shell
                    .toplevel(&toplevel)
                    .expect("Wayland only dispatches references to a live xdg_toplevel");
                if state.protocol_globals.xdg_dialog.contains(&toplevel) {
                    manager.post_error(
                        xdg_wm_dialog_v1::Error::AlreadyUsed,
                        "xdg_toplevel already has an xdg_dialog_v1 object",
                    );
                    return;
                }
                let dialog = data_init.init(
                    id,
                    XdgDialogData {
                        toplevel: toplevel.clone(),
                    },
                );
                state.protocol_globals.xdg_dialog.insert(&toplevel, &dialog);
                state.sync_xdg_dialog_placement(&shell_toplevel);
            }
            xdg_wm_dialog_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<XdgDialogV1, RuntimeState> for XdgDialogData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        dialog: &XdgDialogV1,
        request: xdg_dialog_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_dialog_v1::Request::SetModal => {
                if state
                    .protocol_globals
                    .xdg_dialog
                    .set_modal(&self.toplevel, dialog, true)
                    && let Some(toplevel) =
                        state.protocol_globals.xdg_shell.toplevel(&self.toplevel)
                {
                    state.sync_xdg_dialog_placement(&toplevel);
                }
            }
            xdg_dialog_v1::Request::UnsetModal => {
                if state
                    .protocol_globals
                    .xdg_dialog
                    .set_modal(&self.toplevel, dialog, false)
                    && let Some(toplevel) =
                        state.protocol_globals.xdg_shell.toplevel(&self.toplevel)
                {
                    state.sync_xdg_dialog_placement(&toplevel);
                }
            }
            xdg_dialog_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, dialog: &XdgDialogV1) {
        if state
            .protocol_globals
            .xdg_dialog
            .remove(&self.toplevel, dialog)
            && let Some(toplevel) = state.protocol_globals.xdg_shell.toplevel(&self.toplevel)
        {
            state.sync_xdg_dialog_placement(&toplevel);
        }
    }
}

impl RuntimeState {
    pub(in crate::protocol) fn sync_xdg_dialog_placement(&mut self, toplevel: &Toplevel) {
        let Some(view_id) = self.view_for_surface(toplevel.wl_surface()) else {
            return;
        };
        if self.active_xdg_toplevel_drag_surface() == Some(toplevel.wl_surface().id()) {
            // Toplevel drag owns placement for the duration of the grab. A
            // dialog request or modal update must not replace its retained
            // floating geometry with Attached/Tiled midway through motion.
            return;
        }
        let is_dialog = self
            .protocol_globals
            .xdg_dialog
            .contains(toplevel.xdg_toplevel());
        let parent_view = is_dialog
            .then(|| toplevel.parent_surface())
            .flatten()
            .and_then(|parent| self.view_for_surface(&parent));
        let placement = match parent_view {
            Some(owner) => ViewPlacement::Attached {
                owner,
                preferred_size: self.xdg_dialog_preferred_size(toplevel, view_id),
            },
            None => ViewPlacement::Tiled,
        };
        let changed = match self.world.set_view_placement(view_id, placement) {
            Ok(changed) => changed,
            Err(error) => {
                tracing::warn!(
                    %error,
                    view_id = view_id.get(),
                    "failed to apply xdg-dialog placement"
                );
                return;
            }
        };
        if changed {
            let _ = self.reflow_default_workspace();
        }

        #[cfg(feature = "tty")]
        if let Some(owner) = parent_view
            && self
                .protocol_globals
                .xdg_dialog
                .is_modal(toplevel.xdg_toplevel())
            && self.world.is_focused(owner)
            && let Some(window) = self.mapped_window_for_view(view_id)
        {
            let _ = self.focus_mapped_window(window, crate::protocol::serial::next_serial());
        }
    }

    fn xdg_dialog_preferred_size(&self, toplevel: &Toplevel, view_id: crate::ecs::ViewId) -> Size {
        if let Some(geometry) = toplevel.geometry()
            && geometry.width > 0
            && geometry.height > 0
        {
            return Size::new(geometry.width, geometry.height);
        }
        if let Some(ViewPlacement::Attached { preferred_size, .. }) =
            self.world.view_placement(view_id)
        {
            return preferred_size;
        }
        DEFAULT_DIALOG_SIZE
    }

    pub(in crate::protocol) fn detach_xdg_dialogs_for_owner(&mut self, owner: &Toplevel) {
        let Some(owner_view) = self.view_for_surface(owner.wl_surface()) else {
            return;
        };
        for child_view in self.world.attached_children(owner_view) {
            let Some(child) = self
                .mapped_window_for_view(child_view)
                .and_then(|window| window.toplevel().cloned())
            else {
                continue;
            };
            if child.parent().as_ref() != Some(owner) {
                continue;
            }
            child.set_parent(None);
            self.sync_xdg_dialog_placement(&child);
        }
    }
}

delegate_global_dispatch!(RuntimeState, XdgWmDialogV1, XdgDialogGlobalData);
delegate_dispatch!(RuntimeState, XdgWmDialogV1, XdgDialogManagerData);
delegate_dispatch!(RuntimeState, XdgDialogV1, XdgDialogData);
