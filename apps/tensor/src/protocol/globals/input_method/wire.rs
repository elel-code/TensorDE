use wayland_protocols::wp::text_input::zv3::server::{
    zwp_text_input_manager_v3::{self, ZwpTextInputManagerV3},
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use wayland_protocols_misc::zwp_input_method_v2::server::{
    zwp_input_method_keyboard_grab_v2::{self, ZwpInputMethodKeyboardGrabV2},
    zwp_input_method_manager_v2::{self, ZwpInputMethodManagerV2},
    zwp_input_method_v2::{self, ZwpInputMethodV2},
    zwp_input_popup_surface_v2::{self, ZwpInputPopupSurfaceV2},
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
    globals::compositor,
    state::{RuntimeState, WaylandClientState},
};
use tensor_util::LogicalRect;

use super::INPUT_POPUP_SURFACE_ROLE;

pub(super) fn create_text_input_global(display: &DisplayHandle, version: u32) -> GlobalId {
    display.create_global::<RuntimeState, ZwpTextInputManagerV3, _>(version, TextInputGlobalData)
}

pub(super) fn create_input_method_global(display: &DisplayHandle, version: u32) -> GlobalId {
    display
        .create_global::<RuntimeState, ZwpInputMethodManagerV2, _>(version, InputMethodGlobalData)
}

#[derive(Debug)]
pub(in crate::protocol) struct TextInputGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct TextInputManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct TextInputData;

#[derive(Debug)]
pub(in crate::protocol) struct InputMethodGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct InputMethodManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct InputMethodData;

#[derive(Debug)]
pub(in crate::protocol) struct InputPopupData {
    surface: WlSurface,
    owner: ObjectId,
}

#[derive(Debug)]
pub(in crate::protocol) struct InputMethodKeyboardGrabData {
    owner: ObjectId,
}

impl GlobalDispatchDelegate<ZwpTextInputManagerV3, RuntimeState> for TextInputGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpTextInputManagerV3>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, TextInputManagerData);
    }
}

impl DispatchDelegate<ZwpTextInputManagerV3, RuntimeState> for TextInputManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        manager: &ZwpTextInputManagerV3,
        request: zwp_text_input_manager_v3::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_text_input_manager_v3::Request::GetTextInput { id, seat } => {
                if !state.protocol_globals.seat.owns(&seat) {
                    manager.post_error(0_u32, "seat is not owned by Tensor");
                    return;
                }
                let resource = data_init.init(id, TextInputData);
                state
                    .protocol_globals
                    .input_method
                    .register_text_input(&resource, client.id());
            }
            zwp_text_input_manager_v3::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwpTextInputV3, RuntimeState> for TextInputData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &ZwpTextInputV3,
        request: zwp_text_input_v3::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_text_input_v3::Request::Destroy => {}
            zwp_text_input_v3::Request::Enable => state
                .protocol_globals
                .input_method
                .reset_text_pending(resource, true),
            zwp_text_input_v3::Request::Disable => state
                .protocol_globals
                .input_method
                .reset_text_pending(resource, false),
            zwp_text_input_v3::Request::SetSurroundingText {
                text,
                cursor,
                anchor,
            } => {
                if !state
                    .protocol_globals
                    .input_method
                    .set_surrounding_text(resource, text, cursor, anchor)
                {
                    resource.post_error(0_u32, "surrounding text has invalid UTF-8 byte indices");
                }
            }
            zwp_text_input_v3::Request::SetTextChangeCause { cause } => {
                if let Ok(cause) = cause.into_result() {
                    state
                        .protocol_globals
                        .input_method
                        .set_change_cause(resource, cause);
                }
            }
            zwp_text_input_v3::Request::SetContentType { hint, purpose } => {
                if let (Ok(hint), Ok(purpose)) = (hint.into_result(), purpose.into_result()) {
                    state
                        .protocol_globals
                        .input_method
                        .set_content_type(resource, hint, purpose);
                }
            }
            zwp_text_input_v3::Request::SetCursorRectangle {
                x,
                y,
                width,
                height,
            } => {
                if !state.protocol_globals.input_method.set_cursor_rectangle(
                    resource,
                    LogicalRect::new((x, y).into(), (width, height).into()),
                ) {
                    resource.post_error(0_u32, "cursor rectangle has a negative size");
                }
            }
            zwp_text_input_v3::Request::Commit => {
                #[cfg(feature = "tty")]
                let previous_root = state.input_method_popup_root();
                state
                    .protocol_globals
                    .input_method
                    .commit_text_input(resource);
                #[cfg(feature = "tty")]
                state.refresh_input_method_popups(previous_root);
            }
            _ => unreachable!("version 2 text-input request reached a version 1 global"),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &ZwpTextInputV3) {
        #[cfg(feature = "tty")]
        let previous_root = state.input_method_popup_root();
        state
            .protocol_globals
            .input_method
            .remove_text_input(resource);
        #[cfg(feature = "tty")]
        state.refresh_input_method_popups(previous_root);
    }
}

