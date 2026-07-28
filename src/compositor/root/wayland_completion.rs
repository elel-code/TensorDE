use std::sync::Arc;

use tensor_runtime::{WakeSink, WorkerBridge, WorkerRx};

use crate::protocol::{
    MAX_PENDING_WAYLAND_CLIENTS, MAX_PENDING_WAYLAND_DISPLAY_CONTROL_EVENTS,
    MAX_PENDING_WAYLAND_DISPLAY_EVENTS, MAX_PENDING_WAYLAND_SOCKET_CONTROL_EVENTS, ProtocolError,
    RuntimeState, WaylandDisplayControlEvent, WaylandDisplayEvent, WaylandRuntime,
    WaylandSocketControlEvent, drain_wayland_display_events, drain_wayland_socket_events,
};
#[cfg(feature = "xwayland")]
use crate::protocol::{
    MAX_PENDING_XWAYLAND_PROPERTY_CONTROL_EVENTS, MAX_PENDING_XWAYLAND_PROPERTY_RESULTS,
    MAX_PENDING_XWAYLAND_STARTUP_CONTROL_EVENTS, MAX_PENDING_XWAYLAND_STARTUP_EVENTS,
    XWaylandPropertyControlEvent, XWaylandPropertyEvent, XWaylandStartupControlEvent,
    XWaylandStartupEvent, drain_xwayland_events,
};

pub(super) struct WaylandCompletionBridges {
    clients: WorkerRx<std::os::unix::net::UnixStream>,
    socket_control: WorkerRx<WaylandSocketControlEvent>,
    display: WorkerRx<WaylandDisplayEvent>,
    display_control: WorkerRx<WaylandDisplayControlEvent>,
    #[cfg(feature = "xwayland")]
    xwayland: WorkerRx<XWaylandStartupEvent>,
    #[cfg(feature = "xwayland")]
    xwayland_control: WorkerRx<XWaylandStartupControlEvent>,
    #[cfg(feature = "xwayland")]
    xwayland_properties: WorkerRx<XWaylandPropertyEvent>,
    #[cfg(feature = "xwayland")]
    xwayland_property_control: WorkerRx<XWaylandPropertyControlEvent>,
}

impl WaylandCompletionBridges {
    pub(super) fn install(
        protocol: &mut WaylandRuntime,
        wake: Arc<dyn WakeSink>,
    ) -> Result<Self, ProtocolError> {
        let (client_sender, clients) =
            WorkerBridge::bounded_with_wake(MAX_PENDING_WAYLAND_CLIENTS, Arc::clone(&wake));
        let (socket_control_sender, socket_control) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_WAYLAND_SOCKET_CONTROL_EVENTS,
            Arc::clone(&wake),
        );
        protocol.install_socket_runtime(client_sender, socket_control_sender)?;

        let (display_sender, display) =
            WorkerBridge::bounded_with_wake(MAX_PENDING_WAYLAND_DISPLAY_EVENTS, Arc::clone(&wake));
        let (display_control_sender, display_control) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_WAYLAND_DISPLAY_CONTROL_EVENTS,
            Arc::clone(&wake),
        );
        protocol.install_display_runtime(display_sender, display_control_sender)?;

        #[cfg(feature = "xwayland")]
        let (xwayland_sender, xwayland) =
            WorkerBridge::bounded_with_wake(MAX_PENDING_XWAYLAND_STARTUP_EVENTS, Arc::clone(&wake));
        #[cfg(feature = "xwayland")]
        let (xwayland_control_sender, xwayland_control) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_XWAYLAND_STARTUP_CONTROL_EVENTS,
            Arc::clone(&wake),
        );
        #[cfg(feature = "xwayland")]
        let (xwayland_property_sender, xwayland_properties) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_XWAYLAND_PROPERTY_RESULTS,
            Arc::clone(&wake),
        );
        #[cfg(feature = "xwayland")]
        let (xwayland_property_control_sender, xwayland_property_control) =
            WorkerBridge::bounded_with_wake(MAX_PENDING_XWAYLAND_PROPERTY_CONTROL_EVENTS, wake);
        #[cfg(feature = "xwayland")]
        protocol.install_xwayland_completion_channels(
            xwayland_sender,
            xwayland_control_sender,
            xwayland_property_sender,
            xwayland_property_control_sender,
        )?;

        Ok(Self {
            clients,
            socket_control,
            display,
            display_control,
            #[cfg(feature = "xwayland")]
            xwayland,
            #[cfg(feature = "xwayland")]
            xwayland_control,
            #[cfg(feature = "xwayland")]
            xwayland_properties,
            #[cfg(feature = "xwayland")]
            xwayland_property_control,
        })
    }

    pub(super) fn drain(&self, state: &mut RuntimeState) -> Result<(), String> {
        drain_wayland_socket_events(&self.clients, &self.socket_control, state)
            .map_err(|message| format!("Wayland accept completion failed: {message}"))?;
        drain_wayland_display_events(&self.display, &self.display_control, state)
            .map_err(|message| format!("Wayland display completion failed: {message}"))?;
        #[cfg(feature = "xwayland")]
        drain_xwayland_events(
            &self.xwayland,
            &self.xwayland_control,
            &self.xwayland_properties,
            &self.xwayland_property_control,
            state,
        )
        .map_err(|message| format!("XWayland completion failed: {message}"))?;
        Ok(())
    }
}
