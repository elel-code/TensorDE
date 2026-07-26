//! Backend-agnostic runtime types.

use crate::dnd::DndOfferId;
use crate::layer_shell::LayerSurfaceError;
use crate::output::OutputId;
use crate::pointer_constraints::PointerConstraintError;
use crate::surface::SurfaceId;

#[derive(Clone, Debug)]
pub struct RuntimeOptions {
    /// Initial capacity for the owned event batch.
    pub event_capacity: usize,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            event_capacity: 128,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub xdg_dialog_v1: bool,
    pub xdg_activation_v1: bool,
    pub xdg_toplevel_icon_v1: bool,
    pub layer_shell_v1: bool,
    pub layer_shell_dynamic_layer: bool,
    pub layer_shell_on_demand_keyboard: bool,
    pub layer_shell_exclusive_edge: bool,
    pub text_input_v3: bool,
    pub pointer_constraints_v1: bool,
    pub relative_pointer_v1: bool,
    pub pointer_gestures_v1: bool,
    pub pointer_gesture_hold_v1: bool,
    pub popup_reposition: bool,
    pub ext_background_effect: bool,
    pub fractional_scale: bool,
    pub cursor_shape: bool,
    /// `wp_presentation` (stable presentation-time feedback).
    pub presentation: bool,
    /// `zwp_primary_selection_device_manager_v1` (middle-click paste).
    pub primary_selection: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("failed to connect to the Wayland compositor: {0}")]
    Connect(String),
    #[error("failed to initialize the Wayland registry: {0}")]
    Registry(String),
    #[error("required Wayland global is unavailable: {0}")]
    MissingGlobal(String),
    #[error("failed to initialize the event loop: {0}")]
    EventLoop(String),
    #[error("surface {0:?} does not exist")]
    SurfaceNotFound(SurfaceId),
    #[error("surface {0:?} cannot be used as a parent for this role")]
    InvalidParent(SurfaceId),
    #[error("popup positioner is invalid: {0}")]
    InvalidPositioner(&'static str),
    #[error("popup grabs require a pointer-press or touch-down serial")]
    InvalidPopupGrab,
    #[error("popup grab serial belongs to another Wayland connection or is no longer current")]
    ForeignOrStalePopupGrab,
    #[error("activation serial belongs to another Wayland connection")]
    ForeignActivationSerial,
    #[error("surface {0:?} is not an activatable toplevel")]
    InvalidActivationTarget(SurfaceId),
    #[error("surface {0:?} cannot have an xdg toplevel icon")]
    InvalidToplevelIconTarget(SurfaceId),
    #[error("surface {0:?} is not an xdg toplevel")]
    InvalidToplevelInteractionTarget(SurfaceId),
    #[error("toplevel interaction requires a focused pointer seat with a pressed button")]
    InvalidToplevelInteractionSerial,
    #[error("surface {0:?} is not a layer surface")]
    InvalidLayerSurfaceTarget(SurfaceId),
    #[error("output {0:?} is no longer available")]
    OutputNotFound(OutputId),
    #[error("invalid layer surface: {0}")]
    InvalidLayerSurface(#[from] LayerSurfaceError),
    #[error("surface {0:?} does not support xdg window geometry")]
    InvalidWindowGeometryTarget(SurfaceId),
    #[error("drag origin has no focused pointer seat with a current button serial")]
    InvalidDragSerial,
    #[error("clipboard selection has no focused seat with a current input serial")]
    InvalidSelectionSerial,
    #[error("clipboard selection is unavailable")]
    SelectionUnavailable,
    #[error("clipboard selection has none of the requested MIME types")]
    SelectionMimeNotFound,
    #[error("DnD offer {0:?} does not exist")]
    DndOfferNotFound(DndOfferId),
    #[error("the compositor does not support {0}")]
    Unsupported(&'static str),
    #[error("surface {0:?} has not requested a locked pointer")]
    PointerNotLocked(SurfaceId),
    #[error("invalid pointer constraint: {0}")]
    InvalidPointerConstraint(#[from] PointerConstraintError),
    #[error("Wayland protocol operation failed: {0}")]
    Protocol(String),
}

/// Thread-safe handle for interrupting a Compio-driven native wait.
///
/// Available when the `compio` feature is enabled (default). Protocol-only
/// embeds use their own wake mechanism.
#[cfg(feature = "compio")]
#[derive(Clone, Debug)]
pub struct WakeHandle(std::sync::Arc<crate::wake_fd::EventFdWake>);

#[cfg(feature = "compio")]
impl WakeHandle {
    pub(crate) fn from_event_fd(wake: std::sync::Arc<crate::wake_fd::EventFdWake>) -> Self {
        Self(wake)
    }

    pub fn wake(&self) {
        self.0.wake();
    }
}
