//! A Wayland-native client runtime.
//!
//! The crate deliberately exposes Wayland concepts instead of reproducing a
//! cross-platform window API. Protocol objects and their parent/child ordering
//! are owned by [`Runtime`]; renderers receive [`SurfaceHandle`] values that
//! implement raw-window-handle 0.6 for both wgpu and direct Vulkan use.
//!
//! **Migration:** the long-term backend is Compio + a native protocol stack
//! (see `ARCHITECTURE.md`). Phase 1 exposes Compio display readiness while
//! protocol handling still uses SCTK/calloop for compatibility.

mod activation;
mod blur;
pub mod clipboard;
pub mod data_transfer;
mod display_io;
mod dnd;
mod event;
mod fractional_scale;
mod geometry;
mod input;
mod layer_shell;
mod native;
mod output;
mod pointer_axis;
mod pointer_constraints;
mod pointer_gestures;
mod runtime;
mod shm_format;
mod surface;
mod text_input;
mod toplevel_icon;
mod toplevel_interaction;
mod touch;

pub use activation::{
    ActivationEvent, ActivationRequestId, ActivationToken, ActivationTokenAttributes,
};
pub use blur::{BlurRegion, BlurState};
pub use data_transfer::{MimePayload, TransferContent, TransferError, TransferReadPipe};
pub use display_io::DisplayReadiness;
pub use native::{
    list_env_globals, GlobalAdvertisement, NativeConnection, NativeError, NativePump,
    NativeRegistry, ProtocolClass, ProtocolSpec, PumpStep, FIKA_PROTOCOL_MATRIX, specs_in_class,
};
pub use dnd::{
    DndAction, DndActions, DndEvent, DndIcon, DndMimePayload, DndOfferId, DndReadPipe, DndSourceId,
};
pub use event::{
    Event, KeyState, KeyboardEvent, Modifiers, PointerEvent, PointerEventKind, PopupConfigureKind,
    SurfaceEvent, ToplevelState, TouchEvent, TouchEventKind,
};
pub use geometry::{LogicalPosition, LogicalRect, LogicalSize, SuggestedSize};
pub use input::{CursorIcon, InputSerial, InputSerialSource};
pub use layer_shell::{
    LayerAnchor, LayerEdge, LayerKeyboardInteractivity, LayerMargins, LayerSurfaceAttributes,
    LayerSurfaceError, LayerSurfaceEvent, LayerSurfaceLayer, LayerSurfaceState,
};
pub use output::{OutputEvent, OutputId, OutputInfo};
pub use pointer_axis::{PointerAxisDirection, PointerAxisSource, PointerAxisValue};
pub use pointer_constraints::{
    PointerCaptureState, PointerConstraint, PointerConstraintError, PointerConstraintEvent,
    PointerConstraintRegion, RelativePointerEvent,
};
pub use pointer_gestures::{
    PointerGestureEvent, PointerHoldEvent, PointerPinchEvent, PointerSwipeEvent,
};
pub use runtime::{Runtime, RuntimeCapabilities, RuntimeError, RuntimeOptions, WakeHandle};
pub use surface::{
    ConstraintAdjustments, DecorationPreference, DialogAttributes, Gravity, PopupAnchor,
    PopupAttributes, PopupPositioner, SurfaceHandle, SurfaceId, SurfaceKind, ToplevelAttributes,
};
pub use text_input::{
    TextInputChangeCause, TextInputContentHint, TextInputContentPurpose, TextInputContentType,
    TextInputDeleteSurrounding, TextInputDone, TextInputError, TextInputEvent, TextInputPreedit,
    TextInputState, TextInputSurroundingText,
};
pub use toplevel_icon::{ToplevelIcon, ToplevelIconBuffer, ToplevelIconError};
pub use toplevel_interaction::ResizeEdge;
