//! Linux dmabuf methods on [`NativeRuntime`].

use crate::runtime_common::RuntimeError;
use crate::surface::SurfaceId;

use super::runtime_facade::{map_native_error, NativeRuntime};

impl NativeRuntime {
    pub fn has_linux_dmabuf(&self) -> bool {
        self.shell.has_linux_dmabuf()
    }

    pub fn linux_dmabuf_version(&self) -> Option<u32> {
        self.shell.linux_dmabuf_version()
    }

    pub fn dmabuf_modifiers(&self) -> &[crate::dmabuf::DmabufFormat] {
        self.shell.dmabuf_modifiers()
    }

    pub fn dmabuf_default_feedback(&self) -> Option<&crate::dmabuf::DmabufFeedback> {
        self.shell.dmabuf_default_feedback()
    }

    pub fn dmabuf_surface_feedback(
        &self,
        surface: SurfaceId,
    ) -> Option<&crate::dmabuf::DmabufFeedback> {
        let native = self.native_ids.get(&surface).copied()?;
        self.shell.dmabuf_surface_feedback(native)
    }

    pub fn request_dmabuf_default_feedback(&mut self) -> Result<(), RuntimeError> {
        if !self.shell.has_linux_dmabuf() {
            return Err(RuntimeError::Unsupported("zwp_linux_dmabuf_v1"));
        }
        self.shell
            .request_dmabuf_default_feedback()
            .map_err(map_native_error)
    }

    pub fn request_dmabuf_surface_feedback(
        &mut self,
        surface: SurfaceId,
    ) -> Result<(), RuntimeError> {
        if !self.shell.has_linux_dmabuf() {
            return Err(RuntimeError::Unsupported("zwp_linux_dmabuf_v1"));
        }
        let native = self.native(surface)?;
        self.shell
            .request_dmabuf_surface_feedback(native)
            .map_err(map_native_error)
    }

    pub fn create_dmabuf_buffer(
        &mut self,
        params: crate::dmabuf::DmabufBufferParams,
    ) -> Result<(), RuntimeError> {
        if !self.shell.has_linux_dmabuf() {
            return Err(RuntimeError::Unsupported("zwp_linux_dmabuf_v1"));
        }
        self.shell
            .create_dmabuf_buffer(params)
            .map_err(map_native_error)
    }

    pub fn create_dmabuf_buffer_immed(
        &mut self,
        params: crate::dmabuf::DmabufBufferParams,
    ) -> Result<crate::dmabuf::DmabufBufferId, RuntimeError> {
        if !self.shell.has_linux_dmabuf() {
            return Err(RuntimeError::Unsupported("zwp_linux_dmabuf_v1"));
        }
        self.shell
            .create_dmabuf_buffer_immed(params)
            .map_err(map_native_error)
    }

    pub fn attach_dmabuf_buffer(
        &mut self,
        surface: SurfaceId,
        buffer: crate::dmabuf::DmabufBufferId,
        x: i32,
        y: i32,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .attach_dmabuf_buffer(native, buffer, x, y)
            .map_err(map_native_error)
    }

    pub fn destroy_dmabuf_buffer(
        &mut self,
        buffer: crate::dmabuf::DmabufBufferId,
    ) -> Result<(), RuntimeError> {
        self.shell
            .destroy_dmabuf_buffer(buffer)
            .map_err(map_native_error)
    }
}
