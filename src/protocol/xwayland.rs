//! Tensor-owned rootless XWayland adapter.
//!
//! Process startup, xwayland-shell association, and XWM state stay on the
//! compositor side. File-descriptor waits are submitted through the Compio
//! completion service; this module does not register a readiness event loop.

mod process;
mod property;
mod shell;
mod xwm;

pub(crate) use process::{XWayland, XWaylandClientData};
pub(crate) use property::{
    X11AtomList, X11PropertyRequest, X11PropertyResult, X11PropertyRuntime, X11PropertyTarget,
    X11PropertyUpdate, X11SizeHints,
};
pub(crate) use shell::XWaylandShellState;
pub(crate) use xwm::{WmWindowProperty, X11Surface, X11Wm, XwmEvent};
