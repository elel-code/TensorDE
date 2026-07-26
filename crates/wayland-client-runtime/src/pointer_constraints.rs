use smithay_client_toolkit::error::GlobalError;
use smithay_client_toolkit::globals::GlobalData;
use wayland_client::globals::GlobalList;
use wayland_client::protocol::{wl_pointer, wl_region, wl_surface};
use wayland_client::{Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_confined_pointer_v1::ZwpConfinedPointerV1;
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_locked_pointer_v1::ZwpLockedPointerV1;
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_pointer_constraints_v1::{
    Lifetime, ZwpPointerConstraintsV1,
};
use wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1;
use wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::ZwpRelativePointerV1;
use smithay_client_toolkit::seat::pointer_constraints::{
    PointerConstraintData, PointerConstraintsState,
};
use smithay_client_toolkit::seat::relative_pointer::{
    RelativePointerData, RelativePointerState,
};

use crate::{LogicalRect, SurfaceId};

/// Desired constraint for a pointer focused on a surface.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PointerConstraint {
    #[default]
    None,
    Confined,
    Locked,
}

/// Declarative pointer protocol state retained for one surface.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct PointerCaptureState {
    /// Confinement or lock requested while a pointer focuses the surface.
    pub constraint: PointerConstraint,
    /// Emit high-frequency relative motion while the surface is focused.
    /// Locked pointers always emit relative motion, regardless of this flag.
    pub relative_motion: bool,
    /// Region in which the constraint may activate.
    pub region: PointerConstraintRegion,
}

/// Surface-local region used by a pointer lock or confinement.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum PointerConstraintRegion {
    /// Use the surface's current input region, as represented by a NULL region
    /// in pointer-constraints-v1.
    #[default]
    SurfaceInput,
    /// Use the union of these surface-local rectangles. An empty vector is an
    /// intentionally empty region.
    Rectangles(Vec<LogicalRect>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PointerConstraintError {
    #[error("pointer constraint rectangle {index} has an empty dimension")]
    EmptyRectangle { index: usize },
    #[error("pointer constraint rectangle {index} exceeds Wayland integer limits")]
    RectangleTooLarge { index: usize },
}

/// A compositor transition for a pointer constraint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PointerConstraintEvent {
    pub surface: SurfaceId,
    pub constraint: PointerConstraint,
    pub active: bool,
}

/// Unaccelerated and accelerated motion from `zwp_relative_pointer_v1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RelativePointerEvent {
    pub surface: SurfaceId,
    /// Monotonic timestamp supplied by the compositor, in microseconds.
    pub time_micros: u64,
    pub delta: (f64, f64),
    pub delta_unaccelerated: (f64, f64),
}

#[derive(Clone, Copy)]
pub(crate) struct PointerCaptureTarget<'a> {
    surface_id: SurfaceId,
    surface: &'a wl_surface::WlSurface,
    pointer: &'a wl_pointer::WlPointer,
    region: Option<&'a wl_region::WlRegion>,
}

impl<'a> PointerCaptureTarget<'a> {
    pub(crate) fn new(
        surface_id: SurfaceId,
        surface: &'a wl_surface::WlSurface,
        pointer: &'a wl_pointer::WlPointer,
        region: Option<&'a wl_region::WlRegion>,
    ) -> Self {
        Self {
            surface_id,
            surface,
            pointer,
            region,
        }
    }
}

/// Bound globals for the two independent pointer extension protocols.
///
/// The core runtime only asks this object to create per-seat resources and
/// never needs to know which generated manager proxy owns them.
#[derive(Debug)]
pub(crate) struct PointerProtocols {
    constraints: Option<PointerConstraintsState>,
    relative_pointer: Option<RelativePointerState>,
}

impl PointerProtocols {
    pub(crate) fn bind<D>(
        globals: &GlobalList,
        queue_handle: &QueueHandle<D>,
        constraints_available: bool,
        relative_pointer_available: bool,
    ) -> Self
    where
        D: Dispatch<ZwpPointerConstraintsV1, GlobalData>
            + Dispatch<ZwpRelativePointerManagerV1, GlobalData>
            + 'static,
    {
        Self {
            constraints: constraints_available
                .then(|| PointerConstraintsState::bind(globals, queue_handle)),
            relative_pointer: relative_pointer_available
                .then(|| RelativePointerState::bind(globals, queue_handle)),
        }
    }

    pub(crate) const fn has_constraints(&self) -> bool {
        self.constraints.is_some()
    }

