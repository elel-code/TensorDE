//! Direct stable xdg-shell dispatch.

use std::sync::Mutex;

use crate::protocol::globals::compositor::{
    self, BufferAssignment, SurfaceAttributes, get_role, with_states,
};
use tensor_util::{Point, Rect, Size};
use wayland_protocols::xdg::shell::server::{
    xdg_popup::{self, XdgPopup},
    xdg_positioner::{self, XdgPositioner},
    xdg_surface::{self, XdgSurface},
    xdg_toplevel::{self, XdgToplevel},
    xdg_wm_base::{self, XdgWmBase},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, WEnum, backend::ClientId,
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::{PopupKind, RuntimeState, surface_has_buffer},
};

use super::{
    Popup, PositionerState, Toplevel, XDG_POPUP_ROLE, XDG_TOPLEVEL_ROLE,
    surface::{PopupCommit, ToplevelCommit},
};

#[derive(Debug)]
pub(in crate::protocol) struct XdgShellGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct XdgWmBaseData;

#[derive(Debug)]
pub(in crate::protocol) struct XdgSurfaceData {
    wl_surface: WlSurface,
    wm_base: XdgWmBase,
}

#[derive(Debug)]
pub(in crate::protocol) struct XdgRoleData {
    xdg_surface: XdgSurface,
}

#[derive(Debug, Default)]
pub(in crate::protocol) struct PositionerData {
    state: Mutex<PositionerState>,
}

impl GlobalDispatchDelegate<XdgWmBase, RuntimeState> for XdgShellGlobalData {
    fn bind(
        &self,
        state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<XdgWmBase>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let base = data_init.init(resource, XdgWmBaseData);
        state.protocol_globals.xdg_shell.insert_base(&base);
    }
}

impl DispatchDelegate<XdgWmBase, RuntimeState> for XdgWmBaseData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        base: &XdgWmBase,
        request: xdg_wm_base::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_wm_base::Request::Destroy => {
                if state.protocol_globals.xdg_shell.base_has_surfaces(base) {
                    base.post_error(
                        xdg_wm_base::Error::DefunctSurfaces,
                        "xdg_wm_base must outlive every xdg_surface created from it",
                    );
                }
            }
            xdg_wm_base::Request::CreatePositioner { id } => {
                data_init.init(id, PositionerData::default());
            }
            xdg_wm_base::Request::GetXdgSurface { id, surface } => {
                if get_role(&surface).is_some()
                    || surface_has_buffer_or_pending(&surface)
                    || state
                        .protocol_globals
                        .xdg_shell
                        .surface_index
                        .contains_key(&surface.id())
                {
                    base.post_error(
                        xdg_wm_base::Error::Role,
                        "wl_surface already has a role, content, or xdg_surface",
                    );
                    return;
                }
                let xdg_surface = data_init.init(
                    id,
                    XdgSurfaceData {
                        wl_surface: surface.clone(),
                        wm_base: base.clone(),
                    },
                );
                if !state
                    .protocol_globals
                    .xdg_shell
                    .insert_surface(&xdg_surface, surface, base)
                {
                    base.post_error(
                        xdg_wm_base::Error::Role,
                        "wl_surface already has an xdg_surface",
                    );
                }
            }
            xdg_wm_base::Request::Pong { .. } => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, base: &XdgWmBase) {
        state.protocol_globals.xdg_shell.remove_base(base);
    }
}

