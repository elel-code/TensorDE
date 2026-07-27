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

    fn client_scale(&self, state: &RuntimeState) -> f64 {
        self.client()
            .map(|client| state.client_scale(&client))
            .unwrap_or(1.0)
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
        self.0.is_alive()
    }
}

impl WaylandFocus for SurfaceFocusTarget {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        Some(Cow::Borrowed(&self.0))
    }
}

impl PointerTarget<RuntimeState> for SurfaceFocusTarget {
    fn enter(&self, _seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &MotionEvent) {
        let scale = self.client_scale(state);
        state
            .protocol_globals
            .seat
            .pointer_enter(&self.0, event, scale);
    }

    fn motion(&self, _seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &MotionEvent) {
        let scale = self.client_scale(state);
        state.protocol_globals.seat.pointer_motion(event, scale);
    }

    fn relative_motion(
        &self,
        _seat: &Seat<RuntimeState>,
        _state: &mut RuntimeState,
        _event: &RelativeMotionEvent,
    ) {
    }

    fn button(&self, _seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &ButtonEvent) {
        state.protocol_globals.seat.pointer_button(event);
    }

    fn axis(&self, _seat: &Seat<RuntimeState>, state: &mut RuntimeState, frame: AxisFrame) {
        let scale = self.client_scale(state);
        state.protocol_globals.seat.pointer_axis(frame, scale);
    }

    fn frame(&self, _seat: &Seat<RuntimeState>, state: &mut RuntimeState) {
        state.protocol_globals.seat.pointer_frame();
    }

    fn gesture_swipe_begin(
        &self,
        _seat: &Seat<RuntimeState>,
        _state: &mut RuntimeState,
        _event: &GestureSwipeBeginEvent,
    ) {
    }

    fn gesture_swipe_update(
        &self,
        _seat: &Seat<RuntimeState>,
        _state: &mut RuntimeState,
        _event: &GestureSwipeUpdateEvent,
    ) {
    }

    fn gesture_swipe_end(
        &self,
        _seat: &Seat<RuntimeState>,
        _state: &mut RuntimeState,
        _event: &GestureSwipeEndEvent,
    ) {
    }

    fn gesture_pinch_begin(
        &self,
        _seat: &Seat<RuntimeState>,
        _state: &mut RuntimeState,
        _event: &GesturePinchBeginEvent,
    ) {
    }

    fn gesture_pinch_update(
        &self,
        _seat: &Seat<RuntimeState>,
        _state: &mut RuntimeState,
        _event: &GesturePinchUpdateEvent,
    ) {
    }

    fn gesture_pinch_end(
        &self,
        _seat: &Seat<RuntimeState>,
        _state: &mut RuntimeState,
        _event: &GesturePinchEndEvent,
    ) {
    }

    fn gesture_hold_begin(
        &self,
        _seat: &Seat<RuntimeState>,
        _state: &mut RuntimeState,
        _event: &GestureHoldBeginEvent,
    ) {
    }

    fn gesture_hold_end(
        &self,
        _seat: &Seat<RuntimeState>,
        _state: &mut RuntimeState,
        _event: &GestureHoldEndEvent,
    ) {
    }

    fn leave(
        &self,
        _seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        serial: Serial,
        _time: u32,
    ) {
        state.protocol_globals.seat.pointer_leave(&self.0, serial);
    }
}

impl TouchTarget<RuntimeState> for SurfaceFocusTarget {
    fn down(&self, _seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &DownEvent) {
        let scale = self.client_scale(state);
        state
            .protocol_globals
            .seat
            .touch_down(&self.0, event, scale);
    }

    fn up(&self, _seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &UpEvent) {
        state.protocol_globals.seat.touch_up(&self.0, event);
    }

    fn motion(
        &self,
        _seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &TouchMotionEvent,
    ) {
        let scale = self.client_scale(state);
        state
            .protocol_globals
            .seat
            .touch_motion(&self.0, event, scale);
    }

    fn frame(&self, _seat: &Seat<RuntimeState>, state: &mut RuntimeState, marker: FrameMarker) {
        state.protocol_globals.seat.touch_frame(&self.0, marker);
    }

    fn cancel(&self, _seat: &Seat<RuntimeState>, state: &mut RuntimeState, marker: FrameMarker) {
        state.protocol_globals.seat.touch_cancel(&self.0, marker);
    }

    fn shape(&self, _seat: &Seat<RuntimeState>, state: &mut RuntimeState, event: &ShapeEvent) {
        state.protocol_globals.seat.touch_shape(&self.0, event);
    }

    fn orientation(
        &self,
        _seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        event: &OrientationEvent,
    ) {
        state
            .protocol_globals
            .seat
            .touch_orientation(&self.0, event);
    }

    fn last_frame(
        &self,
        _seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
    ) -> Option<FrameMarker> {
        state.protocol_globals.seat.last_touch_frame(&self.0)
    }
}
