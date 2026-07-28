//! Tensor-owned pointer-gestures wire state.

use std::collections::HashMap;

use tensor_event::PointerGestureEvent;
use wayland_protocols::wp::pointer_gestures::zv1::server::{
    zwp_pointer_gesture_hold_v1::{self, ZwpPointerGestureHoldV1},
    zwp_pointer_gesture_pinch_v1::{self, ZwpPointerGesturePinchV1},
    zwp_pointer_gesture_swipe_v1::{self, ZwpPointerGestureSwipeV1},
    zwp_pointer_gestures_v1::{self, ZwpPointerGesturesV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::serial::{Serial, next_serial};
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct PointerGesturesProtocol {
    _global: GlobalId,
    clients: HashMap<ClientId, ClientGestures>,
    swipe: Option<ActiveGesture>,
    pinch: Option<ActiveGesture>,
    hold: Option<ActiveGesture>,
}

#[derive(Default)]
struct ClientGestures {
    swipes: Vec<Tracked<ZwpPointerGestureSwipeV1>>,
    pinches: Vec<Tracked<ZwpPointerGesturePinchV1>>,
    holds: Vec<Tracked<ZwpPointerGestureHoldV1>>,
}

struct Tracked<R> {
    resource: R,
    active: bool,
}

struct ActiveGesture {
    client: ClientId,
    surface: ObjectId,
    client_scale: f64,
}

impl PointerGesturesProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, ZwpPointerGesturesV1, _>(
                3,
                PointerGesturesGlobalData,
            ),
            clients: HashMap::new(),
            swipe: None,
            pinch: None,
            hold: None,
        }
    }

    fn insert_swipe(&mut self, client: ClientId, resource: ZwpPointerGestureSwipeV1) {
        self.clients
            .entry(client)
            .or_default()
            .swipes
            .push(Tracked {
                resource,
                active: false,
            });
    }

    fn insert_pinch(&mut self, client: ClientId, resource: ZwpPointerGesturePinchV1) {
        self.clients
            .entry(client)
            .or_default()
            .pinches
            .push(Tracked {
                resource,
                active: false,
            });
    }

    fn insert_hold(&mut self, client: ClientId, resource: ZwpPointerGestureHoldV1) {
        self.clients.entry(client).or_default().holds.push(Tracked {
            resource,
            active: false,
        });
    }

    fn remove_swipe(&mut self, client: &ClientId, resource: &ZwpPointerGestureSwipeV1) {
        if let Some(gestures) = self.clients.get_mut(client) {
            remove_tracked(&mut gestures.swipes, resource);
        }
        self.prune_client(client);
    }

    fn remove_pinch(&mut self, client: &ClientId, resource: &ZwpPointerGesturePinchV1) {
        if let Some(gestures) = self.clients.get_mut(client) {
            remove_tracked(&mut gestures.pinches, resource);
        }
        self.prune_client(client);
    }

    fn remove_hold(&mut self, client: &ClientId, resource: &ZwpPointerGestureHoldV1) {
        if let Some(gestures) = self.clients.get_mut(client) {
            remove_tracked(&mut gestures.holds, resource);
        }
        self.prune_client(client);
    }

    fn prune_client(&mut self, client: &ClientId) {
        if self.clients.get(client).is_some_and(|gestures| {
            gestures.swipes.is_empty() && gestures.pinches.is_empty() && gestures.holds.is_empty()
        }) {
            self.clients.remove(client);
        }
    }

    pub(crate) fn event(&mut self, target: Option<(&WlSurface, f64)>, event: PointerGestureEvent) {
        let time = event.time_msec();
        match event {
            PointerGestureEvent::SwipeBegin { fingers, .. } => {
                let serial = next_serial();
                self.finish_swipe(serial, time, true);
                if fingers != 0 {
                    self.begin_swipe(target, serial, time, fingers);
                }
            }
            PointerGestureEvent::SwipeUpdate {
                delta_x, delta_y, ..
            } => self.update_swipe(time, delta_x, delta_y),
            PointerGestureEvent::SwipeEnd { cancelled, .. } => {
                self.finish_swipe(next_serial(), time, cancelled)
            }
            PointerGestureEvent::PinchBegin { fingers, .. } => {
                let serial = next_serial();
                self.finish_pinch(serial, time, true);
                if fingers != 0 {
                    self.begin_pinch(target, serial, time, fingers);
                }
            }
            PointerGestureEvent::PinchUpdate {
                delta_x,
                delta_y,
                scale,
                rotation,
                ..
            } => self.update_pinch(time, delta_x, delta_y, scale, rotation),
            PointerGestureEvent::PinchEnd { cancelled, .. } => {
                self.finish_pinch(next_serial(), time, cancelled)
            }
            PointerGestureEvent::HoldBegin { fingers, .. } => {
                let serial = next_serial();
                self.finish_hold(serial, time, true);
                if fingers != 0 {
                    self.begin_hold(target, serial, time, fingers);
                }
            }
            PointerGestureEvent::HoldEnd { cancelled, .. } => {
                self.finish_hold(next_serial(), time, cancelled)
            }
        }
    }

    pub(crate) fn focus_changed(&mut self, focus: Option<&WlSurface>, serial: Serial, time: u32) {
        if self.swipe.is_none() && self.pinch.is_none() && self.hold.is_none() {
            return;
        }
        let focus = focus.map(Resource::id);
        if self
            .swipe
            .as_ref()
            .is_some_and(|active| focus.as_ref() != Some(&active.surface))
        {
            self.finish_swipe(serial, time, true);
        }
        if self
            .pinch
            .as_ref()
            .is_some_and(|active| focus.as_ref() != Some(&active.surface))
        {
            self.finish_pinch(serial, time, true);
        }
        if self
            .hold
            .as_ref()
            .is_some_and(|active| focus.as_ref() != Some(&active.surface))
        {
            self.finish_hold(serial, time, true);
        }
    }

    fn target(surface: &WlSurface, client_scale: f64) -> Option<ActiveGesture> {
        if !client_scale.is_finite() || client_scale <= 0.0 {
            return None;
        }
        Some(ActiveGesture {
            client: surface.client()?.id(),
            surface: surface.id(),
            client_scale,
        })
    }

    fn begin_swipe(
        &mut self,
        target: Option<(&WlSurface, f64)>,
        serial: Serial,
        time: u32,
        fingers: u32,
    ) {
        let Some((surface, client_scale)) = target else {
            return;
        };
        let Some(active) = Self::target(surface, client_scale) else {
            return;
        };
        let Some(gestures) = self.clients.get_mut(&active.client) else {
            return;
        };
        for gesture in &mut gestures.swipes {
            gesture.active = true;
            gesture
                .resource
                .begin(serial.into(), time, surface, fingers);
        }
        if !gestures.swipes.is_empty() {
            self.swipe = Some(active);
        }
    }

    fn update_swipe(&mut self, time: u32, delta_x: f64, delta_y: f64) {
        let Some(active) = &self.swipe else {
            return;
        };
        let dx = delta_x * active.client_scale;
        let dy = delta_y * active.client_scale;
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        if let Some(gestures) = self.clients.get_mut(&active.client) {
            for gesture in gestures.swipes.iter().filter(|gesture| gesture.active) {
                gesture.resource.update(time, dx, dy);
            }
        }
    }

    fn finish_swipe(&mut self, serial: Serial, time: u32, cancelled: bool) {
        let Some(active) = self.swipe.take() else {
            return;
        };
        if let Some(gestures) = self.clients.get_mut(&active.client) {
            for gesture in gestures.swipes.iter_mut().filter(|gesture| gesture.active) {
                gesture.active = false;
                gesture
                    .resource
                    .end(serial.into(), time, i32::from(cancelled));
            }
        }
    }

    fn begin_pinch(
        &mut self,
        target: Option<(&WlSurface, f64)>,
        serial: Serial,
        time: u32,
        fingers: u32,
    ) {
        let Some((surface, client_scale)) = target else {
            return;
        };
        let Some(active) = Self::target(surface, client_scale) else {
            return;
        };
        let Some(gestures) = self.clients.get_mut(&active.client) else {
            return;
        };
        for gesture in &mut gestures.pinches {
            gesture.active = true;
            gesture
                .resource
                .begin(serial.into(), time, surface, fingers);
        }
        if !gestures.pinches.is_empty() {
            self.pinch = Some(active);
        }
    }

    fn update_pinch(&mut self, time: u32, delta_x: f64, delta_y: f64, scale: f64, rotation: f64) {
        let Some(active) = &self.pinch else {
            return;
        };
        let dx = delta_x * active.client_scale;
        let dy = delta_y * active.client_scale;
        if !dx.is_finite()
            || !dy.is_finite()
            || !scale.is_finite()
            || scale <= 0.0
            || !rotation.is_finite()
        {
            return;
        }
        if let Some(gestures) = self.clients.get_mut(&active.client) {
            for gesture in gestures.pinches.iter().filter(|gesture| gesture.active) {
                gesture.resource.update(time, dx, dy, scale, rotation);
            }
        }
    }

    fn finish_pinch(&mut self, serial: Serial, time: u32, cancelled: bool) {
        let Some(active) = self.pinch.take() else {
            return;
        };
        if let Some(gestures) = self.clients.get_mut(&active.client) {
            for gesture in gestures.pinches.iter_mut().filter(|gesture| gesture.active) {
                gesture.active = false;
                gesture
                    .resource
                    .end(serial.into(), time, i32::from(cancelled));
            }
        }
    }

    fn begin_hold(
        &mut self,
        target: Option<(&WlSurface, f64)>,
        serial: Serial,
        time: u32,
        fingers: u32,
    ) {
        let Some((surface, client_scale)) = target else {
            return;
        };
        let Some(active) = Self::target(surface, client_scale) else {
            return;
        };
        let Some(gestures) = self.clients.get_mut(&active.client) else {
            return;
        };
        for gesture in &mut gestures.holds {
            gesture.active = true;
            gesture
                .resource
                .begin(serial.into(), time, surface, fingers);
        }
        if !gestures.holds.is_empty() {
            self.hold = Some(active);
        }
    }

    fn finish_hold(&mut self, serial: Serial, time: u32, cancelled: bool) {
        let Some(active) = self.hold.take() else {
            return;
        };
        if let Some(gestures) = self.clients.get_mut(&active.client) {
            for gesture in gestures.holds.iter_mut().filter(|gesture| gesture.active) {
                gesture.active = false;
                gesture
                    .resource
                    .end(serial.into(), time, i32::from(cancelled));
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn resource_count(&self) -> usize {
        self.clients
            .values()
            .map(|gestures| gestures.swipes.len() + gestures.pinches.len() + gestures.holds.len())
            .sum()
    }
}

fn remove_tracked<R: Resource>(resources: &mut Vec<Tracked<R>>, resource: &R) {
    if let Some(index) = resources
        .iter()
        .position(|tracked| tracked.resource.id() == resource.id())
    {
        resources.swap_remove(index);
    }
}

pub(in crate::protocol) struct PointerGesturesGlobalData;

pub(in crate::protocol) struct PointerGesturesManagerData;

pub(in crate::protocol) struct PointerGestureData {
    client: Option<ClientId>,
}

impl GlobalDispatchDelegate<ZwpPointerGesturesV1, RuntimeState> for PointerGesturesGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpPointerGesturesV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, PointerGesturesManagerData);
    }
}

