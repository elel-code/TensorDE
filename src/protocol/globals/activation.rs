//! Tensor-owned xdg-activation-v1 token authority and wire dispatch.

use std::{
    collections::HashMap,
    io,
    time::{Duration, Instant},
};

use tracing::warn;
use wayland_protocols::xdg::activation::v1::server::{
    xdg_activation_token_v1::{self, XdgActivationTokenV1},
    xdg_activation_v1::{self, XdgActivationV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::{wl_seat::WlSeat, wl_surface::WlSurface},
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

const TOKEN_BYTES: usize = 32;
const TOKEN_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PENDING_TOKENS: usize = 4_096;

pub(crate) struct ActivationProtocol {
    _global: GlobalId,
    tokens: HashMap<String, Instant>,
    builders: HashMap<ObjectId, TokenBuilder>,
    keyboard_focus: Option<(Weak<WlSurface>, ClientId)>,
    pointer_focus: Option<(Weak<WlSurface>, ClientId)>,
    last_interaction: Option<InteractionGrant>,
}

struct InteractionGrant {
    client: ClientId,
    serial: u32,
    timestamp: Instant,
}

impl ActivationProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display
                .create_global::<RuntimeState, XdgActivationV1, _>(1, ActivationGlobalData),
            tokens: HashMap::new(),
            builders: HashMap::new(),
            keyboard_focus: None,
            pointer_focus: None,
            last_interaction: None,
        }
    }

    pub(crate) fn sync_keyboard_focus(&mut self, surface: Option<&WlSurface>) {
        self.keyboard_focus =
            surface.and_then(|surface| Some((surface.downgrade(), surface.client()?.id())));
    }

    pub(crate) fn sync_pointer_focus(&mut self, surface: Option<&WlSurface>) {
        if self
            .pointer_focus
            .as_ref()
            .is_some_and(|(known, _)| surface.is_some_and(|surface| known == surface))
        {
            return;
        }
        self.pointer_focus =
            surface.and_then(|surface| Some((surface.downgrade(), surface.client()?.id())));
    }

    /// Tensor currently owns one seat, so button/key paths only overwrite a
    /// fixed-size grant and never scan, lock, allocate, or clone a resource.
    pub(crate) fn note_keyboard_interaction(&mut self, serial: u32) {
        if let Some(client) = self
            .keyboard_focus
            .as_ref()
            .map(|(_, client)| client.clone())
        {
            self.note_interaction(client, serial);
        }
    }

    pub(crate) fn note_pointer_interaction(&mut self, serial: u32) {
        if let Some(client) = self
            .pointer_focus
            .as_ref()
            .map(|(_, client)| client.clone())
        {
            self.note_interaction(client, serial);
        }
    }

    fn note_interaction(&mut self, client: ClientId, serial: u32) {
        self.last_interaction = Some(InteractionGrant {
            client,
            serial,
            timestamp: Instant::now(),
        });
    }

    fn client_is_focused(&self, client: &ClientId) -> bool {
        self.keyboard_client_is_focused(client)
            || self
                .pointer_focus
                .as_ref()
                .is_some_and(|(surface, focused)| surface.is_alive() && focused == client)
    }

    fn keyboard_client_is_focused(&self, client: &ClientId) -> bool {
        self.keyboard_focus
            .as_ref()
            .is_some_and(|(surface, focused)| surface.is_alive() && focused == client)
    }

    fn pointer_client_is_focused(&self, client: &ClientId) -> bool {
        self.pointer_focus
            .as_ref()
            .is_some_and(|(surface, focused)| surface.is_alive() && focused == client)
    }

    fn interaction_matches(&self, client: &ClientId, serial: u32) -> bool {
        self.last_interaction.as_ref().is_some_and(|grant| {
            &grant.client == client
                && grant.serial == serial
                && grant.timestamp.elapsed() < TOKEN_TIMEOUT
        })
    }

    fn insert_client_token(&mut self, token: String, valid: bool) {
        self.prune_expired();
        if valid && self.tokens.len() < MAX_PENDING_TOKENS {
            self.tokens.insert(token, Instant::now());
        }
    }

    fn consume(&mut self, token: &str) -> bool {
        self.tokens
            .remove(token)
            .is_some_and(|timestamp| timestamp.elapsed() < TOKEN_TIMEOUT)
    }

    fn prune_expired(&mut self) {
        self.tokens
            .retain(|_, timestamp| timestamp.elapsed() < TOKEN_TIMEOUT);
    }

    fn mint_unique_token(&self) -> io::Result<String> {
        for _ in 0..4 {
            let token = random_token()?;
            if !self.tokens.contains_key(&token) {
                return Ok(token);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "kernel CSPRNG repeatedly produced an existing activation token",
        ))
    }

    pub(crate) fn issue_external_token(&mut self) -> io::Result<String> {
        self.prune_expired();
        if self.tokens.len() >= MAX_PENDING_TOKENS {
            return Err(io::Error::other("xdg-activation token pool is full"));
        }
        let token = self.mint_unique_token()?;
        self.tokens.insert(token.clone(), Instant::now());
        Ok(token)
    }

    #[cfg(test)]
    pub(crate) fn token_count(&self) -> usize {
        self.tokens.len()
    }

    #[cfg(test)]
    pub(crate) fn builder_count(&self) -> usize {
        self.builders.len()
    }
}

