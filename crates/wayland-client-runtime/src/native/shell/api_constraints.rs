//! Pointer constraints (lock / confine) for [`NativeShell`].

use wayland_client::protocol::wl_compositor;
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_pointer_constraints_v1::Lifetime;

use super::api::NativeShell;
use super::types::{NativeShellState, NativeSurfaceId};
use crate::native::connection::NativeError;
use crate::pointer_constraints::{
    PointerCaptureState, PointerConstraint, PointerConstraintRegion,
};

impl NativeShell {
    /// Retain desired pointer capture for a surface and apply if focused.
    pub fn set_pointer_capture_state(
        &mut self,
        id: NativeSurfaceId,
        state: PointerCaptureState,
    ) -> Result<(), NativeError> {
        if state.constraint != PointerConstraint::None && self.state.pointer_constraints.is_none()
        {
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

        let want_relative =
            state.relative_motion || state.constraint == PointerConstraint::Locked;
        if want_relative {
            let _ = self.enable_relative_pointer();
        }

        if self.state.pointer_focus == Some(id) {
            self.state.apply_pointer_constraint(id, &self.queue.handle())?;
        } else {
            self.state.clear_live_constraints_for(id);
        }
        self.connection.flush()?;
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
}

impl NativeShellState {
    pub(crate) fn clear_live_constraints_for(&mut self, id: NativeSurfaceId) {
        if self
            .locked_pointer
            .as_ref()
            .is_some_and(|(sid, _)| *sid == id)
        {
            if let Some((_, proxy)) = self.locked_pointer.take() {
                proxy.destroy();
            }
        }
        if self
            .confined_pointer
            .as_ref()
            .is_some_and(|(sid, _)| *sid == id)
        {
            if let Some((_, proxy)) = self.confined_pointer.take() {
                proxy.destroy();
            }
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
        if let Some(old) = self.pointer_focus {
            if new_focus != Some(old) {
                self.clear_live_constraints_for(old);
            }
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
