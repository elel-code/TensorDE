//! xdg-activation helpers on [`NativeShell`].

use wayland_client::Proxy;

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::native::connection::NativeError;

impl NativeShell {
    pub fn has_activation(&self) -> bool {
        self.state.activation.is_some()
    }

    /// Request an `xdg_activation_v1` token for `surface`.
    ///
    /// Completes with [`NativeShellEvent::ActivationToken`].
    pub fn request_activation_token(
        &mut self,
        surface: NativeSurfaceId,
        app_id: Option<&str>,
    ) -> Result<(), NativeError> {
        let activation = self
            .state
            .activation
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("xdg_activation_v1 missing".into()))?;
        let wl = self
            .state
            .toplevels
            .get(&surface)
            .map(|t| t.wl.clone())
            .or_else(|| self.state.popups.get(&surface).map(|p| p.wl.clone()))
            .or_else(|| self.state.layers.get(&surface).map(|l| l.wl.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {surface:?}")))?;
        let qh = self.queue.handle();
        let token = activation.get_activation_token(&qh, ());
        if let Some(app_id) = app_id {
            token.set_app_id(app_id.to_string());
        }
        if let (Some(serial), Some(seat)) =
            (self.state.last_input_serial, self.state.seat.as_ref())
        {
            token.set_serial(serial, seat);
        }
        token.set_surface(&wl);
        token.commit();
        let obj_id = token.id().protocol_id();
        self.state
            .activation_tokens
            .insert(obj_id, (surface, token));
        self.connection.mark_dirty();
        Ok(())
    }

    /// Activate `surface` with a previously obtained token string.
    pub fn activate_with_token(
        &mut self,
        surface: NativeSurfaceId,
        token: impl Into<String>,
    ) -> Result<(), NativeError> {
        let activation = self
            .state
            .activation
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("xdg_activation_v1 missing".into()))?;
        let wl = self
            .state
            .toplevels
            .get(&surface)
            .map(|t| t.wl.clone())
            .or_else(|| self.state.popups.get(&surface).map(|p| p.wl.clone()))
            .or_else(|| self.state.layers.get(&surface).map(|l| l.wl.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {surface:?}")))?;
        activation.activate(token.into(), &wl);
        self.connection.mark_dirty();
        Ok(())
    }
}
