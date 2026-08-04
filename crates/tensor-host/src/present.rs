//! Present / scanout intents without owning KMS objects.

use crate::connector::ConnectorId;

/// Triple-buffer / swapchain slot index for one CRTC.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct PresentSlot(pub u8);

/// Lifecycle of a present slot (policy-visible; FDs stay in the adapter).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PresentState {
    #[default]
    Idle,
    Queued,
    WaitingForVBlank,
    Presented,
    Faulted,
}

/// Atomic KMS presentation mode selected by compositor policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PresentMode {
    #[default]
    Vsync,
    Async,
}

/// Value-only request to present a prepared framebuffer on a connector.
///
/// The adapter resolves `serial` / `slot` to real GBM/DRM objects. Policy and
/// the event bus only see this struct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentIntent {
    pub output: ConnectorId,
    pub slot: PresentSlot,
    pub serial: u64,
    /// Timeline value exported from Vulkan (binary SYNC_FD is adapter-side).
    pub timeline_value: u64,
    pub mode: PresentMode,
}

impl PresentIntent {
    #[inline]
    pub const fn new(
        output: ConnectorId,
        slot: PresentSlot,
        serial: u64,
        timeline_value: u64,
    ) -> Self {
        Self {
            output,
            slot,
            serial,
            timeline_value,
            mode: PresentMode::Vsync,
        }
    }

    #[inline]
    pub const fn with_mode(mut self, mode: PresentMode) -> Self {
        self.mode = mode;
        self
    }
}
