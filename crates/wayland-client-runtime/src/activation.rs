use crate::{InputSerial, SurfaceId};
use smithay_client_toolkit::dispatch2::Dispatch2;
use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_token_v1::{
    Event as ActivationTokenEvent, XdgActivationTokenV1,
};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;

/// Correlates an asynchronous xdg-activation token response with its request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ActivationRequestId(pub(crate) u64);

impl ActivationRequestId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// An opaque compositor-issued token used to activate a surface.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ActivationToken(String);

impl ActivationToken {
    /// Wrap a token received through an external channel such as
    /// `XDG_ACTIVATION_TOKEN` or D-Bus platform data.
    pub fn from_raw(token: String) -> Self {
        Self(token)
    }

    /// Borrow the token for transport through an external channel.
    pub fn as_raw(&self) -> &str {
        &self.0
    }

    /// Consume the wrapper and return its protocol string.
    pub fn into_raw(self) -> String {
        self.0
    }
}

/// Optional metadata used by the compositor to validate and identify an
/// activation-token request.
#[derive(Clone, Debug, Default)]
pub struct ActivationTokenAttributes {
    /// Application ID of the application that will be activated.
    pub app_id: Option<String>,
    /// Input or focus serial that triggered the activation request.
    pub serial: Option<InputSerial>,
}

#[derive(Clone, Debug)]
pub enum ActivationEvent {
    /// The compositor completed an earlier token request. The token may be
    /// forwarded to another client over `XDG_ACTIVATION_TOKEN`, D-Bus, or IPC.
    TokenDone {
        request: ActivationRequestId,
        requesting_surface: SurfaceId,
        token: ActivationToken,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ActivationTokenPurpose {
    Export {
        request: ActivationRequestId,
        surface: SurfaceId,
    },
    Attention {
        surface: SurfaceId,
    },
}

pub(crate) trait ActivationHandler {
    fn activation_token_done(&mut self, purpose: ActivationTokenPurpose, token: String);
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ManagerData;

impl<D> Dispatch2<XdgActivationV1, D> for ManagerData {
    fn event(
        &self,
        _: &mut D,
        _: &XdgActivationV1,
        _: <XdgActivationV1 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        unreachable!("xdg_activation_v1 has no events");
    }
}

#[derive(Debug)]
pub(crate) struct TokenData {
    purpose: ActivationTokenPurpose,
}

impl<D> Dispatch2<XdgActivationTokenV1, D> for TokenData
where
    D: ActivationHandler,
{
    fn event(
        &self,
        state: &mut D,
        proxy: &XdgActivationTokenV1,
        event: ActivationTokenEvent,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        let ActivationTokenEvent::Done { token } = event else {
            return;
        };
        state.activation_token_done(self.purpose, token);
        if proxy.is_alive() {
            proxy.destroy();
        }
    }
}

#[derive(Debug)]
pub(crate) struct ActivationManager {
    manager: XdgActivationV1,
}

impl ActivationManager {
    pub(crate) fn bind<D>(
        globals: &GlobalList,
        queue_handle: &QueueHandle<D>,
    ) -> Result<Self, BindError>
    where
        D: Dispatch<XdgActivationV1, ManagerData> + 'static,
    {
        let manager = globals.bind(queue_handle, 1..=1, ManagerData)?;
        Ok(Self { manager })
    }

    pub(crate) fn request_token<D>(
        &self,
        queue_handle: &QueueHandle<D>,
        purpose: ActivationTokenPurpose,
        requesting_surface: &WlSurface,
        attributes: ActivationTokenAttributes,
    ) where
        D: Dispatch<XdgActivationTokenV1, TokenData> + 'static,
    {
        let token = self
            .manager
            .get_activation_token(queue_handle, TokenData { purpose });
        if let Some(app_id) = attributes.app_id {
            token.set_app_id(app_id);
        }
        if let Some(serial) = attributes.serial {
            token.set_serial(serial.serial, &serial.seat);
        }
        token.set_surface(requesting_surface);
        token.commit();
    }

    pub(crate) fn activate(&self, surface: &WlSurface, token: ActivationToken) {
        self.manager.activate(token.into_raw(), surface);
    }
}

impl Drop for ActivationManager {
    fn drop(&mut self) {
        if self.manager.is_alive() {
            self.manager.destroy();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_token_round_trips_external_representation() {
        let token = ActivationToken::from_raw("compositor-token".to_string());
        assert_eq!(token.as_raw(), "compositor-token");
        assert_eq!(token.into_raw(), "compositor-token");
    }

    #[test]
    fn activation_request_id_exposes_stable_value() {
        assert_eq!(ActivationRequestId(42).get(), 42);
    }
}
