//! Native Wayland backend.
//!
//! # Layering
//!
//! | Module | Responsibility | Event-loop free? |
//! | --- | --- | --- |
//! | [`connection`] | `wayland_client::Connection` wrapper | yes |
//! | [`registry`] | Global list snapshot | yes |
//! | [`shell`] | Protocol state + surfaces + input | yes |
//! | [`event_map`] | Map shell events → public [`crate::Event`] | yes |
//! | [`pump`] | Sync flush/read/dispatch helpers | yes |
//! | [`runtime_facade`] | Compio waits + public `NativeRuntime` | **no** (`feature = "compio"`) |
//! | [`protocols`] | Protocol class matrix / docs | yes |
//!
//! Protocol consumers that bring their own loop only need
//! [`NativeShell`] + [`NativeShell::try_read_and_dispatch`] (or
//! [`NativePump`]). Enable `feature = "compio"` for
//! [`NativeRuntime::dispatch`] and async `pump_once`.

mod connection;
mod event_map;
pub mod protocols;
mod pump;
mod registry;
mod shell;

#[cfg(feature = "compio")]
mod runtime_facade;
#[cfg(feature = "compio")]
mod runtime_constraints;
#[cfg(feature = "compio")]
mod runtime_dmabuf;
#[cfg(feature = "compio")]
mod runtime_interaction;
#[cfg(feature = "compio")]
mod runtime_seats;
#[cfg(feature = "compio")]
mod runtime_transfer;

pub use connection::{NativeConnection, NativeError};
pub use event_map::{
    map_native_event, map_native_event_full, map_native_key_text, native_key_text_pressed,
    NativeEventMapState, SurfaceIdMap,
};
pub use protocols::{
    ProtocolClass, ProtocolSpec, FIKA_PROTOCOL_MATRIX, PROTOCOL_MATRIX, specs_in_class,
};
pub use pump::{NativePump, PumpStep};
pub use registry::{list_env_globals, GlobalAdvertisement, NativeRegistry};
pub use shell::{
    NativeCapabilities, NativePopupPositioner, NativeShell, NativeShellEvent, NativeSurfaceHandle,
    NativeSurfaceId,
};

#[cfg(feature = "compio")]
pub use runtime_facade::NativeRuntime;
