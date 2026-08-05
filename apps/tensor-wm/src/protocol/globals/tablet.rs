//! Stable tablet-v2 device discovery and compositor-thread protocol ownership.

use tensor_event::{DeviceChange, DeviceEvent, DeviceGroupId, DeviceId};
use tracing::warn;
use wayland_protocols::wp::tablet::zv2::server::zwp_tablet_tool_v2::ZwpTabletToolV2;
use wayland_protocols::wp::tablet::zv2::server::{
    zwp_tablet_manager_v2::{self, ZwpTabletManagerV2},
    zwp_tablet_seat_v2::{self, ZwpTabletSeatV2},
    zwp_tablet_v2::{self, ZwpTabletV2},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

mod pad;
pub(in crate::protocol) mod tool;

use pad::{PadState, announce_pad};
use tool::{ToolState, announce_tool};

const VERSION: u32 = 2;
const MAX_TABLETS: usize = 16;
const MAX_TABLET_DEVICES: usize = 32;
const MAX_TABLET_SEATS: usize = 64;
const CAPACITY_ERROR: u32 = 0;
const MAX_TOOLS: usize = 64;
const MAX_PADS: usize = 16;

struct TabletDevice {
    id: DeviceGroupId,
    members: u8,
    bus_type: u32,
    vendor_id: u32,
    product_id: u32,
    instances: Vec<TabletInstance>,
}

struct TabletInstance {
    seat: ObjectId,
    resource: Weak<ZwpTabletV2>,
}

pub(crate) struct TabletProtocol {
    _global: GlobalId,
    seats: Vec<Weak<ZwpTabletSeatV2>>,
    tablets: Vec<TabletDevice>,
    devices: Vec<(DeviceId, DeviceGroupId)>,
    tools: Vec<ToolState>,
    cursor_surfaces: Vec<(
        Weak<wayland_server::protocol::wl_surface::WlSurface>,
        tensor_event::TabletToolId,
    )>,
    pads: Vec<PadState>,
}

impl TabletProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display
                .create_global::<RuntimeState, ZwpTabletManagerV2, _>(VERSION, GlobalData),
            seats: Vec::with_capacity(MAX_TABLET_SEATS),
            tablets: Vec::with_capacity(MAX_TABLETS),
            devices: Vec::with_capacity(MAX_TABLET_DEVICES),
            tools: Vec::with_capacity(MAX_TOOLS),
            cursor_surfaces: Vec::with_capacity(MAX_TOOLS),
            pads: Vec::with_capacity(MAX_PADS),
        }
    }

    pub(crate) fn cursor_shape_tool(
        &self,
        resource: &ZwpTabletToolV2,
    ) -> Option<tensor_event::TabletToolId> {
        let id = resource.data::<tool::ToolData>()?.id;
        self.tools.iter().any(|tool| tool.id() == id).then_some(id)
    }

    fn register_seat(
        &mut self,
        display: &DisplayHandle,
        client: &Client,
        seat: &ZwpTabletSeatV2,
    ) -> bool {
        self.seats.retain(|seat| seat.upgrade().is_ok());
        if self.seats.len() == MAX_TABLET_SEATS {
            return false;
        }
        for tablet in &mut self.tablets {
            announce_tablet(display, client, seat, tablet);
        }
        let seat_id = Resource::id(seat);
        let devices = &self.devices;
        let tablets = &self.tablets;
        for tool in &mut self.tools {
            let tablet = devices
                .iter()
                .find_map(|(device, group)| (*device == tool.device()).then_some(*group))
                .and_then(|group| tablet_resource(tablets, group, &seat_id));
            announce_tool(display, client, seat, tool, tablet);
        }
        for pad in self.pads.iter_mut().filter(|pad| pad.complete()) {
            let tablet = tablet_resource(tablets, pad.group(), &seat_id);
            announce_pad(display, client, seat, pad, tablet);
        }
        self.seats.push(seat.downgrade());
        true
    }

    pub(crate) fn device_changed(&mut self, display: &DisplayHandle, event: DeviceEvent) {
        if !event.capabilities.tablet {
            return;
        }
        match event.change {
            DeviceChange::Added => {
                if self.devices.iter().any(|(device, _)| *device == event.id) {
                    return;
                }
                if self.devices.len() == MAX_TABLET_DEVICES {
                    warn!(device = event.id.get(), "tablet device capacity exceeded");
                    return;
                }
                self.devices.push((event.id, event.group));
                if let Some(tablet) = self
                    .tablets
                    .iter_mut()
                    .find(|tablet| tablet.id == event.group)
                {
                    tablet.members = tablet.members.saturating_add(1);
                    return;
                }
                if self.tablets.len() == MAX_TABLETS {
                    warn!(
                        device = event.id.get(),
                        "tablet-v2 device capacity exceeded"
                    );
                    self.devices.pop();
                    return;
                }
                let mut tablet = TabletDevice {
                    id: event.group,
                    members: 1,
                    bus_type: event.bus_type,
                    vendor_id: event.vendor_id,
                    product_id: event.product_id,
                    instances: Vec::with_capacity(MAX_TABLET_SEATS),
                };
                self.seats.retain(|seat| seat.upgrade().is_ok());
                for seat in self.seats.iter().filter_map(|seat| seat.upgrade().ok()) {
                    let Ok(client) = display.get_client(seat.id()) else {
                        continue;
                    };
                    announce_tablet(display, &client, &seat, &mut tablet);
                }
                self.tablets.push(tablet);
            }
            DeviceChange::Removed => {
                self.remove_tools_for_device(event.id);
                self.remove_pad_for_device(event.id);
                let Some(device_index) = self
                    .devices
                    .iter()
                    .position(|(device, _)| *device == event.id)
                else {
                    return;
                };
                let (_, group) = self.devices.swap_remove(device_index);
                let Some(index) = self.tablets.iter().position(|tablet| tablet.id == group) else {
                    return;
                };
                if self.tablets[index].members > 1 {
                    self.tablets[index].members -= 1;
                    return;
                }
                let tablet = self.tablets.swap_remove(index);
                for resource in tablet
                    .instances
                    .into_iter()
                    .filter_map(|instance| instance.resource.upgrade().ok())
                {
                    resource.removed();
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn tablet_count(&self) -> usize {
        self.tablets.len()
    }
}

fn announce_tablet(
    display: &DisplayHandle,
    client: &Client,
    seat: &ZwpTabletSeatV2,
    tablet: &mut TabletDevice,
) {
    let Ok(resource) = client.create_resource::<ZwpTabletV2, _, RuntimeState>(
        display,
        seat.version(),
        TabletData { id: tablet.id },
    ) else {
        return;
    };
    seat.tablet_added(&resource);
    if tablet.vendor_id != 0 || tablet.product_id != 0 {
        resource.id(tablet.vendor_id, tablet.product_id);
    }
    if resource.version() >= 2
        && let Some(bus_type) = protocol_bus_type(tablet.bus_type)
    {
        resource.bustype(bus_type);
    }
    resource.done();
    tablet.instances.push(TabletInstance {
        seat: Resource::id(seat),
        resource: resource.downgrade(),
    });
}

fn protocol_bus_type(bus_type: u32) -> Option<zwp_tablet_v2::Bustype> {
    Some(match bus_type {
        3 => zwp_tablet_v2::Bustype::Usb,
        5 => zwp_tablet_v2::Bustype::Bluetooth,
        6 => zwp_tablet_v2::Bustype::Virtual,
        17 => zwp_tablet_v2::Bustype::Serial,
        24 => zwp_tablet_v2::Bustype::I2c,
        _ => return None,
    })
}

fn tablet_resource(
    tablets: &[TabletDevice],
    group: DeviceGroupId,
    seat: &ObjectId,
) -> Option<ZwpTabletV2> {
    tablets
        .iter()
        .find(|tablet| tablet.id == group)?
        .instances
        .iter()
        .find(|tablet| tablet.seat == *seat)?
        .resource
        .upgrade()
        .ok()
}

#[derive(Clone, Copy, Debug)]
struct GlobalData;

#[derive(Clone, Copy, Debug)]
struct ManagerData;

#[derive(Clone, Copy, Debug)]
struct TabletSeatData;

#[derive(Clone, Copy, Debug)]
struct TabletData {
    #[allow(dead_code)]
    id: DeviceGroupId,
}

impl GlobalDispatchDelegate<ZwpTabletManagerV2, RuntimeState> for GlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpTabletManagerV2>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, ManagerData);
    }
}

