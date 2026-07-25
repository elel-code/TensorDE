use std::{ffi::OsString, sync::Arc};

use smithay::reexports::{
    calloop::{
        EventLoop, Interest, LoopSignal, Mode, PostAction,
        channel::{Channel, Event as ChannelEvent},
        generic::Generic,
    },
    wayland_server::Display,
};
use thiserror::Error;
use tracing::warn;

use crate::{
    backend::BackendConfig,
    ipc::{IpcReply, IpcServer, Request},
    layout::LayoutEngine,
    render::VulkanRenderer,
    scene::SceneAppearance,
};

use super::state::{RuntimeState, WaylandClientState};
use smithay::wayland::socket::ListeningSocketSource;

#[cfg(all(test, feature = "tty"))]
mod focus_tests;
#[cfg(test)]
mod socket_tests;
mod xwayland;

pub struct WaylandRuntime {
    event_loop: EventLoop<'static, RuntimeState>,
    display: Option<Display<RuntimeState>>,
    state: RuntimeState,
    socket: Option<ListeningSocketSource>,
    socket_name: OsString,
    prepared: bool,
    xwayland_display: Option<OsString>,
    #[cfg(feature = "xwayland")]
    xwayland_client: Option<smithay::reexports::wayland_server::Client>,
}

impl WaylandRuntime {
    /// Construct the runtime with value-only compositor appearance.
    pub fn with_appearance(
        layout: LayoutEngine,
        appearance: SceneAppearance,
    ) -> Result<Self, ProtocolError> {
        let event_loop = EventLoop::try_new().map_err(ProtocolError::EventLoop)?;
        crate::signals::install(&event_loop.handle(), event_loop.get_signal())
            .map_err(ProtocolError::SignalSource)?;
        let display = Display::new().map_err(ProtocolError::Display)?;
        let display_handle = display.handle();
        let state =
            RuntimeState::with_appearance(display_handle, event_loop.handle(), layout, appearance);
        let socket = bind_socket_source().map_err(ProtocolError::Socket)?;
        let socket_name = socket.socket_name().to_os_string();
        Ok(Self {
            event_loop,
            display: Some(display),
            state,
            socket: Some(socket),
            socket_name,
            prepared: false,
            xwayland_display: None,
            #[cfg(feature = "xwayland")]
            xwayland_client: None,
        })
    }

    pub fn backend_name(&self) -> &'static str {
        let _ = &self.event_loop;
        "smithay/calloop"
    }

    pub fn socket_name(&self) -> &std::ffi::OsStr {
        &self.socket_name
    }

    pub fn stop_signal(&self) -> LoopSignal {
        self.event_loop.get_signal()
    }

    pub fn xwayland_display(&self) -> Option<OsString> {
        self.xwayland_display.clone()
    }

    pub(crate) fn state_mut(&mut self) -> &mut RuntimeState {
        &mut self.state
    }

    pub(crate) fn install_renderer(&mut self, renderer: VulkanRenderer) {
        self.state.install_renderer(renderer);
    }

    pub(crate) fn renderer(&self) -> Option<&VulkanRenderer> {
        self.state.renderer()
    }

    pub(crate) fn prepare_backend(&mut self, config: &BackendConfig) -> Result<(), ProtocolError> {
        #[cfg(feature = "tty")]
        {
            if self.state.backend.is_some() {
                return Ok(());
            }
            let backend = crate::backend::TtyBackend::new(self.event_loop.handle(), config)
                .map_err(|error| ProtocolError::Backend(error.to_string()))?;
            self.state.install_backend(backend);
            let events = self
                .state
                .backend
                .as_mut()
                .expect("backend was installed")
                .take_output_events();
            self.state
                .apply_backend_output_events(events)
                .map_err(ProtocolError::Backend)?;
            Ok(())
        }
        #[cfg(not(feature = "tty"))]
        {
            let _ = config;
            Err(ProtocolError::TtyBackendDisabled)
        }
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
        self.event_loop
            .handle()
            .insert_source(socket, move |client_stream, _, state| {
                if let Err(error) = state
                    .display_handle
                    .insert_client(client_stream, Arc::new(WaylandClientState::default()))
                {
                    warn!(%error, "failed to insert Wayland client");
                }
            })
            .map_err(|error| ProtocolError::SocketSource(error.to_string()))?;
        self.event_loop
            .handle()
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| dispatch_display(display, state),
            )
            .map_err(|error| ProtocolError::DisplaySource(error.to_string()))?;
        self.prepared = true;
        Ok(())
    }

    pub fn run(&mut self) -> Result<(), ProtocolError> {
        if !self.prepared {
            return Err(ProtocolError::RuntimeNotPrepared);
        }
        self.event_loop
            .run(None, &mut self.state, RuntimeState::flush_wayland_clients)
            .map_err(ProtocolError::Run)
    }

    /// Run the compositor while dispatching a bounded value-only worker channel.
    pub fn run_with_ipc_and_channel<H, T, C>(
        &mut self,
        ipc: &IpcServer,
        channel: Channel<T>,
        mut channel_handler: C,
        handler: H,
    ) -> Result<(), ProtocolError>
    where
        H: FnMut(Request, &mut RuntimeState) -> IpcReply + 'static,
        T: Send + 'static,
        C: FnMut(ChannelEvent<T>, &mut RuntimeState) + 'static,
    {
        ipc.register(&self.event_loop.handle(), handler)
            .map_err(|error| ProtocolError::IpcSource(error.to_string()))?;
        self.event_loop
            .handle()
            .insert_source(channel, move |event, _, state| {
                channel_handler(event, state);
            })
            .map_err(|error| ProtocolError::ChannelSource(error.to_string()))?;
        self.run()
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
    #[error("failed to register the compositor signal source: {0}")]
    SignalSource(smithay::reexports::calloop::Error),
    #[error("failed to initialize the Wayland display: {0}")]
    Display(smithay::reexports::wayland_server::backend::InitError),
    #[error("failed to bind the Wayland socket: {0}")]
    Socket(smithay::reexports::wayland_server::BindError),
    #[error("failed to register the Wayland socket source: {0}")]
    SocketSource(String),
    #[error("failed to register the Wayland display source: {0}")]
    DisplaySource(String),
    #[error("failed to register the IPC source: {0}")]
    IpcSource(String),
    #[error("failed to register the worker channel source: {0}")]
    ChannelSource(String),
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
    #[error("the tty DRM backend was not compiled in")]
    TtyBackendDisabled,
    #[cfg(feature = "tty")]
    #[error("failed to initialize the tty DRM backend: {0}")]
    Backend(String),
}

#[allow(unsafe_code)]
fn dispatch_display(
    display: &mut smithay::reexports::calloop::generic::NoIoDrop<Display<RuntimeState>>,
    state: &mut RuntimeState,
) -> Result<PostAction, std::io::Error> {
    // Generic owns the display source for the entire event loop, so this mutable access does not
    // alias another display owner. Smithay exposes dispatch through this narrow unsafe adapter.
    let display = unsafe { display.get_mut() };
    display.dispatch_clients(state)?;
    display.flush_clients()?;
    Ok(PostAction::Continue)
}

#[cfg(test)]
mod tests;
