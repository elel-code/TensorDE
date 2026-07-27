//! Tensor-owned cursor-shape protocol state.

use std::collections::HashMap;

use cursor_icon::CursorIcon;
use smithay::{
    backend::input::TabletToolDescriptor,
    input::{
        pointer::{CursorImageStatus, PointerHandle},
        tablet::{TabletSeatHandler, tool::TabletToolHandle},
    },
    utils::Serial,
};
use wayland_protocols::wp::{
    cursor_shape::v1::server::{
        wp_cursor_shape_device_v1::{self, Shape, WpCursorShapeDeviceV1},
        wp_cursor_shape_manager_v1::{self, WpCursorShapeManagerV1},
    },
    tablet::zv2::server::zwp_tablet_tool_v2::ZwpTabletToolV2,
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, WEnum, Weak,
    backend::{ClientId, GlobalId, ObjectId},
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct CursorShapeProtocol {
    _global: GlobalId,
    tablet_grants: HashMap<TabletToolDescriptor, TabletCursorGrant>,
    tablet_device_count: usize,
}

struct TabletCursorGrant {
    serial: Serial,
    client: Option<ClientId>,
}

impl CursorShapeProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display
                .create_global::<RuntimeState, WpCursorShapeManagerV1, _>(2, CursorShapeGlobalData),
            tablet_grants: HashMap::new(),
            tablet_device_count: 0,
        }
    }

    pub(crate) const fn has_tablet_devices(&self) -> bool {
        self.tablet_device_count != 0
    }

    pub(crate) fn note_tablet_proximity(
        &mut self,
        tool: &TabletToolDescriptor,
        serial: Serial,
        client: Option<ClientId>,
    ) {
        self.tablet_grants
            .insert(tool.clone(), TabletCursorGrant { serial, client });
    }

    pub(crate) fn note_tablet_focus(
        &mut self,
        tool: &TabletToolDescriptor,
        client: Option<ClientId>,
    ) {
        if let Some(grant) = self.tablet_grants.get_mut(tool) {
            grant.client = client;
        }
    }

    pub(crate) fn clear_tablet_proximity(&mut self, tool: &TabletToolDescriptor) {
        self.tablet_grants.remove(tool);
    }

    fn tablet_may_set_cursor(
        &self,
        tool: &TabletToolHandle<RuntimeState>,
        serial: Serial,
        device: &ObjectId,
        client: &ClientId,
    ) -> bool {
        if tool
            .grab_start_data()
            .and_then(|data| data.focus)
            .is_some_and(|(surface, _)| surface.id().same_client_as(device))
        {
            return true;
        }
        self.tablet_grants
            .get(tool.descriptor())
            .is_some_and(|grant| grant.serial == serial && grant.client.as_ref() == Some(client))
    }
}

pub(in crate::protocol) struct CursorShapeGlobalData;

pub(in crate::protocol) struct CursorShapeManagerData;

pub(in crate::protocol) struct CursorShapeDeviceData(CursorShapeSource);

enum CursorShapeSource {
    Pointer(Option<PointerHandle<RuntimeState>>),
    Tablet {
        resource: Weak<ZwpTabletToolV2>,
        tool: Option<TabletToolHandle<RuntimeState>>,
    },
}

impl GlobalDispatchDelegate<WpCursorShapeManagerV1, RuntimeState> for CursorShapeGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpCursorShapeManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, CursorShapeManagerData);
    }
}

impl DispatchDelegate<WpCursorShapeManagerV1, RuntimeState> for CursorShapeManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _manager: &WpCursorShapeManagerV1,
        request: wp_cursor_shape_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_cursor_shape_manager_v1::Request::GetPointer {
                cursor_shape_device,
                pointer,
            } => {
                let pointer = PointerHandle::<RuntimeState>::from_resource(&pointer)
                    .filter(|handle| state.seat.get_pointer().as_ref() == Some(handle));
                data_init.init(
                    cursor_shape_device,
                    CursorShapeDeviceData(CursorShapeSource::Pointer(pointer)),
                );
            }
            wp_cursor_shape_manager_v1::Request::GetTabletToolV2 {
                cursor_shape_device,
                tablet_tool,
            } => {
                let tool = TabletToolHandle::<RuntimeState>::from_resource(&tablet_tool);
                if tool.is_some() {
                    state.protocol_globals.cursor_shape.tablet_device_count += 1;
                }
                data_init.init(
                    cursor_shape_device,
                    CursorShapeDeviceData(CursorShapeSource::Tablet {
                        resource: tablet_tool.downgrade(),
                        tool,
                    }),
                );
            }
            wp_cursor_shape_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpCursorShapeDeviceV1, RuntimeState> for CursorShapeDeviceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        device: &WpCursorShapeDeviceV1,
        request: wp_cursor_shape_device_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_cursor_shape_device_v1::Request::SetShape {
                serial,
                shape: WEnum::Value(shape),
            } => match &self.0 {
                CursorShapeSource::Pointer(Some(pointer)) => {
                    if pointer_may_set_cursor(pointer, Serial::from(serial), &device.id()) {
                        set_named_pointer_cursor(state, shape_to_icon(shape));
                    }
                }
                CursorShapeSource::Pointer(None) => {}
                CursorShapeSource::Tablet {
                    resource,
                    tool: Some(tool),
                } if resource.upgrade().is_ok()
                    && state.protocol_globals.cursor_shape.tablet_may_set_cursor(
                        tool,
                        Serial::from(serial),
                        &device.id(),
                        &client.id(),
                    ) =>
                {
                    <RuntimeState as TabletSeatHandler>::tablet_tool_image(
                        state,
                        tool.descriptor(),
                        CursorImageStatus::Named(shape_to_icon(shape)),
                    );
                }
                CursorShapeSource::Tablet { .. } => {}
            },
            wp_cursor_shape_device_v1::Request::SetShape { .. } => {}
            wp_cursor_shape_device_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        _device: &WpCursorShapeDeviceV1,
    ) {
        if matches!(&self.0, CursorShapeSource::Tablet { tool: Some(_), .. }) {
            state.protocol_globals.cursor_shape.tablet_device_count -= 1;
        }
    }
}

