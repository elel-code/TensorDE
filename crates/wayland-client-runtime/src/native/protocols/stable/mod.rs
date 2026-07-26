//! **Stable** — `wayland-protocols` `stable/`.
//!
//! Production-ready FDO protocols. Missing globals are unusual on modern
//! compositors; treat as hard errors for a desktop shell.
//!
//! - xdg-shell is wired in [`crate::native::NativeShell`] (toplevel create +
//!   configure/close events). Popup/positioner modules will split out here.
//! - Planned: `viewporter`, `presentation`

// Implementation currently lives in `native/shell.rs` for a cohesive first
// usable surface; further splits land under this tree as the surface grows.