impl GlobalDispatchDelegate<ZwpInputMethodManagerV2, RuntimeState> for InputMethodGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpInputMethodManagerV2>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, InputMethodManagerData);
    }

    fn can_view(&self, client: &Client) -> bool {
        client
            .get_data::<WaylandClientState>()
            .is_none_or(|data| data.security_context.is_none())
    }
}

impl DispatchDelegate<ZwpInputMethodManagerV2, RuntimeState> for InputMethodManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &ZwpInputMethodManagerV2,
        request: zwp_input_method_manager_v2::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_input_method_manager_v2::Request::GetInputMethod { seat, input_method } => {
                if !state.protocol_globals.seat.owns(&seat) {
                    manager.post_error(0_u32, "seat is not owned by Tensor");
                    return;
                }
                let resource = data_init.init(input_method, InputMethodData);
                state
                    .protocol_globals
                    .input_method
                    .register_input_method(&resource);
            }
            zwp_input_method_manager_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwpInputMethodV2, RuntimeState> for InputMethodData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &ZwpInputMethodV2,
        request: zwp_input_method_v2::Request,
        display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_input_method_v2::Request::CommitString { text } => {
                if !state
                    .protocol_globals
                    .input_method
                    .set_commit_string(resource, text)
                {
                    resource.post_error(0_u32, "commit string exceeds 4000 bytes");
                }
            }
            zwp_input_method_v2::Request::SetPreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                if !state.protocol_globals.input_method.set_preedit(
                    resource,
                    text,
                    cursor_begin,
                    cursor_end,
                ) {
                    resource.post_error(0_u32, "preedit has invalid UTF-8 byte indices");
                }
            }
            zwp_input_method_v2::Request::DeleteSurroundingText {
                before_length,
                after_length,
            } => state.protocol_globals.input_method.set_delete_surrounding(
                resource,
                before_length,
                after_length,
            ),
            zwp_input_method_v2::Request::Commit { serial } => state
                .protocol_globals
                .input_method
                .commit_input_method(resource, serial),
            zwp_input_method_v2::Request::GetInputPopupSurface { id, surface } => {
                let available = state
                    .protocol_globals
                    .input_method
                    .input_method_available(resource);
                if available && compositor::give_role(&surface, INPUT_POPUP_SURFACE_ROLE).is_err() {
                    resource.post_error(0_u32, "wl_surface already has a role");
                    return;
                }
                let role = data_init.init(
                    id,
                    InputPopupData {
                        surface: surface.clone(),
                        owner: resource.id(),
                    },
                );
                if available {
                    if !state.protocol_globals.input_method.register_popup(
                        resource,
                        &role,
                        surface.clone(),
                    ) {
                        resource.post_error(0_u32, "input popup capacity exceeded");
                        return;
                    }
                    state.update_surface_scale(&surface);
                    #[cfg(feature = "tty")]
                    state.refresh_input_method_popups(state.input_method_popup_root());
                }
            }
            zwp_input_method_v2::Request::GrabKeyboard { keyboard } => {
                let grab = data_init.init(
                    keyboard,
                    InputMethodKeyboardGrabData {
                        owner: resource.id(),
                    },
                );
                if state
                    .protocol_globals
                    .input_method
                    .input_method_available(resource)
                {
                    if !state
                        .protocol_globals
                        .input_method
                        .register_keyboard_grab(resource, &grab)
                    {
                        resource.post_error(0_u32, "input-method keyboard-grab capacity exceeded");
                        return;
                    }
                    state
                        .protocol_globals
                        .seat
                        .initialize_input_method_grab(&grab);
                    let modifiers = state.protocol_globals.seat.keyboard_modifiers().serialized;
                    grab.modifiers(
                        crate::protocol::serial::next_serial().into(),
                        modifiers.depressed,
                        modifiers.latched,
                        modifiers.locked,
                        modifiers.layout,
                    );
                }
            }
            zwp_input_method_v2::Request::Destroy => {
                let available = state
                    .protocol_globals
                    .input_method
                    .input_method_available(resource);
                #[cfg(feature = "tty")]
                let previous_root = state.input_method_popup_root();
                #[cfg(feature = "tty")]
                if available {
                    state.leave_all_input_method_popups();
                }
                if available {
                    let backend = display.backend_handle();
                    if state
                        .protocol_globals
                        .input_method
                        .destroy_children(resource, |id| {
                            let _ = backend.destroy_object::<RuntimeState>(&id);
                        })
                    {
                        state.input_seat.clear_input_method_key_routes();
                    }
                }
                #[cfg(feature = "tty")]
                if available {
                    state.refresh_input_method_popups(previous_root);
                }
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &ZwpInputMethodV2) {
        #[cfg(feature = "tty")]
        let available = state
            .protocol_globals
            .input_method
            .input_method_available(resource);
        #[cfg(feature = "tty")]
        let previous_root = state.input_method_popup_root();
        #[cfg(feature = "tty")]
        if available {
            state.leave_all_input_method_popups();
        }
        if state
            .protocol_globals
            .input_method
            .remove_input_method(resource)
        {
            state.input_seat.clear_input_method_key_routes();
        }
        #[cfg(feature = "tty")]
        if available {
            state.refresh_input_method_popups(previous_root);
        }
    }
}

