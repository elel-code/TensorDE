mod core;

pub const CHOOSER_CANCEL_EXIT_CODE: i32 = 75;

pub use core::archive::{
    ARK_DND_EXTRACT_INTERFACE, ARK_DND_EXTRACT_METHOD, ARK_DND_EXTRACT_PATH_MIME,
    ARK_DND_EXTRACT_SERVICE_MIME, ArkDndExtractError, ArkDndExtractPayload, ArkDndExtractRequest,
    ark_dnd_extract_payload, ark_dnd_extract_request, execute_ark_dnd_extract,
    execute_ark_dnd_extract_with_bus, is_archive_mime_or_path,
};
pub use core::bus::{BusCallTarget, BusConfig, BusController, BusError, BusKind};
pub use core::cache::{
    DirectoryCache, DirectoryCacheDebugSnapshot, DirectoryCacheDirectorySummary,
    DirectoryCacheLimits, DirectoryCacheSnapshot, DirectoryCacheStats, normalize_cache_path,
};
pub use core::clipboard::{
    FileClipboardPayload, FileClipboardRole, decode_file_clipboard_text, encode_file_clipboard_text,
};
pub use core::devices::{
    DeviceActionError, DeviceDiscoveryError, DeviceEvent, DeviceInfo, DeviceMonitorMessage,
    DeviceMountResult, DevicePlaceOperation, DevicePlaceOperationResult, device_events_between,
    eject_device, mount_device, perform_device_place_operation, read_devices, read_gio_devices,
    safely_remove_device, unmount_device, watch_devices,
};
pub use core::directory::{
    ClassifiedWatcherDelta, DirectoryLister, DirectoryListerEvent, LoadMode, RefreshPair,
    WatcherDelta, nearest_existing_ancestor,
};
pub use core::entries::{
    Entry, EntryData, EntryMetadataRole, ItemId, ModelEntry, format_modified_secs, format_size,
    format_trash_deletion_time, format_trash_original_location, read_entries_sync, read_entry_sync,
};
pub use core::file_ops;
pub use core::filter::{FilteredModel, NameFilter, NameFilterMode};
pub use core::launcher::{
    ArkCompressionMode, DesktopAction, DesktopApplication, DesktopLaunchCommand, DesktopLaunchPlan,
    DesktopServiceMenu, LauncherError, MimeApplication, MimeApplicationCache, MimeAppsList,
    MimeInfoCache, NewWindowLaunchResult, OpenWithLaunchResult, ServiceMenuAction,
    ServiceMenuLaunchResult, ServiceMenuPriority, ServiceMenuTarget, SystemdLaunchResult,
    SystemdLaunchUnit, ark_compress_launch_plan, ark_extract_and_trash_launch_plan,
    ark_extract_here_launch_plan, ark_extract_to_launch_plan, current_executable_launch_plan,
    default_mimeapps_list_path, launch_with_systemd_user, parse_mimeapps_list,
    parse_mimeinfo_cache, service_menu_target_label, set_default_mime_application,
    set_default_mime_application_at, set_default_mime_application_in_contents,
    systemd_launch_unit_name, systemd_units_for_launch_plan, terminal_launch_plan_for_directory,
};
pub use core::listing_worker::{
    ListingRequest, ListingRequestKey, ListingWorker, LoadingPaneState,
    listing_requests_from_events, update_loading_state_for_event,
};
pub use core::location::{
    BreadcrumbSegment, breadcrumb_segments, complete_location_input, expand_user_path, home_dir,
    normalize_start_dir, parent_location, resolve_location_input,
};
pub use core::metadata::{
    MetadataRoleBatch, MetadataRoleCandidate, MetadataRolePriority, MetadataRoleRequest,
    MetadataRoleResult, MetadataRoleScheduler, MetadataRoleWorkKey,
    apply_metadata_role_result_to_model, metadata_role_result_for_request,
    metadata_role_results_for_requests,
};
pub use core::mime::{
    GENERIC_BINARY_MIME, MimeDatabase, detect_mime_from_magic, generic_mime_icon_name,
    mime_icon_name, mime_magic_resolution_required,
};
pub use core::model::{
    ChangedRoles, DirectoryModel, DirectoryModelSignal, ItemRange, ItemRangeList, SortDescriptor,
    SortOrder, SortRole,
};
pub use core::network::{
    DOLPHIN_REMOTE_ROOT_URI, NETWORK_ROOT_ICON, NETWORK_ROOT_LABEL, NETWORK_ROOT_URI, NetworkAuth,
    NetworkFilesystemKind, NetworkLocation, NetworkScanError, NetworkUrlError,
    classify_network_filesystem, filesystem_type_is_remote, forget_network_auth, is_network_path,
    is_network_root_path, is_network_root_uri, is_supported_network_scheme, network_child_path,
    network_parent_path, network_path_display_name, network_path_from_uri, network_root_location,
    network_root_path, network_uri_from_path, normalize_network_uri, parse_network_location,
    read_network_entry_batches_sync_cancellable, remember_network_auth, supported_network_schemes,
};
pub use core::operation_runtime::{
    OperationController, OperationId, OperationRuntime, OperationRuntimeError, OperationSnapshot,
    run_blocking_operation, run_operation_blocking, run_operation_task, run_registered_operation,
    spawn_blocking_operation_with_completion, spawn_operation_task,
    spawn_operation_task_with_completion,
};
pub use core::operations::{
    AffectedDirectoryRefresh, CreateItemResult, CreateUndoItem, CreatedItemKind, FileTransferMode,
    Operation, OperationQueue, RenameItemResult, RenameUndoItem, TransferTaskResult,
    TransferUndoItem, TrashSelectionResult, TrashUndoItem, TrashViewOperation,
    TrashViewOperationResult, UndoPayload, UndoRecord, UndoSerial, UndoTaskResult, action_status,
    create_item_result, create_item_result_async, created_item_label, default_created_item_name,
    parent_dirs, paste_text_result, paste_text_result_async, push_unique_path, rename_item_result,
    rename_item_result_async, transfer_paths_result, transfer_paths_result_async,
    trash_selection_result, trash_selection_result_async, trash_view_operation_result,
    trash_view_operation_result_async, undo_record_result, undo_record_result_async,
};
pub use core::pane::{
    DEFAULT_ZOOM_LEVEL, Generation, MAX_ZOOM_LEVEL, MIN_ZOOM_LEVEL, PaneController,
    PaneGenerationCounter, PaneId, PaneIdAllocator, PaneState, RequestSerial, SelectionMove,
    SelectionState, ViewMode, ViewState, ZoomChange, icon_size_for_zoom_level,
    normalize_viewport_extent,
};
pub use core::places::{
    UserPlace, default_user_places_path, load_place_order, load_user_places, parse_place_order_xml,
    parse_user_places_xbel, place_order_path_for_user_places_path, place_order_xml,
    save_place_order, save_user_places, user_places_xbel,
};
pub use core::privilege::{
    HelperBus, PrivilegedCommand, PrivilegedOperationResult, run_dbus_service, run_via_dbus,
};
pub use core::settings::{
    AppSettings, AppearanceSettings, PlacesSidebarSettings, ViewSettings, app_settings_tsv,
    default_app_settings_path, load_app_settings, parse_app_settings, save_app_settings,
};
pub use core::thumbnails::{
    ExternalThumbnailerCommand, ThumbnailCacheHit, ThumbnailCachePaths, ThumbnailCandidate,
    ThumbnailMetadata, ThumbnailProbeBatch, ThumbnailProbeCancelHandle, ThumbnailProbeResult,
    ThumbnailRequest, ThumbnailRequestPriority, ThumbnailRequestQueue, ThumbnailScheduler,
    ThumbnailSize, ThumbnailWorkKey, ThumbnailerRegistry, apply_thumbnail_probe_result_to_model,
    cached_thumbnail_for_path, cached_thumbnail_for_request, cached_thumbnail_for_uri,
    default_thumbnail_cache_root, external_thumbnailer_commands_for_path,
    generate_thumbnail_with_external_thumbnailer,
    generate_thumbnail_with_external_thumbnailer_registry, record_thumbnail_failure,
    thumbnail_cache_key, thumbnail_cache_path, thumbnail_cache_paths_for_uri, thumbnail_cache_root,
    thumbnail_candidate_failure_is_cached, thumbnail_failure_is_cached, thumbnail_failure_path,
    thumbnail_metadata, thumbnail_probe_results_for_requests, thumbnail_request_may_have_preview,
    thumbnail_uri_for_path, write_thumbnail_metadata,
};
pub use core::trash_monitor::TrashEmptinessMonitor;
pub use core::uri::{file_uri_from_path, file_uri_to_path, path_uri_from_path};
pub use core::view::{
    CompactColumnMetrics, CompactLayout, CompactLayoutOptions, IconsLayout, IconsLayoutOptions,
    ItemLayout, RangeSelection, ViewPoint, ViewRect, ViewSize,
};
