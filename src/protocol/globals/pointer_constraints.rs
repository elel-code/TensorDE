//! Tensor-owned pointer-constraints wire state.

mod region;

use std::collections::HashMap;

use smithay::{
    input::pointer::PointerHandle,
    utils::{Logical, Point},
    wayland::compositor::{self, HookId, RectangleKind, get_region_attributes},
};
use wayland_protocols::wp::pointer_constraints::zv1::server::{
    zwp_confined_pointer_v1::{self, ZwpConfinedPointerV1},
    zwp_locked_pointer_v1::{self, ZwpLockedPointerV1},
    zwp_pointer_constraints_v1::{self, Lifetime, ZwpPointerConstraintsV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, WEnum, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::{wl_pointer::WlPointer, wl_region::WlRegion, wl_surface::WlSurface},
};

use self::region::{ConstraintRegion, RegionOp, RegionOpKind};
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct PointerConstraintsProtocol {
    _global: GlobalId,
    constraints: HashMap<ObjectId, Constraint>,
    active: Option<ActiveConstraint>,
}

struct Constraint {
    handle: ConstraintHandle,
    lifetime: ConstraintLifetime,
    region: ConstraintRegion,
    pending_region: Option<ConstraintRegion>,
    cursor_hint: Option<(f64, f64)>,
    pending_cursor_hint: Option<(f64, f64)>,
    hook: Option<HookId>,
}

enum ConstraintHandle {
    Confined(ZwpConfinedPointerV1),
    Locked(ZwpLockedPointerV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConstraintKind {
    Confined,
    Locked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConstraintLifetime {
    OneShot,
    Persistent,
}

struct ActiveConstraint {
    surface: ObjectId,
    origin: Point<f64, Logical>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::protocol) enum ConstraintMotion {
    Free(Point<f64, Logical>),
    Confined(Point<f64, Logical>),
    Locked(Point<f64, Logical>),
}

enum AttachResult {
    Attached,
    AlreadyConstrained,
}

impl PointerConstraintsProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, ZwpPointerConstraintsV1, _>(
                1,
                PointerConstraintsGlobalData,
            ),
            constraints: HashMap::new(),
            active: None,
        }
    }

    fn attach(
        &mut self,
        surface: &WlSurface,
        handle: ConstraintHandle,
        lifetime: ConstraintLifetime,
        region: ConstraintRegion,
    ) -> AttachResult {
        let key = surface.id();
        if self.constraints.contains_key(&key) {
            return AttachResult::AlreadyConstrained;
        }
        self.constraints.insert(
            key,
            Constraint {
                handle,
                lifetime,
                region,
                pending_region: None,
                cursor_hint: None,
                pending_cursor_hint: None,
                hook: None,
            },
        );
        AttachResult::Attached
    }

    fn install_hook(&mut self, surface: &WlSurface, hook: HookId) {
        if let Some(constraint) = self.constraints.get_mut(&surface.id()) {
            constraint.hook = Some(hook);
        } else {
            compositor::remove_post_commit_hook(surface, &hook);
        }
    }

    fn set_region(
        &mut self,
        surface: &WlSurface,
        resource: &ObjectId,
        kind: ConstraintKind,
        region: Option<&WlRegion>,
    ) {
        let Some(constraint) = self.constraints.get_mut(&surface.id()) else {
            return;
        };
        if constraint.handle.kind() == kind && constraint.handle.id() == *resource {
            constraint.pending_region = Some(constraint_region(region));
        }
    }

    fn set_cursor_hint(&mut self, surface: &WlSurface, resource: &ObjectId, hint: (f64, f64)) {
        let Some(constraint) = self.constraints.get_mut(&surface.id()) else {
            return;
        };
        if constraint.handle.kind() == ConstraintKind::Locked
            && constraint.handle.id() == *resource
            && hint.0.is_finite()
            && hint.1.is_finite()
        {
            constraint.pending_cursor_hint = Some(hint);
        }
    }

    fn commit(&mut self, surface: &WlSurface) -> bool {
        let Some(constraint) = self.constraints.get_mut(&surface.id()) else {
            return false;
        };
        let region_changed = if let Some(region) = constraint.pending_region.take() {
            constraint.region = region;
            true
        } else {
            false
        };
        if let Some(hint) = constraint.pending_cursor_hint.take() {
            constraint.cursor_hint = Some(hint);
        }
        region_changed
    }

    pub(in crate::protocol) fn constrain_motion(
        &self,
        current: Point<f64, Logical>,
        proposed: Point<f64, Logical>,
    ) -> ConstraintMotion {
        let Some(active) = &self.active else {
            return ConstraintMotion::Free(proposed);
        };
        let Some(constraint) = self.constraints.get(&active.surface) else {
            return ConstraintMotion::Free(proposed);
        };
        if constraint.handle.kind() == ConstraintKind::Locked {
            return ConstraintMotion::Locked(current);
        }
        let current_local = (current.x - active.origin.x, current.y - active.origin.y);
        let proposed_local = (proposed.x - active.origin.x, proposed.y - active.origin.y);
        let Some(local) = constraint.region.confine(current_local, proposed_local) else {
            return ConstraintMotion::Confined(current);
        };
        ConstraintMotion::Confined((active.origin.x + local.0, active.origin.y + local.1).into())
    }

    pub(in crate::protocol) fn active_matches(&self, surface: Option<&ObjectId>) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| surface.is_some_and(|surface| *surface == active.surface))
    }

    pub(in crate::protocol) fn focus_changed(
        &mut self,
        focus: Option<(&WlSurface, Point<f64, Logical>)>,
        location: Point<f64, Logical>,
    ) -> Option<Point<f64, Logical>> {
        let active_valid = self.active.as_ref().is_some_and(|active| {
            let Some((surface, origin)) = focus else {
                return false;
            };
            if surface.id() != active.surface {
                return false;
            }
            self.constraints
                .get(&active.surface)
                .is_some_and(|constraint| {
                    constraint
                        .region
                        .contains((location.x - origin.x, location.y - origin.y))
                })
        });
        if active_valid {
            if let (Some(active), Some((_, origin))) = (&mut self.active, focus) {
                active.origin = origin;
            }
            return None;
        }

        let warp = self.deactivate_active(true);
        if warp.is_some() {
            return warp;
        }
        let (surface, origin) = focus?;
        let key = surface.id();
        let constraint = self.constraints.get(&key)?;
        if !constraint
            .region
            .contains((location.x - origin.x, location.y - origin.y))
        {
            return None;
        }
        constraint.handle.activate();
        self.active = Some(ActiveConstraint {
            surface: key,
            origin,
        });
        None
    }

    fn detach_resource(
        &mut self,
        surface: &WlSurface,
        resource: &ObjectId,
    ) -> Option<Point<f64, Logical>> {
        let key = surface.id();
        if self
            .constraints
            .get(&key)
            .is_none_or(|constraint| constraint.handle.id() != *resource)
        {
            return None;
        }
        let warp = self.active.as_ref().and_then(|active| {
            (active.surface == key).then(|| {
                self.constraints
                    .get(&key)
                    .and_then(|constraint| constraint.global_hint(active.origin))
            })
        });
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.surface == key)
        {
            self.active = None;
        }
        self.remove_entry(surface, &key);
        warp.flatten()
    }

    pub(super) fn remove_surface(&mut self, surface: &WlSurface) {
        let key = surface.id();
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.surface == key)
        {
            let _ = self.deactivate_active(true);
        }
        self.remove_entry(surface, &key);
    }

    fn deactivate_active(&mut self, send_event: bool) -> Option<Point<f64, Logical>> {
        let active = self.active.take()?;
        let constraint = self.constraints.get(&active.surface)?;
        if send_event {
            constraint.handle.deactivate();
        }
        let warp = constraint.global_hint(active.origin);
        let one_shot = constraint.lifetime == ConstraintLifetime::OneShot;
        if one_shot && let Ok(surface) = constraint.handle.surface().upgrade() {
            self.remove_entry(&surface, &active.surface);
        }
        warp
    }

    fn remove_entry(&mut self, surface: &WlSurface, key: &ObjectId) {
        if let Some(mut constraint) = self.constraints.remove(key)
            && let Some(hook) = constraint.hook.take()
        {
            compositor::remove_post_commit_hook(surface, &hook);
        }
    }

    #[cfg(test)]
    pub(crate) fn constraint_count(&self) -> usize {
        self.constraints.len()
    }
}

