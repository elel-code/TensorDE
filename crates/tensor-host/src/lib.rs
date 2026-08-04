//! Smithay-free **host contracts** for Tensor.
//!
//! This crate is the long-term boundary between compositor policy and OS/backend
//! adapters. Types here are pure display/presentation values: no Wayland
//! objects, no DRM FDs, no Vulkan handles. Tensor's native owners convert at
//! the edge. Input values belong to `tensor-event`.
//!
//! # Exit path
//!
//! When every compositor module depends only on `tensor-host` / `tensor-event`
//! / `tensor-drm` / `tensor-present` for display/present *semantics*, native
//! owners can exchange values without leaking handles across boundaries.

mod connector;
mod format;
mod mode;
mod present;
mod session;
mod subpixel;
mod vblank;

pub use connector::{ConnectorId, ConnectorState};
pub use format::{
    DrmFormat, FormatCapability, FormatError, Fourcc, GbmCapability, Modifier, OutputFormat,
    negotiate_output_formats,
};
pub use mode::PhysicalMode;
pub use present::{PresentIntent, PresentMode, PresentSlot, PresentState};
pub use session::SessionEvent;
pub use subpixel::SubpixelLayout;
pub use vblank::{
    MAX_VBLANK_EVENTS_PER_READ, VblankBatch, VblankClock, VblankEvent, VblankMetadata,
};
