//! Tensor-owned xdg-toplevel and xdg-popup handles.

mod queue;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicU32, Ordering},
};

use tensor_util::{Point, Rect, Size};
use wayland_protocols::xdg::shell::server::{
    xdg_popup::{self, XdgPopup},
    xdg_surface::{self, XdgSurface},
    xdg_toplevel::{self, XdgToplevel},
    xdg_wm_base::{self, XdgWmBase},
};
use wayland_server::{Resource, backend::ObjectId, protocol::wl_surface::WlSurface};

use super::PositionerState;
use queue::ConfigureQueue;

static NEXT_CONFIGURE_SERIAL: AtomicU32 = AtomicU32::new(1);

fn next_serial() -> u32 {
    NEXT_CONFIGURE_SERIAL
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |serial| {
            Some(serial.wrapping_add(1).max(1))
        })
        .expect("XDG configure serial update cannot fail")
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::protocol) struct ClientSurfaceState {
    pub(in crate::protocol) geometry: Option<Rect>,
    pub(in crate::protocol) min_size: Size,
    pub(in crate::protocol) max_size: Size,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ToplevelServerState {
    size: Option<Size>,
    bounds: Option<Size>,
    activated: bool,
}

#[derive(Clone, Copy, Debug)]
struct ToplevelConfigure {
    generation: u64,
}

#[derive(Debug)]
struct ToplevelState {
    pending_client: ClientSurfaceState,
    current_client: ClientSurfaceState,
    pending_server: Option<ToplevelServerState>,
    last_sent: Option<ToplevelServerState>,
    last_acked: Option<ToplevelConfigure>,
    configures: ConfigureQueue<ToplevelConfigure>,
    parent: Option<Toplevel>,
    title: Option<String>,
    app_id: Option<String>,
    generation: u64,
    mapped: bool,
    configure_ready: bool,
    initial_configure_sent: bool,
    capabilities_sent: bool,
}

impl Default for ToplevelState {
    fn default() -> Self {
        Self {
            pending_client: ClientSurfaceState::default(),
            current_client: ClientSurfaceState::default(),
            pending_server: Some(ToplevelServerState::default()),
            last_sent: None,
            last_acked: None,
            configures: ConfigureQueue::new(),
            parent: None,
            title: None,
            app_id: None,
            generation: 1,
            mapped: false,
            configure_ready: false,
            initial_configure_sent: false,
            capabilities_sent: false,
        }
    }
}

#[derive(Debug)]
struct ToplevelInner {
    wl_surface: WlSurface,
    xdg_surface: XdgSurface,
    resource: XdgToplevel,
    destroyed: Cell<bool>,
    state: RefCell<ToplevelState>,
}

#[derive(Clone, Debug)]
pub(crate) struct Toplevel(Rc<ToplevelInner>);

impl PartialEq for Toplevel {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Toplevel {}

impl Toplevel {
    pub(super) fn new(
        wl_surface: WlSurface,
        xdg_surface: XdgSurface,
        resource: XdgToplevel,
    ) -> Self {
        Self(Rc::new(ToplevelInner {
            wl_surface,
            xdg_surface,
            resource,
            destroyed: Cell::new(false),
            state: RefCell::new(ToplevelState::default()),
        }))
    }

    pub(in crate::protocol) fn protocol_id(&self) -> ObjectId {
        self.0.resource.id()
    }

    pub(in crate::protocol) fn xdg_toplevel(&self) -> &XdgToplevel {
        &self.0.resource
    }

    pub(in crate::protocol) fn wl_surface(&self) -> &WlSurface {
        &self.0.wl_surface
    }

    pub(in crate::protocol) fn alive(&self) -> bool {
        !self.0.destroyed.get() && self.0.wl_surface.is_alive() && self.0.resource.is_alive()
    }

    pub(in crate::protocol) fn request_close(&self) -> bool {
        if !self.alive() {
            return false;
        }
        self.0.resource.close();
        true
    }

    pub(super) fn mark_destroyed(&self) {
        self.0.destroyed.set(true);
    }

    pub(in crate::protocol) fn mapped(&self) -> bool {
        self.0.state.borrow().mapped
    }

    pub(in crate::protocol) fn geometry(&self) -> Option<Rect> {
        self.0.state.borrow().current_client.geometry
    }

    pub(in crate::protocol) fn constraints(&self) -> (Size, Size) {
        let state = self.0.state.borrow();
        (state.current_client.min_size, state.current_client.max_size)
    }

    pub(in crate::protocol) fn set_window_geometry(&self, geometry: Rect) {
        self.0.state.borrow_mut().pending_client.geometry = Some(geometry);
    }

    pub(in crate::protocol) fn set_min_size(&self, size: Size) {
        self.0.state.borrow_mut().pending_client.min_size = size;
    }

    pub(in crate::protocol) fn set_max_size(&self, size: Size) {
        self.0.state.borrow_mut().pending_client.max_size = size;
    }

    pub(in crate::protocol) fn set_activated(&self, activated: bool) -> bool {
        let mut role = self.0.state.borrow_mut();
        let current = role.pending_server.or(role.last_sent).unwrap_or_default();
        if current.activated == activated {
            return false;
        }
        role.pending_server = Some(ToplevelServerState {
            activated,
            ..current
        });
        true
    }

    pub(in crate::protocol) fn set_layout(&self, size: Size, bounds: Size) {
        let mut role = self.0.state.borrow_mut();
        let current = role.pending_server.or(role.last_sent).unwrap_or_default();
        role.pending_server = Some(ToplevelServerState {
            size: Some(size),
            bounds: Some(bounds),
            ..current
        });
    }

    pub(in crate::protocol) fn initial_configure_sent(&self) -> bool {
        self.0.state.borrow().initial_configure_sent
    }

    pub(in crate::protocol) fn send_pending_configure(&self) -> Option<u32> {
        self.prepare_configure(false).map(|emission| {
            self.send_configure_emission(emission);
            emission.serial
        })
    }

    pub(in crate::protocol) fn send_configure(&self) -> Option<u32> {
        self.prepare_configure(true).map(|emission| {
            self.send_configure_emission(emission);
            emission.serial
        })
    }

    fn prepare_configure(&self, force: bool) -> Option<ToplevelEmission> {
        let mut role = self.0.state.borrow_mut();
        if !role.configure_ready {
            return None;
        }
        let state = role
            .pending_server
            .take()
            .or(role.last_sent)
            .unwrap_or_default();
        if !force && role.initial_configure_sent && role.last_sent == Some(state) {
            return None;
        }
        let serial = next_serial();
        let generation = role.generation;
        let configure = ToplevelConfigure { generation };
        role.configures.push(serial, configure);
        let send_bounds = role
            .last_sent
            .is_none_or(|last| last.bounds != state.bounds);
        let send_capabilities = !role.capabilities_sent;
        role.initial_configure_sent = true;
        role.capabilities_sent = true;
        role.last_sent = Some(state);
        Some(ToplevelEmission {
            serial,
            state,
            send_bounds,
            send_capabilities,
        })
    }

    fn send_configure_emission(&self, emission: ToplevelEmission) {
        if emission.send_bounds
            && self.0.resource.version() >= xdg_toplevel::EVT_CONFIGURE_BOUNDS_SINCE
        {
            let bounds = emission.state.bounds.unwrap_or_default();
            self.0.resource.configure_bounds(
                protocol_dimension(bounds.width),
                protocol_dimension(bounds.height),
            );
        }
        if emission.send_capabilities
            && self.0.resource.version() >= xdg_toplevel::EVT_WM_CAPABILITIES_SINCE
        {
            self.0.resource.wm_capabilities(encode_u32s(&[
                xdg_toplevel::WmCapabilities::WindowMenu as u32,
                xdg_toplevel::WmCapabilities::Maximize as u32,
                xdg_toplevel::WmCapabilities::Fullscreen as u32,
                xdg_toplevel::WmCapabilities::Minimize as u32,
            ]));
        }
        let size = emission.state.size.unwrap_or_default();
        let states = emission
            .state
            .activated
            .then_some(xdg_toplevel::State::Activated as u32)
            .map_or_else(Vec::new, |state| encode_u32s(&[state]));
        self.0.resource.configure(
            protocol_dimension(size.width),
            protocol_dimension(size.height),
            states,
        );
        self.0.xdg_surface.configure(emission.serial);
    }

    pub(in crate::protocol) fn ack_configure(&self, serial: u32) -> bool {
        let mut role = self.0.state.borrow_mut();
        let Some(configure) = role.configures.ack(serial) else {
            return false;
        };
        if configure.generation == role.generation {
            role.last_acked = Some(configure);
        }
        true
    }

    pub(in crate::protocol) fn commit(&self, has_buffer: bool) -> ToplevelCommit {
        let mut role = self.0.state.borrow_mut();
        if has_buffer
            && role
                .last_acked
                .is_none_or(|configure| configure.generation != role.generation)
        {
            self.0.xdg_surface.post_error(
                xdg_surface::Error::UnconfiguredBuffer,
                "must acknowledge this mapping's initial configure before attaching a buffer",
            );
            return ToplevelCommit::Rejected;
        }
        if role.mapped && !has_buffer {
            let configures = std::mem::replace(&mut role.configures, ConfigureQueue::new());
            let generation = role.generation.wrapping_add(1);
            *role = ToplevelState::default();
            role.configures = configures;
            role.generation = generation;
            return ToplevelCommit::Unmapped;
        }
        role.current_client = role.pending_client;
        role.mapped = has_buffer;
        if !has_buffer {
            role.configure_ready = true;
        }
        ToplevelCommit::Applied
    }

    pub(in crate::protocol) fn parent(&self) -> Option<Toplevel> {
        self.0.state.borrow().parent.clone()
    }

    pub(in crate::protocol) fn set_parent(&self, parent: Option<Toplevel>) -> bool {
        if let Some(candidate) = &parent {
            if candidate.wl_surface() == self.wl_surface() {
                return false;
            }
            let mut ancestor = Some(candidate.clone());
            for _ in 0..256 {
                let Some(current) = ancestor else {
                    break;
                };
                if current.wl_surface() == self.wl_surface() {
                    return false;
                }
                ancestor = current.parent();
            }
            if ancestor.is_some() {
                return false;
            }
        }
        self.0.state.borrow_mut().parent = parent;
        true
    }

    pub(in crate::protocol) fn parent_surface(&self) -> Option<WlSurface> {
        self.parent().map(|parent| parent.wl_surface().clone())
    }

    pub(in crate::protocol) fn set_title(&self, title: String) -> bool {
        let mut role = self.0.state.borrow_mut();
        if role.title.as_ref() == Some(&title) {
            return false;
        }
        role.title = Some(title);
        true
    }

    pub(in crate::protocol) fn set_app_id(&self, app_id: String) -> bool {
        let mut role = self.0.state.borrow_mut();
        if role.app_id.as_ref() == Some(&app_id) {
            return false;
        }
        role.app_id = Some(app_id);
        true
    }

    pub(in crate::protocol) fn metadata(&self) -> (Option<String>, Option<String>) {
        let role = self.0.state.borrow();
        (role.title.clone(), role.app_id.clone())
    }
}

#[derive(Clone, Copy)]
struct ToplevelEmission {
    serial: u32,
    state: ToplevelServerState,
    send_bounds: bool,
    send_capabilities: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::protocol) enum ToplevelCommit {
    Applied,
    Unmapped,
    Rejected,
}

#[derive(Clone, Debug)]
pub(crate) enum PopupParent {
    Surface(WlSurface),
    Popup(Popup),
}

impl PopupParent {
    pub(in crate::protocol) fn wl_surface(&self) -> &WlSurface {
        match self {
            Self::Surface(surface) => surface,
            Self::Popup(popup) => popup.wl_surface(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PopupServerState {
    positioner: PositionerState,
    geometry: Rect,
}

#[derive(Clone, Copy, Debug)]
struct PopupConfigure {
    state: PopupServerState,
    generation: u64,
}

#[derive(Debug)]
struct PopupState {
    pending_client: ClientSurfaceState,
    current_client: ClientSurfaceState,
    pending_server: Option<PopupServerState>,
    last_sent: Option<PopupServerState>,
    last_acked: Option<PopupConfigure>,
    committed: Option<PopupServerState>,
    configures: ConfigureQueue<PopupConfigure>,
    parent: Option<PopupParent>,
    requested_grab: Option<(wayland_server::protocol::wl_seat::WlSeat, u32)>,
    generation: u64,
    mapped: bool,
    configure_ready: bool,
    initial_configure_sent: bool,
}

impl PopupState {
    fn new(positioner: PositionerState, parent: Option<PopupParent>) -> Self {
        Self {
            pending_client: ClientSurfaceState::default(),
            current_client: ClientSurfaceState::default(),
            pending_server: Some(PopupServerState {
                positioner,
                geometry: positioner.geometry(),
            }),
            last_sent: None,
            last_acked: None,
            committed: None,
            configures: ConfigureQueue::new(),
            parent,
            requested_grab: None,
            generation: 1,
            mapped: false,
            configure_ready: false,
            initial_configure_sent: false,
        }
    }
}

#[derive(Debug)]
struct PopupInner {
    wl_surface: WlSurface,
    xdg_surface: XdgSurface,
    resource: XdgPopup,
    wm_base: XdgWmBase,
    destroyed: Cell<bool>,
    state: RefCell<PopupState>,
}

#[derive(Clone, Debug)]
pub(crate) struct Popup(Rc<PopupInner>);

impl PartialEq for Popup {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Popup {}

impl Popup {
    pub(super) fn new(
        wl_surface: WlSurface,
        xdg_surface: XdgSurface,
        resource: XdgPopup,
        wm_base: XdgWmBase,
        positioner: PositionerState,
        parent: Option<PopupParent>,
    ) -> Self {
        Self(Rc::new(PopupInner {
            wl_surface,
            xdg_surface,
            resource,
            wm_base,
            destroyed: Cell::new(false),
            state: RefCell::new(PopupState::new(positioner, parent)),
        }))
    }

    pub(in crate::protocol) fn protocol_id(&self) -> ObjectId {
        self.0.resource.id()
    }

    pub(in crate::protocol) fn xdg_popup(&self) -> &XdgPopup {
        &self.0.resource
    }

    pub(in crate::protocol) fn wl_surface(&self) -> &WlSurface {
        &self.0.wl_surface
    }

    pub(in crate::protocol) fn alive(&self) -> bool {
        !self.0.destroyed.get() && self.0.wl_surface.is_alive() && self.0.resource.is_alive()
    }

    pub(super) fn mark_destroyed(&self) {
        self.0.destroyed.set(true);
    }

    pub(in crate::protocol) fn mapped(&self) -> bool {
        self.0.state.borrow().mapped
    }

    pub(in crate::protocol) fn parent(&self) -> Option<PopupParent> {
        self.0.state.borrow().parent.clone()
    }

    pub(in crate::protocol) fn parent_surface(&self) -> Option<WlSurface> {
        self.parent().map(|parent| parent.wl_surface().clone())
    }

    pub(in crate::protocol) fn set_parent_if_unset(&self, parent: WlSurface) -> bool {
        let mut role = self.0.state.borrow_mut();
        if role.parent.is_some() || role.initial_configure_sent {
            return false;
        }
        role.parent = Some(PopupParent::Surface(parent));
        true
    }

    pub(in crate::protocol) fn window_geometry(&self) -> Rect {
        self.0
            .state
            .borrow()
            .current_client
            .geometry
            .unwrap_or_default()
    }

    pub(in crate::protocol) fn placement(&self) -> Rect {
        self.0
            .state
            .borrow()
            .committed
            .map(|state| state.geometry)
            .unwrap_or_default()
    }

    pub(in crate::protocol) fn set_window_geometry(&self, geometry: Rect) {
        self.0.state.borrow_mut().pending_client.geometry = Some(geometry);
    }

    pub(in crate::protocol) fn update_positioner(&self, positioner: PositionerState) {
        self.0.state.borrow_mut().pending_server = Some(PopupServerState {
            positioner,
            geometry: positioner.geometry(),
        });
    }

    pub(in crate::protocol) fn constrain(&self, target: Rect) {
        let mut role = self.0.state.borrow_mut();
        let mut pending = role
            .pending_server
            .or(role.last_sent)
            .unwrap_or(PopupServerState {
                positioner: PositionerState::default(),
                geometry: Rect::default(),
            });
        pending.geometry = pending.positioner.constrained_geometry(target);
        role.pending_server = Some(pending);
    }

    pub(in crate::protocol) fn initial_configure_sent(&self) -> bool {
        self.0.state.borrow().initial_configure_sent
    }

    pub(in crate::protocol) fn send_configure(&self) -> Option<u32> {
        self.send_configure_internal(None, false)
    }

    pub(in crate::protocol) fn send_repositioned(&self, token: u32) -> Option<u32> {
        self.send_configure_internal(Some(token), true)
    }

    fn send_configure_internal(&self, token: Option<u32>, force: bool) -> Option<u32> {
        let emission = {
            let mut role = self.0.state.borrow_mut();
            if !role.configure_ready {
                return None;
            }
            if role.initial_configure_sent && token.is_none() {
                if self.0.resource.version() < xdg_popup::EVT_REPOSITIONED_SINCE {
                    return None;
                }
                let reactive = role
                    .committed
                    .or(role.last_acked.map(|configure| configure.state))
                    .is_some_and(|state| state.positioner.reactive);
                if !reactive {
                    return None;
                }
            }
            let state = role
                .pending_server
                .take()
                .or(role.last_sent)
                .unwrap_or(PopupServerState {
                    positioner: PositionerState::default(),
                    geometry: Rect::default(),
                });
            if !force && role.initial_configure_sent && role.last_sent == Some(state) {
                return None;
            }
            let serial = next_serial();
            let configure = PopupConfigure {
                state,
                generation: role.generation,
            };
            role.configures.push(serial, configure);
            role.initial_configure_sent = true;
            role.last_sent = Some(state);
            PopupEmission {
                serial,
                geometry: state.geometry,
                token,
            }
        };
        if let Some(token) = emission.token {
            self.0.resource.repositioned(token);
        }
        self.0.resource.configure(
            emission.geometry.x,
            emission.geometry.y,
            protocol_dimension(emission.geometry.width),
            protocol_dimension(emission.geometry.height),
        );
        self.0.xdg_surface.configure(emission.serial);
        Some(emission.serial)
    }

    pub(in crate::protocol) fn ack_configure(&self, serial: u32) -> bool {
        let mut role = self.0.state.borrow_mut();
        let Some(configure) = role.configures.ack(serial) else {
            return false;
        };
        if configure.generation == role.generation {
            role.last_acked = Some(configure);
        }
        true
    }

    pub(in crate::protocol) fn request_grab(
        &self,
        seat: wayland_server::protocol::wl_seat::WlSeat,
        serial: u32,
    ) {
        self.0.state.borrow_mut().requested_grab = Some((seat, serial));
    }

    pub(in crate::protocol) fn commit(&self, has_buffer: bool) -> PopupCommit {
        let mut role = self.0.state.borrow_mut();
        if role.parent.is_none() {
            self.0.xdg_surface.post_error(
                xdg_surface::Error::NotConstructed,
                "xdg_popup must have a parent before commit",
            );
            return PopupCommit::Rejected;
        }
        if role.requested_grab.is_some() && role.mapped {
            self.0.resource.post_error(
                xdg_popup::Error::InvalidGrab,
                "xdg_popup.grab must be requested before the popup is mapped",
            );
            role.requested_grab = None;
            return PopupCommit::Rejected;
        }
        if has_buffer
            && role
                .last_acked
                .is_none_or(|configure| configure.generation != role.generation)
        {
            self.0.xdg_surface.post_error(
                xdg_surface::Error::UnconfiguredBuffer,
                "must acknowledge this mapping's initial configure before attaching a buffer",
            );
            return PopupCommit::Rejected;
        }
        if role.mapped && !has_buffer {
            let configures = std::mem::replace(&mut role.configures, ConfigureQueue::new());
            let generation = role.generation.wrapping_add(1);
            *role = PopupState::new(PositionerState::default(), None);
            role.configures = configures;
            role.generation = generation;
            return PopupCommit::Unmapped;
        }
        role.current_client = role.pending_client;
        role.mapped = has_buffer;
        if has_buffer {
            role.committed = role.last_acked.map(|configure| configure.state);
        } else {
            role.configure_ready = true;
        }
        PopupCommit::Applied(role.requested_grab.take())
    }

    pub(in crate::protocol) fn send_popup_done(&self) {
        if self.0.resource.is_alive() {
            self.0.resource.popup_done();
        }
    }

    pub(in crate::protocol) fn post_not_topmost(&self) {
        self.0.wm_base.post_error(
            xdg_wm_base::Error::NotTheTopmostPopup,
            "an xdg_popup was destroyed while it still had a live child popup",
        );
    }

    pub(in crate::protocol) fn root_surface(&self) -> Option<WlSurface> {
        let mut parent = self.parent()?;
        for _ in 0..256 {
            match parent {
                PopupParent::Surface(surface) => return Some(surface),
                PopupParent::Popup(popup) => parent = popup.parent()?,
            }
        }
        None
    }

    pub(in crate::protocol) fn toplevel_coords(&self) -> Point {
        let mut offset = Point::default();
        let Some(mut parent) = self.parent() else {
            return offset;
        };
        for _ in 0..256 {
            match parent {
                PopupParent::Surface(_) => break,
                PopupParent::Popup(popup) => {
                    let location = popup.placement();
                    offset.x = offset.x.saturating_add(location.x);
                    offset.y = offset.y.saturating_add(location.y);
                    let Some(next) = popup.parent() else {
                        break;
                    };
                    parent = next;
                }
            }
        }
        offset
    }

    pub(super) fn wm_base(&self) -> &XdgWmBase {
        &self.0.wm_base
    }
}

#[derive(Clone, Copy)]
struct PopupEmission {
    serial: u32,
    geometry: Rect,
    token: Option<u32>,
}

#[derive(Debug)]
pub(in crate::protocol) enum PopupCommit {
    Applied(Option<(wayland_server::protocol::wl_seat::WlSeat, u32)>),
    Unmapped,
    Rejected,
}

fn encode_u32s(values: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(values));
    for value in values {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }
    bytes
}

fn protocol_dimension(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}
