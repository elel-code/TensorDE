//! Tensor-owned core drag-and-drop state.

mod offer;

use std::{
    collections::HashMap,
    os::fd::{AsFd, OwnedFd},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use wayland_server::{
    DisplayHandle, Resource, Weak,
    backend::{ClientId, Handle, ObjectData, ObjectId, protocol::Message},
    protocol::{
        wl_data_device::{self, WlDataDevice},
        wl_data_device_manager::DndAction as WlDndAction,
        wl_data_offer::{self, WlDataOffer},
        wl_data_source::{self, WlDataSource},
        wl_surface::WlSurface,
    },
};

use super::{
    SelectionProtocol, SetSelectionError, SourceKind, SourceResource, SourceToken, SourceUse,
};
use crate::protocol::{globals::compositor, serial::Serial, state::RuntimeState};

const DND_ICON_ROLE: &str = "wl_data_device.dnd_icon";
const ACTION_NEGOTIATION_VERSION: u32 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DndGrabKind {
    Pointer,
}

#[derive(Clone)]
struct DndSourceMetadata {
    mime_types: Vec<String>,
    actions: WlDndAction,
}

trait DndSource: Send + Sync {
    fn alive(&self) -> bool;
    fn local_for(&self, surface: &WlSurface) -> bool;
    fn metadata(&self) -> Option<DndSourceMetadata>;
    fn resource(&self) -> Option<Weak<WlDataSource>>;
    fn choose_action(&self, action: WlDndAction);
    fn send(&self, mime_type: &str, fd: OwnedFd);
    fn drop_performed(&self);
    fn cancel(&self);
    fn finished(&self);
}

struct CoreDndSource {
    resource: WlDataSource,
    metadata: DndSourceMetadata,
    cancelled: AtomicBool,
    finished: AtomicBool,
}

impl DndSource for CoreDndSource {
    fn alive(&self) -> bool {
        self.resource.is_alive() && !self.cancelled.load(Ordering::Acquire)
    }

    fn local_for(&self, _surface: &WlSurface) -> bool {
        false
    }

    fn metadata(&self) -> Option<DndSourceMetadata> {
        self.alive().then(|| self.metadata.clone())
    }

    fn resource(&self) -> Option<Weak<WlDataSource>> {
        Some(self.resource.downgrade())
    }

    fn choose_action(&self, action: WlDndAction) {
        if self.resource.version() >= wl_data_source::EVT_ACTION_SINCE && self.resource.is_alive() {
            self.resource.action(action);
        }
    }

    fn send(&self, mime_type: &str, fd: OwnedFd) {
        if self.alive()
            && self
                .metadata
                .mime_types
                .iter()
                .any(|known| known == mime_type)
        {
            self.resource.send(mime_type.to_owned(), fd.as_fd());
        }
    }

    fn drop_performed(&self) {
        if self.resource.version() >= wl_data_source::EVT_DND_DROP_PERFORMED_SINCE
            && self.resource.is_alive()
        {
            self.resource.dnd_drop_performed();
        }
    }

    fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::AcqRel)
            && self.resource.version() >= ACTION_NEGOTIATION_VERSION
            && self.resource.is_alive()
        {
            self.resource.cancelled();
        }
    }

    fn finished(&self) {
        if !self.finished.swap(true, Ordering::AcqRel)
            && self.resource.version() >= wl_data_source::EVT_DND_FINISHED_SINCE
            && self.resource.is_alive()
        {
            self.resource.dnd_finished();
        }
    }
}

struct LocalDndSource {
    origin: WlSurface,
}

impl DndSource for LocalDndSource {
    fn alive(&self) -> bool {
        self.origin.is_alive()
    }

    fn local_for(&self, surface: &WlSurface) -> bool {
        surface.id().same_client_as(&self.origin.id())
    }

    fn metadata(&self) -> Option<DndSourceMetadata> {
        None
    }

    fn resource(&self) -> Option<Weak<WlDataSource>> {
        None
    }

    fn choose_action(&self, _action: WlDndAction) {}
    fn send(&self, _mime_type: &str, _fd: OwnedFd) {}
    fn drop_performed(&self) {}
    fn cancel(&self) {}
    fn finished(&self) {}
}

struct DndOfferState {
    active: bool,
    dropped: bool,
    accepted: bool,
    finished: bool,
    requires_accept: bool,
    requires_action: bool,
    source_actions: WlDndAction,
    chosen_action: WlDndAction,
}

