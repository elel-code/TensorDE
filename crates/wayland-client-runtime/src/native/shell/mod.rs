//! Usable native shell: core + stable xdg + staging scale + seat pointer/keyboard.
//!
//! No SCTK. Compio drives the display pump. Events land in an owned queue for
//! linear async consumers.

mod api;
mod dispatch;
mod handle;
mod types;

pub use api::NativeShell;
pub use handle::NativeSurfaceHandle;
pub use types::{NativeCapabilities, NativeShellEvent, NativeShellState, NativeSurfaceId};