impl DispatchDelegate<ZwpTabletManagerV2, RuntimeState> for ManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        resource: &ZwpTabletManagerV2,
        request: zwp_tablet_manager_v2::Request,
        display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_tablet_manager_v2::Request::GetTabletSeat { tablet_seat, seat } => {
                if !state.protocol_globals.seat.owns(&seat) {
                    resource.post_error(CAPACITY_ERROR, "seat is not owned by Tensor");
                    return;
                }
                let tablet_seat = data_init.init(tablet_seat, TabletSeatData);
                if !state
                    .protocol_globals
                    .tablet
                    .register_seat(display, client, &tablet_seat)
                {
                    resource.post_error(CAPACITY_ERROR, "tablet-seat capacity exceeded");
                }
            }
            zwp_tablet_manager_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwpTabletSeatV2, RuntimeState> for TabletSeatData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &ZwpTabletSeatV2,
        request: zwp_tablet_seat_v2::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_tablet_seat_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwpTabletV2, RuntimeState> for TabletData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &ZwpTabletV2,
        request: zwp_tablet_v2::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_tablet_v2::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &ZwpTabletV2) {
        if let Some(tablet) = state
            .protocol_globals
            .tablet
            .tablets
            .iter_mut()
            .find(|tablet| tablet.id == self.id)
        {
            tablet
                .instances
                .retain(|instance| instance.resource.id() != Resource::id(resource));
        }
    }
}