    pub(crate) const fn has_relative_pointer(&self) -> bool {
        self.relative_pointer.is_some()
    }

    fn get_relative_pointer<D>(
        &self,
        pointer: &wl_pointer::WlPointer,
        queue_handle: &QueueHandle<D>,
    ) -> Result<Option<ZwpRelativePointerV1>, GlobalError>
    where
        D: Dispatch<ZwpRelativePointerV1, RelativePointerData> + 'static,
    {
        self.relative_pointer
            .as_ref()
            .map(|manager| manager.get_relative_pointer(pointer, queue_handle))
            .transpose()
    }

    fn create_constraint<D>(
        &self,
        capture: &PointerCaptureState,
        target: PointerCaptureTarget<'_>,
        queue_handle: &QueueHandle<D>,
    ) -> Result<Option<ActiveConstraint>, GlobalError>
    where
        D: Dispatch<ZwpConfinedPointerV1, PointerConstraintData>
            + Dispatch<ZwpLockedPointerV1, PointerConstraintData>
            + 'static,
    {
        let Some(manager) = self.constraints.as_ref() else {
            return match capture.constraint {
                PointerConstraint::None => Ok(None),
                PointerConstraint::Confined | PointerConstraint::Locked => {
                    Err(GlobalError::MissingGlobal("zwp_pointer_constraints_v1"))
                }
            };
        };
        let active = match capture.constraint {
            PointerConstraint::None => return Ok(None),
            PointerConstraint::Confined => ActiveConstraint::Confined {
                surface: target.surface_id,
                region: capture.region.clone(),
                proxy: manager.confine_pointer(
                    target.surface,
                    target.pointer,
                    target.region,
                    Lifetime::Persistent,
                    queue_handle,
                )?,
                active: false,
            },
            PointerConstraint::Locked => ActiveConstraint::Locked {
                surface: target.surface_id,
                region: capture.region.clone(),
                proxy: manager.lock_pointer(
                    target.surface,
                    target.pointer,
                    target.region,
                    Lifetime::Persistent,
                    queue_handle,
                )?,
                active: false,
            },
        };
        Ok(Some(active))
    }
}

#[derive(Debug)]
enum ActiveConstraint {
    Confined {
        surface: SurfaceId,
        region: PointerConstraintRegion,
        proxy: ZwpConfinedPointerV1,
        active: bool,
    },
    Locked {
        surface: SurfaceId,
        region: PointerConstraintRegion,
        proxy: ZwpLockedPointerV1,
        active: bool,
    },
}

impl ActiveConstraint {
    fn surface(&self) -> SurfaceId {
        match self {
            Self::Confined { surface, .. } | Self::Locked { surface, .. } => *surface,
        }
    }

    fn constraint(&self) -> PointerConstraint {
        match self {
            Self::Confined { .. } => PointerConstraint::Confined,
            Self::Locked { .. } => PointerConstraint::Locked,
        }
    }

    fn update_region(
        &mut self,
        desired: &PointerConstraintRegion,
        region: Option<&wl_region::WlRegion>,
    ) {
        match self {
            Self::Confined {
                region: current,
                proxy,
                ..
            } => {
                if current == desired {
                    return;
                }
                proxy.set_region(region);
                current.clone_from(desired);
            }
            Self::Locked {
                region: current,
                proxy,
                ..
            } => {
                if current == desired {
                    return;
                }
                proxy.set_region(region);
                current.clone_from(desired);
            }
        }
    }

    fn destroy(self) {
        match self {
            Self::Confined { proxy, .. } => {
                if proxy.is_alive() {
                    proxy.destroy();
                }
            }
            Self::Locked { proxy, .. } => {
                if proxy.is_alive() {
                    proxy.destroy();
                }
            }
        }
    }
}

/// Per-seat pointer extension session.
///
/// Constraints are recreated from retained surface state when focus enters.
/// Destroying the old object before creating a new one guarantees the
/// protocol's single-constraint-per-pointer rule across focus changes.
#[derive(Debug, Default)]
pub(crate) struct SeatPointerSession {
    relative_pointer: Option<ZwpRelativePointerV1>,
    focus: Option<SurfaceId>,
    active_constraint: Option<ActiveConstraint>,
    emit_relative: bool,
}

impl SeatPointerSession {
    pub(crate) fn attach(&mut self) {
        self.detach();
    }

