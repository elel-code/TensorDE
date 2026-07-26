//! Value-only present scheduling helpers.
//!
//! Tracks per-output redraw / slot readiness **without** DRM or Vulkan handles.
//! The Smithay (or future native) KMS path maps [`PresentIntent`] to real
//! atomic commits. This keeps page-flip policy testable offline.

mod readiness;
mod schedule;

pub use readiness::{OutputReadiness, SlotReadiness};
pub use schedule::{PresentQueue, PresentQueueStats, QueueError};
pub use tensor_host::{PresentIntent, PresentSlot, PresentState};
