//! xdg-popup helpers on [`NativeShell`].

use std::sync::Arc;

use wayland_client::Proxy;
use wayland_protocols::xdg::shell::client::xdg_positioner;

use super::api::NativeShell;
use super::handle::NativeSurfaceLease;
use super::types::{NativePopupPositioner, NativeSurfaceId, PopupRecord};
use crate::native::connection::NativeError;
use crate::native::protocols::core::shm;
use crate::surface::{ConstraintAdjustments, Gravity, PopupAnchor};

impl NativeShell {
    /// Create a bufferless GPU-friendly popup (no solid SHM fill).
    ///
    /// When `grab` is `Some`, the popup is grabbed with that seat+serial.
    /// When `None`, no grab is requested.
    pub fn create_popup_gpu(
        &mut self,
        parent: NativeSurfaceId,
        positioner: &NativePopupPositioner,
        grab: Option<&crate::InputSerial>,
    ) -> Result<NativeSurfaceId, NativeError> {
        if positioner.size.width == 0 || positioner.size.height == 0 {
            return Err(NativeError::Protocol("popup size must be non-zero".into()));
        }
        let parent_xdg = self
            .state
            .toplevels
            .get(&parent)
            .map(|t| t.xdg.clone())
            .or_else(|| self.state.popups.get(&parent).map(|p| p.xdg.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown parent {parent:?}")))?;

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

        let pos = wm_base.create_positioner(&qh, ());
        apply_positioner(&pos, positioner);

        let wl = compositor.create_surface(&qh, ());
        wl.set_buffer_scale(1);
        let xdg = wm_base.get_xdg_surface(&wl, &qh, ());
        let popup = xdg.get_popup(Some(&parent_xdg), &pos, &qh, ());
        pos.destroy();

        if let Some(serial) = grab {
            popup.grab(serial.seat(), serial.serial());
        }

        let w = positioner.size.width;
        let h = positioner.size.height;
        wl.commit();

        let id = self.state.alloc_id();
        self.state
            .popup_objects
            .insert(popup.id().protocol_id(), id);
        self.state
            .xdg_surface_objects
            .insert(xdg.id().protocol_id(), id);
        self.state
            .wl_surface_objects
            .insert(wl.id().protocol_id(), id);
        let parent_lease = self
            .surface_lease(parent)
            .ok_or_else(|| NativeError::Protocol(format!("unknown popup parent {parent:?}")))?;
        let surface_lease = Arc::new(NativeSurfaceLease::popup(
            self.connection.connection().clone(),
            wl.clone(),
            id,
            xdg.clone(),
            popup.clone(),
            parent_lease,
        ));
        self.state.popups.insert(
            id,
            PopupRecord {
                surface_lease,
                wl,
                xdg,
                popup,
                parent,
                buffer: None,
                _pool: None,
                _file: None,
                configured: false,
                pending_geom: None,
                last_configure_serial: 0,
                pending_reposition_token: None,
                logical_w: w,
                logical_h: h,
            },
        );
        self.connection.mark_dirty();
        Ok(id)
    }
    /// Create an `xdg_popup` child of a configured toplevel (or another popup).
    ///
    /// When `grab` is `Some`, the popup is grabbed with that seat+serial.
    pub fn create_popup(
        &mut self,
        parent: NativeSurfaceId,
        positioner: &NativePopupPositioner,
        grab: Option<&crate::InputSerial>,
    ) -> Result<NativeSurfaceId, NativeError> {
        if positioner.size.width == 0 || positioner.size.height == 0 {
            return Err(NativeError::Protocol("popup size must be non-zero".into()));
        }
        let parent_xdg = self
            .state
            .toplevels
            .get(&parent)
            .map(|t| t.xdg.clone())
            .or_else(|| self.state.popups.get(&parent).map(|p| p.xdg.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown parent {parent:?}")))?;

        let qh = self.queue.handle();
        let compositor = self
            .state
            .compositor
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?;
        let shm = self
            .state
            .shm
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_shm".into()))?;
        let wm_base = self
            .state
            .wm_base
            .as_ref()
            .ok_or_else(|| NativeError::Registry("xdg_wm_base".into()))?;

        let pos = wm_base.create_positioner(&qh, ());
        apply_positioner(&pos, positioner);

        let wl = compositor.create_surface(&qh, ());
        wl.set_buffer_scale(1);
        let xdg = wm_base.get_xdg_surface(&wl, &qh, ());
        let popup = xdg.get_popup(Some(&parent_xdg), &pos, &qh, ());
        pos.destroy();

        if let Some(serial) = grab {
            popup.grab(serial.seat(), serial.serial());
        }

        let w = positioner.size.width;
        let h = positioner.size.height;
        let (file, pool, buffer) =
            shm::create_solid_buffer(shm, &qh, w, h, [0xff, 0x33, 0x33, 0x33])
                .map_err(|e| NativeError::Io(e.to_string()))?;
        wl.commit();

        let id = self.state.alloc_id();
        self.state
            .popup_objects
            .insert(popup.id().protocol_id(), id);
        self.state
            .xdg_surface_objects
            .insert(xdg.id().protocol_id(), id);
        self.state
            .wl_surface_objects
            .insert(wl.id().protocol_id(), id);
        let parent_lease = self
            .surface_lease(parent)
            .ok_or_else(|| NativeError::Protocol(format!("unknown popup parent {parent:?}")))?;
        let surface_lease = Arc::new(NativeSurfaceLease::popup(
            self.connection.connection().clone(),
            wl.clone(),
            id,
            xdg.clone(),
            popup.clone(),
            parent_lease,
        ));
        self.state.popups.insert(
            id,
            PopupRecord {
                surface_lease,
                wl,
                xdg,
                popup,
                parent,
                buffer: Some(buffer),
                _pool: Some(pool),
                _file: Some(file),
                configured: false,
                pending_geom: None,
                last_configure_serial: 0,
                pending_reposition_token: None,
                logical_w: w,
                logical_h: h,
            },
        );
        self.connection.mark_dirty();
        Ok(id)
    }

    /// Reposition an existing popup (`xdg_popup.reposition`, requires xdg_wm_base ≥ 3).
    pub fn reposition_popup(
        &mut self,
        id: NativeSurfaceId,
        positioner: &NativePopupPositioner,
        token: u32,
    ) -> Result<(), NativeError> {
        if self.state.wm_base_version < 3 {
            return Err(NativeError::Protocol(
                "xdg_popup.reposition requires xdg_wm_base version 3+".into(),
            ));
        }
        if positioner.size.width == 0 || positioner.size.height == 0 {
            return Err(NativeError::Protocol("popup size must be non-zero".into()));
        }
        let qh = self.queue.handle();
        let wm_base = self
            .state
            .wm_base
            .as_ref()
            .ok_or_else(|| NativeError::Registry("xdg_wm_base".into()))?;
        let record = self
            .state
            .popups
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown popup {id:?}")))?;
        let pos = wm_base.create_positioner(&qh, ());
        apply_positioner(&pos, positioner);
        record.popup.reposition(&pos, token);
        pos.destroy();
        if let Some(record) = self.state.popups.get_mut(&id) {
            record.pending_reposition_token = Some(token);
            record.logical_w = positioner.size.width;
            record.logical_h = positioner.size.height;
        }
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn supports_popup_reposition(&self) -> bool {
        self.state.wm_base_version >= 3
    }

    pub fn destroy_popup(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        self.state.cancel_touch_for_surface(id);
        self.state.clear_surface_protocol_state(id);
        let Some(record) = self.state.popups.remove(&id) else {
            return Err(NativeError::Protocol(format!("unknown popup {id:?}")));
        };
        self.state
            .popup_objects
            .remove(&record.popup.id().protocol_id());
        self.state
            .xdg_surface_objects
            .remove(&record.xdg.id().protocol_id());
        self.state
            .wl_surface_objects
            .remove(&record.wl.id().protocol_id());
        if let Some(buffer) = record.buffer {
            buffer.destroy();
        }
        if let Some(pool) = record._pool {
            pool.destroy();
        }
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn popup_count(&self) -> usize {
        self.state.popups.len()
    }

    pub fn is_popup_configured(&self, id: NativeSurfaceId) -> bool {
        self.state.popups.get(&id).is_some_and(|p| p.configured)
    }
}

fn apply_positioner(pos: &xdg_positioner::XdgPositioner, value: &NativePopupPositioner) {
    pos.set_size(value.size.width as i32, value.size.height as i32);
    pos.set_anchor_rect(
        value.anchor_rect.origin.x,
        value.anchor_rect.origin.y,
        value.anchor_rect.size.width as i32,
        value.anchor_rect.size.height as i32,
    );
    pos.set_anchor(map_anchor(value.anchor));
    pos.set_gravity(map_gravity(value.gravity));
    pos.set_constraint_adjustment(map_constraints(value.constraints));
    pos.set_offset(value.offset.x, value.offset.y);
}

fn map_anchor(value: PopupAnchor) -> xdg_positioner::Anchor {
    match value {
        PopupAnchor::None => xdg_positioner::Anchor::None,
        PopupAnchor::Top => xdg_positioner::Anchor::Top,
        PopupAnchor::Bottom => xdg_positioner::Anchor::Bottom,
        PopupAnchor::Left => xdg_positioner::Anchor::Left,
        PopupAnchor::Right => xdg_positioner::Anchor::Right,
        PopupAnchor::TopLeft => xdg_positioner::Anchor::TopLeft,
        PopupAnchor::BottomLeft => xdg_positioner::Anchor::BottomLeft,
        PopupAnchor::TopRight => xdg_positioner::Anchor::TopRight,
        PopupAnchor::BottomRight => xdg_positioner::Anchor::BottomRight,
    }
}

fn map_gravity(value: Gravity) -> xdg_positioner::Gravity {
    match value {
        Gravity::None => xdg_positioner::Gravity::None,
        Gravity::Top => xdg_positioner::Gravity::Top,
        Gravity::Bottom => xdg_positioner::Gravity::Bottom,
        Gravity::Left => xdg_positioner::Gravity::Left,
        Gravity::Right => xdg_positioner::Gravity::Right,
        Gravity::TopLeft => xdg_positioner::Gravity::TopLeft,
        Gravity::BottomLeft => xdg_positioner::Gravity::BottomLeft,
        Gravity::TopRight => xdg_positioner::Gravity::TopRight,
        Gravity::BottomRight => xdg_positioner::Gravity::BottomRight,
    }
}

fn map_constraints(value: ConstraintAdjustments) -> xdg_positioner::ConstraintAdjustment {
    let mut result = xdg_positioner::ConstraintAdjustment::empty();
    if value.contains(ConstraintAdjustments::SLIDE_X) {
        result |= xdg_positioner::ConstraintAdjustment::SlideX;
    }
    if value.contains(ConstraintAdjustments::SLIDE_Y) {
        result |= xdg_positioner::ConstraintAdjustment::SlideY;
    }
    if value.contains(ConstraintAdjustments::FLIP_X) {
        result |= xdg_positioner::ConstraintAdjustment::FlipX;
    }
    if value.contains(ConstraintAdjustments::FLIP_Y) {
        result |= xdg_positioner::ConstraintAdjustment::FlipY;
    }
    if value.contains(ConstraintAdjustments::RESIZE_X) {
        result |= xdg_positioner::ConstraintAdjustment::ResizeX;
    }
    if value.contains(ConstraintAdjustments::RESIZE_Y) {
        result |= xdg_positioner::ConstraintAdjustment::ResizeY;
    }
    result
}
