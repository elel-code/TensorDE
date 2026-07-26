//! **Unstable** — `wayland-protocols` `unstable/` (legacy `zwp_*`).
//!
//! Prefer **staging** replacements when available. Kept only for protocols
//! Fika still needs that have not fully moved (text-input-v3, pointer
//! constraints/relative/gestures as currently deployed).
//!
//! - `text_input_v3` — wired in [`crate::native::NativeShell`]
//! - `pointer_gestures` — swipe / pinch / hold on `NativeShell`
//! - `relative_pointer` — opt-in via `enable_relative_pointer`
//! - `xdg_decoration` — `zxdg_decoration_manager_v1` server/client mode
//! - Planned: `pointer_constraints`
