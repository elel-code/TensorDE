//! text-input-v3 dispatch for the native shell.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3, zwp_text_input_v3,
};

use super::types::{NativeShellEvent, NativeShellState};

impl Dispatch<zwp_text_input_manager_v3::ZwpTextInputManagerV3, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &zwp_text_input_manager_v3::ZwpTextInputManagerV3,
        _: zwp_text_input_manager_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwp_text_input_v3::ZwpTextInputV3, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        _: &zwp_text_input_v3::ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::Enter { surface } => {
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                state.text_input_surface = id;
                state.text_input_pending_commit = None;
                state.text_input_pending_preedit = None;
                state.text_input_pending_delete = (0, 0);
                if let Some(surface) = id {
                    state.push(NativeShellEvent::TextInputEnter { surface });
                }
            }
            zwp_text_input_v3::Event::Leave { surface } => {
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied()
                    .or(state.text_input_surface);
                state.text_input_surface = None;
                state.text_input_pending_commit = None;
                state.text_input_pending_preedit = None;
                state.text_input_pending_delete = (0, 0);
                if let Some(surface) = id {
                    state.push(NativeShellEvent::TextInputLeave { surface });
                }
            }
            zwp_text_input_v3::Event::PreeditString { text, .. } => {
                state.text_input_pending_preedit = text;
            }
            zwp_text_input_v3::Event::CommitString { text } => {
                state.text_input_pending_preedit = None;
                state.text_input_pending_commit = text;
            }
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                state.text_input_pending_delete = (before_length, after_length);
            }
            zwp_text_input_v3::Event::Done { serial } => {
                state.text_input_serial = serial;
                let surface = state.text_input_surface;
                let commit = state.text_input_pending_commit.take();
                let preedit = state.text_input_pending_preedit.take();
                let (delete_before, delete_after) =
                    std::mem::take(&mut state.text_input_pending_delete);
                if let Some(surface) = surface {
                    state.push(NativeShellEvent::TextInputDone {
                        surface,
                        serial,
                        commit,
                        preedit,
                        delete_before,
                        delete_after,
                    });
                }
            }
            _ => {}
        }
    }
}
