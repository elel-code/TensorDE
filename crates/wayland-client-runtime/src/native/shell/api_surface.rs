//! Toplevel / surface lifecycle methods for [`NativeShell`].

use super::api::NativeShell;
use super::types::{NativeSurfaceId, ToplevelRecord};
use crate::geometry::{LogicalPosition, LogicalSize};
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
        self.create_toplevel_inner(title, app_id, width, height, Some(argb), None, false)
    }

    /// Create a bufferless toplevel for wgpu / Vulkan (initial commit without attach).
    pub fn create_toplevel_gpu(
        &mut self,
        title: impl Into<String>,
        app_id: impl Into<String>,
        width: u32,
        height: u32,
    ) -> Result<NativeSurfaceId, NativeError> {
        self.create_toplevel_inner(
            title,
            app_id,
            width.max(1),
            height.max(1),
            None,
            None,
            false,
        )
    }

    /// Create a parented dialog toplevel (xdg_toplevel.set_parent + optional xdg_dialog modal).
    pub fn create_dialog_gpu(
        &mut self,
        parent: NativeSurfaceId,
        title: impl Into<String>,
        app_id: impl Into<String>,
        width: u32,
        height: u32,
        modal: bool,
    ) -> Result<NativeSurfaceId, NativeError> {
        if !self.state.toplevels.contains_key(&parent) {
            return Err(NativeError::Protocol(format!(
                "unknown parent toplevel {parent:?}"
            )));
        }
        self.create_toplevel_inner(
            title,
            app_id,
            width.max(1),
            height.max(1),
            None,
            Some(parent),
            modal,
        )
    }

    #[allow(clippy::too_many_arguments)] // shared path for GPU / SHM / dialog roles
    fn create_toplevel_inner(
        &mut self,
        title: impl Into<String>,
        app_id: impl Into<String>,
        width: u32,
        height: u32,
        solid_argb: Option<[u8; 4]>,
        parent: Option<NativeSurfaceId>,
        modal: bool,
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

        let title_str = title.into();
        let app_id_str = app_id.into();

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
        toplevel.set_title(title_str.clone());
        toplevel.set_app_id(app_id_str);

        if let Some(parent_id) = parent
            && let Some(parent_rec) = self.state.toplevels.get(&parent_id)
        {
            toplevel.set_parent(Some(&parent_rec.toplevel));
        }

        let dialog = if modal {
            if let Some(wm_dialog) = self.state.xdg_wm_dialog.as_ref() {
                let d = wm_dialog.get_xdg_dialog(&toplevel, &qh, ());
                d.set_modal();
                Some(d)
            } else {
                None
            }
        } else if parent.is_some() {
            // Non-modal dialog still benefits from xdg_dialog when available.
            self.state
                .xdg_wm_dialog
                .as_ref()
                .map(|wm_dialog| wm_dialog.get_xdg_dialog(&toplevel, &qh, ()))
        } else {
            None
        };

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
                dialog,
                parent,
                buffer,
                _pool: pool,
                _file: file,
                viewport,
                fractional,
                icon_shm: Vec::new(),
                blur_effect: None,
                pending_blur: None,
                decoration: None,
                decoration_mode: None,
                decorations_preference: crate::surface::DecorationPreference::Server,
                pointer_capture: crate::PointerCaptureState::default(),
                configured: false,
                pending_size: Some((width as i32, height as i32)),
                pending_states: crate::ToplevelState::empty(),
                last_configure_serial: 0,
                logical_w: width,
                logical_h: height,
                scale_factor: 1.0,
                title: title_str,
            },
        );
        // Prepare CSD as fallback when compositor has no SSD / user later asks Client.
        let _ = self.sync_csd_for(id);
        self.connection.mark_dirty();
        Ok(id)
    }

    pub fn commit_surface(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        let wl = self
            .state
            .wl_surface(id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        wl.commit();
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn set_buffer_scale(
        &mut self,
        id: NativeSurfaceId,
        factor: i32,
    ) -> Result<(), NativeError> {
        let factor = factor.max(1);
        let wl = self
            .state
            .wl_surface(id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        wl.set_buffer_scale(factor);
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn set_viewport_destination(
        &mut self,
        id: NativeSurfaceId,
        width: i32,
        height: i32,
    ) -> Result<(), NativeError> {
        let w = width.max(1);
        let h = height.max(1);
        if let Some(record) = self.state.toplevels.get_mut(&id) {
            if width > 0 {
                record.logical_w = width as u32;
            }
            if height > 0 {
                record.logical_h = height as u32;
            }
            if let Some(vp) = record.viewport.as_ref() {
                vp.set_destination(w, h);
            }
            self.connection.mark_dirty();
            return Ok(());
        }
        if let Some(record) = self.state.layers.get_mut(&id) {
            if width > 0 {
                record.logical_w = width as u32;
                record.state.size.width = width as u32;
            }
            if height > 0 {
                record.logical_h = height as u32;
                record.state.size.height = height as u32;
            }
            if let Some(vp) = record.viewport.as_ref() {
                vp.set_destination(w, h);
            }
            self.connection.mark_dirty();
            return Ok(());
        }
        Err(NativeError::Protocol(format!("unknown surface {id:?}")))
    }

    pub fn set_min_size(
        &mut self,
        id: NativeSurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        let (w, h) = size
            .map(|s| (s.width as i32, s.height as i32))
            .unwrap_or((0, 0));
        record.toplevel.set_min_size(w, h);
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn set_max_size(
        &mut self,
        id: NativeSurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        let (w, h) = size
            .map(|s| (s.width as i32, s.height as i32))
            .unwrap_or((0, 0));
        record.toplevel.set_max_size(w, h);
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn set_window_geometry(
        &mut self,
        id: NativeSurfaceId,
        origin: LogicalPosition,
        size: LogicalSize,
    ) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.xdg.set_window_geometry(
            origin.x,
            origin.y,
            size.width.max(1) as i32,
            size.height.max(1) as i32,
        );
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn set_parent(
        &mut self,
        id: NativeSurfaceId,
        parent: Option<NativeSurfaceId>,
    ) -> Result<(), NativeError> {
        let parent_toplevel = match parent {
            Some(pid) => {
                let p = self
                    .state
                    .toplevels
                    .get(&pid)
                    .ok_or_else(|| NativeError::Protocol(format!("unknown parent {pid:?}")))?;
                Some(p.toplevel.clone())
            }
            None => None,
        };
        let record = self
            .state
            .toplevels
            .get_mut(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.toplevel.set_parent(parent_toplevel.as_ref());
        record.parent = parent;
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn set_dialog_modal(
        &mut self,
        id: NativeSurfaceId,
        modal: bool,
    ) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        let Some(dialog) = record.dialog.as_ref() else {
            return Err(NativeError::Protocol(
                "surface has no xdg_dialog_v1 role".into(),
            ));
        };
        if modal {
            dialog.set_modal();
        } else {
            dialog.unset_modal();
        }
        self.connection.mark_dirty();
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

    pub fn destroy_toplevel(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        // Drop idle inhibitor before the surface goes away.
        let _ = self.set_idle_inhibit(id, false);
        // Cancel any live touch points on this surface before proxies die.
        self.state.cancel_touch_for_surface(id);
        self.state.clear_surface_protocol_state(id);
        // Drop surface-scoped dmabuf feedback.
        if let Some(fb) = self.state.dmabuf_surface_feedback_objs.remove(&id) {
            let pid = fb.id().protocol_id();
            self.state.dmabuf_feedback_surfaces.remove(&pid);
            self.state.dmabuf_feedback_pending.remove(&pid);
            self.state.dmabuf_tranche_pending.remove(&pid);
            fb.destroy();
        }
        self.state.dmabuf_surface_feedback.remove(&id);
        // Destroy child popups first (parent must outlive them for some compositors).
        let child_popups: Vec<_> = self
            .state
            .popups
            .iter()
            .filter(|(_, popup)| popup.parent == id)
            .map(|(&popup_id, _)| popup_id)
            .collect();
        for popup_id in child_popups {
            let _ = self.destroy_popup(popup_id);
        }

        // Also destroy child dialog toplevels that named us as parent.
        let child_dialogs: Vec<_> = self
            .state
            .toplevels
            .iter()
            .filter(|(_, toplevel)| toplevel.parent == Some(id))
            .map(|(&child_id, _)| child_id)
            .collect();
        for child_id in child_dialogs {
            let _ = self.destroy_toplevel(child_id);
        }

        self.destroy_csd(id);
        let Some(record) = self.state.toplevels.remove(&id) else {
            return Err(NativeError::Protocol(format!("unknown surface {id:?}")));
        };
        self.state.clear_live_constraints_for(id);
        self.state
            .toplevel_objects
            .remove(&record.toplevel.id().protocol_id());
        self.state
            .xdg_surface_objects
            .remove(&record.xdg.id().protocol_id());
        self.state
            .wl_surface_objects
            .remove(&record.wl.id().protocol_id());
        if let Some(dialog) = record.dialog {
            dialog.destroy();
        }
        if let Some(effect) = record.blur_effect {
            effect.destroy();
        }
        if let Some(decoration) = record.decoration {
            self.state
                .decoration_objects
                .remove(&decoration.id().protocol_id());
            decoration.destroy();
        }
        for (_file, pool, buffer) in record.icon_shm {
            buffer.destroy();
            pool.destroy();
        }
        if let Some(fractional) = record.fractional {
            self.state
                .fractional_objects
                .remove(&fractional.id().protocol_id());
            fractional.destroy();
        }
        if let Some(viewport) = record.viewport {
            viewport.destroy();
        }
        record.toplevel.destroy();
        record.xdg.destroy();
        if let Some(buffer) = record.buffer {
            buffer.destroy();
        }
        if let Some(pool) = record._pool {
            pool.destroy();
        }
        record.wl.destroy();
        self.connection.mark_dirty();
        Ok(())
    }
}
