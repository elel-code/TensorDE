mod clock;
#[cfg(feature = "tty")]
mod cursor;
mod dispatch;
mod extensions;
mod focus;
mod globals;
mod handlers;
#[cfg(feature = "tty")]
mod input;
mod runtime;
mod seat;
mod serial;
mod state;
#[cfg(feature = "xwayland")]
mod xwayland;

pub(crate) use extensions::security_context::{
    MAX_PENDING_SECURITY_CONTEXT_EVENTS, SecurityContextEvent, SecurityContextRuntime,
    SecurityContextRuntimeError, drain_security_context_events,
};
#[cfg(test)]
pub(crate) use runtime::test_runtime_state;
pub(crate) use runtime::{
    MAX_PENDING_WAYLAND_CLIENTS, MAX_PENDING_WAYLAND_DISPLAY_CONTROL_EVENTS,
    MAX_PENDING_WAYLAND_DISPLAY_EVENTS, MAX_PENDING_WAYLAND_SOCKET_CONTROL_EVENTS,
    WaylandDisplayControlEvent, WaylandDisplayEvent, WaylandSocketControlEvent,
    drain_wayland_display_events, drain_wayland_socket_events,
};
#[cfg(feature = "xwayland")]
pub(crate) use runtime::{
    MAX_PENDING_XWAYLAND_PROPERTY_CONTROL_EVENTS, MAX_PENDING_XWAYLAND_PROPERTY_RESULTS,
    MAX_PENDING_XWAYLAND_STARTUP_CONTROL_EVENTS, MAX_PENDING_XWAYLAND_STARTUP_EVENTS,
    XWaylandPropertyControlEvent, XWaylandPropertyEvent, XWaylandStartupControlEvent,
    XWaylandStartupEvent, drain_xwayland_events,
};
pub use runtime::{ProtocolError, WaylandRuntime};
pub(crate) use state::RuntimeState;
pub(crate) use tensor_protocol::{PROTOCOL_CATALOG, ProtocolTier};
