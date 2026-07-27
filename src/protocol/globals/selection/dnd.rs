//! Tensor-owned core drag-and-drop source and offer state.

mod offer;

use std::{
    any::Any,
    collections::HashMap,
    os::fd::{AsFd, OwnedFd},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use smithay::{
    input::{
        Seat,
        dnd::{DnDGrab, DndAction, DndFocus, OfferData, Source, SourceMetadata},
        pointer::Focus,
    },
    utils::{IsAlive, Logical, Point, SERIAL_COUNTER, Serial},
    wayland::compositor,
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
use crate::protocol::{focus::SurfaceFocusTarget, state::RuntimeState};

const DND_ICON_ROLE: &str = "wl_data_device.dnd_icon";
const ACTION_NEGOTIATION_VERSION: u32 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DndGrabKind {
    Pointer,
    Touch,
}

pub(super) struct ActiveDnd {
    pub(super) source: Option<SourceToken>,
    pub(super) origin: ObjectId,
    pub(super) kind: DndGrabKind,
}

struct CoreDndSource {
    resource: WlDataSource,
    metadata: SourceMetadata,
    cancelled: AtomicBool,
    finished: AtomicBool,
}

impl IsAlive for CoreDndSource {
    fn alive(&self) -> bool {
        self.resource.is_alive() && !self.cancelled.load(Ordering::Acquire)
    }
}

impl Source for CoreDndSource {
    fn metadata(&self) -> Option<SourceMetadata> {
        self.alive().then(|| self.metadata.clone())
    }

    fn choose_action(&self, action: DndAction) {
        if self.resource.version() >= wl_data_source::EVT_ACTION_SINCE && self.resource.is_alive() {
            self.resource.action(to_wire_action(action));
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

impl IsAlive for LocalDndSource {
    fn alive(&self) -> bool {
        self.origin.is_alive()
    }
}

impl Source for LocalDndSource {
    fn is_client_local(&self, target: &dyn Any) -> bool {
        target
            .downcast_ref::<SurfaceFocusTarget>()
            .is_some_and(|target| target.id().same_client_as(&self.origin.id()))
    }

    fn metadata(&self) -> Option<SourceMetadata> {
        None
    }

    fn choose_action(&self, _action: DndAction) {}

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

struct DndOfferObject<S: Source> {
    state: Arc<Mutex<DndOfferState>>,
    source: Arc<Mutex<Option<Arc<S>>>>,
    target_source: Option<Weak<WlDataSource>>,
    mime_types: Arc<[String]>,
}

impl<S: Source> ObjectData<RuntimeState> for DndOfferObject<S> {
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

pub(crate) struct SurfaceDndOffer<S: Source> {
    state: Arc<Mutex<DndOfferState>>,
    source: Arc<Mutex<Option<Arc<S>>>>,
    devices: Vec<WlDataDevice>,
    _offers: Vec<WlDataOffer>,
}

impl<S: Source> OfferData for SurfaceDndOffer<S> {
    fn disable(&self) {
        let mut state = self.state.lock().unwrap();
        state.active = false;
        state.accepted = false;
        state.chosen_action = WlDndAction::empty();
        if let Some(source) = self.source.lock().unwrap().take() {
            source.choose_action(DndAction::None);
        }
    }

    fn drop(&self) {
        self.state.lock().unwrap().dropped = true;
    }

    fn validated(&self) -> bool {
        self.state.lock().unwrap().validated()
    }
}

impl DndFocus<RuntimeState> for SurfaceFocusTarget {
    type OfferData<S: Source> = SurfaceDndOffer<S>;

    fn enter<S: Source>(
        &self,
        state: &mut RuntimeState,
        display: &DisplayHandle,
        source: Arc<S>,
        _seat: &Seat<RuntimeState>,
        location: Point<f64, Logical>,
        serial: &Serial,
    ) -> Option<Self::OfferData<S>> {
        let devices = state
            .protocol_globals
            .selection
            .core_devices_for_surface(self.surface());
        if devices.is_empty() {
            return None;
        }

        if source.is_client_local(self) {
            for device in &devices {
                device.enter(
                    (*serial).into(),
                    self.surface(),
                    location.x,
                    location.y,
                    None,
                );
            }
            return Some(offer::local(source, devices));
        }

        let metadata = source.metadata()?;
        let mime_types: Arc<[String]> = metadata.mime_types.into();
        let source_actions = actions_to_wire(&metadata.dnd_actions);
        let requires_accept = devices.iter().any(|device| device.version() >= 3);
        let requires_action = requires_accept;
        let offer_state = Arc::new(Mutex::new(DndOfferState {
            active: true,
            dropped: false,
            accepted: !requires_accept,
            finished: false,
            requires_accept,
            requires_action,
            source_actions,
            chosen_action: WlDndAction::empty(),
        }));
        let target_source = (source.as_ref() as &dyn Any)
            .downcast_ref::<CoreDndSource>()
            .map(|source| source.resource.downgrade());
        let reply_source = Arc::new(Mutex::new(Some(source)));
        let mut offers = Vec::with_capacity(devices.len());
        let backend = display.backend_handle();
        let target_client = backend.get_client(self.id()).ok()?;

        for device in &devices {
            let object = backend
                .create_object::<RuntimeState>(
                    target_client.clone(),
                    WlDataOffer::interface(),
                    device.version(),
                    Arc::new(DndOfferObject {
                        state: Arc::clone(&offer_state),
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
                offer.source_actions(source_actions);
            }
            device.enter(
                (*serial).into(),
                self.surface(),
                location.x,
                location.y,
                Some(&offer),
            );
            offers.push(offer);
        }

        Some(SurfaceDndOffer {
            state: offer_state,
            source: reply_source,
            devices,
            _offers: offers,
        })
    }

    fn motion<S: Source>(
        &self,
        _state: &mut RuntimeState,
        offer: Option<&mut Self::OfferData<S>>,
        _seat: &Seat<RuntimeState>,
        location: Point<f64, Logical>,
        time: u32,
    ) {
        let Some(offer) = offer else {
            return;
        };
        for device in offer.devices.iter().filter(|device| device.is_alive()) {
            device.motion(time, location.x, location.y);
        }
    }

    fn leave<S: Source>(
        &self,
        _state: &mut RuntimeState,
        offer: Option<&mut Self::OfferData<S>>,
        _seat: &Seat<RuntimeState>,
    ) {
        let Some(offer) = offer else {
            return;
        };
        for device in offer.devices.iter().filter(|device| device.is_alive()) {
            device.leave();
        }
    }

    fn drop<S: Source>(
        &self,
        _state: &mut RuntimeState,
        offer: Option<&mut Self::OfferData<S>>,
        _seat: &Seat<RuntimeState>,
    ) {
        let Some(offer) = offer else {
            return;
        };
        let state = offer.state.lock().unwrap();
        for device in offer.devices.iter().filter(|device| device.is_alive()) {
            if device.version() < 3 || !state.chosen_action.is_empty() {
                device.drop();
            }
        }
    }
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
        kind: DndGrabKind,
    ) -> Result<CoreDndSource, SetSelectionError> {
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
        let mut metadata = SourceMetadata {
            mime_types: record.mime_types.clone(),
            ..SourceMetadata::default()
        };
        if resource.version() < ACTION_NEGOTIATION_VERSION {
            metadata.dnd_actions.push(DndAction::Copy);
        } else {
            for action in [DndAction::Copy, DndAction::Move, DndAction::Ask] {
                if record.core_actions.contains(to_wire_action(action)) {
                    metadata.dnd_actions.push(action);
                }
            }
        }
        self.active_dnd = Some(ActiveDnd {
            source: Some(token),
            origin,
            kind,
        });
        Ok(CoreDndSource {
            resource,
            metadata,
            cancelled: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        })
    }

    fn begin_local_dnd(&mut self, origin: ObjectId, kind: DndGrabKind) -> bool {
        if self.active_dnd.is_some() {
            return false;
        }
        self.active_dnd = Some(ActiveDnd {
            source: None,
            origin,
            kind,
        });
        true
    }

    fn finish_dnd(&mut self) {
        self.active_dnd = None;
    }

    fn dnd_for_surface(&self, surface: &ObjectId) -> Option<DndGrabKind> {
        self.active_dnd
            .as_ref()
            .filter(|dnd| &dnd.origin == surface)
            .map(|dnd| dnd.kind)
    }
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
        if let Some(pointer) = self.seat.get_pointer()
            && pointer.has_grab(serial)
            && pointer.grab_start_data().is_some_and(|start| {
                start.focus.is_some_and(|(focus, _)| {
                    focus.id().same_client_as(&origin.id())
                        && origin.id().same_client_as(&device.id())
                })
            })
        {
            let Some(start) = pointer.grab_start_data() else {
                return;
            };
            if !assign_icon_role(device, icon.as_ref()) {
                return;
            }
            let origin_id = origin.id();
            if let Some(token) = source {
                match self.protocol_globals.selection.begin_core_dnd(
                    client,
                    token,
                    origin_id,
                    DndGrabKind::Pointer,
                ) {
                    Ok(source) => {
                        self.dnd_icon = icon;
                        pointer.set_grab(
                            self,
                            DnDGrab::new_pointer(
                                &self.display_handle,
                                start,
                                source,
                                self.seat.clone(),
                            ),
                            serial,
                            Focus::Keep,
                        );
                    }
                    Err(error) => {
                        post_start_error(device, error);
                        return;
                    }
                }
            } else {
                if !self
                    .protocol_globals
                    .selection
                    .begin_local_dnd(origin_id, DndGrabKind::Pointer)
                {
                    return;
                }
                self.dnd_icon = icon;
                pointer.set_grab(
                    self,
                    DnDGrab::new_pointer(
                        &self.display_handle,
                        start,
                        LocalDndSource { origin },
                        self.seat.clone(),
                    ),
                    serial,
                    Focus::Keep,
                );
            }
            #[cfg(feature = "tty")]
            self.request_redraw_at(pointer.current_location());
            return;
        }

        if let Some(touch) = self.seat.get_touch()
            && touch.has_grab(serial)
            && touch.grab_start_data().is_some_and(|start| {
                start.focus.is_some_and(|(focus, _)| {
                    focus.id().same_client_as(&origin.id())
                        && origin.id().same_client_as(&device.id())
                })
            })
        {
            let Some(start) = touch.grab_start_data() else {
                return;
            };
            if !assign_icon_role(device, icon.as_ref()) {
                return;
            }
            let origin_id = origin.id();
            let started = if let Some(token) = source {
                match self.protocol_globals.selection.begin_core_dnd(
                    client,
                    token,
                    origin_id,
                    DndGrabKind::Touch,
                ) {
                    Ok(source) => {
                        touch.set_grab(
                            self,
                            DnDGrab::new_touch(
                                &self.display_handle,
                                start,
                                source,
                                self.seat.clone(),
                            ),
                            serial,
                        );
                        true
                    }
                    Err(error) => {
                        post_start_error(device, error);
                        false
                    }
                }
            } else if self
                .protocol_globals
                .selection
                .begin_local_dnd(origin_id, DndGrabKind::Touch)
            {
                touch.set_grab(
                    self,
                    DnDGrab::new_touch(
                        &self.display_handle,
                        start,
                        LocalDndSource { origin },
                        self.seat.clone(),
                    ),
                    serial,
                );
                true
            } else {
                false
            };
            if started {
                self.dnd_icon = icon;
                #[cfg(feature = "tty")]
                self.request_redraw_workspace();
            }
        }
    }

    pub(super) fn selection_source_destroyed(&mut self, token: SourceToken) {
        let cancel = self.protocol_globals.selection.source_destroyed(token);
        if let Some(kind) = cancel {
            self.cancel_selection_dnd(kind);
        }
    }

    pub(in crate::protocol) fn selection_surface_destroyed(&mut self, surface: &WlSurface) {
        if self.dnd_icon.as_ref() == Some(surface) {
            self.dnd_icon = None;
        }
        if let Some(kind) = self
            .protocol_globals
            .selection
            .dnd_for_surface(&surface.id())
        {
            self.cancel_selection_dnd(kind);
        }
    }

    pub(in crate::protocol) fn finish_selection_dnd(&mut self) {
        self.protocol_globals.selection.finish_dnd();
        self.dnd_icon = None;
        #[cfg(feature = "tty")]
        self.request_redraw_workspace();
    }

    fn cancel_selection_dnd(&mut self, kind: DndGrabKind) {
        match kind {
            DndGrabKind::Pointer => {
                if let Some(pointer) = self.seat.get_pointer() {
                    pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), 0);
                } else {
                    self.finish_selection_dnd();
                }
            }
            DndGrabKind::Touch => {
                if let Some(touch) = self.seat.get_touch() {
                    touch.unset_grab(self);
                } else {
                    self.finish_selection_dnd();
                }
            }
        }
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

fn actions_to_wire(actions: &[DndAction]) -> WlDndAction {
    actions
        .iter()
        .copied()
        .fold(WlDndAction::empty(), |mask, action| {
            mask | to_wire_action(action)
        })
}

fn to_wire_action(action: DndAction) -> WlDndAction {
    match action {
        DndAction::None => WlDndAction::empty(),
        DndAction::Copy => WlDndAction::Copy,
        DndAction::Move => WlDndAction::Move,
        DndAction::Ask => WlDndAction::Ask,
    }
}
