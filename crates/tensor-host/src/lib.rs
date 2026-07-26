//! Smithay-free **host contracts** for Tensor.
//!
//! This crate is the long-term boundary between compositor policy and OS/backend
//! adapters. Types here are pure values: no Wayland objects, no DRM FDs, no
//! Vulkan handles. Adapters (Smithay today, native later) convert at the edge.
//!
//! # Exit path
//!
//! When every compositor module depends only on `tensor-host` / `tensor-event`
//! / `tensor-drm` / `tensor-present` for display/input/present *semantics*, the
//! Smithay dependency can be confined to an optional adapter crate and then
//! removed. See `docs/smithay-exit.md`.

mod connector;
mod format;
mod input;
mod mode;
mod present;
mod subpixel;

pub use connector::{ConnectorId, ConnectorState};
pub use format::{
    DrmFormat, FormatCapability, FormatError, Fourcc, GbmCapability, Modifier, OutputFormat,
    negotiate_output_formats,
};
pub use input::{AxisSource, KeyState, PointerAxis, PointerButton, PointerMotion};
pub use mode::PhysicalMode;
pub use present::{PresentIntent, PresentSlot, PresentState};
pub use subpixel::SubpixelLayout;