fn pointer_may_set_cursor(
    pointer: &PointerHandle<RuntimeState>,
    serial: Serial,
    device: &ObjectId,
) -> bool {
    if pointer
        .grab_start_data()
        .and_then(|data| data.focus)
        .is_some_and(|(surface, _)| surface.id().same_client_as(device))
    {
        return true;
    }
    pointer.last_enter() == Some(serial)
        && pointer
            .current_focus()
            .is_some_and(|surface| surface.id().same_client_as(device))
}

fn set_named_pointer_cursor(state: &mut RuntimeState, icon: CursorIcon) {
    #[cfg(feature = "tty")]
    if state
        .cursor
        .set_image(crate::protocol::cursor::CursorImage::Named(icon))
    {
        if let Some(pointer) = state.seat.get_pointer() {
            state.request_redraw_at(pointer.current_location());
        } else {
            state.request_redraw_workspace();
        }
    }
    #[cfg(not(feature = "tty"))]
    let _ = (state, icon);
}

fn shape_to_icon(shape: Shape) -> CursorIcon {
    match shape {
        Shape::Default => CursorIcon::Default,
        Shape::ContextMenu => CursorIcon::ContextMenu,
        Shape::Help => CursorIcon::Help,
        Shape::Pointer => CursorIcon::Pointer,
        Shape::Progress => CursorIcon::Progress,
        Shape::Wait => CursorIcon::Wait,
        Shape::Cell => CursorIcon::Cell,
        Shape::Crosshair => CursorIcon::Crosshair,
        Shape::Text => CursorIcon::Text,
        Shape::VerticalText => CursorIcon::VerticalText,
        Shape::Alias => CursorIcon::Alias,
        Shape::Copy => CursorIcon::Copy,
        Shape::Move => CursorIcon::Move,
        Shape::NoDrop => CursorIcon::NoDrop,
        Shape::NotAllowed => CursorIcon::NotAllowed,
        Shape::Grab => CursorIcon::Grab,
        Shape::Grabbing => CursorIcon::Grabbing,
        Shape::EResize => CursorIcon::EResize,
        Shape::NResize => CursorIcon::NResize,
        Shape::NeResize => CursorIcon::NeResize,
        Shape::NwResize => CursorIcon::NwResize,
        Shape::SResize => CursorIcon::SResize,
        Shape::SeResize => CursorIcon::SeResize,
        Shape::SwResize => CursorIcon::SwResize,
        Shape::WResize => CursorIcon::WResize,
        Shape::EwResize => CursorIcon::EwResize,
        Shape::NsResize => CursorIcon::NsResize,
        Shape::NeswResize => CursorIcon::NeswResize,
        Shape::NwseResize => CursorIcon::NwseResize,
        Shape::ColResize => CursorIcon::ColResize,
        Shape::RowResize => CursorIcon::RowResize,
        Shape::AllScroll => CursorIcon::AllScroll,
        Shape::ZoomIn => CursorIcon::ZoomIn,
        Shape::ZoomOut => CursorIcon::ZoomOut,
        Shape::DndAsk => CursorIcon::DndAsk,
        Shape::AllResize => CursorIcon::AllResize,
        _ => CursorIcon::Default,
    }
}

delegate_global_dispatch!(RuntimeState, WpCursorShapeManagerV1, CursorShapeGlobalData);
delegate_dispatch!(RuntimeState, WpCursorShapeManagerV1, CursorShapeManagerData);
delegate_dispatch!(RuntimeState, WpCursorShapeDeviceV1, CursorShapeDeviceData);
