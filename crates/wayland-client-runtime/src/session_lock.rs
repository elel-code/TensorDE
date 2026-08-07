use crate::{LogicalSize, OutputId, SurfaceId};

/// Lifecycle state of the connection's single `ext-session-lock-v1` request.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SessionLockState {
    #[default]
    Unlocked,
    Pending,
    Locked,
    Finished,
}

/// Events emitted by `ext-session-lock-v1`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionLockEvent {
    /// A role-less GPU surface was created for a current or hotplugged output.
    SurfaceAdded {
        surface: SurfaceId,
        output: OutputId,
    },
    /// A lock surface is configured and its configure serial has been acked.
    /// The caller may now create or resize its Vulkan swapchain and present.
    Configure {
        surface: SurfaceId,
        output: OutputId,
        size: LogicalSize,
        serial: u32,
    },
    /// The corresponding output disappeared and the protocol surface was torn down.
    SurfaceRemoved {
        surface: SurfaceId,
        output: OutputId,
    },
    /// Every output has presented a protected frame; unlocking is now legal.
    Locked,
    /// The compositor will no longer use this lock request.
    Finished { was_locked: bool },
}