pub(in crate::protocol) struct ActivationGlobalData;
pub(in crate::protocol) struct ActivationManagerData;

pub(in crate::protocol) struct ActivationTokenData;

#[derive(Default)]
struct TokenBuilder {
    committed: bool,
    serial: Option<(u32, Weak<WlSeat>)>,
    app_id: Option<String>,
    surface: Option<Weak<WlSurface>>,
}

impl GlobalDispatchDelegate<XdgActivationV1, RuntimeState> for ActivationGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<XdgActivationV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, ActivationManagerData);
    }
}

impl DispatchDelegate<XdgActivationV1, RuntimeState> for ActivationManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _manager: &XdgActivationV1,
        request: xdg_activation_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_activation_v1::Request::Destroy => {}
            xdg_activation_v1::Request::GetActivationToken { id } => {
                let resource = data_init.init(id, ActivationTokenData);
                if state.protocol_globals.activation.builders.len() < MAX_PENDING_TOKENS {
                    state
                        .protocol_globals
                        .activation
                        .builders
                        .insert(resource.id(), TokenBuilder::default());
                }
            }
            xdg_activation_v1::Request::Activate { token, surface } => {
                state.activate_surface_with_token(&token, surface);
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<XdgActivationTokenV1, RuntimeState> for ActivationTokenData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        resource: &XdgActivationTokenV1,
        request: xdg_activation_token_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_activation_token_v1::Request::SetSerial { serial, seat } => {
                let Some(builder) = token_builder(state, resource) else {
                    return;
                };
                builder.serial = Some((serial, seat.downgrade()));
            }
            xdg_activation_token_v1::Request::SetAppId { app_id } => {
                let Some(builder) = token_builder(state, resource) else {
                    return;
                };
                builder.app_id = Some(app_id);
            }
            xdg_activation_token_v1::Request::SetSurface { surface } => {
                let Some(builder) = token_builder(state, resource) else {
                    return;
                };
                builder.surface = Some(surface.downgrade());
            }
            xdg_activation_token_v1::Request::Commit => {
                let Some(builder) = token_builder(state, resource) else {
                    return;
                };
                builder.committed = true;
                let builder = TokenBuilder {
                    committed: true,
                    serial: builder.serial.take(),
                    app_id: builder.app_id.take(),
                    surface: builder.surface.take(),
                };
                let valid = state.activation_request_is_authorized(&client.id(), &builder);
                match state.protocol_globals.activation.mint_unique_token() {
                    Ok(token) => {
                        state
                            .protocol_globals
                            .activation
                            .insert_client_token(token.clone(), valid);
                        resource.done(token);
                    }
                    Err(error) => {
                        warn!(%error, "could not mint xdg-activation token");
                        // No weak random fallback: an empty, untracked token is unusable.
                        resource.done(String::new());
                    }
                }
            }
            xdg_activation_token_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &XdgActivationTokenV1,
    ) {
        state
            .protocol_globals
            .activation
            .builders
            .remove(&resource.id());
    }
}