impl DndOfferState {
    fn validated(&self) -> bool {
        self.active
            && (!self.requires_accept || self.accepted)
            && (!self.requires_action || !self.chosen_action.is_empty())
    }

    fn ready_to_finish(&self) -> bool {
        self.validated() && self.dropped && !self.finished
    }
}

struct DndOfferObject {
    state: Arc<Mutex<DndOfferState>>,
    source: Arc<Mutex<Option<Arc<dyn DndSource>>>>,
    target_source: Option<Weak<WlDataSource>>,
    mime_types: Arc<[String]>,
}

impl ObjectData<RuntimeState> for DndOfferObject {
    fn request(
        self: Arc<Self>,
        handle: &Handle,
        _state: &mut RuntimeState,
        _client: ClientId,
        message: Message<ObjectId, OwnedFd>,
    ) -> Option<Arc<dyn ObjectData<RuntimeState>>> {
        let display = DisplayHandle::from(handle.clone());
        if let Ok((offer, request)) = WlDataOffer::parse_request(&display, message) {
            offer::handle_request(&offer, request, &self);
        }
        None
    }

    fn destroyed(
        self: Arc<Self>,
        _handle: &Handle,
        _state: &mut RuntimeState,
        _client: ClientId,
        _object: ObjectId,
    ) {
        let mut state = self.state.lock().unwrap();
        if state.finished {
            return;
        }
        state.active = false;
        state.accepted = false;
        state.chosen_action = WlDndAction::empty();
        if state.dropped
            && let Some(source) = self.source.lock().unwrap().take()
        {
            source.cancel();
        }
    }
}

struct SurfaceDndOffer {
    state: Arc<Mutex<DndOfferState>>,
    source: Arc<Mutex<Option<Arc<dyn DndSource>>>>,
    devices: Vec<WlDataDevice>,
    _offers: Vec<WlDataOffer>,
}

impl SurfaceDndOffer {
    fn disable(&self) {
        let mut state = self.state.lock().unwrap();
        state.active = false;
        state.accepted = false;
        state.chosen_action = WlDndAction::empty();
        if let Some(source) = self.source.lock().unwrap().take() {
            source.choose_action(WlDndAction::empty());
        }
    }

    fn motion(&self, time: u32, location: (f64, f64)) {
        for device in self.devices.iter().filter(|device| device.is_alive()) {
            device.motion(time, location.0, location.1);
        }
    }

    fn leave(&self) {
        for device in self.devices.iter().filter(|device| device.is_alive()) {
            device.leave();
        }
    }

    fn drop(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        state.dropped = true;
        let validated = state.validated();
        for device in self.devices.iter().filter(|device| device.is_alive()) {
            if device.version() < 3 || !state.chosen_action.is_empty() {
                device.drop();
            }
        }
        validated
    }
}

struct DndTarget {
    surface: WlSurface,
    offer: Option<SurfaceDndOffer>,
}

pub(super) struct ActiveDnd {
    pub(super) source_token: Option<SourceToken>,
    pub(super) origin: ObjectId,
    pub(super) kind: DndGrabKind,
    source: Arc<dyn DndSource>,
    target: Option<DndTarget>,
}

impl SelectionProtocol {
    fn core_devices_for_surface(&self, surface: &WlSurface) -> Vec<WlDataDevice> {
        let Some(client) = surface.client() else {
            return Vec::new();
        };
        self.core_devices
            .get(&client.id())
            .into_iter()
            .flat_map(HashMap::values)
            .filter_map(|device| device.upgrade().ok())
            .collect()
    }

    fn begin_core_dnd(
        &mut self,
        client: &ClientId,
        token: SourceToken,
        origin: ObjectId,
    ) -> Result<(), SetSelectionError> {
        let record = self
            .sources
            .get_mut(&token)
            .ok_or(SetSelectionError::UnknownSource)?;
        if &record.client != client || record.kind != SourceKind::Core {
            return Err(SetSelectionError::WrongSource);
        }
        if record.use_.is_some() || self.active_dnd.is_some() {
            return Err(SetSelectionError::UsedSource);
        }
        let SourceResource::Core(resource) = &record.resource else {
            return Err(SetSelectionError::WrongSource);
        };
        let resource = resource
            .upgrade()
            .map_err(|_| SetSelectionError::UnknownSource)?;
        record.use_ = Some(SourceUse::Dnd);
        let actions = if resource.version() < ACTION_NEGOTIATION_VERSION {
            WlDndAction::Copy
        } else {
            record.core_actions
        };
        let source: Arc<dyn DndSource> = Arc::new(CoreDndSource {
            resource,
            metadata: DndSourceMetadata {
                mime_types: record.mime_types.clone(),
                actions,
            },
            cancelled: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        });
        self.active_dnd = Some(ActiveDnd {
            source_token: Some(token),
            origin,
            kind: DndGrabKind::Pointer,
            source,
            target: None,
        });
        Ok(())
    }

