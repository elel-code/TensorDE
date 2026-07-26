//! **Core** — `wayland.xml` (`wl_*`).
//!
//! Always-on foundation: compositor, subcompositor, shm, seat, data device,
//! output. Native shell cannot start without these globals.
//!
//! Planned modules (Phase 2b+):
//! - `compositor` — `wl_compositor` / `wl_surface`
//! - `shm` — `wl_shm` / pools / buffers
//! - `seat` — `wl_seat`, pointer, keyboard, touch + xkb
//! - `data_device` — clipboard + dnd (shared MIME model with public API)
//! - `output` — `wl_output` geometry / scale fallback

// Placeholders keep the tree intentional; implementations land per global.
