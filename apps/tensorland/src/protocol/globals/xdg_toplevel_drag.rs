//! Tensor-owned xdg-toplevel-drag state and interactive floating motion.

use std::collections::HashMap;

use tensor_util::{LogicalPoint, Rect};
use wayland_protocols::xdg::{
    shell::server::xdg_toplevel::XdgToplevel,
    toplevel_drag::v1::server::{
        xdg_toplevel_drag_manager_v1::{self, XdgToplevelDragManagerV1},
        xdg_toplevel_drag_v1::{self, XdgToplevelDragV1},
    },
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::wl_data_source::WlDataSource,
};

use super::{selection::SourceToken, xdg_shell::Toplevel};
use crate::{
    ecs::ViewPlacement,
    protocol::{
        dispatch::{
            DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
        },
        state::RuntimeState,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DragPhase {
    Prepared,
    Active,
    Ended,
}

#[derive(Clone, Debug)]
struct DragAttachment {
    toplevel: Toplevel,
    offset: (i32, i32),
}

#[derive(Debug)]
struct DragEntry {
    resource: Weak<XdgToplevelDragV1>,
    manager: Weak<XdgToplevelDragManagerV1>,
    phase: DragPhase,
    attachment: Option<DragAttachment>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachError {
    UnknownDrag,
    ToplevelAttached,
}

pub(crate) struct XdgToplevelDragProtocol {
    _global: GlobalId,
    drags: HashMap<SourceToken, DragEntry>,
    active_source: Option<SourceToken>,
}

impl XdgToplevelDragProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, XdgToplevelDragManagerV1, _>(
                1,
                ToplevelDragGlobalData,
            ),
            drags: HashMap::new(),
            active_source: None,
        }
    }

    fn contains_source(&mut self, token: SourceToken) -> bool {
        let live = self
            .drags
            .get(&token)
            .is_some_and(|entry| entry.resource.upgrade().is_ok());
        if !live {
            self.drags.remove(&token);
        }
        live
    }

    fn insert(
        &mut self,
        token: SourceToken,
        manager: &XdgToplevelDragManagerV1,
        drag: &XdgToplevelDragV1,
    ) {
        self.drags.insert(
            token,
            DragEntry {
                resource: drag.downgrade(),
                manager: manager.downgrade(),
                phase: DragPhase::Prepared,
                attachment: None,
            },
        );
    }

    fn remove(&mut self, token: SourceToken, drag: &XdgToplevelDragV1) {
        let matches = self
            .drags
            .get(&token)
            .and_then(|entry| entry.resource.upgrade().ok())
            .as_ref()
            == Some(drag);
        if matches {
            self.drags.remove(&token);
            if self.active_source == Some(token) {
                self.active_source = None;
            }
        }
    }

    fn ended(&self, token: SourceToken, drag: &XdgToplevelDragV1) -> bool {
        self.drags.get(&token).is_some_and(|entry| {
            entry.resource.upgrade().ok().as_ref() == Some(drag) && entry.phase == DragPhase::Ended
        })
    }

    fn attach(
        &mut self,
        token: SourceToken,
        drag: &XdgToplevelDragV1,
        toplevel: Toplevel,
        offset: (i32, i32),
    ) -> Result<bool, AttachError> {
        let Some(entry) = self.drags.get_mut(&token) else {
            return Err(AttachError::UnknownDrag);
        };
        if entry.resource.upgrade().ok().as_ref() != Some(drag) {
            return Err(AttachError::UnknownDrag);
        }
        if entry
            .attachment
            .as_ref()
            .is_some_and(|attached| attached.toplevel.alive())
        {
            return Err(AttachError::ToplevelAttached);
        }
        entry.attachment = Some(DragAttachment { toplevel, offset });
        Ok(entry.phase == DragPhase::Active)
    }

    pub(in crate::protocol) fn drag_started(&mut self, token: SourceToken) {
        let Some(entry) = self.drags.get_mut(&token) else {
            return;
        };
        entry.phase = DragPhase::Active;
        self.active_source = Some(token);
    }

    pub(in crate::protocol) fn drag_ended(&mut self, token: SourceToken) {
        if let Some(entry) = self.drags.get_mut(&token) {
            entry.phase = DragPhase::Ended;
        }
        if self.active_source == Some(token) {
            self.active_source = None;
        }
    }

    pub(in crate::protocol) fn source_destroyed(&mut self, token: SourceToken) {
        if let Some(entry) = self.drags.get_mut(&token) {
            entry.phase = DragPhase::Ended;
            entry.attachment = None;
        }
        if self.active_source == Some(token) {
            self.active_source = None;
        }
    }

    fn active_attachment(&self) -> Option<DragAttachment> {
        let entry = self.drags.get(&self.active_source?)?;
        (entry.phase == DragPhase::Active)
            .then(|| entry.attachment.clone())
            .flatten()
    }

    pub(in crate::protocol) fn active_toplevel_surface(&self) -> Option<ObjectId> {
        self.active_attachment()
            .map(|attachment| attachment.toplevel.wl_surface().id())
    }

    pub(in crate::protocol) fn detach_toplevel(&mut self, toplevel: &XdgToplevel) {
        for entry in self.drags.values_mut() {
            if entry
                .attachment
                .as_ref()
                .is_some_and(|attached| attached.toplevel.xdg_toplevel() == toplevel)
            {
                entry.attachment = None;
            }
        }
    }

    fn reject_selection_use(&self, token: SourceToken, source: Option<&WlDataSource>) -> bool {
        let Some(entry) = self.drags.get(&token) else {
            return false;
        };
        if let Ok(manager) = entry.manager.upgrade() {
            manager.post_error(
                xdg_toplevel_drag_manager_v1::Error::InvalidSource,
                "data source associated with xdg-toplevel-drag cannot be used as a selection",
            );
        } else if let Some(source) = source {
            source.post_error(
                wayland_server::protocol::wl_data_source::Error::InvalidSource,
                "xdg-toplevel-drag source can only be used for drag-and-drop",
            );
        }
        true
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct ToplevelDragGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct ToplevelDragManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct ToplevelDragData {
    source: SourceToken,
}

impl GlobalDispatchDelegate<XdgToplevelDragManagerV1, RuntimeState> for ToplevelDragGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<XdgToplevelDragManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, ToplevelDragManagerData);
    }
}

