//! Local pointer/touch focus type that breaks Smithay's selection trait bound.

use std::{borrow::Cow, ops::Deref};

use smithay::{
    input::{
        Seat,
        pointer::{
            AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent,
            GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
            GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent, MotionEvent,
            PointerTarget, RelativeMotionEvent,
        },
        touch::{
            DownEvent, FrameMarker, MotionEvent as TouchMotionEvent, OrientationEvent, ShapeEvent,
            TouchTarget, UpEvent,
        },
    },
    utils::{IsAlive, Serial},
    wayland::seat::WaylandFocus,
};
use wayland_server::{Client, Resource, backend::ObjectId, protocol::wl_surface::WlSurface};

use super::KeyboardFocusTarget;
use crate::protocol::state::RuntimeState;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SurfaceFocusTarget(WlSurface);

impl SurfaceFocusTarget {
    pub(crate) fn surface(&self) -> &WlSurface {
        &self.0
    }

    pub(crate) fn id(&self) -> ObjectId {
        self.0.id()
    }

    pub(crate) fn client(&self) -> Option<Client> {
        self.0.client()
    }

    pub(crate) fn into_surface(self) -> WlSurface {
        self.0
    }
}

impl Deref for SurfaceFocusTarget {
    type Target = WlSurface;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq<WlSurface> for SurfaceFocusTarget {
    fn eq(&self, other: &WlSurface) -> bool {
        self.0 == *other
    }
}

impl From<WlSurface> for SurfaceFocusTarget {
    fn from(surface: WlSurface) -> Self {
        Self(surface)
    }
}

impl From<KeyboardFocusTarget> for SurfaceFocusTarget {
    fn from(target: KeyboardFocusTarget) -> Self {
        Self(WlSurface::from(target))
    }
}

impl IsAlive for SurfaceFocusTarget {
    fn alive(&self) -> bool {
        self.0.alive()
    }
}

impl WaylandFocus for SurfaceFocusTarget {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        Some(Cow::Borrowed(&self.0))
    }
}

impl PointerTarget<RuntimeState> for SurfaceFocusTarget {
    fn enter(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &MotionEvent) {
        PointerTarget::<RuntimeState>::enter(&self.0, seat, state, event);
    }

    fn motion(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &MotionEvent) {
        PointerTarget::<RuntimeState>::motion(&self.0, seat, state, event);
    }

    fn relative_motion(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &RelativeMotionEvent,
    ) {
        PointerTarget::<RuntimeState>::relative_motion(&self.0, seat, state, event);
    }

    fn button(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &ButtonEvent) {
        PointerTarget::<RuntimeState>::button(&self.0, seat, state, event);
    }

    fn axis(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState, frame: AxisFrame) {
        PointerTarget::<RuntimeState>::axis(&self.0, seat, state, frame);
    }

    fn frame(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState) {
        PointerTarget::<RuntimeState>::frame(&self.0, seat, state);
    }

    fn gesture_swipe_begin(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &GestureSwipeBeginEvent,
    ) {
        PointerTarget::<RuntimeState>::gesture_swipe_begin(&self.0, seat, state, event);
    }

    fn gesture_swipe_update(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &GestureSwipeUpdateEvent,
    ) {
        PointerTarget::<RuntimeState>::gesture_swipe_update(&self.0, seat, state, event);
    }

    fn gesture_swipe_end(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &GestureSwipeEndEvent,
    ) {
        PointerTarget::<RuntimeState>::gesture_swipe_end(&self.0, seat, state, event);
    }

    fn gesture_pinch_begin(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &GesturePinchBeginEvent,
    ) {
        PointerTarget::<RuntimeState>::gesture_pinch_begin(&self.0, seat, state, event);
    }

    fn gesture_pinch_update(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &GesturePinchUpdateEvent,
    ) {
        PointerTarget::<RuntimeState>::gesture_pinch_update(&self.0, seat, state, event);
    }

    fn gesture_pinch_end(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &GesturePinchEndEvent,
    ) {
        PointerTarget::<RuntimeState>::gesture_pinch_end(&self.0, seat, state, event);
    }

    fn gesture_hold_begin(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &GestureHoldBeginEvent,
    ) {
        PointerTarget::<RuntimeState>::gesture_hold_begin(&self.0, seat, state, event);
    }

    fn gesture_hold_end(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &GestureHoldEndEvent,
    ) {
        PointerTarget::<RuntimeState>::gesture_hold_end(&self.0, seat, state, event);
    }

    fn leave(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        serial: Serial,
        time: u32,
    ) {
        PointerTarget::<RuntimeState>::leave(&self.0, seat, state, serial, time);
    }
}

impl TouchTarget<RuntimeState> for SurfaceFocusTarget {
    fn down(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &DownEvent) {
        TouchTarget::<RuntimeState>::down(&self.0, seat, state, event);
    }

    fn up(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &UpEvent) {
        TouchTarget::<RuntimeState>::up(&self.0, seat, state, event);
    }

    fn motion(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &TouchMotionEvent,
    ) {
        TouchTarget::<RuntimeState>::motion(&self.0, seat, state, event);
    }

    fn frame(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState, marker: FrameMarker) {
        TouchTarget::<RuntimeState>::frame(&self.0, seat, state, marker);
    }

    fn cancel(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState, marker: FrameMarker) {
        TouchTarget::<RuntimeState>::cancel(&self.0, seat, state, marker);
    }

    fn shape(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &ShapeEvent) {
        TouchTarget::<RuntimeState>::shape(&self.0, seat, state, event);
    }

    fn orientation(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &OrientationEvent,
    ) {
        TouchTarget::<RuntimeState>::orientation(&self.0, seat, state, event);
    }

    fn last_frame(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
    ) -> Option<FrameMarker> {
        TouchTarget::<RuntimeState>::last_frame(&self.0, seat, state)
    }
}