impl Constraint {
    fn global_hint(&self, origin: Point<f64, Logical>) -> Option<Point<f64, Logical>> {
        (self.handle.kind() == ConstraintKind::Locked)
            .then_some(self.cursor_hint)
            .flatten()
            .map(|hint| (origin.x + hint.0, origin.y + hint.1).into())
    }
}

impl ConstraintHandle {
    fn kind(&self) -> ConstraintKind {
        match self {
            Self::Confined(_) => ConstraintKind::Confined,
            Self::Locked(_) => ConstraintKind::Locked,
        }
    }

    fn id(&self) -> ObjectId {
        match self {
            Self::Confined(resource) => resource.id(),
            Self::Locked(resource) => resource.id(),
        }
    }

    fn surface(&self) -> Weak<WlSurface> {
        match self {
            Self::Confined(resource) => resource
                .data::<PointerConstraintData>()
                .expect("Tensor initializes confined-pointer data")
                .surface
                .clone(),
            Self::Locked(resource) => resource
                .data::<PointerConstraintData>()
                .expect("Tensor initializes locked-pointer data")
                .surface
                .clone(),
        }
    }

    fn activate(&self) {
        match self {
            Self::Confined(resource) => resource.confined(),
            Self::Locked(resource) => resource.locked(),
        }
    }

