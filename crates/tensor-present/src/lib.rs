//! Retained present scheduling and strict Wayland/Vulkan surface helpers.
//!
//! Tracks per-output redraw / slot readiness **without** DRM or Vulkan handles.
//! Tensor's native KMS path maps [`PresentIntent`] to real
//! atomic commits. Ordinary application surfaces use the retained Vulkan
//! presenter below, while page-flip policy stays testable offline.

mod readiness;
mod schedule;
mod wayland;

pub use readiness::{OutputReadiness, SlotReadiness};
pub use schedule::{PresentQueue, PresentQueueStats, QueueError};
pub use tensor_host::{PresentIntent, PresentSlot, PresentState};
pub use wayland::{ColorRect, SurfaceFrame, SurfacePresenter, SurfacePresenterError};
