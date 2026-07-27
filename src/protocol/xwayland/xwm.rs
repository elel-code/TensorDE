//! Rootless X11 window manager driven by completed X11 socket operations.

mod surface;

use std::{
    collections::HashMap,
    os::fd::{AsFd, BorrowedFd},
    os::unix::net::UnixStream,
    sync::Arc,
};

use smithay::utils::Rectangle;
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
}

impl X11Wm {
    pub(crate) fn start(socket: UnixStream) -> Result<Self, Box<dyn std::error::Error>> {
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
        })
    }

    pub(crate) fn completion_fd(&self) -> BorrowedFd<'_> {
        self.connection.stream().as_fd()
    }

    pub(crate) fn drain_events(&mut self) -> Result<Vec<XwmEvent>, Box<dyn std::error::Error>> {
        let mut output = Vec::new();
        while let Some(event) = self.connection.poll_for_event()? {
            trace!(?event, "completed X11 event");
            self.handle_event(event, &mut output)?;
        }
        self.connection.flush()?;
        Ok(output)
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

    fn handle_event(
        &mut self,
        event: Event,
        output: &mut Vec<XwmEvent>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match event {
            Event::CreateNotify(event) if event.window != self.wm_window => {
                let (window, created) = self.ensure_created(event)?;
                if created {
                    output.push(XwmEvent::NewWindow(window));
                }
            }
            Event::MapRequest(event) => {
                let (window, created) = self.ensure_window(event.window)?;
                if created {
                    output.push(XwmEvent::NewWindow(window.clone()));
                }
                window.refresh_properties(&self.atoms)?;
                output.push(XwmEvent::MapRequested(window));
            }
            Event::MapNotify(event) => {
                if let Some(window) = self.windows.get(&event.window).cloned() {
                    window.mark_mapped(true);
                    self.stacking.retain(|candidate| *candidate != event.window);
                    self.stacking.push(event.window);
                    self.publish_client_lists()?;
                    output.push(XwmEvent::Mapped(window));
                }
            }
            Event::UnmapNotify(event) => {
                if let Some(window) = self.windows.get(&event.window).cloned() {
                    window.mark_mapped(false);
                    self.stacking.retain(|candidate| *candidate != event.window);
                    self.publish_client_lists()?;
                    output.push(XwmEvent::Unmapped(window));
                }
            }
            Event::DestroyNotify(event) => {
                if let Some(window) = self.windows.remove(&event.window) {
                    window.mark_destroyed();
                    self.stacking.retain(|candidate| *candidate != event.window);
                    self.unpaired_windows
                        .retain(|_, candidate| *candidate != event.window);
                    self.publish_client_lists()?;
                    output.push(XwmEvent::Destroyed(window));
                }
            }
            Event::ConfigureRequest(event) => {
                let (window, created) = self.ensure_window(event.window)?;
                if created {
                    output.push(XwmEvent::NewWindow(window.clone()));
                }
                if window.is_override_redirect() {
                    let geometry = Rectangle::new(
                        (i32::from(event.x), i32::from(event.y)).into(),
                        (i32::from(event.width), i32::from(event.height)).into(),
                    );
                    window.configure(Some(geometry))?;
                } else {
                    output.push(XwmEvent::ConfigureRequested {
                        window,
                        width: Some(u32::from(event.width)),
                        height: Some(u32::from(event.height)),
                    });
                }
            }
            Event::ConfigureNotify(event) => {
                if let Some(window) = self.windows.get(&event.window).cloned() {
                    let geometry = Rectangle::new(
                        (i32::from(event.x), i32::from(event.y)).into(),
                        (i32::from(event.width), i32::from(event.height)).into(),
                    );
                    window.update_geometry(geometry);
                    output.push(XwmEvent::Configured {
                        window,
                        above: (event.above_sibling != 0).then_some(event.above_sibling),
                    });
                }
            }
            Event::PropertyNotify(event) => {
                if let Some(window) = self.windows.get(&event.window).cloned()
                    && let Some(property) = window.refresh_property(event.atom, &self.atoms)?
                {
                    output.push(XwmEvent::PropertyChanged(window, property));
                }
            }
            Event::ClientMessage(event) if event.type_ == self.atoms.WL_SURFACE_SERIAL => {
                let data = event.data.as_data32();
                output.push(XwmEvent::SurfaceSerial {
                    window: event.window,
                    serial: u64::from(data[0]) | (u64::from(data[1]) << 32),
                });
            }
            Event::ClientMessage(event) if event.type_ == self.atoms._NET_ACTIVE_WINDOW => {
                if let Some(window) = self.windows.get(&event.window).cloned() {
                    output.push(XwmEvent::FocusRequested(window));
                }
            }
            Event::ClientMessage(event) if event.type_ == self.atoms._NET_WM_MOVERESIZE => {
                output.push(XwmEvent::ReflowRequested);
            }
            other => debug!(?other, "ignored X11 event outside Tensor XWM policy"),
        }
        Ok(())
    }

    fn ensure_created(
        &mut self,
        event: CreateNotifyEvent,
    ) -> Result<(X11Surface, bool), Box<dyn std::error::Error>> {
        if let Some(window) = self.windows.get(&event.window) {
            return Ok((window.clone(), false));
        }
        let geometry = Rectangle::new(
            (i32::from(event.x), i32::from(event.y)).into(),
            (i32::from(event.width), i32::from(event.height)).into(),
        );
        let window = X11Surface::new(
            Arc::clone(&self.connection),
            event.window,
            event.override_redirect,
            geometry,
            self.atoms._NET_WM_STATE,
            self.atoms._NET_WM_STATE_FOCUSED,
        );
        self.windows.insert(event.window, window.clone());
        Ok((window, true))
    }

    fn ensure_window(&mut self, id: u32) -> Result<(X11Surface, bool), Box<dyn std::error::Error>> {
        if let Some(window) = self.windows.get(&id) {
            return Ok((window.clone(), false));
        }
        let attributes = self.connection.get_window_attributes(id)?.reply()?;
        let geometry = self.connection.get_geometry(id)?.reply()?;
        self.ensure_created(CreateNotifyEvent {
            response_type: 0,
            sequence: 0,
            parent: self.screen.root,
            window: id,
            x: geometry.x,
            y: geometry.y,
            width: geometry.width,
            height: geometry.height,
            border_width: geometry.border_width,
            override_redirect: attributes.override_redirect,
        })
    }

    fn publish_client_lists(&self) -> Result<(), Box<dyn std::error::Error>> {
        let clients = self
            .stacking
            .iter()
            .copied()
            .filter(|window| {
                self.windows
                    .get(window)
                    .is_some_and(|surface| !surface.is_override_redirect())
            })
            .collect::<Vec<_>>();
        for atom in [
            self.atoms._NET_CLIENT_LIST,
            self.atoms._NET_CLIENT_LIST_STACKING,
        ] {
            self.connection.change_property32(
                PropMode::REPLACE,
                self.screen.root,
                atom,
                AtomEnum::WINDOW,
                &clients,
            )?;
        }
        Ok(())
    }
}

impl Drop for X11Wm {
    fn drop(&mut self) {
        let _ = self.connection.destroy_window(self.wm_window);
        let _ = self.connection.flush();
    }
}
