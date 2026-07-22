use std::{ffi::OsString, sync::Arc};

use smithay::reexports::{
    calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction, generic::Generic},
    wayland_server::Display,
};
use thiserror::Error;
use tracing::warn;

use crate::{
    backend::BackendConfig,
    ipc::{IpcReply, IpcServer, Request},
    layout::LayoutKind,
};

use super::state::{RuntimeState, WaylandClientState};

#[cfg(feature = "xwayland")]
use smithay::reexports::wayland_server::Client;
use smithay::wayland::socket::ListeningSocketSource;
#[cfg(feature = "xwayland")]
use smithay::xwayland::{XWayland, XWaylandEvent};
#[cfg(feature = "xwayland")]
use tracing::info;

pub struct WaylandRuntime {
    event_loop: EventLoop<'static, RuntimeState>,
    display: Option<Display<RuntimeState>>,
    state: RuntimeState,
    socket: Option<ListeningSocketSource>,
    prepared: bool,
    xwayland_display: Option<OsString>,
    #[cfg(feature = "xwayland")]
    xwayland_client: Option<Client>,
}

impl WaylandRuntime {
    pub fn new(layout: LayoutKind) -> Result<Self, ProtocolError> {
        let event_loop = EventLoop::try_new().map_err(ProtocolError::EventLoop)?;
        let display = Display::new().map_err(ProtocolError::Display)?;
        let display_handle = display.handle();
        let state = RuntimeState::new(display_handle, layout);
        let socket = bind_socket_source().map_err(ProtocolError::Socket)?;
        Ok(Self {
            event_loop,
            display: Some(display),
            state,
            socket: Some(socket),
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

    pub fn socket_name(&self) -> Option<&std::ffi::OsStr> {
        self.socket.as_ref().map(ListeningSocketSource::socket_name)
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

    pub(crate) fn prepare_backend(&mut self, config: &BackendConfig) -> Result<(), ProtocolError> {
        #[cfg(feature = "tty")]
        {
            if self.state.backend.is_some() {
                return Ok(());
            }
            let mut backend = crate::backend::TtyBackend::new(self.event_loop.handle(), config)
                .map_err(|error| ProtocolError::Backend(error.to_string()))?;
            self.state
                .apply_backend_output_events(backend.take_output_events());
            self.state.backend = Some(backend);
            return Ok(());
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
            .run(None, &mut self.state, |_| {})
            .map_err(ProtocolError::Run)
    }

    pub fn run_with_ipc<H>(&mut self, ipc: &IpcServer, handler: H) -> Result<(), ProtocolError>
    where
        H: FnMut(Request, &mut RuntimeState) -> IpcReply + 'static,
    {
        ipc.register(&self.event_loop.handle(), handler)
            .map_err(|error| ProtocolError::IpcSource(error.to_string()))?;
        self.run()
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
        let display_number = xwayland.display_number();
        self.event_loop
            .handle()
            .insert_source(xwayland, |event, _, _| match event {
                XWaylandEvent::Ready { display_number, .. } => {
                    info!(display_number, "XWayland is ready")
                }
                XWaylandEvent::Error => warn!("XWayland exited before becoming ready"),
            })
            .map_err(|error| ProtocolError::XWaylandSource(error.to_string()))?;
        self.xwayland_display = Some(OsString::from(format!(":{display_number}")));
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
    #[error("failed to register the IPC source: {0}")]
    IpcSource(String),
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
mod tests {
    use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

    use wayland_client::{
        Connection, Dispatch, QueueHandle, delegate_noop,
        globals::{GlobalListContents, registry_queue_init},
        protocol::{wl_compositor, wl_registry, wl_surface},
    };
    use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

    use super::*;
    use crate::layout::LayoutKind;

    #[derive(Debug, Eq, PartialEq)]
    enum ClientEvent {
        Configured,
        Destroyed,
    }

    struct TestClient {
        configured: bool,
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for TestClient {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    delegate_noop!(TestClient: ignore wl_compositor::WlCompositor);
    delegate_noop!(TestClient: ignore wl_surface::WlSurface);
    delegate_noop!(TestClient: ignore xdg_toplevel::XdgToplevel);

    impl Dispatch<xdg_wm_base::XdgWmBase, ()> for TestClient {
        fn event(
            _: &mut Self,
            wm_base: &xdg_wm_base::XdgWmBase,
            event: xdg_wm_base::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let xdg_wm_base::Event::Ping { serial } = event {
                wm_base.pong(serial);
            }
        }
    }

    impl Dispatch<xdg_surface::XdgSurface, ()> for TestClient {
        fn event(
            state: &mut Self,
            surface: &xdg_surface::XdgSurface,
            event: xdg_surface::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let xdg_surface::Event::Configure { serial } = event {
                surface.ack_configure(serial);
                state.configured = true;
            }
        }
    }

    #[test]
    fn xdg_toplevel_lifecycle_is_owned_by_runtime_state() {
        let mut runtime = WaylandRuntime::new(LayoutKind::Scrolling1D).unwrap();
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
        let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name().unwrap());
        runtime.prepare(false).unwrap();

        let (event_tx, event_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let client = std::thread::spawn(move || {
            let connection =
                Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
            let (globals, mut queue) = registry_queue_init::<TestClient>(&connection).unwrap();
            let handle = queue.handle();
            let compositor = globals
                .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
                .unwrap();
            let wm_base = globals
                .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
                .unwrap();
            let surface = compositor.create_surface(&handle, ());
            let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
            let toplevel = xdg_surface.get_toplevel(&handle, ());
            surface.commit();

            let mut state = TestClient { configured: false };
            while !state.configured {
                queue.blocking_dispatch(&mut state).unwrap();
            }
            event_tx.send(ClientEvent::Configured).unwrap();
            release_rx.recv().unwrap();

            toplevel.destroy();
            xdg_surface.destroy();
            surface.destroy();
            connection.roundtrip().unwrap();
            event_tx.send(ClientEvent::Destroyed).unwrap();
        });

        assert_eq!(
            dispatch_until(&mut runtime, &event_rx),
            ClientEvent::Configured
        );
        assert_eq!(runtime.state.view_count(), 1);

        release_tx.send(()).unwrap();
        assert_eq!(
            dispatch_until(&mut runtime, &event_rx),
            ClientEvent::Destroyed
        );
        for _ in 0..16 {
            runtime
                .event_loop
                .dispatch(Duration::from_millis(2), &mut runtime.state)
                .unwrap();
            if runtime.state.view_count() == 0 {
                break;
            }
        }
        assert_eq!(runtime.state.view_count(), 0);
        client.join().unwrap();
    }

    fn dispatch_until(
        runtime: &mut WaylandRuntime,
        events: &mpsc::Receiver<ClientEvent>,
    ) -> ClientEvent {
        for _ in 0..200 {
            runtime
                .event_loop
                .dispatch(Duration::from_millis(5), &mut runtime.state)
                .unwrap();
            if let Ok(event) = events.try_recv() {
                return event;
            }
        }
        panic!("Wayland client did not complete before the dispatch limit");
    }
}
