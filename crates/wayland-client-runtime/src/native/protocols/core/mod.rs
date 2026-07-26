//! **Core** — `wayland.xml` (`wl_*`).
//!
//! Always-on foundation: compositor, subcompositor, shm, seat, data device,
//! output. Native shell cannot start without these globals.
//!
//! - `shm` — memfd pools + ARGB8888 solid buffers (used by [`crate::native::NativeShell`])
//! - Planned: richer compositor/seat/data_device/output modules

pub mod shm;
