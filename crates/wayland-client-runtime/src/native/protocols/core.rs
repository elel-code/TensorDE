//! **Core** — `wayland.xml` (`wl_*`).
//!
//! Always-on foundation: compositor, subcompositor, shm, seat, data device,
//! output. Native shell cannot start without these globals.
//!
//! - `shm` — memfd pools + ARGB8888 solid buffers (used by [`crate::native::NativeShell`])
//! - `xkb_state` — libxkbcommon keymap → UTF-8 / keysym for `wl_keyboard`
//! - Planned: richer compositor/seat/data_device/output modules

pub mod shm;
pub mod xkb_state;

pub use xkb_state::NativeXkb;