impl DispatchDelegate<ZwpPointerGesturesV1, RuntimeState> for PointerGesturesManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _manager: &ZwpPointerGesturesV1,
        request: zwp_pointer_gestures_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let active_client = |pointer: &wayland_server::protocol::wl_pointer::WlPointer| {
            state
                .protocol_globals
                .seat
                .owns_pointer(pointer)
                .then(|| client.id())
        };
        match request {
            zwp_pointer_gestures_v1::Request::GetSwipeGesture { id, pointer } => {
                let client = active_client(&pointer);
                let resource = data_init.init(
                    id,
                    PointerGestureData {
                        client: client.clone(),
                    },
                );
                if let Some(client) = client {
                    state
                        .protocol_globals
                        .pointer_gestures
                        .insert_swipe(client, resource);
                }
            }
            zwp_pointer_gestures_v1::Request::GetPinchGesture { id, pointer } => {
                let client = active_client(&pointer);
                let resource = data_init.init(
                    id,
                    PointerGestureData {
                        client: client.clone(),
                    },
                );
                if let Some(client) = client {
                    state
                        .protocol_globals
                        .pointer_gestures
                        .insert_pinch(client, resource);
                }
            }
            zwp_pointer_gestures_v1::Request::GetHoldGesture { id, pointer } => {
                let client = active_client(&pointer);
                let resource = data_init.init(
                    id,
                    PointerGestureData {
                        client: client.clone(),
                    },
                );
                if let Some(client) = client {
                    state
                        .protocol_globals
                        .pointer_gestures
                        .insert_hold(client, resource);
                }
            }
            zwp_pointer_gestures_v1::Request::Release => {}
            _ => unreachable!(),
        }
    }
}

