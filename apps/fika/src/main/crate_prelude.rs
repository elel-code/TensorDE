use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::env;
use std::error::Error;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, OnceLock,
    mpsc::{self, Receiver, Sender},
};
use std::thread;
use std::time::{Duration, Instant};
use cosmic_text::{
    Align, Attrs, Buffer, Color as TextColor, Cursor, Family, FontSystem, Metrics, Shaping,
    Stretch, Style, SwashCache, Weight, Wrap, fontdb,
};
#[cfg(test)]
use fika_core::{
    PrivilegedCommand, ServiceMenuPriority, paste_text_result, trash_view_operation_result,
};
use fika_core::{
    AppSettings, CompactLayout, CompactLayoutOptions, DeviceInfo, DevicePlaceOperation,
    DevicePlaceOperationResult, Entry, FileClipboardRole, FileTransferMode, Generation,
    IconsLayout, IconsLayoutOptions, ItemId, ItemLayout, MetadataRoleResult, MimeApplication,
    MimeApplicationCache, MimeDatabase, NETWORK_ROOT_LABEL, NameFilter, OperationController,
    ServiceMenuAction, ServiceMenuTarget, ThumbnailRequest, ThumbnailRequestPriority,
    ThumbnailerRegistry, TrashViewOperation, TrashViewOperationResult, UserPlace, ViewPoint,
    ViewRect, complete_location_input, decode_file_clipboard_text, default_app_settings_path,
    default_thumbnail_cache_root, default_user_places_path, encode_file_clipboard_text, file_ops,
    format_modified_secs, format_size,
    generate_thumbnail_with_external_thumbnailer_registry_size, ThumbnailSize,
    home_dir, is_network_path, launch_with_systemd_user, load_app_settings, load_place_order,
    load_user_places, mime_magic_resolution_required, network_parent_path,
    network_path_display_name, network_path_from_uri, network_root_path, OpenWithLaunchResult,
    OperationRuntimeError, path_uri_from_path, place_order_path_for_user_places_path,
    read_entries_sync, read_gio_devices, read_network_entry_batches_sync_cancellable,
    resolve_location_input, save_app_settings, save_place_order, save_user_places,
    ServiceMenuLaunchResult, service_menu_target_label, spawn_blocking_operation_with_completion,
    spawn_operation_task_with_completion, thumbnail_request_may_have_preview,
    trash_view_operation_result_async, perform_device_place_operation,
};
use windowing::{
    ActiveEventLoop, ApplicationHandler, AsyncRequestSerial, ControlFlow, CursorIcon,
    DataTransferId, ElementState, EventLoop, EventLoopProxy, ImeChangeCause, ImeCursorArea,
    ImeEvent, ImeState, Key, Modifiers, MouseButton, MouseScrollDelta, NamedKey, PhysicalPosition,
    PhysicalSize, Theme, Window, WindowAttributes, WindowEvent, WindowId,
};
#[cfg(test)]
use windowing::{KeyCode, NativeKey, NativeKeyCode, PhysicalKey};
use wayland_client_runtime::{TextInputContentHint, TextInputContentPurpose};
macro_rules! fika_log {
    ($($arg:tt)*) => {{
        if crate::fika_log_enabled() {
            eprintln!($($arg)*);
        }
    }};
}
macro_rules! fika_dialog_trace {
    ($($arg:tt)*) => {{
        if crate::fika_dialog_trace_enabled() {
            eprintln!($($arg)*);
        }
    }};
}
fn env_flag_enabled(name: &str) -> bool {
    env::var_os(name).is_some_and(|value| {
        let value = value.to_string_lossy();
        let value = value.trim().to_ascii_lowercase();
        !matches!(value.as_str(), "" | "0" | "false" | "no" | "off")
    })
}
fn fika_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_flag_enabled("FIKA_LOG"))
}
fn fika_dialog_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        env_flag_enabled("FIKA_DIALOG_TRACE") || env_flag_enabled("FIKA_LOG")
    })
}
fn fika_dialog_trace_verbose_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_flag_enabled("FIKA_DIALOG_TRACE_VERBOSE"))
}
fn fika_frame_log_all_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_flag_enabled("FIKA_FRAME_LOG_ALL"))
}
fn dialog_lifecycle_autosmoke_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_flag_enabled("FIKA_AUTOSMOKE_DIALOG_LIFECYCLE"))
}
fn window_event_label(event: &WindowEvent) -> &'static str {
    match event {
        WindowEvent::SurfaceResized(_) => "SurfaceResized",
        WindowEvent::CloseRequested => "CloseRequested",
        WindowEvent::DragEntered { .. } => "DragEntered",
        WindowEvent::DragPosition { .. } => "DragPosition",
        WindowEvent::DragDropped { .. } => "DragDropped",
        WindowEvent::DragLeft { .. } => "DragLeft",
        WindowEvent::DataTransferReceived { .. } => "DataTransferReceived",
        WindowEvent::OutgoingDragDropped { .. } => "OutgoingDragDropped",
        WindowEvent::OutgoingDragCanceled { .. } => "OutgoingDragCanceled",
        WindowEvent::KeyboardInput { .. } => "KeyboardInput",
        WindowEvent::Ime(_) => "Ime",
        WindowEvent::ModifiersChanged(_) => "ModifiersChanged",
        WindowEvent::PointerMoved { .. } => "PointerMoved",
        WindowEvent::PointerLeft { .. } => "PointerLeft",
        WindowEvent::MouseWheel { .. } => "MouseWheel",
        WindowEvent::PinchGesture(_) => "PinchGesture",
        WindowEvent::SwipeGesture(_) => "SwipeGesture",
        WindowEvent::PointerButton { .. } => "PointerButton",
        WindowEvent::ScaleFactorChanged { .. } => "ScaleFactorChanged",
        WindowEvent::RedrawRequested => "RedrawRequested",
    }
}
fn window_event_trace_is_high_volume(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::PointerMoved { .. } | WindowEvent::DragPosition { .. }
    )
}