impl DispatchDelegate<XdgToplevelDragManagerV1, RuntimeState> for ToplevelDragManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &XdgToplevelDragManagerV1,
        request: xdg_toplevel_drag_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_toplevel_drag_manager_v1::Request::GetXdgToplevelDrag { id, data_source } => {
                let token = state
                    .protocol_globals
                    .selection
                    .toplevel_drag_source_token(&data_source);
                let valid = token.is_some_and(|token| {
                    state
                        .protocol_globals
                        .selection
                        .source_unused_for_toplevel_drag(token)
                        && !state
                            .protocol_globals
                            .xdg_toplevel_drag
                            .contains_source(token)
                });
                let Some(token) = token.filter(|_| valid) else {
                    manager.post_error(
                        xdg_toplevel_drag_manager_v1::Error::InvalidSource,
                        "data source is unknown, already used, or already owns a toplevel drag",
                    );
                    return;
                };
                let drag = data_init.init(id, ToplevelDragData { source: token });
                state
                    .protocol_globals
                    .xdg_toplevel_drag
                    .insert(token, manager, &drag);
            }
            xdg_toplevel_drag_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<XdgToplevelDragV1, RuntimeState> for ToplevelDragData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        drag: &XdgToplevelDragV1,
        request: xdg_toplevel_drag_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_toplevel_drag_v1::Request::Destroy => {
                if !state
                    .protocol_globals
                    .xdg_toplevel_drag
                    .ended(self.source, drag)
                {
                    drag.post_error(
                        xdg_toplevel_drag_v1::Error::OngoingDrag,
                        "xdg_toplevel_drag can only be destroyed after its drag has ended",
                    );
                }
            }
            xdg_toplevel_drag_v1::Request::Attach {
                toplevel,
                x_offset,
                y_offset,
            } => {
                let shell_toplevel = state
                    .protocol_globals
                    .xdg_shell
                    .toplevel(&toplevel)
                    .expect("Wayland only dispatches references to a live xdg_toplevel");
                match state.protocol_globals.xdg_toplevel_drag.attach(
                    self.source,
                    drag,
                    shell_toplevel,
                    (x_offset, y_offset),
                ) {
                    Ok(active) => {
                        if active && let Some(location) = state.input_seat.pointer_location() {
                            state.move_active_xdg_toplevel_drag(location);
                        }
                    }
                    Err(AttachError::ToplevelAttached) => drag.post_error(
                        xdg_toplevel_drag_v1::Error::ToplevelAttached,
                        "a live xdg_toplevel is already attached to this drag",
                    ),
                    Err(AttachError::UnknownDrag) => {}
                }
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, drag: &XdgToplevelDragV1) {
        state
            .protocol_globals
            .xdg_toplevel_drag
            .remove(self.source, drag);
    }
}