macro_rules! gesture_dispatch {
    ($resource:ty, $request:path, $remove:ident) => {
        impl DispatchDelegate<$resource, RuntimeState> for PointerGestureData {
            fn request(
                &self,
                _state: &mut RuntimeState,
                _client: &Client,
                _gesture: &$resource,
                request: <$resource as Resource>::Request,
                _display: &DisplayHandle,
                _data_init: &mut DataInit<'_, RuntimeState>,
            ) {
                match request {
                    $request => {}
                    _ => unreachable!(),
                }
            }

            fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &$resource) {
                if let Some(client) = &self.client {
                    state
                        .protocol_globals
                        .pointer_gestures
                        .$remove(client, resource);
                }
            }
        }
    };
}

gesture_dispatch!(
    ZwpPointerGestureSwipeV1,
    zwp_pointer_gesture_swipe_v1::Request::Destroy,
    remove_swipe
);
gesture_dispatch!(
    ZwpPointerGesturePinchV1,
    zwp_pointer_gesture_pinch_v1::Request::Destroy,
    remove_pinch
);
gesture_dispatch!(
    ZwpPointerGestureHoldV1,
    zwp_pointer_gesture_hold_v1::Request::Destroy,
    remove_hold
);

delegate_global_dispatch!(
    RuntimeState,
    ZwpPointerGesturesV1,
    PointerGesturesGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ZwpPointerGesturesV1,
    PointerGesturesManagerData
);
delegate_dispatch!(RuntimeState, ZwpPointerGestureSwipeV1, PointerGestureData);
delegate_dispatch!(RuntimeState, ZwpPointerGesturePinchV1, PointerGestureData);
delegate_dispatch!(RuntimeState, ZwpPointerGestureHoldV1, PointerGestureData);
