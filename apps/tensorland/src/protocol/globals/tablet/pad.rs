//! Tablet pad resources, modes, rings, strips, and dials.

use tensor_event::{
    DeviceGroupId, DeviceId, TabletPadDescriptor, TabletPadEvent, TabletPadGroupDescriptor,
};
use wayland_protocols::wp::tablet::zv2::server::{
    zwp_tablet_pad_dial_v2::{self, ZwpTabletPadDialV2},
    zwp_tablet_pad_group_v2::{self, ZwpTabletPadGroupV2},
    zwp_tablet_pad_ring_v2::{self, ZwpTabletPadRingV2},
    zwp_tablet_pad_strip_v2::{self, ZwpTabletPadStripV2},
    zwp_tablet_pad_v2::{self, ZwpTabletPadV2},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, Resource, Weak,
    backend::{ClientId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use super::{MAX_TABLET_SEATS, TabletProtocol};
use crate::protocol::{
    dispatch::{DispatchDelegate, delegate_dispatch},
    serial::next_serial,
    state::RuntimeState,
};

const MAX_PAD_GROUPS: usize = 8;
const MAX_PAD_AXES: usize = 16;

pub(super) struct PadState {
    descriptor: TabletPadDescriptor,
    group: DeviceGroupId,
    groups: Vec<TabletPadGroupDescriptor>,
    instances: Vec<PadInstance>,
    focus: Option<PadFocus>,
    pressed: u64,
}

struct PadInstance {
    seat: ObjectId,
    client: ClientId,
    resource: Weak<ZwpTabletPadV2>,
    groups: Vec<GroupInstance>,
}

struct GroupInstance {
    index: u8,
    resource: Weak<ZwpTabletPadGroupV2>,
    rings: Vec<AxisInstance<ZwpTabletPadRingV2>>,
    strips: Vec<AxisInstance<ZwpTabletPadStripV2>>,
    dials: Vec<AxisInstance<ZwpTabletPadDialV2>>,
}

struct AxisInstance<R: Resource> {
    index: u8,
    resource: Weak<R>,
}

struct PadFocus {
    client: ClientId,
    surface: Weak<WlSurface>,
}

impl PadState {
    fn new(descriptor: TabletPadDescriptor, group: DeviceGroupId) -> Self {
        Self {
            descriptor,
            group,
            groups: Vec::with_capacity(MAX_PAD_GROUPS),
            instances: Vec::with_capacity(MAX_TABLET_SEATS),
            focus: None,
            pressed: 0,
        }
    }

    pub(super) fn complete(&self) -> bool {
        self.groups.len() == usize::from(self.descriptor.groups)
    }

    pub(super) const fn group(&self) -> DeviceGroupId {
        self.group
    }
}

pub(super) fn announce_pad(
    display: &DisplayHandle,
    client: &Client,
    seat: &super::ZwpTabletSeatV2,
    pad: &mut PadState,
    tablet: Option<super::ZwpTabletV2>,
) {
    let Ok(resource) = client.create_resource::<ZwpTabletPadV2, _, RuntimeState>(
        display,
        seat.version(),
        PadData {
            device: pad.descriptor.device,
        },
    ) else {
        return;
    };
    seat.pad_added(&resource);
    if pad.descriptor.buttons > 0 {
        resource.buttons(pad.descriptor.buttons.into());
    }
    let mut groups = Vec::with_capacity(pad.groups.len());
    for descriptor in &pad.groups {
        let Ok(group) = client.create_resource::<ZwpTabletPadGroupV2, _, RuntimeState>(
            display,
            resource.version(),
            GroupData,
        ) else {
            continue;
        };
        resource.group(&group);
        group.buttons(button_array(descriptor.buttons));
        let mut instance = GroupInstance {
            index: descriptor.index,
            resource: group.downgrade(),
            rings: Vec::with_capacity(MAX_PAD_AXES),
            strips: Vec::with_capacity(MAX_PAD_AXES),
            dials: Vec::with_capacity(MAX_PAD_AXES),
        };
        announce_axes(display, client, &group, descriptor, &mut instance);
        if descriptor.modes > 1 {
            group.modes(descriptor.modes.into());
        }
        group.done();
        groups.push(instance);
    }
    resource.done();
    if let (Some(focus), Some(tablet)) = (pad.focus.as_ref(), tablet)
        && focus.client == client.id()
        && let Ok(surface) = focus.surface.upgrade()
    {
        let serial: u32 = next_serial().into();
        resource.enter(serial, &tablet, &surface);
        for group in &groups {
            if let Some(descriptor) = pad
                .groups
                .iter()
                .find(|descriptor| descriptor.index == group.index)
                && let Ok(resource) = group.resource.upgrade()
            {
                resource.mode_switch(0, serial, descriptor.current_mode.into());
            }
        }
    }
    pad.instances.push(PadInstance {
        seat: Resource::id(seat),
        client: client.id(),
        resource: resource.downgrade(),
        groups,
    });
}

fn announce_axes(
    display: &DisplayHandle,
    client: &Client,
    group: &ZwpTabletPadGroupV2,
    descriptor: &TabletPadGroupDescriptor,
    instance: &mut GroupInstance,
) {
    for index in set_bits(descriptor.rings) {
        if let Ok(resource) = client.create_resource::<ZwpTabletPadRingV2, _, RuntimeState>(
            display,
            group.version(),
            AxisData,
        ) {
            group.ring(&resource);
            instance.rings.push(AxisInstance {
                index,
                resource: resource.downgrade(),
            });
        }
    }
    for index in set_bits(descriptor.strips) {
        if let Ok(resource) = client.create_resource::<ZwpTabletPadStripV2, _, RuntimeState>(
            display,
            group.version(),
            AxisData,
        ) {
            group.strip(&resource);
            instance.strips.push(AxisInstance {
                index,
                resource: resource.downgrade(),
            });
        }
    }
    if group.version() >= 2 {
        for index in set_bits(descriptor.dials) {
            if let Ok(resource) = client.create_resource::<ZwpTabletPadDialV2, _, RuntimeState>(
                display,
                group.version(),
                AxisData,
            ) {
                group.dial(&resource);
                instance.dials.push(AxisInstance {
                    index,
                    resource: resource.downgrade(),
                });
            }
        }
    }
}

impl TabletProtocol {
    pub(in crate::protocol) fn pad_event(
        &mut self,
        display: &DisplayHandle,
        event: TabletPadEvent,
    ) {
        match event {
            TabletPadEvent::Added(descriptor) => self.add_pad(descriptor),
            TabletPadEvent::Group(descriptor) => self.add_pad_group(display, descriptor),
            event => self.forward_pad_event(event),
        }
    }

    fn add_pad(&mut self, descriptor: TabletPadDescriptor) {
        if self
            .pads
            .iter()
            .any(|pad| pad.descriptor.device == descriptor.device)
        {
            return;
        }
        let Some(group) = self.group_for_device(descriptor.device) else {
            return;
        };
        if self.pads.len() == super::MAX_PADS {
            tracing::warn!(
                device = descriptor.device.get(),
                "tablet pad capacity exceeded"
            );
            return;
        }
        self.pads.push(PadState::new(descriptor, group));
    }

    fn add_pad_group(&mut self, display: &DisplayHandle, descriptor: TabletPadGroupDescriptor) {
        let Some(pad) = self
            .pads
            .iter_mut()
            .find(|pad| pad.descriptor.device == descriptor.device)
        else {
            return;
        };
        if usize::from(descriptor.index) != pad.groups.len() || pad.groups.len() == MAX_PAD_GROUPS {
            tracing::warn!(
                device = descriptor.device.get(),
                "invalid tablet pad group topology"
            );
            return;
        }
        pad.groups.push(descriptor);
        if !descriptor.final_group || !pad.complete() {
            return;
        }
        let group = pad.group;
        self.seats.retain(|seat| seat.upgrade().is_ok());
        for seat in self.seats.iter().filter_map(|seat| seat.upgrade().ok()) {
            let Ok(client) = display.get_client(seat.id()) else {
                continue;
            };
            let tablet = super::tablet_resource(&self.tablets, pad.group, &Resource::id(&seat));
            announce_pad(display, &client, &seat, pad, tablet);
        }
        let target = self.pad_target_for_group(group);
        self.sync_pad_focus(group, target, 0);
    }

    fn forward_pad_event(&mut self, event: TabletPadEvent) {
        let device = match event {
            TabletPadEvent::Button { device, .. } | TabletPadEvent::Dial { device, .. } => device,
            TabletPadEvent::Ring(event) => event.device,
            TabletPadEvent::Strip(event) => event.device,
            TabletPadEvent::Added(_) | TabletPadEvent::Group(_) => return,
        };
        let Some(pad) = self
            .pads
            .iter_mut()
            .find(|pad| pad.descriptor.device == device)
        else {
            return;
        };
        match event {
            TabletPadEvent::Button {
                button,
                mode_group,
                mode,
                pressed,
                time_ns,
                ..
            } => send_button(pad, button, mode_group, mode, pressed, time_ns),
            TabletPadEvent::Ring(event) => send_ring(pad, event),
            TabletPadEvent::Strip(event) => send_strip(pad, event),
            TabletPadEvent::Dial {
                index,
                mode_group,
                mode,
                delta_v120,
                time_ns,
                ..
            } => send_dial(pad, index, mode_group, mode, delta_v120, time_ns),
            TabletPadEvent::Added(_) | TabletPadEvent::Group(_) => {}
        }
    }

    pub(super) fn sync_pad_focus(
        &mut self,
        group: DeviceGroupId,
        target: Option<(ClientId, WlSurface)>,
        time_ns: u64,
    ) {
        let tablets = &self.tablets;
        for pad in self.pads.iter_mut().filter(|pad| pad.group == group) {
            let current = pad
                .focus
                .as_ref()
                .and_then(|focus| focus.surface.upgrade().ok());
            let next_id = target.as_ref().map(|(_, surface)| Resource::id(surface));
            if current.as_ref().map(Resource::id) == next_id {
                continue;
            }
            if let (Some(focus), Some(surface)) = (pad.focus.as_ref(), current) {
                let serial: u32 = next_serial().into();
                for instance in &pad.instances {
                    if instance.client == focus.client
                        && let Ok(resource) = instance.resource.upgrade()
                    {
                        resource.leave(serial, &surface);
                    }
                }
            }
            pad.focus = None;
            let Some((client, surface)) = target.clone() else {
                continue;
            };
            let serial: u32 = next_serial().into();
            let mut delivered = false;
            for instance in &pad.instances {
                if instance.client != client {
                    continue;
                }
                let Some(tablet) = tablets.iter().find(|tablet| tablet.id == group) else {
                    continue;
                };
                let Some(tablet) = tablet.instances.iter().find(|tablet| {
                    tablet.seat == instance.seat && tablet.resource.upgrade().is_ok()
                }) else {
                    continue;
                };
                let (Ok(resource), Ok(tablet)) =
                    (instance.resource.upgrade(), tablet.resource.upgrade())
                else {
                    continue;
                };
                resource.enter(serial, &tablet, &surface);
                for group in &instance.groups {
                    if let Some(descriptor) = pad
                        .groups
                        .iter()
                        .find(|descriptor| descriptor.index == group.index)
                        && let Ok(resource) = group.resource.upgrade()
                    {
                        resource.mode_switch(
                            time_msec(time_ns),
                            serial,
                            descriptor.current_mode.into(),
                        );
                    }
                }
                delivered = true;
            }
            if delivered {
                pad.focus = Some(PadFocus {
                    client,
                    surface: surface.downgrade(),
                });
            }
        }
    }

    pub(super) fn remove_pad_for_device(&mut self, device: DeviceId) {
        let Some(index) = self
            .pads
            .iter()
            .position(|pad| pad.descriptor.device == device)
        else {
            return;
        };
        let pad = self.pads.swap_remove(index);
        for instance in pad.instances {
            if let Ok(resource) = instance.resource.upgrade() {
                resource.removed();
            }
        }
    }

    pub(super) fn pad_surface_destroyed(&mut self, surface: &WlSurface) {
        for pad in &mut self.pads {
            if pad
                .focus
                .as_ref()
                .and_then(|focus| focus.surface.upgrade().ok())
                .is_some_and(|focused| Resource::id(&focused) == Resource::id(surface))
            {
                pad.focus = None;
                pad.pressed = 0;
            }
        }
    }
}

fn send_button(pad: &mut PadState, button: u8, group: u8, mode: u8, pressed: bool, time_ns: u64) {
    if button >= 64 {
        return;
    }
    let bit = 1_u64 << button;
    if (pad.pressed & bit != 0) == pressed {
        return;
    }
    if pressed {
        pad.pressed |= bit;
    } else {
        pad.pressed &= !bit;
    }
    send_mode_switch(pad, group, mode, time_ns);
    let state = if pressed {
        zwp_tablet_pad_v2::ButtonState::Pressed
    } else {
        zwp_tablet_pad_v2::ButtonState::Released
    };
    for_focused_instances(pad, |instance| {
        if let Ok(resource) = instance.resource.upgrade() {
            resource.button(time_msec(time_ns), button.into(), state);
        }
    });
}

fn send_ring(pad: &mut PadState, event: tensor_event::TabletPadRingEvent) {
    send_mode_switch(pad, event.mode_group, event.mode, event.time_ns);
    for_focused_instances(pad, |instance| {
        let Some(group) = instance
            .groups
            .iter()
            .find(|group| group.index == event.mode_group)
        else {
            return;
        };
        let Some(axis) = group.rings.iter().find(|axis| axis.index == event.index) else {
            return;
        };
        let Ok(resource) = axis.resource.upgrade() else {
            return;
        };
        if event.finger {
            resource.source(zwp_tablet_pad_ring_v2::Source::Finger);
        }
        if let Some(position) = event.position {
            resource.angle(position.into());
        } else {
            resource.stop();
        }
        resource.frame(time_msec(event.time_ns));
    });
}

fn send_strip(pad: &mut PadState, event: tensor_event::TabletPadStripEvent) {
    send_mode_switch(pad, event.mode_group, event.mode, event.time_ns);
    for_focused_instances(pad, |instance| {
        let Some(group) = instance
            .groups
            .iter()
            .find(|group| group.index == event.mode_group)
        else {
            return;
        };
        let Some(axis) = group.strips.iter().find(|axis| axis.index == event.index) else {
            return;
        };
        let Ok(resource) = axis.resource.upgrade() else {
            return;
        };
        if event.finger {
            resource.source(zwp_tablet_pad_strip_v2::Source::Finger);
        }
        if let Some(position) = event.position {
            resource.position(normalized_u16(position));
        } else {
            resource.stop();
        }
        resource.frame(time_msec(event.time_ns));
    });
}

fn send_dial(
    pad: &mut PadState,
    index: u8,
    group_index: u8,
    mode: u8,
    delta_v120: i32,
    time_ns: u64,
) {
    if delta_v120 == 0 {
        return;
    }
    send_mode_switch(pad, group_index, mode, time_ns);
    for_focused_instances(pad, |instance| {
        let Some(group) = instance
            .groups
            .iter()
            .find(|group| group.index == group_index)
        else {
            return;
        };
        let Some(axis) = group.dials.iter().find(|axis| axis.index == index) else {
            return;
        };
        if let Ok(resource) = axis.resource.upgrade() {
            resource.delta(delta_v120);
            resource.frame(time_msec(time_ns));
        }
    });
}

fn send_mode_switch(pad: &mut PadState, index: u8, mode: u8, time_ns: u64) {
    let Some(group) = pad.groups.iter_mut().find(|group| group.index == index) else {
        return;
    };
    if group.current_mode == mode || mode >= group.modes {
        return;
    }
    group.current_mode = mode;
    let serial: u32 = next_serial().into();
    for_focused_instances(pad, |instance| {
        if let Some(group) = instance.groups.iter().find(|group| group.index == index)
            && let Ok(resource) = group.resource.upgrade()
        {
            resource.mode_switch(time_msec(time_ns), serial, mode.into());
        }
    });
}

fn for_focused_instances(pad: &PadState, mut apply: impl FnMut(&PadInstance)) {
    let Some(focus) = pad.focus.as_ref() else {
        return;
    };
    for instance in &pad.instances {
        if instance.client == focus.client {
            apply(instance);
        }
    }
}

fn button_array(mask: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(mask.count_ones() as usize * 4);
    for button in 0..64_u32 {
        if mask & (1_u64 << button) != 0 {
            bytes.extend_from_slice(&button.to_ne_bytes());
        }
    }
    bytes
}

fn set_bits(mask: u16) -> impl Iterator<Item = u8> {
    (0..16_u8).filter(move |index| mask & (1_u16 << index) != 0)
}

fn normalized_u16(value: f32) -> u32 {
    (value.clamp(0.0, 1.0) * 65_535.0).round() as u32
}

fn time_msec(time_ns: u64) -> u32 {
    (time_ns / 1_000_000) as u32
}

#[derive(Clone, Copy, Debug)]
struct PadData {
    device: DeviceId,
}

#[derive(Clone, Copy, Debug)]
struct GroupData;

#[derive(Clone, Copy, Debug)]
struct AxisData;

impl DispatchDelegate<ZwpTabletPadV2, RuntimeState> for PadData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &ZwpTabletPadV2,
        request: zwp_tablet_pad_v2::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_tablet_pad_v2::Request::SetFeedback { .. }
            | zwp_tablet_pad_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &ZwpTabletPadV2) {
        if let Some(pad) = state
            .protocol_globals
            .tablet
            .pads
            .iter_mut()
            .find(|pad| pad.descriptor.device == self.device)
        {
            pad.instances
                .retain(|instance| instance.resource.id() != Resource::id(resource));
        }
    }
}

