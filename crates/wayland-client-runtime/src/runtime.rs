use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::activation::{ActivationHandler, ActivationManager, ActivationTokenPurpose};
use crate::data_transfer::{TransferContent, TransferReadPipe};
use crate::dnd::{DndAction, DndActions, DndEvent, DndIcon, DndOfferId, DndReadPipe, DndSourceId};
use crate::event::{
    Event, EventBuffer, KeyState, KeyboardEvent, Modifiers, PointerEvent, PointerEventKind,
    PopupConfigureKind, SurfaceEvent, ToplevelState, TouchEvent, TouchEventKind,
};
use crate::fractional_scale::{FractionalScaleHandler, FractionalScaleManager};
use crate::input::{InputSerial, InputSerialSource};
use crate::layer_shell::{
    LayerProtocolEvent, LayerShellManager, LayerSurfaceAttributes, LayerSurfaceData,
    LayerSurfaceError, LayerSurfaceEvent, LayerSurfaceState, handle_layer_event,
};
use crate::output::output_info;
use crate::pointer_axis::{map_axis_source, map_axis_value};
use crate::pointer_constraints::{
    PointerCaptureTarget, PointerProtocols, SeatPointerSession, validate_pointer_capture_state,
};
use crate::pointer_gestures::{
    GestureSubscriptionChange, PointerGestureHandler, PointerGestureManager,
    PointerGestureSubscriptions, SeatPointerGestures,
};
use crate::shm_format::copy_rgba_to_premultiplied_argb8888;
use crate::surface::{
    DecorationPreference, Gravity, ManagedBlur, PopupAnchor, PopupPositioner, ProtocolSurface,
    SurfaceHandle, SurfaceId, SurfaceKind, SurfaceShared,
};
use crate::toplevel_icon::ToplevelIconManager;
use crate::text_input::{PendingBatch, SeatTextInput, TextInputHandler, TextInputManager};
use crate::toplevel_interaction::{
    PointerPressTracker, ToplevelInteraction, select_active_pointer_press,
};
use crate::touch::{TouchData, TouchHandler, TouchPoints};
use crate::{
    ActivationEvent, ActivationRequestId, ActivationToken, ActivationTokenAttributes, BlurRegion,
    BlurState, CursorIcon, DialogAttributes, LogicalPosition, LogicalSize, OutputEvent, OutputId,
    OutputInfo, PointerCaptureState, PointerConstraint, PointerConstraintError,
    PointerConstraintRegion, PointerGestureEvent, PopupAttributes, RelativePointerEvent,
    ResizeEdge, SuggestedSize, TextInputEvent, TextInputState, ToplevelAttributes, ToplevelIcon,
};
use smithay_client_toolkit::background_effect::{
    BackgroundEffectHandler, BackgroundEffectState,
};
use smithay_client_toolkit::compositor::{
    CompositorHandler, CompositorState, FrameCallbackData, Region,
};
use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::{
    CopyPasteSource, DataSourceHandler, DragSource,
};
use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, WritePipe};
use smithay_client_toolkit::error::GlobalError;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::{EventLoop as CalloopEventLoop, LoopSignal};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use wayland_client::backend::ObjectId;
use wayland_client::globals::{GlobalList, registry_queue_init};
use wayland_client::protocol::wl_data_device_manager::DndAction as WlDndAction;
use wayland_client::protocol::{
    wl_data_device, wl_data_source, wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface,
    wl_touch,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::shell::client::{
    xdg_positioner, xdg_toplevel,
};
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_confined_pointer_v1::ZwpConfinedPointerV1;
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_locked_pointer_v1::ZwpLockedPointerV1;
use wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::ZwpRelativePointerV1;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ZwpTextInputV3;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::Capability as BackgroundEffectCapability;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardData, KeyboardHandler, Modifiers as SctkModifiers, RawModifiers,
};
use smithay_client_toolkit::seat::pointer::{
    CursorIcon as SctkCursorIcon, PointerData, PointerEvent as SctkPointerEvent,
    PointerEventKind as SctkPointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
};
use smithay_client_toolkit::seat::pointer_constraints::PointerConstraintsHandler;
use smithay_client_toolkit::seat::relative_pointer::{
    RelativeMotionEvent, RelativePointerHandler,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::dialog::{Dialog, DialogHandler};
use smithay_client_toolkit::shell::xdg::popup::{
    ConfigureKind, Popup, PopupConfigure, PopupHandler,
};
use smithay_client_toolkit::shell::xdg::window::{
    Window, WindowConfigure, WindowDecorations, WindowHandler,
};
use smithay_client_toolkit::shell::xdg::{XdgPositioner, XdgShell};
use smithay_client_toolkit::shm::slot::{Buffer as ShmBuffer, SlotPool};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};

include!("runtime/types.rs");
include!("runtime/api.rs");
include!("runtime/api_private.rs");
include!("runtime/runtime_data_transfer.rs");
include!("runtime/runtime_helpers.rs");
include!("runtime/free_helpers.rs");
include!("runtime/state.rs");
include!("runtime/handlers_shell.rs");
include!("runtime/handlers_seat.rs");
include!("runtime/handlers_data_device.rs");
include!("runtime/handlers_protocol.rs");
include!("runtime/runtime_tests.rs");
