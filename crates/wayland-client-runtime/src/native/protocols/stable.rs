//! **Stable** — `wayland-protocols` `stable/`.
//!
//! Production-ready FDO protocols. Missing globals are unusual on modern
//! compositors; treat as hard errors for a desktop shell.
//!
//! Wired in [`crate::native::NativeShell`]:
//! - `xdg_wm_base` — toplevel / popup / dialog
//! - `wp_viewporter` — fractional-scale destination
//! - `wp_presentation` — presentation-time feedback
//!   ([`NativeShell::request_presentation_feedback`])
//! - `zwp_linux_dmabuf_v1` — GPU zero-copy buffers + feedback
//!   ([`NativeShell::create_dmabuf_buffer`], feedback APIs)
