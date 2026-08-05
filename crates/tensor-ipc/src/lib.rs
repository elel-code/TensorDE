//! Lightweight, value-only IPC shared by Tensor products and `tensor-msg`.
//!
//! Product server policy remains in the owning application. This crate owns
//! only wire values, bounded codecs, endpoint resolution, and reusable client
//! behavior; it never owns Wayland, renderer, ECS, DRM, or authentication state.

pub mod land;
pub mod wallpaper;
