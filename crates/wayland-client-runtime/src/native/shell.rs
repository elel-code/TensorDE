//! Usable native shell: core + stable xdg + staging scale + seat pointer/keyboard.
//!
//! No SCTK. Compio drives the display pump. Events land in an owned queue for
//! linear async consumers.

mod api;
mod api_activation;
mod api_chrome;
mod api_constraints;
mod api_csd;
mod api_dmabuf;
mod api_dnd;
mod api_foreign;
mod api_idle_notify;
mod api_interaction;
mod api_layer;
mod api_output;
mod api_output_power;
mod api_popup;
mod api_presentation;
mod api_surface;
mod api_text_input;
mod api_transfer;
mod csd;
mod dispatch;
mod dispatch_activation;
mod dispatch_chrome;
mod dispatch_constraints;
mod dispatch_data;
mod dispatch_decoration;
mod dispatch_dialog;
mod dispatch_dmabuf;
mod dispatch_foreign;
mod dispatch_gestures;
mod dispatch_idle;
mod dispatch_idle_notify;
mod dispatch_layer;
mod dispatch_output;
mod dispatch_output_power;
mod dispatch_presentation;
mod dispatch_primary;
mod dispatch_relative;
mod dispatch_seat;
mod dispatch_text;
mod handle;
mod seat;
mod types;

pub use api::NativeShell;
pub use api_idle_notify::IdleNotifyKind;
pub use handle::NativeSurfaceHandle;
pub use types::{NativeCapabilities, NativePopupPositioner, NativeShellEvent, NativeSurfaceId};
