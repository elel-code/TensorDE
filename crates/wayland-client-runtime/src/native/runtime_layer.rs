//! Layer-shell methods on [`NativeRuntime`].

use crate::layer_shell::{LayerSurfaceAttributes, LayerSurfaceState};
use crate::native::connection::NativeError;
use crate::runtime_common::RuntimeError;
use crate::surface::SurfaceId;

use super::runtime_facade::{NativeRuntime, map_native_error};

impl NativeRuntime {
    pub fn create_layer_surface(
        &mut self,
        attributes: LayerSurfaceAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        if !self.shell.has_layer_shell() {
            return Err(RuntimeError::Unsupported("layer-shell-v1"));
        }
        let output = attributes.output.map(|o| o.get());
        let native = self
            .shell
            .create_layer_surface_full(attributes.namespace, output, attributes.state)
            .map_err(map_native_error)?;
        let public = self.surfaces.intern(native);
        self.native_ids.insert(public, native);
        Ok(public)
    }

    /// Bufferless layer surface for Vulkan / wgpu swapchain present (no SHM fill).
    ///
    /// Prefer this over [`Self::create_layer_surface`] when the client owns
    /// GPU present (e.g. wallpaper engines, custom Vulkan WSI).
    pub fn create_layer_surface_gpu(
        &mut self,
        attributes: LayerSurfaceAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        if !self.shell.has_layer_shell() {
            return Err(RuntimeError::Unsupported("layer-shell-v1"));
        }
        let output = attributes.output.map(|o| o.get());
        let native = self
            .shell
            .create_layer_surface_gpu(attributes.namespace, output, attributes.state)
            .map_err(map_native_error)?;
        let public = self.surfaces.intern(native);
        self.native_ids.insert(public, native);
        Ok(public)
    }

    pub fn set_layer_surface_state(
        &mut self,
        surface: SurfaceId,
        state: LayerSurfaceState,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_layer_surface_state(native, state)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("unknown layer") => {
                    RuntimeError::InvalidLayerSurfaceTarget(surface)
                }
                other => map_native_error(other),
            })
    }

    pub fn layer_surface_state(
        &self,
        surface: SurfaceId,
    ) -> Result<LayerSurfaceState, RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .layer_surface_state(native)
            .map_err(|_| RuntimeError::InvalidLayerSurfaceTarget(surface))
    }
}
