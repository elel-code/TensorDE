use std::{ffi::OsString, sync::Arc};

use calloop::EventLoop;
#[cfg(test)]
use calloop::channel::Event as ChannelEvent;
use thiserror::Error;
use wayland_server::{Display, ListeningSocket};

use crate::{
    backend::BackendConfig, layout::LayoutEngine, render::VulkanRenderer, scene::SceneAppearance,
};

use super::state::RuntimeState;

mod completion;
mod display;
#[cfg(all(test, feature = "tty"))]
mod focus_tests;
mod socket;
#[cfg(test)]
mod socket_tests;
mod xwayland;

pub(crate) use display::{
    MAX_PENDING_WAYLAND_DISPLAY_CONTROL_EVENTS, MAX_PENDING_WAYLAND_DISPLAY_EVENTS,
    WaylandDisplayControlEvent, WaylandDisplayEvent, WaylandDisplayRuntime,
    drain_wayland_display_events,
};
pub(crate) use socket::{
    MAX_PENDING_WAYLAND_CLIENTS, MAX_PENDING_WAYLAND_SOCKET_CONTROL_EVENTS,
    WaylandSocketControlEvent, WaylandSocketRuntime, drain_wayland_socket_events,
};
#[cfg(feature = "xwayland")]
pub(crate) use xwayland::{
    MAX_PENDING_XWAYLAND_STARTUP_CONTROL_EVENTS, MAX_PENDING_XWAYLAND_STARTUP_EVENTS,
    XWaylandStartupControlEvent, XWaylandStartupEvent, drain_xwayland_startup_events,
};

pub struct WaylandRuntime {
    event_loop: EventLoop<'static, RuntimeState>,
    state: RuntimeState,
    display_runtime: Option<WaylandDisplayRuntime>,
    socket_runtime: Option<WaylandSocketRuntime>,
    socket: ListeningSocket,
    socket_name: OsString,
    prepared: bool,
    xwayland_display: Option<OsString>,
    #[cfg(feature = "xwayland")]
    xwayland_completion_channels: Option<xwayland::XWaylandCompletionChannels>,
    #[cfg(feature = "xwayland")]
    xwayland_completion_runtime: Option<tensor_runtime::OpaqueFdCompletionRuntime>,
}

impl WaylandRuntime {
    /// Construct the runtime with value-only compositor appearance.
    pub fn with_appearance(
        layout: LayoutEngine,
        appearance: SceneAppearance,
    ) -> Result<Self, ProtocolError> {
        let event_loop = EventLoop::try_new().map_err(ProtocolError::EventLoop)?;
        let display = Display::new().map_err(ProtocolError::Display)?;
        let state = RuntimeState::with_appearance(display, event_loop.handle(), layout, appearance);
        let socket = bind_socket_source().map_err(ProtocolError::Socket)?;
        let socket_name = socket
            .socket_name()
            .expect("named Wayland listener must retain its name")
            .to_os_string();
        Ok(Self {
            event_loop,
            state,
            display_runtime: None,
            socket_runtime: None,
            socket,
            socket_name,
            prepared: false,
            xwayland_display: None,
            #[cfg(feature = "xwayland")]
            xwayland_completion_channels: None,
            #[cfg(feature = "xwayland")]
            xwayland_completion_runtime: None,
        })
    }

