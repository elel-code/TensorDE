//! Selection device indices, focus publication, and data-control broadcasts.

use std::collections::HashMap;

use wayland_protocols::{
    ext::data_control::v1::server::{
        ext_data_control_device_v1::ExtDataControlDeviceV1,
        ext_data_control_offer_v1::ExtDataControlOfferV1,
    },
    wp::primary_selection::zv1::server::{
        zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
        zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1,
    },
};
use wayland_protocols_wlr::data_control::v1::server::{
    zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
    zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
};
use wayland_server::{
    Client, Resource, Weak,
    backend::{ClientId, ObjectId},
    protocol::{wl_data_device::WlDataDevice, wl_data_offer::WlDataOffer},
};

use super::{
    SelectionProtocol, SelectionTarget, SourceToken,
    offer::{CoreOfferData, SelectionOfferData},
};
use crate::protocol::state::RuntimeState;

impl SelectionProtocol {
    pub(super) fn add_core_device(&mut self, client: &Client, device: &WlDataDevice) {
        self.core_devices
            .entry(client.id())
            .or_default()
            .insert(device.id(), device.downgrade());
        if self.focused_client.as_ref() == Some(&client.id()) {
            self.send_core_device(client, device);
        }
    }

    pub(super) fn remove_core_device(&mut self, client: &ClientId, device: &ObjectId) {
        remove_client_device(&mut self.core_devices, client, device);
    }

    pub(super) fn add_primary_device(
        &mut self,
        client: &Client,
        device: &ZwpPrimarySelectionDeviceV1,
    ) {
        self.primary_devices
            .entry(client.id())
            .or_default()
            .insert(device.id(), device.downgrade());
        if self.focused_client.as_ref() == Some(&client.id()) {
            self.send_primary_device(client, device);
        }
    }

    pub(super) fn remove_primary_device(&mut self, client: &ClientId, device: &ObjectId) {
        remove_client_device(&mut self.primary_devices, client, device);
    }

    pub(super) fn add_wlr_data_control_device(
        &mut self,
        client: &Client,
        device: &ZwlrDataControlDeviceV1,
    ) {
        self.wlr_data_control_devices
            .insert(device.id(), device.downgrade());
        self.send_wlr_device(client, device, SelectionTarget::Clipboard);
        if device.version() >= 2 {
            self.send_wlr_device(client, device, SelectionTarget::Primary);
        }
    }

    pub(super) fn remove_wlr_data_control_device(&mut self, device: &ObjectId) {
        self.wlr_data_control_devices.remove(device);
    }

    pub(super) fn add_ext_data_control_device(
        &mut self,
        client: &Client,
        device: &ExtDataControlDeviceV1,
    ) {
        self.ext_data_control_devices
            .insert(device.id(), device.downgrade());
        self.send_ext_device(client, device, SelectionTarget::Clipboard);
        self.send_ext_device(client, device, SelectionTarget::Primary);
    }

    pub(super) fn remove_ext_data_control_device(&mut self, device: &ObjectId) {
        self.ext_data_control_devices.remove(device);
    }

    pub(super) fn clear_client_devices(&self, client: &ClientId) {
        if let Some(devices) = self.core_devices.get(client) {
            for device in devices.values().filter_map(|device| device.upgrade().ok()) {
                device.selection(None);
            }
        }
        if let Some(devices) = self.primary_devices.get(client) {
            for device in devices.values().filter_map(|device| device.upgrade().ok()) {
                device.selection(None);
            }
        }
    }

    pub(super) fn send_client_devices(&self, client_id: &ClientId) {
        let Some(client) = self.client_for_id(client_id) else {
            return;
        };
        if let Some(devices) = self.core_devices.get(client_id) {
            for device in devices.values().filter_map(|device| device.upgrade().ok()) {
                self.send_core_device(&client, &device);
            }
        }
        if let Some(devices) = self.primary_devices.get(client_id) {
            for device in devices.values().filter_map(|device| device.upgrade().ok()) {
                self.send_primary_device(&client, &device);
            }
        }
    }

    pub(super) fn broadcast_selection(&self, target: SelectionTarget) {
        if let Some(client) = self
            .focused_client
            .as_ref()
            .and_then(|client| self.client_for_id(client))
        {
            match target {
                SelectionTarget::Clipboard => {
                    if let Some(devices) = self.core_devices.get(&client.id()) {
                        for device in devices.values().filter_map(|device| device.upgrade().ok()) {
                            self.send_core_device(&client, &device);
                        }
                    }
                }
                SelectionTarget::Primary => {
                    if let Some(devices) = self.primary_devices.get(&client.id()) {
                        for device in devices.values().filter_map(|device| device.upgrade().ok()) {
                            self.send_primary_device(&client, &device);
                        }
                    }
                }
            }
        }
        for device in self
            .wlr_data_control_devices
            .values()
            .filter_map(|device| device.upgrade().ok())
        {
            if target == SelectionTarget::Primary && device.version() < 2 {
                continue;
            }
            if let Some(client) = device.client() {
                self.send_wlr_device(&client, &device, target);
            }
        }
        for device in self
            .ext_data_control_devices
            .values()
            .filter_map(|device| device.upgrade().ok())
        {
            if let Some(client) = device.client() {
                self.send_ext_device(&client, &device, target);
            }
        }
    }

