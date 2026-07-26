//! Toplevel / surface lifecycle methods for [`NativeShell`].

use super::api::NativeShell;
use super::types::{NativeSurfaceId, ToplevelRecord};
use crate::native::connection::NativeError;
use crate::native::protocols::core::shm;
use wayland_client::Proxy;

impl NativeShell {
    pub fn create_toplevel(
        &mut self,
        title: impl Into<String>,
        app_id: impl Into<String>,
    ) -> Result<NativeSurfaceId, NativeError> {
        self.create_toplevel_sized(title, app_id, 640, 480, [0xff, 0x22, 0x66, 0xcc])
    }

    /// Create a toplevel with a solid SHM buffer (smoke / fallback).
    pub fn create_toplevel_sized(
        &mut self,
        title: impl Into<String>,
        app_id: impl Into<String>,
        width: u32,
        height: u32,
        argb: [u8; 4],
    ) -> Result<NativeSurfaceId, NativeError> {
        self.create_toplevel_inner(title, app_id, width, height, Some(argb))
    }

    /// Create a bufferless toplevel for wgpu / Vulkan (initial commit without attach).
    pub fn create_toplevel_gpu(
        &mut self,
        title: impl Into<String>,
        app_id: impl Into<String>,
        width: u32,
        height: u32,
    ) -> Result<NativeSurfaceId, NativeError> {
        self.create_toplevel_inner(title, app_id, width.max(1), height.max(1), None)
    }

    fn create_toplevel_inner(
        &mut self,
        title: impl Into<String>,
        app_id: impl Into<String>,
        width: u32,
        height: u32,
        solid_argb: Option<[u8; 4]>,
    ) -> Result<NativeSurfaceId, NativeError> {
        let qh = self.queue.handle();
        let compositor = self
            .state
            .compositor
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?;
        let wm_base = self
            .state
            .wm_base
            .as_ref()
            .ok_or_else(|| NativeError::Registry("xdg_wm_base".into()))?;

        let wl = compositor.create_surface(&qh, ());
        // Fractional-scale clients keep buffer_scale at 1.
        wl.set_buffer_scale(1);

        let viewport = self
            .state
            .viewporter
            .as_ref()
            .map(|vp| vp.get_viewport(&wl, &qh, ()));
        if let Some(vp) = viewport.as_ref() {
            vp.set_destination(width as i32, height as i32);
        }

        let fractional = self
            .state
            .fractional_manager
            .as_ref()
            .map(|mgr| mgr.get_fractional_scale(&wl, &qh, ()));

        let (file, pool, buffer) = if let Some(argb) = solid_argb {
            let shm = self
                .state
                .shm
                .as_ref()
                .ok_or_else(|| NativeError::Registry("wl_shm".into()))?;
            let (file, pool, buffer) = shm::create_solid_buffer(shm, &qh, width, height, argb)
                .map_err(|e| NativeError::Io(e.to_string()))?;
            (Some(file), Some(pool), Some(buffer))
        } else {
            (None, None, None)
        };
        let xdg = wm_base.get_xdg_surface(&wl, &qh, ());
        let toplevel = xdg.get_toplevel(&qh, ());
        toplevel.set_title(title.into());
        toplevel.set_app_id(app_id.into());
        wl.commit();

        let id = self.state.alloc_id();
        self.state
            .toplevel_objects
            .insert(toplevel.id().protocol_id(), id);
        self.state
            .xdg_surface_objects
            .insert(xdg.id().protocol_id(), id);
        self.state
            .wl_surface_objects
            .insert(wl.id().protocol_id(), id);
        if let Some(ref frac) = fractional {
            self.state
                .fractional_objects
                .insert(frac.id().protocol_id(), id);
        }
        self.state.toplevels.insert(
            id,
            ToplevelRecord {
                wl,
                xdg,
                toplevel,
                buffer,
                _pool: pool,
                _file: file,
                viewport,
                fractional,
                configured: false,
                pending_size: Some((width as i32, height as i32)),
                logical_w: width,
                logical_h: height,
                scale_factor: 1.0,
            },
        );
        self.connection.flush()?;
        Ok(id)
    }

    pub fn commit_surface(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.wl.commit();
        self.connection.flush()?;
        Ok(())
    }

    pub fn set_buffer_scale(
        &mut self,
        id: NativeSurfaceId,
        factor: i32,
    ) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.wl.set_buffer_scale(factor.max(1));
        self.connection.flush()?;
        Ok(())
    }

    pub fn set_viewport_destination(
        &mut self,
        id: NativeSurfaceId,
        width: i32,
        height: i32,
    ) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get_mut(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        if width > 0 {
            record.logical_w = width as u32;
        }
        if height > 0 {
            record.logical_h = height as u32;
        }
        if let Some(vp) = record.viewport.as_ref() {
            vp.set_destination(width.max(1), height.max(1));
        }
        self.connection.flush()?;
        Ok(())
    }

    /// Public crate [`SurfaceHandle`] for wgpu (wraps native handle).
    pub fn public_surface_handle(
        &self,
        id: NativeSurfaceId,
    ) -> Result<crate::SurfaceHandle, NativeError> {
        let native = self.surface_handle(id)?;
        Ok(crate::SurfaceHandle::from_native(native))
    }

}