    fn deactivate(&self) {
        match self {
            Self::Confined(resource) => resource.unconfined(),
            Self::Locked(resource) => resource.unlocked(),
        }
    }
}

fn constraint_lifetime(lifetime: WEnum<Lifetime>) -> Option<ConstraintLifetime> {
    match lifetime {
        WEnum::Value(Lifetime::Oneshot) => Some(ConstraintLifetime::OneShot),
        WEnum::Value(Lifetime::Persistent) => Some(ConstraintLifetime::Persistent),
        _ => None,
    }
}

fn constraint_region(region: Option<&WlRegion>) -> ConstraintRegion {
    let Some(region) = region else {
        return ConstraintRegion::unbounded();
    };
    let attributes = get_region_attributes(region);
    ConstraintRegion::from_ops(
        attributes
            .rects
            .into_iter()
            .map(|(kind, rectangle)| RegionOp {
                kind: match kind {
                    RectangleKind::Add => RegionOpKind::Add,
                    RectangleKind::Subtract => RegionOpKind::Subtract,
                },
                x: rectangle.loc.x,
                y: rectangle.loc.y,
                width: rectangle.size.w,
                height: rectangle.size.h,
            }),
    )
}

fn pointer_is_active(state: &RuntimeState, pointer: &WlPointer) -> bool {
    PointerHandle::<RuntimeState>::from_resource(pointer)
        .is_some_and(|handle| state.seat.get_pointer().as_ref() == Some(&handle))
}

fn install_constraint_hook(state: &mut RuntimeState, surface: &WlSurface) {
    let hook = compositor::add_post_commit_hook::<RuntimeState, _>(
        surface,
        pointer_constraint_post_commit,
    );
    state
        .protocol_globals
        .pointer_constraints
        .install_hook(surface, hook);
}

fn pointer_constraint_post_commit(
    state: &mut RuntimeState,
    _display: &DisplayHandle,
    surface: &WlSurface,
) {
    let region_changed = state.protocol_globals.pointer_constraints.commit(surface);
    #[cfg(feature = "tty")]
    if region_changed {
        state.reconcile_pointer_constraint();
    }
    #[cfg(not(feature = "tty"))]
    let _ = region_changed;
}

#[cfg(feature = "tty")]
impl RuntimeState {
    pub(crate) fn reconcile_pointer_constraint(&mut self) {
        let Some(pointer) = self.seat.get_pointer() else {
            let warp = self
                .protocol_globals
                .pointer_constraints
                .focus_changed(None, (0.0, 0.0).into());
            self.apply_pointer_constraint_hint(warp);
            return;
        };
        let location = pointer.current_location();
        let hit = self.pointer_focus_under(location);
        let current_focus = pointer.current_focus();
        let focus = current_focus.as_ref().and_then(|current| {
            hit.as_ref()
                .filter(|(surface, _)| surface.id() == current.id())
                .map(|(_, origin)| (current, *origin))
        });
        let warp = self
            .protocol_globals
            .pointer_constraints
            .focus_changed(focus, location);
        self.apply_pointer_constraint_hint(warp);
    }

    pub(in crate::protocol) fn apply_pointer_constraint_hint(
        &mut self,
        hint: Option<Point<f64, Logical>>,
    ) {
        let Some(mut location) = hint else {
            return;
        };
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let previous = pointer.current_location();
        if let Some(bounds) = self.pointer_coordinate_space() {
            location = crate::protocol::input::constrain_pointer_location(location, bounds);
        }
        pointer.set_location(location);
        self.request_redraw_at(previous);
        self.request_redraw_at(location);
    }