fn token_builder<'a>(
    state: &'a mut RuntimeState,
    resource: &XdgActivationTokenV1,
) -> Option<&'a mut TokenBuilder> {
    let builder = state
        .protocol_globals
        .activation
        .builders
        .get_mut(&resource.id());
    if builder.as_ref().is_some_and(|builder| !builder.committed) {
        return builder;
    }
    resource.post_error(
        xdg_activation_token_v1::Error::AlreadyUsed,
        "activation token has already been committed or rejected",
    );
    None
}

impl RuntimeState {
    /// Mint an external token in the same one-shot authority used by clients.
    pub(crate) fn issue_spawn_activation_token(&mut self) -> io::Result<String> {
        self.protocol_globals.activation.issue_external_token()
    }

    fn activation_request_is_authorized(&self, client: &ClientId, builder: &TokenBuilder) -> bool {
        if !self.protocol_globals.activation.client_is_focused(client) {
            return false;
        }
        if builder.surface.as_ref().is_some_and(|surface| {
            surface
                .upgrade()
                .ok()
                .and_then(|surface| surface.client())
                .is_none_or(|owner| owner.id() != *client)
        }) {
            return false;
        }
        let Some((serial, seat)) = &builder.serial else {
            return true;
        };
        let Ok(seat) = seat.upgrade() else {
            return false;
        };
        if !self.seat.owns(&seat) {
            return false;
        }
        if self
            .protocol_globals
            .activation
            .interaction_matches(client, *serial)
        {
            return true;
        }
        let pointer_enter = self.seat.get_pointer().is_some_and(|pointer| {
            pointer.last_enter().map(u32::from) == Some(*serial)
                && self
                    .protocol_globals
                    .activation
                    .pointer_client_is_focused(client)
        });
        pointer_enter
            || self.seat.get_keyboard().is_some_and(|keyboard| {
                keyboard.last_enter().map(u32::from) == Some(*serial)
                    && self
                        .protocol_globals
                        .activation
                        .keyboard_client_is_focused(client)
            })
    }

    fn activate_surface_with_token(&mut self, token: &str, surface: WlSurface) {
        if !self.protocol_globals.activation.consume(token) {
            return;
        }
        #[cfg(feature = "tty")]
        {
            let Some(view) = self.view_for_surface(&surface) else {
                return;
            };
            let Some(window) = self.mapped_window_for_view(view) else {
                return;
            };
            let _ = self.focus_mapped_window(window, smithay::utils::SERIAL_COUNTER.next_serial());
            self.request_redraw_workspace();
        }
        #[cfg(not(feature = "tty"))]
        let _ = surface;
    }
}

fn random_token() -> io::Result<String> {
    super::random_handle::random_hex::<TOKEN_BYTES>()
}

delegate_global_dispatch!(RuntimeState, XdgActivationV1, ActivationGlobalData);
delegate_dispatch!(RuntimeState, XdgActivationV1, ActivationManagerData);
delegate_dispatch!(RuntimeState, XdgActivationTokenV1, ActivationTokenData);

#[cfg(test)]
mod tests {
    use super::*;
    use wayland_server::Display;

    #[test]
    fn random_tokens_are_fixed_width_hex_and_distinct() {
        let first = random_token().unwrap();
        let second = random_token().unwrap();
        assert_eq!(first.len(), TOKEN_BYTES * 2);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn expired_tokens_are_removed_when_consumed() {
        let display = Display::<RuntimeState>::new().unwrap();
        let mut protocol = ActivationProtocol::new(&display.handle());
        protocol.tokens.insert(
            "expired".to_owned(),
            Instant::now().checked_sub(TOKEN_TIMEOUT).unwrap(),
        );
        assert!(!protocol.consume("expired"));
        assert_eq!(protocol.token_count(), 0);
    }
}
