//! Map [`NativeShellEvent`] toward the public [`crate::Event`] model.
//!
//! Structural bridge for migrating Fika off SCTK `Runtime`. With a live seat
//! proxy and focus tracking, keyboard / pointer / gesture events map fully.

mod gestures;
mod helpers;
mod ids;
mod input;
mod map;
mod surface;
mod system;
mod transfer;

#[cfg(test)]
mod tests;

pub use helpers::{map_native_key_text, native_key_text_pressed};
pub use ids::{NativeEventMapState, SurfaceIdMap};
pub use map::{map_native_event, map_native_event_full};