    fn detach_pointer_constraint(&mut self, surface: &WlSurface, resource: &ObjectId) {
        let warp = self
            .protocol_globals
            .pointer_constraints
            .detach_resource(surface, resource);
        self.apply_pointer_constraint_hint(warp);
        self.reconcile_pointer_constraint();
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct PointerConstraintsGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct PointerConstraintsManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct PointerConstraintData {
    surface: Weak<WlSurface>,
    kind: ConstraintKind,
}

impl GlobalDispatchDelegate<ZwpPointerConstraintsV1, RuntimeState>
    for PointerConstraintsGlobalData
{
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpPointerConstraintsV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, PointerConstraintsManagerData);
    }
}

impl DispatchDelegate<ZwpPointerConstraintsV1, RuntimeState> for PointerConstraintsManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &ZwpPointerConstraintsV1,
        request: zwp_pointer_constraints_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let (surface, pointer, wire_lifetime, region, handle) = match request {
            zwp_pointer_constraints_v1::Request::LockPointer {
                id,
                surface,
                pointer,
                region,
                lifetime,
            } => {
                let resource = data_init.init(
                    id,
                    PointerConstraintData {
                        surface: surface.downgrade(),
                        kind: ConstraintKind::Locked,
                    },
                );
                (
                    surface,
                    pointer,
                    lifetime,
                    region,
                    ConstraintHandle::Locked(resource),
                )
            }
            zwp_pointer_constraints_v1::Request::ConfinePointer {
                id,
                surface,
                pointer,
                region,
                lifetime,
            } => {
                let resource = data_init.init(
                    id,
                    PointerConstraintData {
                        surface: surface.downgrade(),
                        kind: ConstraintKind::Confined,
                    },
                );
                (
                    surface,
                    pointer,
                    lifetime,
                    region,
                    ConstraintHandle::Confined(resource),
                )
            }
            zwp_pointer_constraints_v1::Request::Destroy => return,
            _ => unreachable!(),
        };
        if !pointer_is_active(state, &pointer) {
            return;
        }
        let Some(lifetime) = constraint_lifetime(wire_lifetime) else {
            return;
        };
        let region = constraint_region(region.as_ref());
        match state
            .protocol_globals
            .pointer_constraints
            .attach(&surface, handle, lifetime, region)
        {
            AttachResult::AlreadyConstrained => manager.post_error(
                zwp_pointer_constraints_v1::Error::AlreadyConstrained,
                "the surface already has a pointer constraint for this seat",
            ),
            AttachResult::Attached => {
                install_constraint_hook(state, &surface);
                #[cfg(feature = "tty")]
                state.reconcile_pointer_constraint();
            }
        }
    }
}

impl DispatchDelegate<ZwpConfinedPointerV1, RuntimeState> for PointerConstraintData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &ZwpConfinedPointerV1,
        request: zwp_confined_pointer_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        match request {
            zwp_confined_pointer_v1::Request::SetRegion { region } => state
                .protocol_globals
                .pointer_constraints
                .set_region(&surface, &resource.id(), self.kind, region.as_ref()),
            zwp_confined_pointer_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &ZwpConfinedPointerV1,
    ) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        #[cfg(feature = "tty")]
        state.detach_pointer_constraint(&surface, &resource.id());
        #[cfg(not(feature = "tty"))]
        let _ = state
            .protocol_globals
            .pointer_constraints
            .detach_resource(&surface, &resource.id());
    }
}

impl DispatchDelegate<ZwpLockedPointerV1, RuntimeState> for PointerConstraintData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &ZwpLockedPointerV1,
        request: zwp_locked_pointer_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        match request {
            zwp_locked_pointer_v1::Request::SetCursorPositionHint {
                surface_x,
                surface_y,
            } => state.protocol_globals.pointer_constraints.set_cursor_hint(
                &surface,
                &resource.id(),
                (surface_x, surface_y),
            ),
            zwp_locked_pointer_v1::Request::SetRegion { region } => state
                .protocol_globals
                .pointer_constraints
                .set_region(&surface, &resource.id(), self.kind, region.as_ref()),
            zwp_locked_pointer_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &ZwpLockedPointerV1,
    ) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        #[cfg(feature = "tty")]
        state.detach_pointer_constraint(&surface, &resource.id());
        #[cfg(not(feature = "tty"))]
        let _ = state
            .protocol_globals
            .pointer_constraints
            .detach_resource(&surface, &resource.id());
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ZwpPointerConstraintsV1,
    PointerConstraintsGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ZwpPointerConstraintsV1,
    PointerConstraintsManagerData
);
delegate_dispatch!(RuntimeState, ZwpConfinedPointerV1, PointerConstraintData);
delegate_dispatch!(RuntimeState, ZwpLockedPointerV1, PointerConstraintData);