impl RuntimeState {
    pub(in crate::protocol) fn reject_toplevel_drag_selection_use(
        &self,
        token: SourceToken,
        source: Option<&WlDataSource>,
    ) -> bool {
        self.protocol_globals
            .xdg_toplevel_drag
            .reject_selection_use(token, source)
    }

    pub(in crate::protocol) fn move_active_xdg_toplevel_drag(
        &mut self,
        pointer: LogicalPoint<f64>,
    ) {
        let Some(attachment) = self.protocol_globals.xdg_toplevel_drag.active_attachment() else {
            return;
        };
        let Some(view_id) = self.view_for_surface(attachment.toplevel.wl_surface()) else {
            return;
        };
        let Some(window) = self.mapped_window_for_view(view_id) else {
            return;
        };
        let current = self.world.geometry(view_id).unwrap_or_else(|| {
            let geometry = window.geometry();
            Rect::new(
                0,
                0,
                u32::try_from(geometry.size.w.max(1)).unwrap_or(u32::MAX),
                u32::try_from(geometry.size.h.max(1)).unwrap_or(u32::MAX),
            )
        });
        let location = (
            drag_axis(pointer.x, attachment.offset.0),
            drag_axis(pointer.y, attachment.offset.1),
        );
        let geometry = Rect::new(location.0, location.1, current.width, current.height);
        let floating = matches!(
            self.world.view_placement(view_id),
            Some(ViewPlacement::Floating { .. })
        );
        let changed = if floating {
            self.world
                .update_floating_geometry(view_id, geometry)
                .unwrap_or(false)
        } else {
            self.world
                .set_view_placement(view_id, ViewPlacement::Floating { geometry })
                .unwrap_or(false)
        };
        if !changed {
            return;
        }
        if floating {
            self.space.relocate_element(&window, location);
            self.space.refresh(&self.popups);
        } else {
            let _ = self.reflow_default_workspace();
            self.space.raise_element(&window, true);
        }
        #[cfg(feature = "tty")]
        self.request_redraw_workspace();
    }

    pub(in crate::protocol) fn active_xdg_toplevel_drag_surface(&self) -> Option<ObjectId> {
        self.protocol_globals
            .xdg_toplevel_drag
            .active_toplevel_surface()
    }
}

fn drag_axis(pointer: f64, offset: i32) -> i32 {
    let value = pointer - f64::from(offset);
    if !value.is_finite() {
        return 0;
    }
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

delegate_global_dispatch!(
    RuntimeState,
    XdgToplevelDragManagerV1,
    ToplevelDragGlobalData
);
delegate_dispatch!(
    RuntimeState,
    XdgToplevelDragManagerV1,
    ToplevelDragManagerData
);
delegate_dispatch!(RuntimeState, XdgToplevelDragV1, ToplevelDragData);