impl DispatchDelegate<ZwpInputPopupSurfaceV2, RuntimeState> for InputPopupData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &ZwpInputPopupSurfaceV2,
        request: zwp_input_popup_surface_v2::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_input_popup_surface_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &ZwpInputPopupSurfaceV2,
    ) {
        let _ = (&self.surface, &self.owner);
        #[cfg(feature = "tty")]
        let previous_root = state.input_method_popup_root();
        #[cfg(feature = "tty")]
        state.leave_input_method_popup(&self.surface);
        state.protocol_globals.input_method.remove_popup(resource);
        #[cfg(feature = "tty")]
        state.refresh_input_method_popups(previous_root);
    }
}

impl DispatchDelegate<ZwpInputMethodKeyboardGrabV2, RuntimeState> for InputMethodKeyboardGrabData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &ZwpInputMethodKeyboardGrabV2,
        request: zwp_input_method_keyboard_grab_v2::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_input_method_keyboard_grab_v2::Request::Release => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &ZwpInputMethodKeyboardGrabV2,
    ) {
        let _ = &self.owner;
        if state
            .protocol_globals
            .input_method
            .remove_keyboard_grab(resource)
        {
            state.input_seat.clear_input_method_key_routes();
        }
    }
}

delegate_global_dispatch!(RuntimeState, ZwpTextInputManagerV3, TextInputGlobalData);
delegate_dispatch!(RuntimeState, ZwpTextInputManagerV3, TextInputManagerData);
delegate_dispatch!(RuntimeState, ZwpTextInputV3, TextInputData);
delegate_global_dispatch!(RuntimeState, ZwpInputMethodManagerV2, InputMethodGlobalData);
delegate_dispatch!(
    RuntimeState,
    ZwpInputMethodManagerV2,
    InputMethodManagerData
);
delegate_dispatch!(RuntimeState, ZwpInputMethodV2, InputMethodData);
delegate_dispatch!(RuntimeState, ZwpInputPopupSurfaceV2, InputPopupData);
delegate_dispatch!(
    RuntimeState,
    ZwpInputMethodKeyboardGrabV2,
    InputMethodKeyboardGrabData
);
