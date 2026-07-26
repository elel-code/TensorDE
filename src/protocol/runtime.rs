use std::ffi::OsString;

use calloop::{
    EventLoop, Interest, LoopSignal, Mode, PostAction,
    channel::{Channel, Event as ChannelEvent},
    generic::Generic,
};
use thiserror::Error;
use wayland_server::{Display, ListeningSocket};

use crate::{
    backend::BackendConfig, layout::LayoutEngine, render::VulkanRenderer, scene::SceneAppearance,
};

use super::state::RuntimeState;

#[cfg(all(test, feature = "tty"))]
mod focus_tests;
mod socket;
#[cfg(test)]
mod socket_tests;
mod xwayland;

pub(crate) use socket::{
    MAX_PENDING_WAYLAND_CLIENTS, MAX_PENDING_WAYLAND_SOCKET_CONTROL_EVENTS,
    WaylandSocketControlEvent, WaylandSocketRuntime, drain_wayland_socket_events,
};

pub struct WaylandRuntime {
    event_loop: EventLoop<'static, RuntimeState>,
    display: Option<Display<RuntimeState>>,
    state: RuntimeState,
    socket_runtime: Option<WaylandSocketRuntime>,
    socket: ListeningSocket,
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
        let display = Display::new().map_err(ProtocolError::Display)?;
        let display_handle = display.handle();
        let state =
            RuntimeState::with_appearance(display_handle, event_loop.handle(), layout, appearance);
        let socket = bind_socket_source().map_err(ProtocolError::Socket)?;
        let socket_name = socket
            .socket_name()
            .expect("named Wayland listener must retain its name")
            .to_os_string();
        Ok(Self {
            event_loop,
            display: Some(display),
            state,
            socket_runtime: None,
            socket,
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

    pub(crate) fn install_socket_runtime(
        &mut self,
        clients: tensor_runtime::WorkerTx<std::os::unix::net::UnixStream>,
        control: tensor_runtime::WorkerTx<WaylandSocketControlEvent>,
    ) -> Result<(), ProtocolError> {
        if self.socket_runtime.is_some() {
            return Ok(());
        }
        self.socket_runtime = Some(
            WaylandSocketRuntime::start(&self.socket, clients, control)
                .map_err(|error| ProtocolError::SocketRuntime(error.to_string()))?,
        );
        Ok(())
    }

    pub(crate) fn take_socket_runtime(&mut self) -> Option<WaylandSocketRuntime> {
        self.socket_runtime.take()
    }

    #[cfg(test)]
    pub(crate) fn prepare_for_test(
        &mut self,
        enable_xwayland: bool,
    ) -> Result<tensor_runtime::EventfdCompletionRelay, ProtocolError> {
        let (notification_sender, notifications) = calloop::channel::sync_channel(1);
        let relay = tensor_runtime::EventfdCompletionRelay::start(
            "tensor-wayland-test-completions",
            move |_| {
                let _ = notification_sender.try_send(());
            },
        )?;
        let (client_sender, clients) = tensor_runtime::WorkerBridge::bounded_with_wake(
            MAX_PENDING_WAYLAND_CLIENTS,
            relay.wake(),
        );
        let (control_sender, control) = tensor_runtime::WorkerBridge::bounded_with_wake(
            MAX_PENDING_WAYLAND_SOCKET_CONTROL_EVENTS,
            relay.wake(),
        );
        self.install_socket_runtime(client_sender, control_sender)?;
        self.event_loop
            .handle()
            .insert_source(notifications, move |event, _, state| {
                if matches!(event, ChannelEvent::Msg(()))
                    && let Err(message) = drain_wayland_socket_events(&clients, &control, state)
                {
                    panic!("Wayland accept completion runtime failed: {message}");
                }
            })
            .map_err(|error| ProtocolError::ChannelSource(error.to_string()))?;
        self.prepare(enable_xwayland)?;
        Ok(relay)
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
        if self.socket_runtime.is_none() {
            return Err(ProtocolError::SocketRuntimeMissing);
        }
        let display = self.display.take().ok_or(ProtocolError::DisplayConsumed)?;
        self.event_loop
            .handle()
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| dispatch_display(display, state),
            )
            .map_err(|error| ProtocolError::DisplaySource(error.to_string()))?;
        // Advertise the fixed workspace pool to any early ext-workspace binders.
        self.state.refresh_ext_workspace_protocol();
        self.prepared = true;
        Ok(())
    }

    pub fn run(&mut self) -> Result<(), ProtocolError> {
        if !self.prepared {
            return Err(ProtocolError::RuntimeNotPrepared);
        }
        // Between waits: Tensor event turn (inject/drain/coalesced redraw), then
        // flush Wayland clients — same slot calloop used for idle work.
        self.event_loop
            .run(None, &mut self.state, RuntimeState::on_loop_idle)
            .map_err(ProtocolError::Run)
    }

    /// Run the compositor while dispatching a bounded value-only worker channel.
    pub fn run_with_channel<T, C>(
        &mut self,
        channel: Channel<T>,
        mut channel_handler: C,
    ) -> Result<(), ProtocolError>
    where
        T: Send + 'static,
        C: FnMut(ChannelEvent<T>, &mut RuntimeState) + 'static,
    {
        self.event_loop
            .handle()
            .insert_source(channel, move |event, _, state| {
                channel_handler(event, state);
            })
            .map_err(|error| ProtocolError::ChannelSource(error.to_string()))?;
        self.run()
    }
}

#[cfg(test)]
pub(crate) fn test_runtime_state(
    layout: LayoutEngine,
    appearance: SceneAppearance,
) -> RuntimeState {
    let event_loop = EventLoop::<RuntimeState>::try_new().unwrap();
    let display = Display::<RuntimeState>::new().unwrap();
    RuntimeState::with_appearance(display.handle(), event_loop.handle(), layout, appearance)
}

fn bind_socket_source() -> Result<ListeningSocket, wayland_server::BindError> {
    let mut last_error = None;
    for ordinal in 0..32 {
        let name = format!("tensor-{ordinal}");
        match ListeningSocket::bind(&name) {
            Ok(source) => return Ok(source),
            Err(error @ wayland_server::BindError::AlreadyInUse) => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or(wayland_server::BindError::AlreadyInUse))
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("failed to initialize the Smithay event loop: {0}")]
    EventLoop(calloop::Error),
    #[error("failed to initialize the Wayland display: {0}")]
    Display(wayland_server::backend::InitError),
    #[error("failed to bind the Wayland socket: {0}")]
    Socket(wayland_server::BindError),
    #[error("failed to start the Wayland socket completion runtime: {0}")]
    SocketRuntime(String),
    #[error("failed to register the Wayland display source: {0}")]
    DisplaySource(String),
    #[error("failed to register the worker channel source: {0}")]
    ChannelSource(String),
    #[cfg(test)]
    #[error(transparent)]
    CompletionRelay(#[from] tensor_runtime::CompletionRelayError),
    #[error("Wayland display was already moved into the event loop")]
    DisplayConsumed,
    #[error("Wayland socket completion runtime is not installed")]
    SocketRuntimeMissing,
    #[error("Wayland runtime must be prepared before entering the event loop")]
    RuntimeNotPrepared,
    #[error("failed to run the Smithay event loop: {0}")]
    Run(calloop::Error),
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
    display: &mut calloop::generic::NoIoDrop<Display<RuntimeState>>,
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