    pub fn backend_name(&self) -> &'static str {
        let _ = &self.event_loop;
        "smithay/compio-main"
    }

    pub fn socket_name(&self) -> &std::ffi::OsStr {
        &self.socket_name
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

    pub(crate) fn install_display_runtime(
        &mut self,
        events: tensor_runtime::WorkerTx<WaylandDisplayEvent>,
        control: tensor_runtime::WorkerTx<WaylandDisplayControlEvent>,
    ) -> Result<(), ProtocolError> {
        if self.display_runtime.is_some() {
            return Ok(());
        }
        self.display_runtime = Some(
            WaylandDisplayRuntime::start(self.state.display(), events, control)
                .map_err(|error| ProtocolError::DisplayRuntime(error.to_string()))?,
        );
        Ok(())
    }

    pub(crate) fn take_socket_runtime(&mut self) -> Option<WaylandSocketRuntime> {
        self.socket_runtime.take()
    }

    pub(crate) fn take_display_runtime(&mut self) -> Option<WaylandDisplayRuntime> {
        self.display_runtime.take()
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
        let (display_sender, display_events) = tensor_runtime::WorkerBridge::bounded_with_wake(
            MAX_PENDING_WAYLAND_DISPLAY_EVENTS,
            relay.wake(),
        );
        let (display_control_sender, display_control) =
            tensor_runtime::WorkerBridge::bounded_with_wake(
                MAX_PENDING_WAYLAND_DISPLAY_CONTROL_EVENTS,
                relay.wake(),
            );
        self.install_display_runtime(display_sender, display_control_sender)?;
        #[cfg(feature = "xwayland")]
        let (xwayland_sender, xwayland_events) = tensor_runtime::WorkerBridge::bounded_with_wake(
            MAX_PENDING_XWAYLAND_STARTUP_EVENTS,
            relay.wake(),
        );
        #[cfg(feature = "xwayland")]
        let (xwayland_control_sender, xwayland_control) =
            tensor_runtime::WorkerBridge::bounded_with_wake(
                MAX_PENDING_XWAYLAND_STARTUP_CONTROL_EVENTS,
                relay.wake(),
            );
        #[cfg(feature = "xwayland")]
        self.install_xwayland_completion_channels(xwayland_sender, xwayland_control_sender);
        self.event_loop
            .handle()
            .insert_source(notifications, move |event, _, state| {
                if matches!(event, ChannelEvent::Msg(()))
                    && let Err(message) = drain_wayland_socket_events(&clients, &control, state)
                {
                    panic!("Wayland accept completion runtime failed: {message}");
                }
                if matches!(event, ChannelEvent::Msg(()))
                    && let Err(message) =
                        drain_wayland_display_events(&display_events, &display_control, state)
                {
                    panic!("Wayland display completion runtime failed: {message}");
                }
                #[cfg(feature = "xwayland")]
                if matches!(event, ChannelEvent::Msg(())) {
                    drain_xwayland_startup_events(&xwayland_events, &xwayland_control, state);
                }
            })
            .map_err(|error| ProtocolError::ChannelSource(error.to_string()))?;
        self.prepare(enable_xwayland)?;
        Ok(relay)
    }

    pub(crate) fn prepare_backend(
        &mut self,
        config: &BackendConfig,
        completion_wake: Arc<dyn tensor_runtime::WakeSink>,
    ) -> Result<(), ProtocolError> {
        #[cfg(feature = "tty")]
        {
            if self.state.backend.is_some() {
                return Ok(());
            }
            let backend =
                crate::backend::TtyBackend::new(self.event_loop.handle(), config, completion_wake)
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
            let _ = (config, completion_wake);
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
        if self.display_runtime.is_none() {
            return Err(ProtocolError::DisplayRuntimeMissing);
        }
        // Advertise the fixed workspace pool to any early ext-workspace binders.
        self.state.refresh_ext_workspace_protocol();
        self.prepared = true;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn test_runtime_state(
    layout: LayoutEngine,
    appearance: SceneAppearance,
) -> RuntimeState {
    let event_loop = EventLoop::<RuntimeState>::try_new().unwrap();
    let display = Display::<RuntimeState>::new().unwrap();
    RuntimeState::with_appearance(display, event_loop.handle(), layout, appearance)
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
    #[error("failed to start the Wayland display completion runtime: {0}")]
    DisplayRuntime(String),
    #[error("failed to register the worker channel source: {0}")]
    ChannelSource(String),
    #[cfg(test)]
    #[error(transparent)]
    CompletionRelay(#[from] tensor_runtime::CompletionRelayError),
    #[error("Wayland socket completion runtime is not installed")]
    SocketRuntimeMissing,
    #[error("Wayland display completion runtime is not installed")]
    DisplayRuntimeMissing,
    #[error("Wayland runtime must be prepared before entering the event loop")]
    RuntimeNotPrepared,
    #[error("failed to run the Smithay event loop: {0}")]
    Run(calloop::Error),
    #[error("compositor-thread completion loop failed: {0}")]
    MainCompletion(String),
    #[error("failed to spawn XWayland: {0}")]
    XWayland(std::io::Error),
    #[error("failed to start the XWayland completion runtime: {0}")]
    XWaylandCompletion(String),
    #[error("XWayland completion channels are not installed")]
    XWaylandCompletionRuntimeMissing,
    #[error("XWayland support was not compiled in")]
    #[allow(dead_code)]
    XWaylandDisabled,
    #[error("the tty DRM backend was not compiled in")]
    TtyBackendDisabled,
    #[cfg(feature = "tty")]
    #[error("failed to initialize the tty DRM backend: {0}")]
    Backend(String),
}

#[cfg(test)]
mod tests;