delegate_global_dispatch!(RuntimeState, ZwpTabletManagerV2, GlobalData);
delegate_dispatch!(RuntimeState, ZwpTabletManagerV2, ManagerData);
delegate_dispatch!(RuntimeState, ZwpTabletSeatV2, TabletSeatData);
delegate_dispatch!(RuntimeState, ZwpTabletV2, TabletData);

#[cfg(test)]
mod tests {
    use tensor_event::{DeviceCapabilities, DeviceGroupId};
    use wayland_server::Display;

    use super::*;

    fn event(id: u64, group: u64, change: DeviceChange) -> DeviceEvent {
        DeviceEvent {
            id: DeviceId::new(id),
            group: DeviceGroupId::new(group),
            bus_type: 3,
            vendor_id: 0x56,
            product_id: 0x78,
            capabilities: DeviceCapabilities {
                tablet: true,
                ..DeviceCapabilities::empty()
            },
            change,
        }
    }

    #[test]
    fn grouped_tool_and_pad_nodes_share_one_tablet_lifetime() {
        let display = Display::<RuntimeState>::new().unwrap();
        let handle = display.handle();
        let mut protocol = TabletProtocol::new(&handle);

        protocol.device_changed(&handle, event(1, 9, DeviceChange::Added));
        protocol.device_changed(&handle, event(2, 9, DeviceChange::Added));
        assert_eq!(protocol.tablet_count(), 1);

        protocol.device_changed(&handle, event(1, 9, DeviceChange::Removed));
        assert_eq!(protocol.tablet_count(), 1);
        protocol.device_changed(&handle, event(2, 9, DeviceChange::Removed));
        assert_eq!(protocol.tablet_count(), 0);
    }
}
