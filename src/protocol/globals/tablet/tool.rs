//! Tablet tool resources, focus, axes, grabs, and cursor validation.

use tensor_event::{
    DeviceGroupId, DeviceId, TabletToolAxesEvent, TabletToolButtonEvent, TabletToolDescriptor,
    TabletToolId, TabletToolProximityEvent, TabletToolTipEvent, TabletToolType,
};
use tensor_util::{LogicalPoint, Point};
use tracing::warn;
use wayland_protocols::wp::tablet::zv2::server::zwp_tablet_tool_v2::{self, ZwpTabletToolV2};
use wayland_server::{
    Client, DataInit, DisplayHandle, Resource, Weak,
    backend::{ClientId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use super::{MAX_TABLET_SEATS, TabletDevice, TabletProtocol};
#[cfg(feature = "tty")]
use crate::protocol::cursor::CursorImage;
use crate::protocol::{
    dispatch::{DispatchDelegate, delegate_dispatch},
    globals::{
        compositor::{get_role, give_role, with_states},
        seat::CursorSurfaceState,
    },
    serial::next_serial,
    state::RuntimeState,
};

const TOOL_CURSOR_ROLE: &str = "zwp_tablet_tool_v2_cursor";
const MAX_TOOL_BUTTONS: usize = 32;

pub(super) struct ToolState {
    descriptor: TabletToolDescriptor,
    instances: Vec<ToolInstance>,
    normalized: (f32, f32),
    focus: Option<ToolFocus>,
    down: bool,
    buttons: [Option<u32>; MAX_TOOL_BUTTONS],
}

struct ToolInstance {
    seat: ObjectId,
    client: ClientId,
    resource: Weak<ZwpTabletToolV2>,
}

struct ToolFocus {
    client: ClientId,
    group: DeviceGroupId,
    surface: Weak<WlSurface>,
    origin: LogicalPoint<f64>,
    location: LogicalPoint<f64>,
    scale: f64,
    serial: u32,
}

/// Hit-test result computed by compositor policy before borrowing the owner.
pub(in crate::protocol) struct TabletTarget {
    pub(in crate::protocol) surface: WlSurface,
    pub(in crate::protocol) origin: LogicalPoint<f64>,
    pub(in crate::protocol) location: LogicalPoint<f64>,
    pub(in crate::protocol) scale: f64,
}

impl ToolState {
    fn new(descriptor: TabletToolDescriptor) -> Self {
        Self {
            descriptor,
            instances: Vec::with_capacity(MAX_TABLET_SEATS),
            normalized: (0.0, 0.0),
            focus: None,
            down: false,
            buttons: [None; MAX_TOOL_BUTTONS],
        }
    }

    fn grabbed(&self) -> bool {
        self.down || self.buttons.iter().any(Option::is_some)
    }

    pub(super) const fn device(&self) -> DeviceId {
        self.descriptor.device
    }

    pub(super) const fn id(&self) -> TabletToolId {
        self.descriptor.id
    }
}

pub(super) fn announce_tool(
    display: &DisplayHandle,
    client: &Client,
    seat: &super::ZwpTabletSeatV2,
    tool: &mut ToolState,
    tablet: Option<super::ZwpTabletV2>,
) {
    let Ok(resource) = client.create_resource::<ZwpTabletToolV2, _, RuntimeState>(
        display,
        seat.version(),
        ToolData {
            id: tool.descriptor.id,
        },
    ) else {
        return;
    };
    seat.tool_added(&resource);
    resource._type(protocol_tool_type(tool.descriptor.tool_type));
    let serial = tool.descriptor.hardware_serial;
    if serial != 0 {
        resource.hardware_serial((serial >> 32) as u32, serial as u32);
    }
    let hardware_id = tool.descriptor.hardware_id;
    if hardware_id != 0 {
        resource.hardware_id_wacom((hardware_id >> 32) as u32, hardware_id as u32);
    }
    for (bit, capability) in [
        (
            tensor_event::TabletToolCapabilities::TILT,
            zwp_tablet_tool_v2::Capability::Tilt,
        ),
        (
            tensor_event::TabletToolCapabilities::PRESSURE,
            zwp_tablet_tool_v2::Capability::Pressure,
        ),
        (
            tensor_event::TabletToolCapabilities::DISTANCE,
            zwp_tablet_tool_v2::Capability::Distance,
        ),
        (
            tensor_event::TabletToolCapabilities::ROTATION,
            zwp_tablet_tool_v2::Capability::Rotation,
        ),
        (
            tensor_event::TabletToolCapabilities::SLIDER,
            zwp_tablet_tool_v2::Capability::Slider,
        ),
        (
            tensor_event::TabletToolCapabilities::WHEEL,
            zwp_tablet_tool_v2::Capability::Wheel,
        ),
    ] {
        if tool.descriptor.capabilities.contains(bit) {
            resource.capability(capability);
        }
    }
    resource.done();
    tool.instances.push(ToolInstance {
        seat: Resource::id(seat),
        client: client.id(),
        resource: resource.downgrade(),
    });
    if let (Some(focus), Some(tablet)) = (tool.focus.as_ref(), tablet)
        && focus.client == client.id()
        && let Ok(surface) = focus.surface.upgrade()
    {
        resource.proximity_in(focus.serial, &tablet, &surface);
        resource.motion(
            (focus.location.x - focus.origin.x) * focus.scale,
            (focus.location.y - focus.origin.y) * focus.scale,
        );
        if tool.down {
            resource.down(next_serial().into());
        }
        for button in tool.buttons.into_iter().flatten() {
            resource.button(
                next_serial().into(),
                button,
                zwp_tablet_tool_v2::ButtonState::Pressed,
            );
        }
        resource.frame(0);
    }
}

impl TabletProtocol {
    pub(in crate::protocol) fn tool_ids_for_device(
        &self,
        device: DeviceId,
    ) -> impl Iterator<Item = TabletToolId> + '_ {
        self.tools
            .iter()
            .filter(move |tool| tool.descriptor.device == device)
            .map(|tool| tool.descriptor.id)
    }

    #[cfg(test)]
    pub(crate) fn client_for_tool(
        &self,
        display: &DisplayHandle,
        id: TabletToolId,
    ) -> Option<Client> {
        let resource = self
            .tools
            .iter()
            .find(|tool| tool.descriptor.id == id)?
            .instances
            .first()?
            .resource
            .upgrade()
            .ok()?;
        display.get_client(Resource::id(&resource)).ok()
    }

    pub(in crate::protocol) fn add_tool(
        &mut self,
        display: &DisplayHandle,
        descriptor: TabletToolDescriptor,
    ) {
        if self
            .tools
            .iter()
            .any(|tool| tool.descriptor.id == descriptor.id)
        {
            return;
        }
        if self.tools.len() == super::MAX_TOOLS {
            warn!(tool = descriptor.id.get(), "tablet tool capacity exceeded");
            return;
        }
        let mut tool = ToolState::new(descriptor);
        self.seats.retain(|seat| seat.upgrade().is_ok());
        for seat in self.seats.iter().filter_map(|seat| seat.upgrade().ok()) {
            let Ok(client) = display.get_client(seat.id()) else {
                continue;
            };
            announce_tool(display, &client, &seat, &mut tool, None);
        }
        self.tools.push(tool);
    }

    pub(in crate::protocol) fn normalized_after_axes(
        &mut self,
        event: TabletToolAxesEvent,
    ) -> Option<(f32, f32)> {
        let tool = self
            .tools
            .iter_mut()
            .find(|tool| tool.descriptor.id == event.id)?;
        if let Some(x) = event.x() {
            tool.normalized.0 = x;
        }
        if let Some(y) = event.y() {
            tool.normalized.1 = y;
        }
        Some(tool.normalized)
    }

    pub(in crate::protocol) fn tool_proximity(
        &mut self,
        event: TabletToolProximityEvent,
        target: Option<TabletTarget>,
    ) {
        let Some(group) = self.group_for_device(event.device) else {
            return;
        };
        {
            let (tools, tablets) = (&mut self.tools, &self.tablets);
            let Some(tool) = tools.iter_mut().find(|tool| tool.descriptor.id == event.id) else {
                return;
            };
            tool.normalized = (event.x, event.y);
            if event.in_proximity {
                replace_focus(tool, tablets, group, target, event.time_ns);
            } else {
                release_focus(tool, event.time_ns);
            }
        }
        let target = self.pad_target_for_group(group);
        self.sync_pad_focus(group, target, event.time_ns);
    }

    pub(in crate::protocol) fn tool_axes(
        &mut self,
        event: TabletToolAxesEvent,
        target: Option<TabletTarget>,
    ) {
        let group = self
            .tools
            .iter()
            .find(|tool| tool.descriptor.id == event.id)
            .and_then(|tool| {
                tool.focus
                    .as_ref()
                    .map(|focus| focus.group)
                    .or_else(|| self.group_for_device(tool.descriptor.device))
            });
        {
            let (tools, tablets) = (&mut self.tools, &self.tablets);
            let Some(tool) = tools.iter_mut().find(|tool| tool.descriptor.id == event.id) else {
                return;
            };
            if !tool.grabbed() {
                if let Some(group) = group {
                    replace_focus(tool, tablets, group, target, event.time_ns);
                }
            } else if let (Some(focus), Some(target)) = (tool.focus.as_mut(), target) {
                focus.location = target.location;
            }
            send_axes(tool, event);
        }
        if let Some(group) = group {
            let target = self.pad_target_for_group(group);
            self.sync_pad_focus(group, target, event.time_ns);
        }
    }

    pub(in crate::protocol) fn tool_tip(&mut self, event: TabletToolTipEvent) {
        let Some(tool) = self
            .tools
            .iter_mut()
            .find(|tool| tool.descriptor.id == event.id)
        else {
            return;
        };
        if tool.focus.is_none() || tool.down == event.down {
            return;
        }
        tool.down = event.down;
        for_focused_resources(tool, |resource| {
            if event.down {
                resource.down(next_serial().into());
            } else {
                resource.up();
            }
            resource.frame(time_msec(event.time_ns));
        });
    }

    pub(in crate::protocol) fn tool_button(&mut self, event: TabletToolButtonEvent) {
        let Some(tool) = self
            .tools
            .iter_mut()
            .find(|tool| tool.descriptor.id == event.id)
        else {
            return;
        };
        if tool.focus.is_none() || !update_button(tool, event.button, event.pressed) {
            return;
        }
        let state = if event.pressed {
            zwp_tablet_tool_v2::ButtonState::Pressed
        } else {
            zwp_tablet_tool_v2::ButtonState::Released
        };
        for_focused_resources(tool, |resource| {
            resource.button(next_serial().into(), event.button, state);
            resource.frame(time_msec(event.time_ns));
        });
    }

    pub(super) fn remove_tools_for_device(&mut self, device: DeviceId) {
        let mut index = 0;
        while index < self.tools.len() {
            if self.tools[index].descriptor.device != device {
                index += 1;
                continue;
            }
            let mut tool = self.tools.swap_remove(index);
            release_focus(&mut tool, 0);
            for instance in tool.instances {
                if let Ok(resource) = instance.resource.upgrade() {
                    resource.removed();
                }
            }
        }
    }

    pub(in crate::protocol) fn tablet_surface_destroyed(&mut self, surface: &WlSurface) {
        for tool in &mut self.tools {
            if tool
                .focus
                .as_ref()
                .and_then(|focus| focus.surface.upgrade().ok())
                .is_some_and(|focused| Resource::id(&focused) == Resource::id(surface))
            {
                release_focus(tool, 0);
            }
        }
        self.cursor_surfaces
            .retain(|(cursor, _)| cursor.id() != Resource::id(surface));
        self.pad_surface_destroyed(surface);
    }

    pub(super) fn group_for_device(&self, device: DeviceId) -> Option<DeviceGroupId> {
        self.devices
            .iter()
            .find_map(|(id, group)| (*id == device).then_some(*group))
    }

    pub(super) fn pad_target_for_group(
        &self,
        group: DeviceGroupId,
    ) -> Option<(ClientId, WlSurface)> {
        self.tools.iter().rev().find_map(|tool| {
            let focus = tool.focus.as_ref()?;
            if focus.group != group {
                return None;
            }
            focus
                .surface
                .upgrade()
                .ok()
                .map(|surface| (focus.client.clone(), surface))
        })
    }

    pub(crate) fn may_set_cursor(&self, id: TabletToolId, client: &ClientId, serial: u32) -> bool {
        self.tools.iter().any(|tool| {
            tool.descriptor.id == id
                && tool
                    .focus
                    .as_ref()
                    .is_some_and(|focus| focus.client == *client && focus.serial == serial)
        })
    }
}

fn replace_focus(
    tool: &mut ToolState,
    tablets: &[TabletDevice],
    group: DeviceGroupId,
    target: Option<TabletTarget>,
    time_ns: u64,
) {
    let next_id = target.as_ref().map(|target| Resource::id(&target.surface));
    let current_id = tool
        .focus
        .as_ref()
        .and_then(|focus| focus.surface.upgrade().ok())
        .map(|surface| Resource::id(&surface));
    if current_id == next_id
        && tool
            .focus
            .as_ref()
            .is_some_and(|focus| focus.group == group)
    {
        if let (Some(focus), Some(target)) = (tool.focus.as_mut(), target) {
            focus.origin = target.origin;
            focus.location = target.location;
            focus.scale = target.scale;
        }
        return;
    }
    release_focus(tool, time_ns);
    let Some(target) = target else {
        return;
    };
    let Some(client) = target.surface.client() else {
        return;
    };
    let serial: u32 = next_serial().into();
    let mut delivered = false;
    for instance in &tool.instances {
        if instance.client != client.id() {
            continue;
        }
        let Some(tablet) = tablets.iter().find(|tablet| tablet.id == group) else {
            continue;
        };
        let Some(tablet) = tablet
            .instances
            .iter()
            .find(|tablet| tablet.seat == instance.seat && tablet.resource.upgrade().is_ok())
        else {
            continue;
        };
        let (Ok(resource), Ok(tablet)) = (instance.resource.upgrade(), tablet.resource.upgrade())
        else {
            continue;
        };
        resource.proximity_in(serial, &tablet, &target.surface);
        resource.motion(
            (target.location.x - target.origin.x) * target.scale,
            (target.location.y - target.origin.y) * target.scale,
        );
        delivered = true;
    }
    if delivered {
        tool.focus = Some(ToolFocus {
            client: client.id(),
            group,
            surface: target.surface.downgrade(),
            origin: target.origin,
            location: target.location,
            scale: target.scale,
            serial,
        });
    }
}

fn release_focus(tool: &mut ToolState, time_ns: u64) {
    if tool.focus.is_none() {
        tool.down = false;
        tool.buttons.fill(None);
        return;
    }
    let serial: u32 = next_serial().into();
    let buttons = tool.buttons;
    let down = tool.down;
    for_focused_resources(tool, |resource| {
        for button in buttons.into_iter().flatten() {
            resource.button(serial, button, zwp_tablet_tool_v2::ButtonState::Released);
        }
        if down {
            resource.up();
        }
        resource.proximity_out();
        resource.frame(time_msec(time_ns));
    });
    tool.buttons.fill(None);
    tool.down = false;
    tool.focus = None;
}

fn send_axes(tool: &ToolState, event: TabletToolAxesEvent) {
    let Some(focus) = tool.focus.as_ref() else {
        return;
    };
    let x = event.x().map(|value| value as f64);
    let y = event.y().map(|value| value as f64);
    for_focused_resources(tool, |resource| {
        if x.is_some() || y.is_some() {
            resource.motion(
                (focus.location.x - focus.origin.x) * focus.scale,
                (focus.location.y - focus.origin.y) * focus.scale,
            );
        }
        if let Some(value) = event.pressure() {
            resource.pressure(normalized_u16(value));
        }
        if let Some(value) = event.distance() {
            resource.distance(normalized_u16(value));
        }
        if let Some((x, y)) = event.tilt() {
            resource.tilt(x.into(), y.into());
        }
        if let Some(value) = event.rotation() {
            resource.rotation(value.into());
        }
        if let Some(value) = event.slider() {
            resource.slider((value.clamp(-1.0, 1.0) * 65_535.0).round() as i32);
        }
        if let Some((degrees, clicks)) = event.wheel() {
            resource.wheel(degrees.into(), clicks.into());
        }
        if event.final_frame() {
            resource.frame(time_msec(event.time_ns));
        }
    });
}

fn for_focused_resources(tool: &ToolState, mut apply: impl FnMut(ZwpTabletToolV2)) {
    let Some(focus) = tool.focus.as_ref() else {
        return;
    };
    for instance in &tool.instances {
        if instance.client == focus.client
            && let Ok(resource) = instance.resource.upgrade()
        {
            apply(resource);
        }
    }
}

fn update_button(tool: &mut ToolState, button: u32, pressed: bool) -> bool {
    if pressed {
        if tool.buttons.contains(&Some(button)) {
            return false;
        }
        let Some(slot) = tool.buttons.iter_mut().find(|slot| slot.is_none()) else {
            warn!(
                tool = tool.descriptor.id.get(),
                "tablet tool button capacity exceeded"
            );
            return false;
        };
        *slot = Some(button);
        true
    } else if let Some(slot) = tool.buttons.iter_mut().find(|slot| **slot == Some(button)) {
        *slot = None;
        true
    } else {
        false
    }
}

fn normalized_u16(value: f32) -> u32 {
    (value.clamp(0.0, 1.0) * 65_535.0).round() as u32
}

fn time_msec(time_ns: u64) -> u32 {
    (time_ns / 1_000_000) as u32
}

fn protocol_tool_type(tool_type: TabletToolType) -> zwp_tablet_tool_v2::Type {
    match tool_type {
        TabletToolType::Pen => zwp_tablet_tool_v2::Type::Pen,
        TabletToolType::Eraser => zwp_tablet_tool_v2::Type::Eraser,
        TabletToolType::Brush => zwp_tablet_tool_v2::Type::Brush,
        TabletToolType::Pencil => zwp_tablet_tool_v2::Type::Pencil,
        TabletToolType::Airbrush => zwp_tablet_tool_v2::Type::Airbrush,
        TabletToolType::Mouse => zwp_tablet_tool_v2::Type::Mouse,
        TabletToolType::Lens => zwp_tablet_tool_v2::Type::Lens,
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ToolData {
    pub(super) id: TabletToolId,
}

impl DispatchDelegate<ZwpTabletToolV2, RuntimeState> for ToolData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        resource: &ZwpTabletToolV2,
        request: zwp_tablet_tool_v2::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_tablet_tool_v2::Request::SetCursor {
                serial,
                surface,
                hotspot_x,
                hotspot_y,
            } => {
                if !state
                    .protocol_globals
                    .tablet
                    .may_set_cursor(self.id, &client.id(), serial)
                {
                    return;
                }
                let surface = match surface {
                    Some(surface) => {
                        let owner = state
                            .protocol_globals
                            .tablet
                            .cursor_surfaces
                            .iter()
                            .find(|(cursor, _)| cursor.id() == Resource::id(&surface))
                            .map(|(_, tool)| *tool);
                        if owner.is_some_and(|tool| tool != self.id)
                            || (give_role(&surface, TOOL_CURSOR_ROLE).is_err()
                                && get_role(&surface) != Some(TOOL_CURSOR_ROLE))
                        {
                            resource.post_error(
                                zwp_tablet_tool_v2::Error::Role,
                                "tablet cursor surface already has another role or tool",
                            );
                            return;
                        }
                        if owner.is_none() {
                            state
                                .protocol_globals
                                .tablet
                                .cursor_surfaces
                                .retain(|(cursor, _)| cursor.upgrade().is_ok());
                            if state.protocol_globals.tablet.cursor_surfaces.len()
                                == super::MAX_TOOLS
                            {
                                resource.post_error(
                                    zwp_tablet_tool_v2::Error::Role,
                                    "tablet cursor surface capacity exceeded",
                                );
                                return;
                            }
                            state
                                .protocol_globals
                                .tablet
                                .cursor_surfaces
                                .push((surface.downgrade(), self.id));
                        }
                        let scale = state.client_scale(client);
                        let hotspot = Point::new(
                            logical_hotspot(hotspot_x, scale),
                            logical_hotspot(hotspot_y, scale),
                        );
                        with_states(&surface, |states| {
                            let storage = states.data_map.get_or_insert(|| {
                                std::sync::Mutex::new(CursorSurfaceState::default())
                            });
                            storage.lock().unwrap().hotspot = hotspot;
                        });
                        Some(surface)
                    }
                    None => None,
                };
                #[cfg(feature = "tty")]
                if state.cursor.set_tablet_image(
                    self.id,
                    match surface {
                        Some(surface) => CursorImage::Surface(surface),
                        None => CursorImage::Hidden,
                    },
                ) {
                    state.request_redraw_all();
                }
                #[cfg(not(feature = "tty"))]
                let _ = surface;
            }
            zwp_tablet_tool_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &ZwpTabletToolV2) {
        if let Some(tool) = state
            .protocol_globals
            .tablet
            .tools
            .iter_mut()
            .find(|tool| tool.descriptor.id == self.id)
        {
            tool.instances
                .retain(|instance| instance.resource.id() != Resource::id(resource));
        }
    }
}

fn logical_hotspot(value: i32, scale: f64) -> i32 {
    if !scale.is_finite() || scale <= 0.0 {
        return value;
    }
    (f64::from(value) / scale)
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

delegate_dispatch!(RuntimeState, ZwpTabletToolV2, ToolData);