    pub(crate) fn detach(&mut self) {
        if let Some(constraint) = self.active_constraint.take() {
            constraint.destroy();
        }
        self.clear_relative_pointer();
        self.focus = None;
        self.emit_relative = false;
    }

    pub(crate) fn enter<D>(
        &mut self,
        target: PointerCaptureTarget<'_>,
        capture: &PointerCaptureState,
        protocols: &PointerProtocols,
        queue_handle: &QueueHandle<D>,
    ) -> Result<(), GlobalError>
    where
        D: Dispatch<ZwpConfinedPointerV1, PointerConstraintData>
            + Dispatch<ZwpLockedPointerV1, PointerConstraintData>
            + Dispatch<ZwpRelativePointerV1, RelativePointerData>
            + 'static,
    {
        self.focus_enter(target.surface_id);
        self.sync_capture(target, capture, protocols, queue_handle)
    }

    pub(crate) fn focus_enter(&mut self, surface: SurfaceId) {
        if self.focus != Some(surface) {
            self.clear_constraint();
            self.clear_relative_pointer();
            self.focus = Some(surface);
            self.emit_relative = false;
        }
    }

    pub(crate) fn leave(&mut self, surface: SurfaceId) {
        if self.focus == Some(surface) {
            self.clear_constraint();
            self.clear_relative_pointer();
            self.focus = None;
            self.emit_relative = false;
        }
    }

    pub(crate) fn remove_surface(&mut self, surface: SurfaceId) {
        self.leave(surface);
    }

    pub(crate) fn focus(&self) -> Option<SurfaceId> {
        self.focus
    }

    pub(crate) fn sync_capture<D>(
        &mut self,
        target: PointerCaptureTarget<'_>,
        capture: &PointerCaptureState,
        protocols: &PointerProtocols,
        queue_handle: &QueueHandle<D>,
    ) -> Result<(), GlobalError>
    where
        D: Dispatch<ZwpConfinedPointerV1, PointerConstraintData>
            + Dispatch<ZwpLockedPointerV1, PointerConstraintData>
            + Dispatch<ZwpRelativePointerV1, RelativePointerData>
            + 'static,
    {
        if self.focus != Some(target.surface_id) {
            return Ok(());
        }
        let emit_relative = wants_relative_pointer(capture);
        self.sync_relative_pointer(target.pointer, emit_relative, protocols, queue_handle)?;
        if self.active_constraint.as_ref().is_some_and(|active| {
            active.surface() == target.surface_id && active.constraint() == capture.constraint
        }) {
            self.active_constraint
                .as_mut()
                .expect("constraint was checked above")
                .update_region(&capture.region, target.region);
            self.emit_relative = emit_relative;
            return Ok(());
        }
        self.clear_constraint();
        self.active_constraint = protocols.create_constraint(capture, target, queue_handle)?;
        self.emit_relative = emit_relative;
        Ok(())
    }