    fn begin_local_dnd(&mut self, origin: WlSurface) -> bool {
        if self.active_dnd.is_some() {
            return false;
        }
        let source: Arc<dyn DndSource> = Arc::new(LocalDndSource {
            origin: origin.clone(),
        });
        self.active_dnd = Some(ActiveDnd {
            source_token: None,
            origin: origin.id(),
            kind: DndGrabKind::Pointer,
            source,
            target: None,
        });
        true
    }

    pub(crate) fn dnd_active(&self) -> bool {
        self.active_dnd.is_some()
    }

    pub(crate) fn dnd_motion(
        &mut self,
        target: Option<(WlSurface, (f64, f64))>,
        serial: Serial,
        time: u32,
    ) {
        let changed = self.active_dnd.as_ref().is_some_and(|active| {
            active.target.as_ref().map(|target| target.surface.id())
                != target.as_ref().map(|(surface, _)| surface.id())
        });
        if changed {
            let old = self
                .active_dnd
                .as_mut()
                .and_then(|active| active.target.take());
            if let Some(old) = old
                && let Some(offer) = old.offer
            {
                offer.leave();
                offer.disable();
            }
            if let Some((surface, location)) = target.as_ref() {
                let devices = self.core_devices_for_surface(surface);
                let source = self
                    .active_dnd
                    .as_ref()
                    .map(|active| Arc::clone(&active.source));
                if let Some(source) = source {
                    let offer =
                        enter_offer(&self.display, surface, location, serial, source, devices);
                    if let Some(active) = self.active_dnd.as_mut() {
                        active.target = Some(DndTarget {
                            surface: surface.clone(),
                            offer,
                        });
                    }
                }
            }
        }
        if let Some((_, location)) = target
            && let Some(offer) = self
                .active_dnd
                .as_ref()
                .and_then(|active| active.target.as_ref())
                .and_then(|target| target.offer.as_ref())
        {
            offer.motion(time, location);
        }
    }

    pub(crate) fn drop_dnd(&mut self) {
        let Some(active) = self.active_dnd.take() else {
            return;
        };
        let validated = active
            .target
            .as_ref()
            .and_then(|target| target.offer.as_ref())
            .is_some_and(SurfaceDndOffer::drop);
        if validated {
            active.source.drop_performed();
        } else {
            active.source.cancel();
        }
    }

    pub(crate) fn cancel_dnd(&mut self) {
        let Some(active) = self.active_dnd.take() else {
            return;
        };
        if let Some(offer) = active.target.and_then(|target| target.offer) {
            offer.leave();
            offer.disable();
        }
        active.source.cancel();
    }

    fn dnd_for_surface(&self, surface: &ObjectId) -> Option<DndGrabKind> {
        self.active_dnd
            .as_ref()
            .filter(|dnd| &dnd.origin == surface)
            .map(|dnd| dnd.kind)
    }
}

fn enter_offer(
    display: &DisplayHandle,
    surface: &WlSurface,
    location: &(f64, f64),
    serial: Serial,
    source: Arc<dyn DndSource>,
    devices: Vec<WlDataDevice>,
) -> Option<SurfaceDndOffer> {
    if devices.is_empty() || !source.alive() {
        return None;
    }
    if source.local_for(surface) {
        for device in &devices {
            device.enter(serial.into(), surface, location.0, location.1, None);
        }
        return Some(offer::local(source, devices));
    }
    let metadata = source.metadata()?;
    let mime_types: Arc<[String]> = metadata.mime_types.into();
    let requires_action = devices.iter().any(|device| device.version() >= 3);
    let state = Arc::new(Mutex::new(DndOfferState {
        active: true,
        dropped: false,
        accepted: !requires_action,
        finished: false,
        requires_accept: requires_action,
        requires_action,
        source_actions: metadata.actions,
        chosen_action: WlDndAction::empty(),
    }));
    let target_source = source.resource();
    let reply_source = Arc::new(Mutex::new(Some(source)));
    let backend = display.backend_handle();
    let target_client = backend.get_client(surface.id()).ok()?;
    let mut offers = Vec::with_capacity(devices.len());
    for device in &devices {
        let object = backend
            .create_object::<RuntimeState>(
                target_client.clone(),
                WlDataOffer::interface(),
                device.version(),
                Arc::new(DndOfferObject {
                    state: Arc::clone(&state),
                    source: Arc::clone(&reply_source),
                    target_source: target_source.clone(),
                    mime_types: Arc::clone(&mime_types),
                }),
            )
            .ok()?;
        let offer = WlDataOffer::from_id(display, object).ok()?;
        device.data_offer(&offer);
        for mime_type in mime_types.iter() {
            offer.offer(mime_type.clone());
        }
        if offer.version() >= wl_data_offer::EVT_SOURCE_ACTIONS_SINCE {
            offer.source_actions(metadata.actions);
        }
        device.enter(serial.into(), surface, location.0, location.1, Some(&offer));
        offers.push(offer);
    }
    Some(SurfaceDndOffer {
        state,
        source: reply_source,
        devices,
        _offers: offers,
    })
}

