use smithay::reexports::{
    calloop::EventLoop,
    wayland_server::{Display, ListeningSocket},
};
use thiserror::Error;

pub struct WaylandRuntime {
    event_loop: EventLoop<'static, ()>,
    display: Display<()>,
    socket: ListeningSocket,
}

impl WaylandRuntime {
    pub fn new() -> Result<Self, ProtocolError> {
        let event_loop = EventLoop::try_new().map_err(ProtocolError::EventLoop)?;
        let display = Display::new().map_err(ProtocolError::Display)?;
        let socket = ListeningSocket::bind_auto("tensor", 0..32).map_err(ProtocolError::Socket)?;
        Ok(Self {
            event_loop,
            display,
            socket,
        })
    }

    pub fn backend_name(&self) -> &'static str {
        let _ = (&self.event_loop, &self.display);
        "smithay/calloop"
    }

    pub fn socket_name(&self) -> Option<&std::ffi::OsStr> {
        self.socket.socket_name()
    }
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("failed to initialize the Smithay event loop: {0}")]
    EventLoop(smithay::reexports::calloop::Error),
    #[error("failed to initialize the Wayland display: {0}")]
    Display(smithay::reexports::wayland_server::backend::InitError),
    #[error("failed to bind the Wayland socket: {0}")]
    Socket(smithay::reexports::wayland_server::BindError),
}
