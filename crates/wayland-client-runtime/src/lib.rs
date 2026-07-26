//! A Wayland-native client protocol stack (optional Compio event loop).
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │ Public types (Event, SurfaceId, TransferContent, …)      │
//! ├──────────────────────────────────────────────────────────┤
//! │ Protocol layer (always available)                        │
//! │   NativeShell · NativeConnection · NativePump            │
//! │   plain non-blocking display_fd()                        │
//! │   try_read_and_dispatch / dispatch_pending / drain_*     │
//! ├──────────────────────────────────────────────────────────┤
//! │ Compio adapter (feature = "compio", default)             │
//! │   NativeRuntime · CompioFdReady (readiness only)         │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! Protocol I/O always uses a **normal non-blocking Wayland fd**. Compio (or
//! any other runtime) only waits for readiness; it does not replace `read`.
//!
//! ## Using without Compio
//!
//! ```toml
//! wayland-client-runtime = { version = "0.1", default-features = false }
//! ```
//!
//! ```no_run
//! use wayland_client_runtime::NativeShell;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut shell = NativeShell::connect_to_env()?;
//! // Register shell.display_fd() (non-blocking) with your reactor.
//! loop {
//!     shell.try_read_and_dispatch()?;
//!     for event in shell.drain_events() {
//!         let _ = event;
//!     }
//! }
//! # }
//! ```
//!
//! ## Using with Compio (default)
//!
//! [`NativeRuntime`] keeps long-lived [`CompioFdReady`] watches on the display
//! and wake fds, then calls the same protocol read path. Fika uses this.
//! Renderers take [`SurfaceHandle`] (raw-window-handle 0.6).

mod activation {
    use crate::{InputSerial, SurfaceId};

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub struct ActivationRequestId(pub(crate) u64);
    impl ActivationRequestId {
        pub const fn get(self) -> u64 {
            self.0
        }
    }

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct ActivationToken(String);
    impl ActivationToken {
        pub fn from_raw(token: String) -> Self {
            Self(token)
        }
        pub fn as_raw(&self) -> &str {
            &self.0
        }
        pub fn into_raw(self) -> String {
            self.0
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct ActivationTokenAttributes {
        pub app_id: Option<String>,
        pub serial: Option<InputSerial>,
    }

    #[derive(Clone, Debug)]
    pub enum ActivationEvent {
        TokenDone {
            request: ActivationRequestId,
            requesting_surface: SurfaceId,
            token: ActivationToken,
        },
    }
}
mod blur;
pub mod clipboard;
pub mod data_transfer;
#[cfg(feature = "compio")]
mod display_io;
mod dnd;
mod event;
mod geometry;
mod input;
mod layer_shell;
mod native;
mod output;
mod pointer_axis;
mod pointer_constraints;
mod pointer_gestures;
mod runtime_common;
mod shm_format;
mod surface;
mod text_input;
mod toplevel_icon;
mod toplevel_interaction;
#[cfg(feature = "compio")]
mod wake_fd;

pub use activation::{
    ActivationEvent, ActivationRequestId, ActivationToken, ActivationTokenAttributes,
};
pub use blur::{BlurRegion, BlurState};
pub use data_transfer::{MimePayload, TransferContent, TransferError, TransferReadPipe};
#[cfg(feature = "compio")]
pub use display_io::{CompioFdReady, DisplayReadiness};
pub use native::{
    list_env_globals, map_native_event, map_native_event_full, map_native_key_text,
    native_key_text_pressed, GlobalAdvertisement, NativeCapabilities, NativeConnection,
    NativeError, NativeEventMapState, NativePopupPositioner, NativePump, NativeRegistry,
    NativeShell, NativeShellEvent, NativeSurfaceHandle, NativeSurfaceId, ProtocolClass,
    ProtocolSpec, PumpStep, SurfaceIdMap, FIKA_PROTOCOL_MATRIX, specs_in_class,
};
#[cfg(feature = "compio")]
pub use native::NativeRuntime;
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
pub use runtime_common::{RuntimeCapabilities, RuntimeError, RuntimeOptions};
#[cfg(feature = "compio")]
pub use runtime_common::WakeHandle;
/// Production runtime (Compio + native shell). Available with `feature = "compio"`.
#[cfg(feature = "compio")]
pub type Runtime = NativeRuntime;
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
