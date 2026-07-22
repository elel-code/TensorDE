use std::sync::Arc;

use smithay::reexports::{
    calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic},
    wayland_server::{
        Display, DisplayHandle,
        backend::{ClientData, ClientId, DisconnectReason},
    },
};
use thiserror::Error;
use tracing::warn;

#[cfg(feature = "xwayland")]
use smithay::reexports::wayland_server::Client;
use smithay::wayland::socket::ListeningSocketSource;
#[cfg(feature = "xwayland")]
use smithay::xwayland::{XWayland, XWaylandEvent};
#[cfg(feature = "xwayland")]
use tracing::info;

pub struct WaylandRuntime {
    event_loop: EventLoop<'static, ()>,
    display: Option<Display<()>>,
    display_handle: DisplayHandle,
    socket: Option<ListeningSocketSource>,
    prepared: bool,
    #[cfg(feature = "xwayland")]
    xwayland_client: Option<Client>,
}

impl WaylandRuntime {
    pub fn new() -> Result<Self, ProtocolError> {
        let event_loop = EventLoop::try_new().map_err(ProtocolError::EventLoop)?;
        let display = Display::new().map_err(ProtocolError::Display)?;
        let display_handle = display.handle();
        let socket = bind_socket_source().map_err(ProtocolError::Socket)?;
        Ok(Self {
            event_loop,
            display: Some(display),
            display_handle,
            socket: Some(socket),
            prepared: false,
            #[cfg(feature = "xwayland")]
            xwayland_client: None,
        })
    }

    pub fn backend_name(&self) -> &'static str {
        let _ = &self.event_loop;
        "smithay/calloop"
    }

    pub fn socket_name(&self) -> Option<&std::ffi::OsStr> {
        self.socket.as_ref().map(ListeningSocketSource::socket_name)
    }

    pub fn prepare(&mut self, enable_xwayland: bool) -> Result<(), ProtocolError> {
        if self.prepared {
            return Ok(());
        }
        if enable_xwayland {
            self.start_xwayland()?;
        }
        let socket = self.socket.take().ok_or(ProtocolError::SocketConsumed)?;
        let display = self.display.take().ok_or(ProtocolError::DisplayConsumed)?;
        let mut display_handle = self.display_handle.clone();
        self.event_loop
            .handle()
            .insert_source(socket, move |client_stream, _, _| {
                if let Err(error) =
                    display_handle.insert_client(client_stream, Arc::new(ClientState))
                {
                    warn!(%error, "failed to insert Wayland client");
                }
            })
            .map_err(|error| ProtocolError::SocketSource(error.to_string()))?;
        self.event_loop
            .handle()
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, _| dispatch_display(display),
            )
            .map_err(|error| ProtocolError::DisplaySource(error.to_string()))?;
        self.prepared = true;
        Ok(())
    }

    pub fn run(&mut self) -> Result<(), ProtocolError> {
        if !self.prepared {
            return Err(ProtocolError::RuntimeNotPrepared);
        }
        let mut state = ();
        self.event_loop
            .run(None, &mut state, |_| {})
            .map_err(ProtocolError::Run)
    }

    #[cfg(feature = "xwayland")]
    fn start_xwayland(&mut self) -> Result<(), ProtocolError> {
        if self.xwayland_client.is_some() {
            return Ok(());
        }

        let display = self
            .display
            .as_ref()
            .ok_or(ProtocolError::DisplayConsumed)?;
        let (xwayland, client) = XWayland::spawn(
            &display.handle(),
            None,
            std::iter::empty::<(&str, &str)>(),
            std::iter::empty::<&str>(),
            true,
            std::process::Stdio::null(),
            std::process::Stdio::null(),
            |_| {},
        )
        .map_err(ProtocolError::XWayland)?;
        self.event_loop
            .handle()
            .insert_source(xwayland, |event, _, _| match event {
                XWaylandEvent::Ready { display_number, .. } => {
                    info!(display_number, "XWayland is ready")
                }
                XWaylandEvent::Error => warn!("XWayland exited before becoming ready"),
            })
            .map_err(|error| ProtocolError::XWaylandSource(error.to_string()))?;
        self.xwayland_client = Some(client);
        Ok(())
    }

    #[cfg(not(feature = "xwayland"))]
    fn start_xwayland(&mut self) -> Result<(), ProtocolError> {
        Err(ProtocolError::XWaylandDisabled)
    }
}

fn bind_socket_source()
-> Result<ListeningSocketSource, smithay::reexports::wayland_server::BindError> {
    let mut last_error = None;
    for ordinal in 0..32 {
        let name = format!("tensor-{ordinal}");
        match ListeningSocketSource::with_name(&name) {
            Ok(source) => return Ok(source),
            Err(error @ smithay::reexports::wayland_server::BindError::AlreadyInUse) => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or(smithay::reexports::wayland_server::BindError::AlreadyInUse))
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("failed to initialize the Smithay event loop: {0}")]
    EventLoop(smithay::reexports::calloop::Error),
    #[error("failed to initialize the Wayland display: {0}")]
    Display(smithay::reexports::wayland_server::backend::InitError),
    #[error("failed to bind the Wayland socket: {0}")]
    Socket(smithay::reexports::wayland_server::BindError),
    #[error("failed to register the Wayland socket source: {0}")]
    SocketSource(String),
    #[error("failed to register the Wayland display source: {0}")]
    DisplaySource(String),
    #[error("Wayland display was already moved into the event loop")]
    DisplayConsumed,
    #[error("Wayland socket was already moved into the event loop")]
    SocketConsumed,
    #[error("Wayland runtime must be prepared before entering the event loop")]
    RuntimeNotPrepared,
    #[error("failed to run the Smithay event loop: {0}")]
    Run(smithay::reexports::calloop::Error),
    #[error("failed to spawn XWayland: {0}")]
    XWayland(std::io::Error),
    #[error("failed to register XWayland with the event loop: {0}")]
    XWaylandSource(String),
    #[error("XWayland support was not compiled in")]
    #[allow(dead_code)]
    XWaylandDisabled,
}

#[derive(Debug)]
struct ClientState;

#[allow(unsafe_code)]
fn dispatch_display(
    display: &mut smithay::reexports::calloop::generic::NoIoDrop<Display<()>>,
) -> Result<PostAction, std::io::Error> {
    // Generic owns the display source for the entire event loop, so this mutable access does not
    // alias another display owner. Smithay exposes dispatch through this narrow unsafe adapter.
    let display = unsafe { display.get_mut() };
    display.dispatch_clients(&mut ())?;
    display.flush_clients()?;
    Ok(PostAction::Continue)
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}

    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
