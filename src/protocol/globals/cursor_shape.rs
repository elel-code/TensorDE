//! Tensor-owned cursor-shape protocol state.

use cursor_icon::CursorIcon;
use wayland_protocols::wp::cursor_shape::v1::server::{
    wp_cursor_shape_device_v1::{self, Shape, WpCursorShapeDeviceV1},
    wp_cursor_shape_manager_v1::{self, WpCursorShapeManagerV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, WEnum,
    backend::{GlobalId, ObjectId},
};

use crate::protocol::serial::Serial;
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct CursorShapeProtocol {
    _global: GlobalId,
}

impl CursorShapeProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display
                .create_global::<RuntimeState, WpCursorShapeManagerV1, _>(2, CursorShapeGlobalData),
        }
    }
}

pub(in crate::protocol) struct CursorShapeGlobalData;

pub(in crate::protocol) struct CursorShapeManagerData;

pub(in crate::protocol) struct CursorShapeDeviceData(bool);

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
                let pointer = state.protocol_globals.seat.owns_pointer(&pointer);
                data_init.init(cursor_shape_device, CursorShapeDeviceData(pointer));
            }
            wp_cursor_shape_manager_v1::Request::GetTabletToolV2 { .. } => {
                unreachable!("tablet tool resources are not advertised")
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
        _client: &Client,
        device: &WpCursorShapeDeviceV1,
        request: wp_cursor_shape_device_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_cursor_shape_device_v1::Request::SetShape {
                serial,
                shape: WEnum::Value(shape),
            } => {
                if self.0 && pointer_may_set_cursor(state, Serial::from(serial), &device.id()) {
                    set_named_pointer_cursor(state, shape_to_icon(shape));
                }
            }
            wp_cursor_shape_device_v1::Request::SetShape { .. } => {}
            wp_cursor_shape_device_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

fn pointer_may_set_cursor(state: &RuntimeState, serial: Serial, device: &ObjectId) -> bool {
    let grab_surface = state
        .input_seat
        .pointer_grab_start()
        .and_then(|data| data.focus.as_ref())
        .map(Resource::id);
    state
        .protocol_globals
        .seat
        .pointer_may_set_cursor(serial, device, grab_surface.as_ref())
}

fn set_named_pointer_cursor(state: &mut RuntimeState, icon: CursorIcon) {
    #[cfg(feature = "tty")]
    if state
        .cursor
        .set_image(crate::protocol::cursor::CursorImage::Named(icon))
    {
        if let Some(location) = state.input_seat.pointer_location() {
            state.request_redraw_at(location);
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
