//! Full client-side decorations for the native shell.
//!
//! Geometry, hit-testing, paint, and input state machine are pure logic.
//! [`frame`] owns Wayland subsurfaces and SHM buffers attached around a
//! toplevel content surface (header + four borders), matching the classic
//! CSD layout used by Adwaita/SCTK frames.

mod buttons;
mod frame;
mod geometry;
mod input;
mod paint;
mod theme;

pub use frame::ClientSideFrame;
#[allow(unused_imports)] // public CSD geometry surface for callers / tests
pub use geometry::{content_insets, DecorationInsets, HEADER_SIZE, RESIZE_BORDER};
pub use input::{FrameAction, FrameCursor, FramePartKind};
#[allow(unused_imports)]
pub use input::HitLocation;
#[allow(unused_imports)]
pub use theme::ColorTheme;
