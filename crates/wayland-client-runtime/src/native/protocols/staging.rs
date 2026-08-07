//! **Staging** — `wayland-protocols` `staging/`.
//!
//! Still evolving; bind with version caps and capability flags. Preferred over
//! unstable for new features.
//!
//! - `fractional_scale` — wired in [`crate::native::NativeShell`] with
//!   viewporter destination + `buffer_scale = 1`
//! - `cursor_shape` — `NativeShell::set_cursor_shape`
//! - `xdg_activation` — `request_activation_token` / `activate_with_token`
//! - `xdg_dialog` — `create_dialog_gpu` + `set_dialog_modal` when global present
//! - `xdg_toplevel_icon` — named + pixel SHM icons via `set_toplevel_icon`
//! - `ext_background_effect` — blur via `set_blur` when capability advertised
//! - `ext_session_lock` — secure per-output lock surfaces and explicit unlock
