//! Usable native shell: core + stable xdg + staging scale + seat pointer/keyboard.
//!
//! No SCTK. Compio drives the display pump. Events land in an owned queue for
//! linear async consumers.

mod api;
mod api_surface;
mod api_transfer;
mod dispatch;
mod dispatch_activation;
mod dispatch_data;
mod dispatch_gestures;
mod dispatch_layer;
mod dispatch_relative;
mod dispatch_text;
mod handle;
mod types;

pub use api::NativeShell;
pub use handle::NativeSurfaceHandle;
pub use types::{
    NativeCapabilities, NativePopupPositioner, NativeShellEvent, NativeSurfaceId,
};
