//! xdg-activation-v1 dispatch for the native shell.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1};

use super::types::{NativeShellEvent, NativeShellState};

impl Dispatch<xdg_activation_v1::XdgActivationV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &xdg_activation_v1::XdgActivationV1,
        _: xdg_activation_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        token_obj: &xdg_activation_token_v1::XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            let obj_id = token_obj.id().protocol_id();
            if let Some((surface, proxy)) = state.activation_tokens.remove(&obj_id) {
                proxy.destroy();
                state.push(NativeShellEvent::ActivationToken { surface, token });
            }
        }
    }
}