macro_rules! axis_dispatch {
    ($resource:ty, $request:path) => {
        impl DispatchDelegate<$resource, RuntimeState> for AxisData {
            fn request(
                &self,
                _state: &mut RuntimeState,
                _client: &Client,
                _resource: &$resource,
                request: <$resource as Resource>::Request,
                _display: &DisplayHandle,
                _data_init: &mut DataInit<'_, RuntimeState>,
            ) {
                match request {
                    $request { .. } => {}
                    _ => {}
                }
            }
        }
    };
}

impl DispatchDelegate<ZwpTabletPadGroupV2, RuntimeState> for GroupData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &ZwpTabletPadGroupV2,
        request: zwp_tablet_pad_group_v2::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_tablet_pad_group_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

axis_dispatch!(
    ZwpTabletPadRingV2,
    zwp_tablet_pad_ring_v2::Request::SetFeedback
);
axis_dispatch!(
    ZwpTabletPadStripV2,
    zwp_tablet_pad_strip_v2::Request::SetFeedback
);
axis_dispatch!(
    ZwpTabletPadDialV2,
    zwp_tablet_pad_dial_v2::Request::SetFeedback
);

delegate_dispatch!(RuntimeState, ZwpTabletPadV2, PadData);
delegate_dispatch!(RuntimeState, ZwpTabletPadGroupV2, GroupData);
delegate_dispatch!(RuntimeState, ZwpTabletPadRingV2, AxisData);
delegate_dispatch!(RuntimeState, ZwpTabletPadStripV2, AxisData);
delegate_dispatch!(RuntimeState, ZwpTabletPadDialV2, AxisData);
