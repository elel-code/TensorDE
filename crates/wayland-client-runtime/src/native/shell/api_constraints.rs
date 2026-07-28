//! Pointer constraints (lock / confine) for [`NativeShell`].

use wayland_client::Proxy;
use wayland_client::protocol::wl_compositor;
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_pointer_constraints_v1::Lifetime;

use super::api::NativeShell;
use super::types::{NativeShellState, NativeSurfaceId};
use crate::native::connection::NativeError;
use crate::pointer_constraints::{PointerCaptureState, PointerConstraint, PointerConstraintRegion};

impl NativeShell {
    /// Retain desired pointer capture for a surface and apply if focused.
    pub fn set_pointer_capture_state(
        &mut self,
        id: NativeSurfaceId,
        state: PointerCaptureState,
    ) -> Result<(), NativeError> {
        if state.constraint != PointerConstraint::None && self.state.pointer_constraints.is_none() {
            return Err(NativeError::Protocol(
                "zwp_pointer_constraints_v1 missing".into(),
            ));
        }
        if state.relative_motion && self.state.relative_pointer_manager.is_none() {
            return Err(NativeError::Protocol(
                "zwp_relative_pointer_manager_v1 missing".into(),
            ));
        }
        let record = self
            .state
            .toplevels
            .get_mut(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        if record.pointer_capture == state {
            return Ok(());
        }
        record.pointer_capture = state.clone();

        let want_relative = state.relative_motion || state.constraint == PointerConstraint::Locked;
        if want_relative {
            self.state.relative_pointer_wanted = true;
            let _ = self.ensure_all_seat_relative_pointers();
        }

        if self.state.pointer_focus == Some(id) {
            self.state
                .apply_pointer_constraint(id, &self.queue.handle())?;
        } else {
            self.state.clear_live_constraints_for(id);
        }
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn set_pointer_constraint(
        &mut self,
        id: NativeSurfaceId,
        constraint: PointerConstraint,
    ) -> Result<(), NativeError> {
        let mut state = self
            .state
            .toplevels
            .get(&id)
            .map(|r| r.pointer_capture.clone())
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        state.constraint = constraint;
        self.set_pointer_capture_state(id, state)
    }

    pub fn set_relative_pointer_enabled(
        &mut self,
        id: NativeSurfaceId,
        enabled: bool,
    ) -> Result<(), NativeError> {
        let mut state = self
            .state
            .toplevels
            .get(&id)
            .map(|r| r.pointer_capture.clone())
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        state.relative_motion = enabled;
        self.set_pointer_capture_state(id, state)
    }

    /// Enable `zwp_relative_pointer_v1` on every seat that has a pointer.
    ///
    /// Events are tagged with the owning seat via
    /// [`NativeShellEvent::RelativePointer::seat`].
    pub fn enable_relative_pointer(&mut self) -> Result<(), NativeError> {
        self.state.relative_pointer_wanted = true;
        self.ensure_all_seat_relative_pointers()
    }

    /// Destroy all per-seat relative pointer objects.
    pub fn disable_relative_pointer(&mut self) -> Result<(), NativeError> {
        self.state.relative_pointer_wanted = false;
        self.state.clear_all_relative_pointers();
        self.connection.mark_dirty();
        Ok(())
    }

    /// Bind relative-pointer objects for every seat pointer that lacks one.
    pub(crate) fn ensure_all_seat_relative_pointers(&mut self) -> Result<(), NativeError> {
        if !self.state.relative_pointer_wanted {
            return Ok(());
        }
        let Some(manager) = self.state.relative_pointer_manager.clone() else {
            return Err(NativeError::Protocol(
                "relative_pointer_manager missing".into(),
            ));
        };
        let qh = self.queue.handle();
        let primary = self.primary_seat_id().map(|id| id.get());
        let seats: Vec<(u32, wayland_client::protocol::wl_pointer::WlPointer)> = self
            .state
            .seats
            .iter()
            .filter_map(|(g, rec)| {
                if rec.relative_pointer.is_some() {
                    return None;
                }
                rec.pointer.as_ref().map(|p| (*g, p.clone()))
            })
            .collect();
        if seats.is_empty() && self.state.relative_pointer.is_none() {
            // No pointers yet; succeed so callers can enable before seats arrive.
            return Ok(());
        }
        for (global, pointer) in seats {
            let rel = manager.get_relative_pointer(&pointer, &qh, ());
            self.state
                .relative_pointer_objects
                .insert(rel.id().protocol_id(), global);
            if primary == Some(global) {
                self.state.relative_pointer = Some(rel.clone());
            }
            if let Some(rec) = self.state.seats.get_mut(&global) {
                rec.relative_pointer = Some(rel);
            }
        }
        // Keep shell-wide mirror on primary if still empty.
        if self.state.relative_pointer.is_none()
            && let Some(primary_id) = primary
            && let Some(rec) = self.state.seats.get(&primary_id)
        {
            self.state.relative_pointer = rec.relative_pointer.clone();
        }
        self.connection.mark_dirty();
        Ok(())
    }
}

impl NativeShellState {
    pub(crate) fn clear_all_relative_pointers(&mut self) {
        for rec in self.seats.values_mut() {
            if let Some(rel) = rec.relative_pointer.take() {
                self.relative_pointer_objects
                    .remove(&rel.id().protocol_id());
                rel.destroy();
            }
        }
        self.relative_pointer = None;
        self.relative_pointer_objects.clear();
    }

    pub(crate) fn seat_for_relative_pointer(
        &self,
        rel: &wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::ZwpRelativePointerV1,
    ) -> Option<u32> {
        self.relative_pointer_objects
            .get(&rel.id().protocol_id())
            .copied()
    }

    /// Bind relative pointer for one seat when `relative_pointer_wanted`.
    pub(crate) fn bind_relative_for_seat(
        &mut self,
        global: u32,
        pointer: &wayland_client::protocol::wl_pointer::WlPointer,
        qh: &wayland_client::QueueHandle<NativeShellState>,
        is_primary: bool,
    ) {
        if !self.relative_pointer_wanted {
            return;
        }
        if self
            .seats
            .get(&global)
            .is_some_and(|rec| rec.relative_pointer.is_some())
        {
            return;
        }
        let Some(manager) = self.relative_pointer_manager.clone() else {
            return;
        };
        let rel = manager.get_relative_pointer(pointer, qh, ());
        self.relative_pointer_objects
            .insert(rel.id().protocol_id(), global);
        if is_primary {
            self.relative_pointer = Some(rel.clone());
        }
        if let Some(rec) = self.seats.get_mut(&global) {
            rec.relative_pointer = Some(rel);
        }
    }

    pub(crate) fn clear_live_constraints_for(&mut self, id: NativeSurfaceId) {
        if self
            .locked_pointer
            .as_ref()
            .is_some_and(|(sid, _)| *sid == id)
            && let Some((_, proxy)) = self.locked_pointer.take()
        {
            proxy.destroy();
        }
        if self
            .confined_pointer
            .as_ref()
            .is_some_and(|(sid, _)| *sid == id)
            && let Some((_, proxy)) = self.confined_pointer.take()
        {
            proxy.destroy();
        }
    }

    /// Apply retained capture for `id` when it holds pointer focus.
    pub(crate) fn apply_pointer_constraint(
        &mut self,
        id: NativeSurfaceId,
        qh: &wayland_client::QueueHandle<Self>,
    ) -> Result<(), NativeError> {
        self.clear_live_constraints_for(id);
        let capture = self
            .toplevels
            .get(&id)
            .map(|r| r.pointer_capture.clone())
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        if capture.constraint == PointerConstraint::None {
            return Ok(());
        }
        let manager = self
            .pointer_constraints
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("pointer_constraints missing".into()))?
            .clone();
        let pointer = self
            .pointer
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("no pointer".into()))?
            .clone();
        let wl = self
            .toplevels
            .get(&id)
            .map(|r| r.wl.clone())
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        let compositor = self
            .compositor
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?
            .clone();

        let region = make_region(&compositor, qh, &capture.region);
        let region_ref = region.as_ref();

        match capture.constraint {
            PointerConstraint::None => {}
            PointerConstraint::Confined => {
                let confined = manager.confine_pointer(
                    &wl,
                    &pointer,
                    region_ref,
                    Lifetime::Persistent,
                    qh,
                    (),
                );
                self.confined_pointer = Some((id, confined));
            }
            PointerConstraint::Locked => {
                let locked =
                    manager.lock_pointer(&wl, &pointer, region_ref, Lifetime::Persistent, qh, ());
                self.locked_pointer = Some((id, locked));
            }
        }
        if let Some(region) = region {
            region.destroy();
        }
        Ok(())
    }

    /// Called from pointer enter/leave dispatch.
    pub(crate) fn on_pointer_focus_changed(
        &mut self,
        new_focus: Option<NativeSurfaceId>,
        qh: &wayland_client::QueueHandle<Self>,
    ) {
        if let Some(old) = self.pointer_focus
            && new_focus != Some(old)
        {
            self.clear_live_constraints_for(old);
        }
        self.pointer_focus = new_focus;
        if let Some(id) = new_focus {
            let _ = self.apply_pointer_constraint(id, qh);
        }
    }
}

fn make_region(
    compositor: &wl_compositor::WlCompositor,
    qh: &wayland_client::QueueHandle<NativeShellState>,
    region: &PointerConstraintRegion,
) -> Option<wayland_client::protocol::wl_region::WlRegion> {
    match region {
        PointerConstraintRegion::SurfaceInput => None,
        PointerConstraintRegion::Rectangles(rects) => {
            let region = compositor.create_region(qh, ());
            for rect in rects.iter().filter(|r| !r.is_empty()) {
                region.add(
                    rect.origin.x,
                    rect.origin.y,
                    rect.size.width.max(1) as i32,
                    rect.size.height.max(1) as i32,
                );
            }
            Some(region)
        }
    }
}