    fn sync_relative_pointer<D>(
        &mut self,
        pointer: &wl_pointer::WlPointer,
        enabled: bool,
        protocols: &PointerProtocols,
        queue_handle: &QueueHandle<D>,
    ) -> Result<(), GlobalError>
    where
        D: Dispatch<ZwpRelativePointerV1, RelativePointerData> + 'static,
    {
        match (enabled, self.relative_pointer.is_some()) {
            (true, false) => {
                self.relative_pointer = protocols.get_relative_pointer(pointer, queue_handle)?;
            }
            (false, true) => {
                self.clear_relative_pointer();
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn relative_matches(&self, relative_pointer: &ZwpRelativePointerV1) -> bool {
        self.relative_pointer
            .as_ref()
            .is_some_and(|current| current.id() == relative_pointer.id())
    }

    pub(crate) fn should_emit_relative(&self) -> bool {
        self.focus.is_some() && self.emit_relative
    }

    pub(crate) fn set_locked_position_hint(
        &self,
        surface: SurfaceId,
        position: (f64, f64),
    ) -> bool {
        let Some(ActiveConstraint::Locked {
            surface: active_surface,
            proxy,
            ..
        }) = self.active_constraint.as_ref()
        else {
            return false;
        };
        if *active_surface != surface {
            return false;
        }
        proxy.set_cursor_position_hint(position.0, position.1);
        true
    }

    pub(crate) fn confined_changed(
        &mut self,
        proxy: &ZwpConfinedPointerV1,
        active: bool,
    ) -> Option<PointerConstraintEvent> {
        let Some(ActiveConstraint::Confined {
            surface,
            proxy: current,
            active: current_active,
            ..
        }) = self.active_constraint.as_mut()
        else {
            return None;
        };
        if current.id() != proxy.id() || *current_active == active {
            return None;
        }
        *current_active = active;
        Some(PointerConstraintEvent {
            surface: *surface,
            constraint: PointerConstraint::Confined,
            active,
        })
    }

    pub(crate) fn locked_changed(
        &mut self,
        proxy: &ZwpLockedPointerV1,
        active: bool,
    ) -> Option<PointerConstraintEvent> {
        let Some(ActiveConstraint::Locked {
            surface,
            proxy: current,
            active: current_active,
            ..
        }) = self.active_constraint.as_mut()
        else {
            return None;
        };
        if current.id() != proxy.id() || *current_active == active {
            return None;
        }
        *current_active = active;
        Some(PointerConstraintEvent {
            surface: *surface,
            constraint: PointerConstraint::Locked,
            active,
        })
    }

    fn clear_constraint(&mut self) {
        if let Some(active) = self.active_constraint.take() {
            active.destroy();
        }
    }

    fn clear_relative_pointer(&mut self) {
        if let Some(relative_pointer) = self.relative_pointer.take()
            && relative_pointer.is_alive()
        {
            relative_pointer.destroy();
        }
    }
}

impl Drop for SeatPointerSession {
    fn drop(&mut self) {
        self.detach();
    }
}

const fn wants_relative_pointer(capture: &PointerCaptureState) -> bool {
    capture.relative_motion || matches!(capture.constraint, PointerConstraint::Locked)
}

pub(crate) fn validate_pointer_capture_state(
    state: &PointerCaptureState,
) -> Result<(), PointerConstraintError> {
    let PointerConstraintRegion::Rectangles(rectangles) = &state.region else {
        return Ok(());
    };
    for (index, rectangle) in rectangles.iter().enumerate() {
        if rectangle.is_empty() {
            return Err(PointerConstraintError::EmptyRectangle { index });
        }
        if rectangle.size.width > i32::MAX as u32 || rectangle.size.height > i32::MAX as u32 {
            return Err(PointerConstraintError::RectangleTooLarge { index });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_state_defaults_to_no_extra_pointer_work() {
        assert_eq!(
            PointerCaptureState::default().constraint,
            PointerConstraint::None
        );
        assert!(!PointerCaptureState::default().relative_motion);
    }

    #[test]
    fn relative_pointer_is_lazy_except_for_an_explicit_subscription_or_lock() {
        assert!(!wants_relative_pointer(&PointerCaptureState::default()));
        assert!(!wants_relative_pointer(&PointerCaptureState {
            constraint: PointerConstraint::Confined,
            relative_motion: false,
            ..Default::default()
        }));
        assert!(wants_relative_pointer(&PointerCaptureState {
            constraint: PointerConstraint::None,
            relative_motion: true,
            ..Default::default()
        }));
        assert!(wants_relative_pointer(&PointerCaptureState {
            constraint: PointerConstraint::Locked,
            relative_motion: false,
            ..Default::default()
        }));
    }

    #[test]
    fn constraint_regions_reject_invalid_wire_rectangles() {
        let state = PointerCaptureState {
            constraint: PointerConstraint::Confined,
            region: PointerConstraintRegion::Rectangles(vec![LogicalRect::new(0, 0, 0, 20)]),
            ..Default::default()
        };
        assert_eq!(
            validate_pointer_capture_state(&state),
            Err(PointerConstraintError::EmptyRectangle { index: 0 })
        );

        let state = PointerCaptureState {
            constraint: PointerConstraint::Locked,
            region: PointerConstraintRegion::Rectangles(vec![LogicalRect::new(
                0,
                0,
                i32::MAX as u32 + 1,
                20,
            )]),
            ..Default::default()
        };
        assert_eq!(
            validate_pointer_capture_state(&state),
            Err(PointerConstraintError::RectangleTooLarge { index: 0 })
        );
    }

    #[test]
    fn leaving_focus_clears_the_relative_event_cache() {
        let surface = SurfaceId(7);
        let mut session = SeatPointerSession::default();
        session.focus_enter(surface);
        session.emit_relative = true;
        assert!(session.should_emit_relative());

        session.leave(surface);
        assert!(!session.should_emit_relative());
        assert_eq!(session.focus(), None);
    }
}
