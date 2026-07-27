//! Tensor-owned rootless XWayland adapter.
//!
//! Process startup, xwayland-shell association, and XWM state stay on the
//! compositor side. File-descriptor waits are submitted through the Compio
//! completion service; this module does not register a readiness event loop.

mod process;
mod shell;
mod xwm;

pub(crate) use process::{XWayland, XWaylandClientData};
pub(crate) use shell::XWaylandShellState;
pub(crate) use xwm::{WmWindowProperty, X11Surface, X11Wm, XwmEvent};
