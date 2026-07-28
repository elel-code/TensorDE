//! Rootless X11 window manager driven by completed X11 socket operations.

mod surface;

use std::{
    collections::{HashMap, VecDeque},
    os::fd::{AsFd, BorrowedFd},
    os::unix::net::UnixStream,
    sync::Arc,
};

const MAX_PENDING_XWM_EVENTS: usize = 64;

use tensor_runtime::WorkerTx;
use tensor_util::LogicalRect;
use tracing::{debug, trace};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};
use x11rb::{
    COPY_FROM_PARENT, CURRENT_TIME,
    connection::Connection,
    protocol::{
        Event,
        composite::{ConnectionExt as _, Redirect},
        xproto::{
            AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt as _,
            CreateNotifyEvent, EventMask, PropMode, Screen, StackMode, WindowClass,
        },
    },
    rust_connection::{DefaultStream, RustConnection},
    wrapper::ConnectionExt as _,
};

pub(crate) use surface::X11Surface;

use super::{X11PropertyRequest, X11PropertyResult, X11PropertyUpdate};

x11rb::atom_manager! {
    pub(crate) Atoms: AtomsCookie {
        WM_S0,
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
        WM_TAKE_FOCUS,
        WM_STATE,
        WL_SURFACE_SERIAL,
        _NET_SUPPORTED,
        _NET_SUPPORTING_WM_CHECK,
        _NET_WM_NAME,
        UTF8_STRING,
        _NET_WM_CM_S0,
        _NET_ACTIVE_WINDOW,
        _NET_CLIENT_LIST,
        _NET_CLIENT_LIST_STACKING,
        _NET_WM_STATE,
        _NET_WM_STATE_FOCUSED,
        _NET_WM_MOVERESIZE,
        _NET_CLOSE_WINDOW,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WmWindowProperty {
    NormalHints,
    TransientFor,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum PropertyQuery {
    TransientFor,
    NormalHints,
    NetState,
}

impl PropertyQuery {
    const fn bit(self) -> u8 {
        match self {
            Self::TransientFor => 1 << 0,
            Self::NormalHints => 1 << 1,
            Self::NetState => 1 << 2,
        }
    }
}

enum PropertyDisposition {
    Discard,
    Resubmitted,
    Apply,
}

#[derive(Clone, Debug)]
pub(crate) enum XwmEvent {
    NewWindow(X11Surface),
    MapRequested(X11Surface),
    Mapped(X11Surface),
    Unmapped(X11Surface),
    Destroyed(X11Surface),
    ConfigureRequested {
        window: X11Surface,
        width: Option<u32>,
        height: Option<u32>,
    },
    Configured {
        window: X11Surface,
        above: Option<u32>,
    },
    PropertyChanged(X11Surface, WmWindowProperty),
    SurfaceSerial {
        window: u32,
        serial: u64,
    },
    FocusRequested(X11Surface),
    ReflowRequested,
}

#[derive(Debug)]
pub(crate) struct X11Wm {
    connection: Arc<RustConnection>,
    screen: Screen,
    atoms: Atoms,
    wm_window: u32,
    windows: HashMap<u32, X11Surface>,
    unpaired_windows: HashMap<u64, u32>,
    stacking: Vec<u32>,
    pending_events: VecDeque<XwmEvent>,
    property_requests: WorkerTx<X11PropertyRequest>,
    next_window_generation: u64,
}

impl X11Wm {
    pub(crate) fn start(
        socket: UnixStream,
        property_requests: WorkerTx<X11PropertyRequest>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let stream = DefaultStream::from_unix_stream(socket)?.0;
        let connection = RustConnection::connect_to_stream(stream, 0)?;
        let atoms = Atoms::new(&connection)?.reply()?;
        let screen = connection.setup().roots[0].clone();

        connection.change_window_attributes(
            screen.root,
            &ChangeWindowAttributesAux::new().event_mask(
                EventMask::SUBSTRUCTURE_REDIRECT
                    | EventMask::SUBSTRUCTURE_NOTIFY
                    | EventMask::PROPERTY_CHANGE
                    | EventMask::FOCUS_CHANGE,
            ),
        )?;
        let wm_window = connection.generate_id()?;
        connection.create_window(
            screen.root_depth,
            wm_window,
            screen.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            COPY_FROM_PARENT,
            &Default::default(),
        )?;
        connection.set_selection_owner(wm_window, atoms.WM_S0, CURRENT_TIME)?;
        connection.set_selection_owner(wm_window, atoms._NET_WM_CM_S0, CURRENT_TIME)?;
        connection.composite_redirect_subwindows(screen.root, Redirect::MANUAL)?;
        connection.change_property32(
            PropMode::REPLACE,
            screen.root,
            atoms._NET_SUPPORTED,
            AtomEnum::ATOM,
            &[
                atoms._NET_ACTIVE_WINDOW,
                atoms._NET_CLIENT_LIST,
                atoms._NET_CLIENT_LIST_STACKING,
                atoms._NET_WM_STATE,
                atoms._NET_WM_STATE_FOCUSED,
                atoms._NET_WM_MOVERESIZE,
                atoms._NET_CLOSE_WINDOW,
            ],
        )?;
        for target in [screen.root, wm_window] {
            connection.change_property32(
                PropMode::REPLACE,
                target,
                atoms._NET_SUPPORTING_WM_CHECK,
                AtomEnum::WINDOW,
                &[wm_window],
            )?;
        }
        connection.change_property8(
            PropMode::REPLACE,
            wm_window,
            atoms._NET_WM_NAME,
            atoms.UTF8_STRING,
            b"Tensor XWM",
        )?;
        connection.change_property32(
            PropMode::REPLACE,
            screen.root,
            atoms._NET_CLIENT_LIST,
            AtomEnum::WINDOW,
            &[],
        )?;
        connection.change_property32(
            PropMode::REPLACE,
            screen.root,
            atoms._NET_CLIENT_LIST_STACKING,
            AtomEnum::WINDOW,
            &[],
        )?;
        connection.flush()?;

        Ok(Self {
            connection: Arc::new(connection),
            screen,
            atoms,
            wm_window,
            windows: HashMap::new(),
            unpaired_windows: HashMap::new(),
            stacking: Vec::new(),
            pending_events: VecDeque::with_capacity(MAX_PENDING_XWM_EVENTS),
            property_requests,
            next_window_generation: 1,
        })
    }

    pub(crate) fn completion_fd(&self) -> BorrowedFd<'_> {
        self.connection.stream().as_fd()
    }

    pub(crate) fn drain_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        debug_assert!(self.pending_events.is_empty());
        while let Some(event) = self.connection.poll_for_event()? {
            trace!(?event, "completed X11 event");
            self.handle_event(event)?;
        }
        self.connection.flush()?;
        Ok(())
    }

    pub(crate) fn next_event(&mut self) -> Option<XwmEvent> {
        self.pending_events.pop_front()
    }

    pub(crate) fn apply_property_result(
        &mut self,
        result: X11PropertyResult,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(window) = self.windows.get(&result.target.window).cloned() else {
            trace!(
                window = result.target.window,
                "discarded property completion for a destroyed X11 window"
            );
            return Ok(());
        };
        if !result.targets(window.property_target()) {
            trace!(
                window = result.target.window,
                generation = result.target.generation,
                "discarded property completion for a reused X11 id"
            );
            return Ok(());
        };
        match result.update {
            X11PropertyUpdate::Initial {
                transient_for,
                size_hints,
                net_state,
            } => {
                if window.apply_initial_properties(transient_for, size_hints, net_state) {
                    self.push_event(XwmEvent::MapRequested(window))?;
                }
            }
            X11PropertyUpdate::TransientFor(transient_for) => {
                if !matches!(
                    self.property_disposition(&window, PropertyQuery::TransientFor)?,
                    PropertyDisposition::Apply
                ) {
                    return Ok(());
                }
                window.apply_transient_for(transient_for);
                self.push_event(XwmEvent::PropertyChanged(
                    window.clone(),
                    WmWindowProperty::TransientFor,
                ))?;
                self.publish_completed_map_request(window)?;
            }
            X11PropertyUpdate::NormalHints(size_hints) => {
                if !matches!(
                    self.property_disposition(&window, PropertyQuery::NormalHints)?,
                    PropertyDisposition::Apply
                ) {
                    return Ok(());
                }
                window.apply_size_hints(size_hints);
                self.push_event(XwmEvent::PropertyChanged(
                    window.clone(),
                    WmWindowProperty::NormalHints,
                ))?;
                self.publish_completed_map_request(window)?;
            }
            X11PropertyUpdate::NetState(net_state) => {
                if !matches!(
                    self.property_disposition(&window, PropertyQuery::NetState)?,
                    PropertyDisposition::Apply
                ) {
                    return Ok(());
                }
                window.apply_net_state(net_state);
                self.publish_completed_map_request(window)?;
            }
        }
        Ok(())
    }

    pub(crate) fn window(&self, id: u32) -> Option<X11Surface> {
        self.windows.get(&id).cloned()
    }

    pub(crate) fn window_for_wayland_surface(&self, surface: &WlSurface) -> Option<X11Surface> {
        let id = surface.id();
        self.windows
            .values()
            .find(|window| {
                window
                    .wl_surface()
                    .is_some_and(|surface| surface.id() == id)
            })
            .cloned()
    }

    pub(crate) fn remember_unpaired_window(&mut self, serial: u64, window: u32) {
        self.unpaired_windows.insert(serial, window);
    }

    pub(crate) fn take_unpaired_window(&mut self, serial: u64) -> Option<u32> {
        self.unpaired_windows.remove(&serial)
    }

    pub(crate) fn raise_window(
        &mut self,
        window: &X11Surface,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if window.is_override_redirect() {
            return Ok(());
        }
        let id = window.window_id();
        if self.stacking.last() == Some(&id) {
            return Ok(());
        }
        self.connection
            .configure_window(id, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        self.stacking.retain(|candidate| *candidate != id);
        self.stacking.push(id);
        self.publish_client_lists()?;
        self.connection.flush()?;
        Ok(())
    }

    fn handle_event(&mut self, event: Event) -> Result<(), Box<dyn std::error::Error>> {
        match event {
            Event::CreateNotify(event) if event.window != self.wm_window => {
                let window = self.create_window(event)?;
                self.push_event(XwmEvent::NewWindow(window))?;
            }
            Event::MapRequest(event) => {
                let window = self.require_window(event.window)?;
                if !window.begin_map_request() {
                    return Ok(());
                }
                if let Err(error) = self.submit_property_request(X11PropertyRequest::Initial {
                    target: window.property_target(),
                    net_wm_state: self.atoms._NET_WM_STATE,
                }) {
                    window.cancel_map_request();
                    return Err(error);
                }
            }
            Event::MapNotify(event) => {
                if let Some(window) = self.windows.get(&event.window).cloned() {
                    window.mark_mapped(true);
                    if !window.is_override_redirect() {
                        self.stacking.retain(|candidate| *candidate != event.window);
                        self.stacking.push(event.window);
                    }
                    self.publish_client_lists()?;
                    self.push_event(XwmEvent::Mapped(window))?;
                }
            }
            Event::UnmapNotify(event) => {
                if let Some(window) = self.windows.get(&event.window).cloned() {
                    window.mark_mapped(false);
                    self.stacking.retain(|candidate| *candidate != event.window);
                    self.publish_client_lists()?;
                    self.push_event(XwmEvent::Unmapped(window))?;
                }
            }
            Event::DestroyNotify(event) => {
                if let Some(window) = self.windows.remove(&event.window) {
                    window.mark_destroyed();
                    self.stacking.retain(|candidate| *candidate != event.window);
                    self.unpaired_windows
                        .retain(|_, candidate| *candidate != event.window);
                    self.publish_client_lists()?;
                    self.push_event(XwmEvent::Destroyed(window))?;
                }
            }
            Event::ConfigureRequest(event) => {
                let window = self.require_window(event.window)?;
                if window.is_override_redirect() {
                    let geometry = LogicalRect::new(
                        (i32::from(event.x), i32::from(event.y)).into(),
                        (i32::from(event.width), i32::from(event.height)).into(),
                    );
                    window.configure(Some(geometry))?;
                } else {
                    self.push_event(XwmEvent::ConfigureRequested {
                        window,
                        width: Some(u32::from(event.width)),
                        height: Some(u32::from(event.height)),
                    })?;
                }
            }
            Event::ConfigureNotify(event) => {
                if let Some(window) = self.windows.get(&event.window).cloned() {
                    let geometry = LogicalRect::new(
                        (i32::from(event.x), i32::from(event.y)).into(),
                        (i32::from(event.width), i32::from(event.height)).into(),
                    );
                    window.update_geometry(geometry);
                    self.push_event(XwmEvent::Configured {
                        window,
                        above: (event.above_sibling != 0).then_some(event.above_sibling),
                    })?;
                }
            }
            Event::PropertyNotify(event) => {
                if let Some(window) = self.windows.get(&event.window).cloned() {
                    let query = if event.atom == u32::from(AtomEnum::WM_TRANSIENT_FOR) {
                        Some(PropertyQuery::TransientFor)
                    } else if event.atom == u32::from(AtomEnum::WM_NORMAL_HINTS) {
                        Some(PropertyQuery::NormalHints)
                    } else if event.atom == self.atoms._NET_WM_STATE {
                        Some(PropertyQuery::NetState)
                    } else {
                        None
                    };
                    if let Some(query) = query {
                        self.schedule_property_query(&window, query)?;
                    }
                }
            }
            Event::ClientMessage(event) if event.type_ == self.atoms.WL_SURFACE_SERIAL => {
                let data = event.data.as_data32();
                self.push_event(XwmEvent::SurfaceSerial {
                    window: event.window,
                    serial: u64::from(data[0]) | (u64::from(data[1]) << 32),
                })?;
            }
            Event::ClientMessage(event) if event.type_ == self.atoms._NET_ACTIVE_WINDOW => {
                if let Some(window) = self.windows.get(&event.window).cloned() {
                    self.push_event(XwmEvent::FocusRequested(window))?;
                }
            }
            Event::ClientMessage(event) if event.type_ == self.atoms._NET_WM_MOVERESIZE => {
                self.push_event(XwmEvent::ReflowRequested)?;
            }
            other => debug!(?other, "ignored X11 event outside Tensor XWM policy"),
        }
        Ok(())
    }

    fn create_window(
        &mut self,
        event: CreateNotifyEvent,
    ) -> Result<X11Surface, Box<dyn std::error::Error>> {
        if self.windows.contains_key(&event.window) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("duplicate CreateNotify for X11 window {:#x}", event.window),
            )
            .into());
        }
        let geometry = LogicalRect::new(
            (i32::from(event.x), i32::from(event.y)).into(),
            (i32::from(event.width), i32::from(event.height)).into(),
        );
        let window = X11Surface::new(
            Arc::clone(&self.connection),
            event.window,
            self.next_window_generation,
            event.override_redirect,
            geometry,
            self.atoms._NET_WM_STATE,
            self.atoms._NET_WM_STATE_FOCUSED,
        );
        self.next_window_generation = self
            .next_window_generation
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("X11 window generation counter overflowed"))?;
        self.windows.insert(event.window, window.clone());
        Ok(window)
    }

    fn require_window(&self, id: u32) -> Result<X11Surface, Box<dyn std::error::Error>> {
        self.windows.get(&id).cloned().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("X11 event for window {id:#x} arrived before CreateNotify"),
            )
            .into()
        })
    }

    fn submit_property_request(
        &self,
        request: X11PropertyRequest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.property_requests.try_send(request).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                format!("X11 property completion queue rejected a request: {error:?}"),
            )
            .into()
        })
    }

    fn schedule_property_query(
        &self,
        window: &X11Surface,
        query: PropertyQuery,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !window.schedule_property_query(query) {
            return Ok(());
        }
        if let Err(error) = self.submit_property_request(self.property_request(window, query)) {
            window.cancel_property_query(query);
            return Err(error);
        }
        Ok(())
    }

    fn property_disposition(
        &self,
        window: &X11Surface,
        query: PropertyQuery,
    ) -> Result<PropertyDisposition, Box<dyn std::error::Error>> {
        match window.complete_property_query(query) {
            None => Ok(PropertyDisposition::Discard),
            Some(false) => Ok(PropertyDisposition::Apply),
            Some(true) => {
                if let Err(error) =
                    self.submit_property_request(self.property_request(window, query))
                {
                    window.cancel_property_query(query);
                    return Err(error);
                }
                Ok(PropertyDisposition::Resubmitted)
            }
        }
    }

    fn property_request(&self, window: &X11Surface, query: PropertyQuery) -> X11PropertyRequest {
        let target = window.property_target();
        match query {
            PropertyQuery::TransientFor => X11PropertyRequest::TransientFor { target },
            PropertyQuery::NormalHints => X11PropertyRequest::NormalHints { target },
            PropertyQuery::NetState => X11PropertyRequest::NetState {
                target,
                net_wm_state: self.atoms._NET_WM_STATE,
            },
        }
    }

    fn publish_completed_map_request(
        &mut self,
        window: X11Surface,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if window.take_completed_map_request() {
            self.push_event(XwmEvent::MapRequested(window))?;
        }
        Ok(())
    }

    fn push_event(&mut self, event: XwmEvent) -> Result<(), Box<dyn std::error::Error>> {
        push_xwm_event(&mut self.pending_events, event).map_err(Into::into)
    }

    fn publish_client_lists(&self) -> Result<(), Box<dyn std::error::Error>> {
        for atom in [
            self.atoms._NET_CLIENT_LIST,
            self.atoms._NET_CLIENT_LIST_STACKING,
        ] {
            self.connection.change_property32(
                PropMode::REPLACE,
                self.screen.root,
                atom,
                AtomEnum::WINDOW,
                &self.stacking,
            )?;
        }
        Ok(())
    }
}

fn push_xwm_event(queue: &mut VecDeque<XwmEvent>, event: XwmEvent) -> std::io::Result<()> {
    if queue.len() >= MAX_PENDING_XWM_EVENTS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::OutOfMemory,
            "X11 event queue exceeded Tensor's fixed capacity",
        ));
    }
    queue.push_back(event);
    Ok(())
}

impl Drop for X11Wm {
    fn drop(&mut self) {
        let _ = self.connection.destroy_window(self.wm_window);
        let _ = self.connection.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xwm_event_queue_fails_closed_at_its_fixed_capacity() {
        let mut queue = VecDeque::with_capacity(MAX_PENDING_XWM_EVENTS);
        for _ in 0..MAX_PENDING_XWM_EVENTS {
            push_xwm_event(&mut queue, XwmEvent::ReflowRequested).unwrap();
        }

        let error = push_xwm_event(&mut queue, XwmEvent::ReflowRequested).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::OutOfMemory);
        assert_eq!(queue.len(), MAX_PENDING_XWM_EVENTS);
    }
}