    pub(super) fn selection(&self, target: SelectionTarget) -> Option<SourceToken> {
        match target {
            SelectionTarget::Clipboard => self.clipboard,
            SelectionTarget::Primary => self.primary,
        }
    }

    fn client_for_id(&self, client: &ClientId) -> Option<Client> {
        let object = self
            .core_devices
            .get(client)
            .and_then(|devices| devices.keys().next())
            .or_else(|| {
                self.primary_devices
                    .get(client)
                    .and_then(|devices| devices.keys().next())
            })?;
        self.display.get_client(object.clone()).ok()
    }

    fn send_core_device(&self, client: &Client, device: &WlDataDevice) {
        let Some(token) = self.clipboard else {
            device.selection(None);
            return;
        };
        let Some(source) = self.sources.get(&token) else {
            device.selection(None);
            return;
        };
        let Ok(offer) = client.create_resource::<WlDataOffer, _, RuntimeState>(
            &self.display,
            device.version(),
            CoreOfferData::selection(token),
        ) else {
            device.selection(None);
            return;
        };
        device.data_offer(&offer);
        for mime_type in &source.mime_types {
            offer.offer(mime_type.clone());
        }
        device.selection(Some(&offer));
    }

    fn send_primary_device(&self, client: &Client, device: &ZwpPrimarySelectionDeviceV1) {
        let Some(token) = self.primary else {
            device.selection(None);
            return;
        };
        let Some(source) = self.sources.get(&token) else {
            device.selection(None);
            return;
        };
        let Ok(offer) = client.create_resource::<ZwpPrimarySelectionOfferV1, _, RuntimeState>(
            &self.display,
            device.version(),
            SelectionOfferData::focused(token, SelectionTarget::Primary),
        ) else {
            device.selection(None);
            return;
        };
        device.data_offer(&offer);
        for mime_type in &source.mime_types {
            offer.offer(mime_type.clone());
        }
        device.selection(Some(&offer));
    }

    fn send_wlr_device(
        &self,
        client: &Client,
        device: &ZwlrDataControlDeviceV1,
        target: SelectionTarget,
    ) {
        let Some(token) = self.selection(target) else {
            match target {
                SelectionTarget::Clipboard => device.selection(None),
                SelectionTarget::Primary => device.primary_selection(None),
            }
            return;
        };
        let Some(source) = self.sources.get(&token) else {
            return;
        };
        let Ok(offer) = client.create_resource::<ZwlrDataControlOfferV1, _, RuntimeState>(
            &self.display,
            device.version(),
            SelectionOfferData::control(token, target),
        ) else {
            return;
        };
        device.data_offer(&offer);
        for mime_type in &source.mime_types {
            offer.offer(mime_type.clone());
        }
        match target {
            SelectionTarget::Clipboard => device.selection(Some(&offer)),
            SelectionTarget::Primary => device.primary_selection(Some(&offer)),
        }
    }

    fn send_ext_device(
        &self,
        client: &Client,
        device: &ExtDataControlDeviceV1,
        target: SelectionTarget,
    ) {
        let Some(token) = self.selection(target) else {
            match target {
                SelectionTarget::Clipboard => device.selection(None),
                SelectionTarget::Primary => device.primary_selection(None),
            }
            return;
        };
        let Some(source) = self.sources.get(&token) else {
            return;
        };
        let Ok(offer) = client.create_resource::<ExtDataControlOfferV1, _, RuntimeState>(
            &self.display,
            device.version(),
            SelectionOfferData::control(token, target),
        ) else {
            return;
        };
        device.data_offer(&offer);
        for mime_type in &source.mime_types {
            offer.offer(mime_type.clone());
        }
        match target {
            SelectionTarget::Clipboard => device.selection(Some(&offer)),
            SelectionTarget::Primary => device.primary_selection(Some(&offer)),
        }
    }
}

fn remove_client_device<R>(
    devices: &mut HashMap<ClientId, HashMap<ObjectId, Weak<R>>>,
    client: &ClientId,
    device: &ObjectId,
) where
    R: Resource,
{
    let remove_client = devices.get_mut(client).is_some_and(|client_devices| {
        client_devices.remove(device);
        client_devices.is_empty()
    });
    if remove_client {
        devices.remove(client);
    }
}