impl DispatchDelegate<XdgPositioner, RuntimeState> for PositionerData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        positioner: &XdgPositioner,
        request: xdg_positioner::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let mut state = self.state.lock().unwrap();
        match request {
            xdg_positioner::Request::Destroy => {}
            xdg_positioner::Request::SetSize { width, height } => {
                if width <= 0 || height <= 0 {
                    invalid_positioner(positioner, "positioner size must be positive");
                } else {
                    state.size = Size::new(width as u32, height as u32);
                }
            }
            xdg_positioner::Request::SetAnchorRect {
                x,
                y,
                width,
                height,
            } => {
                if width <= 0 || height <= 0 {
                    invalid_positioner(positioner, "anchor rectangle must be positive");
                } else {
                    state.anchor_rect = Rect::new(x, y, width as u32, height as u32);
                }
            }
            xdg_positioner::Request::SetAnchor { anchor } => {
                let WEnum::Value(anchor) = anchor else {
                    invalid_positioner(positioner, "unknown anchor value");
                    return;
                };
                state.anchor = anchor;
            }
            xdg_positioner::Request::SetGravity { gravity } => {
                let WEnum::Value(gravity) = gravity else {
                    invalid_positioner(positioner, "unknown gravity value");
                    return;
                };
                state.gravity = gravity;
            }
            xdg_positioner::Request::SetConstraintAdjustment {
                constraint_adjustment,
            } => {
                let WEnum::Value(adjustment) = constraint_adjustment else {
                    invalid_positioner(positioner, "unknown constraint adjustment bits");
                    return;
                };
                state.adjustment = adjustment;
            }
            xdg_positioner::Request::SetOffset { x, y } => state.offset = Point::new(x, y),
            xdg_positioner::Request::SetReactive => state.reactive = true,
            xdg_positioner::Request::SetParentSize {
                parent_width,
                parent_height,
            } => {
                if parent_width <= 0 || parent_height <= 0 {
                    invalid_positioner(positioner, "parent size must be positive");
                } else {
                    state.parent_size = Some(Size::new(parent_width as u32, parent_height as u32));
                }
            }
            xdg_positioner::Request::SetParentConfigure { serial } => {
                state.parent_configure = Some(serial);
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<XdgSurface, RuntimeState> for XdgSurfaceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        xdg_surface: &XdgSurface,
        request: xdg_surface::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_surface::Request::Destroy => {
                if state
                    .protocol_globals
                    .xdg_shell
                    .surface_has_role(xdg_surface)
                {
                    xdg_surface.post_error(
                        xdg_surface::Error::DefunctRoleObject,
                        "destroy the xdg_toplevel or xdg_popup before xdg_surface",
                    );
                }
            }
            xdg_surface::Request::GetToplevel { id } => {
                if state
                    .protocol_globals
                    .xdg_shell
                    .surface_has_role(xdg_surface)
                {
                    xdg_surface.post_error(
                        xdg_surface::Error::AlreadyConstructed,
                        "xdg_surface already has a role object",
                    );
                    return;
                }
                if compositor::give_role(&self.wl_surface, XDG_TOPLEVEL_ROLE).is_err() {
                    self.wm_base.post_error(
                        xdg_wm_base::Error::Role,
                        "wl_surface already has another permanent role",
                    );
                    return;
                }
                let resource = data_init.init(
                    id,
                    XdgRoleData {
                        xdg_surface: xdg_surface.clone(),
                    },
                );
                let toplevel =
                    Toplevel::new(self.wl_surface.clone(), xdg_surface.clone(), resource);
                if !state
                    .protocol_globals
                    .xdg_shell
                    .insert_toplevel(xdg_surface, toplevel.clone())
                {
                    xdg_surface.post_error(
                        xdg_surface::Error::AlreadyConstructed,
                        "xdg_surface already has a role object",
                    );
                    return;
                }
                compositor::add_pre_commit_hook::<RuntimeState, _>(
                    &self.wl_surface,
                    toplevel_pre_commit,
                );
                state.register_toplevel(toplevel);
            }
            xdg_surface::Request::GetPopup {
                id,
                parent,
                positioner,
            } => {
                if state
                    .protocol_globals
                    .xdg_shell
                    .surface_has_role(xdg_surface)
                {
                    xdg_surface.post_error(
                        xdg_surface::Error::AlreadyConstructed,
                        "xdg_surface already has a role object",
                    );
                    return;
                }
                let positioner = positioner
                    .data::<PositionerData>()
                    .map(|data| *data.state.lock().unwrap())
                    .unwrap_or_default();
                if !positioner.complete() {
                    self.wm_base.post_error(
                        xdg_wm_base::Error::InvalidPositioner,
                        "xdg_positioner requires positive size and anchor rectangle",
                    );
                    return;
                }
                let parent = match parent {
                    Some(parent) => {
                        let Some(parent) =
                            state.protocol_globals.xdg_shell.parent_for_surface(&parent)
                        else {
                            self.wm_base.post_error(
                                xdg_wm_base::Error::InvalidPopupParent,
                                "popup parent has no live xdg role",
                            );
                            return;
                        };
                        Some(parent)
                    }
                    None => None,
                };
                if compositor::give_role(&self.wl_surface, XDG_POPUP_ROLE).is_err() {
                    self.wm_base.post_error(
                        xdg_wm_base::Error::Role,
                        "wl_surface already has another permanent role",
                    );
                    return;
                }
                let resource = data_init.init(
                    id,
                    XdgRoleData {
                        xdg_surface: xdg_surface.clone(),
                    },
                );
                let popup = Popup::new(
                    self.wl_surface.clone(),
                    xdg_surface.clone(),
                    resource,
                    self.wm_base.clone(),
                    positioner,
                    parent,
                );
                if !state
                    .protocol_globals
                    .xdg_shell
                    .insert_popup(xdg_surface, popup.clone())
                {
                    xdg_surface.post_error(
                        xdg_surface::Error::AlreadyConstructed,
                        "xdg_surface already has a role object",
                    );
                    return;
                }
                compositor::add_pre_commit_hook::<RuntimeState, _>(
                    &self.wl_surface,
                    popup_pre_commit,
                );
                state.register_xdg_popup(popup);
            }
            xdg_surface::Request::SetWindowGeometry {
                x,
                y,
                width,
                height,
            } => {
                if width <= 0 || height <= 0 {
                    xdg_surface.post_error(
                        xdg_surface::Error::InvalidSize,
                        "window geometry width and height must be positive",
                    );
                    return;
                }
                let geometry = Rect::new(x, y, width as u32, height as u32);
                if let Some(toplevel) = state
                    .protocol_globals
                    .xdg_shell
                    .toplevel_for_surface(&self.wl_surface)
                {
                    toplevel.set_window_geometry(geometry);
                } else if let Some(popup) = state
                    .protocol_globals
                    .xdg_shell
                    .popup_for_surface(&self.wl_surface)
                {
                    popup.set_window_geometry(geometry);
                } else {
                    xdg_surface.post_error(
                        xdg_surface::Error::NotConstructed,
                        "xdg_surface has no role object",
                    );
                }
            }
            xdg_surface::Request::AckConfigure { serial } => {
                let acknowledged = state
                    .protocol_globals
                    .xdg_shell
                    .toplevel_for_surface(&self.wl_surface)
                    .is_some_and(|toplevel| toplevel.ack_configure(serial))
                    || state
                        .protocol_globals
                        .xdg_shell
                        .popup_for_surface(&self.wl_surface)
                        .is_some_and(|popup| popup.ack_configure(serial));
                if !acknowledged {
                    xdg_surface.post_error(
                        xdg_surface::Error::InvalidSerial,
                        format!("unknown or consumed configure serial {serial}"),
                    );
                }
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &XdgSurface) {
        state
            .protocol_globals
            .xdg_shell
            .remove_surface_resource(resource);
    }
}

impl DispatchDelegate<XdgToplevel, RuntimeState> for XdgRoleData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &XdgToplevel,
        request: xdg_toplevel::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let Some(toplevel) = state.protocol_globals.xdg_shell.toplevel(resource) else {
            return;
        };
        match request {
            xdg_toplevel::Request::Destroy => {}
            xdg_toplevel::Request::SetParent { parent } => {
                let parent = match parent {
                    Some(parent) => {
                        let Some(parent) = state.protocol_globals.xdg_shell.toplevel(&parent)
                        else {
                            resource.post_error(
                                xdg_toplevel::Error::InvalidParent,
                                "parent xdg_toplevel is no longer live",
                            );
                            return;
                        };
                        Some(parent)
                    }
                    None => None,
                };
                if !toplevel.set_parent(parent) {
                    resource.post_error(
                        xdg_toplevel::Error::InvalidParent,
                        "toplevel parent relationship is cyclic",
                    );
                }
            }
            xdg_toplevel::Request::SetTitle { title } => {
                if toplevel.set_title(title) {
                    state.refresh_foreign_toplevel_metadata(&toplevel);
                }
            }
            xdg_toplevel::Request::SetAppId { app_id } => {
                if toplevel.set_app_id(app_id) {
                    state.refresh_foreign_toplevel_metadata(&toplevel);
                }
            }
            xdg_toplevel::Request::ShowWindowMenu { .. }
            | xdg_toplevel::Request::Move { .. }
            | xdg_toplevel::Request::SetMinimized => {}
            xdg_toplevel::Request::Resize { edges, .. } => {
                if matches!(edges, WEnum::Unknown(_)) {
                    resource.post_error(
                        xdg_toplevel::Error::InvalidResizeEdge,
                        "unknown resize edge",
                    );
                }
            }
            xdg_toplevel::Request::SetMaxSize { width, height } => {
                if width < 0 || height < 0 {
                    resource.post_error(
                        xdg_toplevel::Error::InvalidSize,
                        "maximum size cannot be negative",
                    );
                } else {
                    toplevel.set_max_size(Size::new(width as u32, height as u32));
                }
            }
            xdg_toplevel::Request::SetMinSize { width, height } => {
                if width < 0 || height < 0 {
                    resource.post_error(
                        xdg_toplevel::Error::InvalidSize,
                        "minimum size cannot be negative",
                    );
                } else {
                    toplevel.set_min_size(Size::new(width as u32, height as u32));
                }
            }
            xdg_toplevel::Request::SetMaximized
            | xdg_toplevel::Request::UnsetMaximized
            | xdg_toplevel::Request::SetFullscreen { .. }
            | xdg_toplevel::Request::UnsetFullscreen => {
                toplevel.send_configure();
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &XdgToplevel) {
        if let Some(toplevel) = state.protocol_globals.xdg_shell.remove_toplevel(resource) {
            state.xdg_toplevel_destroyed(toplevel);
        }
        let _ = &self.xdg_surface;
    }
}

impl DispatchDelegate<XdgPopup, RuntimeState> for XdgRoleData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &XdgPopup,
        request: xdg_popup::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let Some(popup) = state.protocol_globals.xdg_shell.popup(resource) else {
            return;
        };
        match request {
            xdg_popup::Request::Destroy => {}
            xdg_popup::Request::Grab { seat, serial } => popup.request_grab(seat, serial),
            xdg_popup::Request::Reposition { positioner, token } => {
                let positioner = positioner
                    .data::<PositionerData>()
                    .map(|data| *data.state.lock().unwrap())
                    .unwrap_or_default();
                if !positioner.complete() {
                    popup.wm_base().post_error(
                        xdg_wm_base::Error::InvalidPositioner,
                        "reposition requires a complete xdg_positioner",
                    );
                    return;
                }
                popup.update_positioner(positioner);
                state.unconstrain_popup(&PopupKind::from(popup.clone()));
                popup.send_repositioned(token);
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &XdgPopup) {
        if state
            .protocol_globals
            .xdg_shell
            .remove_popup(resource)
            .is_some()
        {
            state.xdg_popup_destroyed();
        }
        let _ = &self.xdg_surface;
    }
}

fn invalid_positioner(positioner: &XdgPositioner, message: &'static str) {
    positioner.post_error(xdg_positioner::Error::InvalidInput, message);
}

fn surface_has_buffer_or_pending(surface: &WlSurface) -> bool {
    surface_has_buffer(surface)
        || with_states(surface, |states| {
            let mut attributes = states.cached_state.get::<SurfaceAttributes>();
            matches!(
                attributes.pending().buffer,
                Some(BufferAssignment::NewBuffer(_))
            ) || matches!(
                attributes.current().buffer,
                Some(BufferAssignment::NewBuffer(_))
            )
        })
}

fn pending_has_buffer(surface: &WlSurface, mapped: bool) -> bool {
    with_states(surface, |states| {
        let mut attributes = states.cached_state.get::<SurfaceAttributes>();
        match &attributes.pending().buffer {
            Some(BufferAssignment::NewBuffer(_)) => true,
            Some(BufferAssignment::Removed) => false,
            None => mapped,
        }
    })
}

fn toplevel_pre_commit(state: &mut RuntimeState, _display: &DisplayHandle, surface: &WlSurface) {
    let Some(toplevel) = state
        .protocol_globals
        .xdg_shell
        .toplevel_for_surface(surface)
    else {
        return;
    };
    let has_buffer = pending_has_buffer(surface, toplevel.mapped());
    match toplevel.commit(has_buffer) {
        ToplevelCommit::Unmapped => state.xdg_toplevel_unmapped(&toplevel),
        ToplevelCommit::Applied | ToplevelCommit::Rejected => {}
    }
}

fn popup_pre_commit(state: &mut RuntimeState, _display: &DisplayHandle, surface: &WlSurface) {
    let Some(popup) = state.protocol_globals.xdg_shell.popup_for_surface(surface) else {
        return;
    };
    let has_buffer = pending_has_buffer(surface, popup.mapped());
    if let PopupCommit::Applied(Some((seat, serial))) = popup.commit(has_buffer) {
        state.handle_xdg_popup_grab(popup, seat, serial);
    }
}

delegate_global_dispatch!(RuntimeState, XdgWmBase, XdgShellGlobalData);
delegate_dispatch!(RuntimeState, XdgWmBase, XdgWmBaseData);
delegate_dispatch!(RuntimeState, XdgPositioner, PositionerData);
delegate_dispatch!(RuntimeState, XdgSurface, XdgSurfaceData);
delegate_dispatch!(RuntimeState, XdgToplevel, XdgRoleData);
delegate_dispatch!(RuntimeState, XdgPopup, XdgRoleData);
