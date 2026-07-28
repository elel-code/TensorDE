//! Tensor-owned popup topology, positioning, and explicit grabs.

mod grab;
mod position;
mod registry;

pub(crate) use grab::PopupGrab;
pub(crate) use registry::{PopupKind, PopupManager, find_popup_root_surface};
