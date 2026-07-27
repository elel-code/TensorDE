//! Text input, blur, icon, and attention methods on [`NativeRuntime`].

use crate::native::connection::NativeError;
use crate::runtime_common::RuntimeError;
use crate::surface::SurfaceId;
use crate::{BlurState, TextInputState, ToplevelIcon};

use super::runtime_facade::{map_native_error, NativeRuntime};

impl NativeRuntime {
    pub fn set_text_input_state(
        &mut self,
        surface: SurfaceId,
        state: Option<&TextInputState>,
    ) -> Result<(), RuntimeError> {
        if !self.shell.has_text_input() {
            return Err(RuntimeError::Unsupported("text_input_v3"));
        }
        let native = self.native(surface)?;
        match state {
            Some(state) => self
                .shell
                .set_text_input_state(native, state)
                .map_err(map_native_error),
            None => self.shell.disable_text_input().map_err(map_native_error),
        }
    }

    pub fn request_user_attention(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        if !self.shell.has_activation() {
            return Err(RuntimeError::Unsupported("xdg_activation_v1"));
        }
        let native = self.native(surface)?;
        self.shell
            .request_activation_token(native, None)
            .map_err(map_native_error)
    }

    pub fn set_blur(&mut self, surface: SurfaceId, state: BlurState) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell.set_blur(native, state).map_err(|e| match e {
            NativeError::Protocol(msg) if msg.contains("blur capability") => {
                RuntimeError::Unsupported("ext-background-effect-v1 blur")
            }
            other => map_native_error(other),
        })
    }

    pub fn set_toplevel_icon(
        &mut self,
        surface: SurfaceId,
        icon: Option<ToplevelIcon>,
    ) -> Result<(), RuntimeError> {
        if !self.shell.has_toplevel_icon() {
            return Err(RuntimeError::Unsupported("xdg-toplevel-icon-v1"));
        }
        let native = self.native(surface)?;
        self.shell
            .set_toplevel_icon(native, icon)
            .map_err(map_native_error)
    }
}