impl RuntimeState {
    pub(super) fn start_selection_drag(
        &mut self,
        client: &ClientId,
        device: &WlDataDevice,
        source: Option<SourceToken>,
        origin: WlSurface,
        icon: Option<WlSurface>,
        serial: u32,
    ) {
        let serial = Serial::from(serial);
        let Some(start) = self.input_seat.pointer_grab_start() else {
            return;
        };
        if start.serial != serial
            || start.focus.as_ref().is_none_or(|focus| {
                !focus.id().same_client_as(&origin.id())
                    || !origin.id().same_client_as(&device.id())
            })
            || !assign_icon_role(device, icon.as_ref())
        {
            return;
        }
        let result = if let Some(token) = source {
            self.protocol_globals
                .selection
                .begin_core_dnd(client, token, origin.id())
        } else if self.protocol_globals.selection.begin_local_dnd(origin) {
            Ok(())
        } else {
            Err(SetSelectionError::UsedSource)
        };
        if let Err(error) = result {
            post_start_error(device, error);
            return;
        }
        #[cfg(feature = "tty")]
        self.install_dnd_icon(icon);
        #[cfg(not(feature = "tty"))]
        {
            self.dnd_icon = icon;
        }
        #[cfg(feature = "tty")]
        self.flush_queued_redraws();
    }

    pub(super) fn selection_source_destroyed(&mut self, token: SourceToken) {
        if self
            .protocol_globals
            .selection
            .source_destroyed(token)
            .is_some()
        {
            self.cancel_selection_dnd();
        }
    }

    pub(in crate::protocol) fn selection_surface_destroyed(&mut self, surface: &WlSurface) {
        #[cfg(feature = "tty")]
        if self.destroy_dnd_icon(surface) {
            self.flush_queued_redraws();
        }
        #[cfg(not(feature = "tty"))]
        if self.dnd_icon.as_ref() == Some(surface) {
            self.dnd_icon = None;
        }
        if self
            .protocol_globals
            .selection
            .dnd_for_surface(&surface.id())
            .is_some()
        {
            self.cancel_selection_dnd();
        }
    }

    pub(in crate::protocol) fn finish_selection_dnd(&mut self) {
        self.protocol_globals.selection.drop_dnd();
        #[cfg(feature = "tty")]
        self.retire_dnd_icon();
        #[cfg(not(feature = "tty"))]
        {
            self.dnd_icon = None;
        }
        #[cfg(feature = "tty")]
        self.flush_queued_redraws();
    }

    fn cancel_selection_dnd(&mut self) {
        self.protocol_globals.selection.cancel_dnd();
        #[cfg(feature = "tty")]
        self.retire_dnd_icon();
        #[cfg(not(feature = "tty"))]
        {
            self.dnd_icon = None;
        }
        #[cfg(feature = "tty")]
        self.flush_queued_redraws();
    }
}

fn assign_icon_role(device: &WlDataDevice, icon: Option<&WlSurface>) -> bool {
    let Some(icon) = icon else {
        return true;
    };
    if compositor::give_role(icon, DND_ICON_ROLE).is_ok() {
        true
    } else {
        device.post_error(
            wl_data_device::Error::Role,
            "drag icon surface already has another role",
        );
        false
    }
}

fn post_start_error(device: &WlDataDevice, error: SetSelectionError) {
    if error == SetSelectionError::UsedSource {
        device.post_error(
            wl_data_device::Error::UsedSource,
            "data source has already been used",
        );
    }
}
