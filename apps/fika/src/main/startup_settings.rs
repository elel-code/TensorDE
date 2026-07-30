use ui::animation::ShellAnimationRuntime;
use ui::ark::ArkContextItem;
#[cfg(test)]
use ui::ark::{
    BUILTIN_ARK_COMPRESS_ACTION_ID, BUILTIN_ARK_COMPRESS_SUBMENU,
    BUILTIN_ARK_COMPRESS_TAR_GZ_ACTION_ID, BUILTIN_ARK_COMPRESS_ZIP_ACTION_ID,
    BUILTIN_ARK_EXTRACT_AND_TRASH_ACTION_ID, BUILTIN_ARK_EXTRACT_HERE_ACTION_ID,
    BUILTIN_ARK_EXTRACT_SUBMENU, BUILTIN_ARK_EXTRACT_TO_ACTION_ID,
};
use ui::autosmoke::{AutosmokeScrollAction, autosmoke_scroll_config, autosmoke_zoom_config};
use ui::clipboard::{FileClipboardExportRequest, ShellClipboard};
#[cfg(test)]
use ui::context_menu::paint::context_menu_named_icon_request;
use ui::context_menu::safe_triangle::ShellContextMenuSafeTriangleRuntime;
use ui::context_menu::{
    ShellContextMenu, ShellContextMenuAction, ShellContextMenuCommand, ShellContextTarget,
    ShellDevicePlace, context_menu_items, context_submenu_actions,
    device_place_operation_for_context_action,
};
#[cfg(test)]
use ui::context_menu::{
    ShellContextMenuIcon, ShellContextMenuItem, ShellContextSubmenu, context_menu_actions,
    context_menu_separator_before, service_menu_action_item,
};
#[cfg(test)]
use ui::create_rename::disk::{create_entry_on_disk, rename_entry_on_disk};
#[cfg(test)]
use ui::create_rename::geometry::{
    create_dialog_cancel_button_rect, create_dialog_commit_button_rect, create_dialog_rect,
    rename_dialog_commit_button_rect, rename_dialog_rect,
};
use ui::create_rename::geometry::{
    create_dialog_cancel_button_rect_scaled, create_dialog_commit_button_rect_scaled,
    create_dialog_input_rect_scaled, create_dialog_rect_scaled, create_dialog_window_size_scaled,
    create_kind_button_rect_scaled, rename_dialog_cancel_button_rect_scaled,
    rename_dialog_commit_button_rect_scaled, rename_dialog_input_rect_scaled,
    rename_dialog_rect_scaled, rename_dialog_window_size_scaled,
};
use ui::create_rename::{
    CreateDialogClick, CreateEntryKind, CreateEntryRequest, RenameDialogClick, RenameEntryRequest,
    ShellCreateDialog, ShellRenameDialog, unique_child_name, validate_create_name,
};
use ui::dialog_window::{
    ShellDetachedDialogWindow, ShellDialogWindowHostEvent, ShellDialogWindowKind,
    ShellDialogWindowSpec, ShellDialogWindows,
};
use ui::directory_watch::ShellDirectoryWatcherRuntime;
use ui::file_item_view::item_paint::{
    FileManagerItemGeometry, FileManagerItemInteraction,
    file_manager_item_paint_with_palette_and_hover_progress, file_manager_selection_core_rect,
};
use ui::file_item_view::style::{
    BREEZE_ITEM_ROUNDNESS, FileManagerItemPalette,
    place_row_background_color_for_palette_with_hover_progress,
};
#[cfg(test)]
use ui::file_item_view::text::{compact_entry_text_width, estimated_text_cursor_x};
use ui::file_item_view::text::required_compact_item_width;
use ui::file_item_view::text_layout::{
    file_manager_elide_filename_to_width_shaped, file_manager_icons_filename_line_count,
    file_manager_layout_icons_filename, file_manager_text_width_no_wrap,
};
use ui::file_item_view::{
    file_manager_icon_size_for_zoom_level, file_manager_icons_item_width,
    file_manager_zoom_level_for_icon_size, shell_file_manager_read_ahead_indexes,
    visible_layout_range_for_projection,
};
use ui::drop_menu::{
    ShellDropMenu, ShellDropMenuCommand, ShellDropOperationRequest, ShellDropTarget,
    drop_menu_items,
};
#[cfg(test)]
use ui::folder_preview::{
    FileManagerDirectoryPreviewLayout, folder_preview_thumbnail_angle,
    folder_preview_thumbnail_slots,
};
use ui::folder_preview::{
    FOLDER_PREVIEW_LAYOUT_VERSION, folder_preview_directory_seed,
};
#[cfg(test)]
use ui::icon_resolver::FileIconResolverTestHarness;
use ui::icon_resolver::{FileIconResolver, ResolvedFileIcon};
use ui::icon_role_read_ahead::ShellIconRoleReadAheadQueue;
#[cfg(test)]
use ui::icon_roles::file_icon_profile;
use ui::icon_roles::{
    FileIconKind, FileIconProfile, NamedIconFallback, file_icon_path_cache_key_with_stamp,
    icon_cache_size, thumbnail_display_cache_size,
};
use ui::location::{
    LocationDraftPurpose, PathHistory, ShellLocationDraft, ShellPaneHistories,
    normalized_text_cursor,
};
#[cfg(test)]
use ui::menu_geometry::{context_menu_rect, drop_menu_rect};
use ui::menu_geometry::{
    context_menu_row_at_screen_point, context_submenu_row_at_screen_point,
    drop_menu_row_at_screen_point,
};
use ui::metadata_roles::{
    MetadataRolePrewarmStats, MetadataRoleSyncStats, ShellMetadataRoleRuntime,
    entry_with_metadata_role, shell_entry_path, shell_metadata_entry_index,
    shell_pane_id_for_core_pane,
};
#[cfg(test)]
use ui::metadata_roles::{
    core_pane_id_for_shell_pane, shell_metadata_item_id, shell_metadata_role_candidate,
};
use ui::metrics::*;
use ui::open_file::OpenFileRequest;
use ui::operation_request::{ShellClipboardWork, ShellLaunchWork, ShellOperationRequest};
#[cfg(test)]
use ui::open_file::default_open_file_launch_request;
#[cfg(test)]
use ui::open_with::OpenWithDefaultUpdate;
#[cfg(test)]
use ui::open_with::OpenWithTreeRow;
use ui::open_with::geometry::{
    open_with_chooser_click_at_point, open_with_chooser_list_rect_scaled,
    open_with_chooser_pointer_role_at_point, open_with_chooser_rect_scaled,
    open_with_chooser_query_text_rect_scaled,
    open_with_chooser_scrollbar_rects_scaled, open_with_chooser_visible_row_count,
    open_with_chooser_window_size_scaled, open_with_scroll_delta_rows,
};
#[cfg(test)]
use ui::open_with::geometry::{
    open_with_chooser_default_checkbox_rect, open_with_chooser_list_rect,
    open_with_chooser_open_button_rect, open_with_chooser_query_rect_scaled, open_with_chooser_rect,
};
use ui::open_with::launch::{
    chooser_for_context_target, launch_request_for_chooser, launch_request_for_context_application,
};
use ui::open_with::{
    OpenWithChooserClick, OpenWithChooserPointerRole, OpenWithLaunchRequest, ShellOpenWithChooser,
    open_with_applications_for_mime,
};
use ui::options::{ShellViewMode, parse_start_options};
use ui::paint::ShellPaintPalettes;
use ui::pane::{
    ShellPaneGeometry, ShellPaneId, ShellPaneProjection, ShellPaneScrollMetrics,
    ShellPaneSplitMetrics, ShellPaneState, ShellPaneStates, ShellPaneView, ShellPaneVisibleItem,
    ShellPaneVisibleSlotPools, ShellPaneZoomLevels, ShellVisibleItemSlotStats, ShellVisibleSlotItem,
};
use ui::pane_layout::{
    CompactLayoutCache, CompactLayoutCacheKey, CompactLayoutCacheValue, DetailsLayout,
    IconsLayoutHeightCache, IconsLayoutHeightCacheKey, IconsLayoutHeightCacheValue,
    ShellCompactLayout, ShellLayout, navigation_target,
};
use ui::popup::style::PopupTheme;
use ui::prewarm::{
    IconRolePrewarmStats, default_text_raster_miss_budget, icon_role_prewarm_budget,
    icon_role_read_ahead_queue_budget_for_frame,
};
#[cfg(test)]
use ui::privilege::{run_privileged_command_sync, should_attempt_privileged_operation};
#[cfg(test)]
use ui::properties::geometry::properties_overlay_rect;
use ui::properties::geometry::properties_dialog_window_size_scaled;
#[cfg(test)]
use ui::properties::geometry::properties_overlay_rect_scaled;
use ui::properties::{ShellPropertiesOverlay, property_row};
use ui::render::projections::SceneFrameProjections;
use ui::render::gpu::{hash_bytes_with_len, vertex_pair_hash};
use ui::render::quad::{
    QuadVertex, RoundedHighlightStyle, push_clipped_rect, push_clipped_rect_outline,
    push_clipped_rounded_highlight, push_clipped_rounded_rect, push_rect,
};
use ui::render::texture::{AtlasRect, TextVertex, push_textured_rect};
use ui::role_worker_queue::{PriorityWorkerQueue, PriorityWorkerRequest, WorkerRequestPriority};
use ui::selection::{
    NavigationAction, RubberBand, RubberBandMode, SelectionClick, ShellSelection,
};
use ui::service_menu::ServiceMenuLaunchRequest;
use ui::settings::{
    ShellSettingsAction, ShellSettingsDialogState, ShellSettingsSnapshot,
    background_opacity_percent, opacity_percent_at_settings_point,
    settings_action_at_screen_point, settings_dialog_row_at_screen_point,
    settings_dialog_window_size_scaled,
};
use ui::shortcuts::{
    CreateCommand, FilterCommand, LocationCommand, OpenWithCommand, PathNavigationAction,
    PinchZoomTracker, RenameCommand, SelectionCommand, SwipeNavigationTracker, ZoomAction,
    create_command_for_key_event, escape_requested_for_key_event, open_with_command_for_key_event,
    rename_command_for_key_event,
};
#[cfg(test)]
use ui::shortcuts::{
    FileKeyboardCommand, create_command_for_key_parts, dark_mode_toggle_requested_for_key_parts,
    file_keyboard_command_for_key_parts, filter_command_for_key_parts,
    hidden_toggle_requested_for_key_parts, location_command_for_key_parts,
    path_navigation_action_for_key, path_navigation_action_for_mouse_button,
    reload_requested_for_key_parts, rename_command_for_key_parts, selection_command_for_key_parts,
    view_mode_for_key_parts, zoom_action_for_key, zoom_action_for_scroll_delta,
};
use ui::status::paint::{
    PaneStatusBarPaint, StatusZoomIndicatorRects, pane_status_zoom_indicator_rects,
};
use ui::status::{ShellPaneStatus, ShellTaskStatusStore};
#[cfg(test)]
use ui::tasks::ShellTaskStatusKind;
#[cfg(test)]
use ui::tasks::geometry::{
    task_detail_cancel_button_rect, task_detail_clear_button_rect, task_detail_dialog_rect,
    task_detail_dismiss_button_rect,
};
use ui::tasks::geometry::{
    task_detail_cancel_button_rect_scaled, task_detail_clear_button_rect_scaled,
    task_detail_dialog_window_rect, task_detail_dialog_window_size_scaled,
    task_detail_dismiss_button_rect_scaled,
};
#[cfg(test)]
use ui::tasks::geometry::task_detail_dialog_rect_scaled;
use ui::tasks::{ShellTaskDetailDialog, ShellTaskId, ShellTaskStatus, TaskDetailDialogClick};
use ui::text_input::{
    ShellTextDelete, ShellTextInputBatch, ShellTextInputOutcome, ShellTextPreedit,
    ShellTextSelection, apply_text_input_batch, cursor_with_preedit, text_with_preedit,
};
use ui::theme::ShellTheme;
use ui::toolbar::{
    ShellToolbarLayout, ShellToolbarViewModeSegment, app_toolbar_layout as build_app_toolbar_layout,
};
use ui::transfer::{
    ShellAsyncClipboardCompletion, ShellAsyncCreateCompletion, ShellAsyncDeviceCompletion,
    ShellAsyncLaunchCompletion, ShellAsyncLaunchKind, ShellAsyncMoveToTrashCompletion,
    ShellAsyncNavigationCompletion, ShellAsyncRenameCompletion, ShellAsyncTaskResult,
    ShellAsyncTransferCompletion, ShellAsyncTransferSource, ShellAsyncTrashViewCompletion,
    ShellNavigationHistoryUpdate, ShellPasteResult, ShellTransferExecution,
    async_transfer_task_detail, async_transfer_task_label, paste_text_async,
    transfer_paths_async_with_controller_and_privilege, transfer_runtime_failure,
    trash_paths_async_with_privilege,
};
#[cfg(test)]
use ui::transfer::transfer_paths_with_privilege;
use ui::trash_conflict::{ShellTrashConflictDialog, TrashConflictDialogClick};
use ui::window_semantics::{ShellWindowRole, apply_window_semantics};
fn startup_view_mode(
    requested: ShellViewMode,
    explicit: bool,
    settings: &AppSettings,
) -> ShellViewMode {
    if explicit {
        return requested;
    }
    settings.view.mode.unwrap_or(requested)
}
fn startup_show_hidden(settings: &AppSettings) -> bool {
    settings.view.show_hidden.unwrap_or(false)
}
fn startup_zoom_levels(settings: &AppSettings) -> ShellPaneZoomLevels {
    let mut levels = ShellPaneZoomLevels::default();
    for (mode, size) in [
        (ShellViewMode::Icons, settings.view.icons_preview_size),
        (ShellViewMode::Compact, settings.view.compact_preview_size),
        (ShellViewMode::Details, settings.view.details_preview_size),
    ] {
        if let Some(size) = size {
            levels.set(mode, file_manager_zoom_level_for_icon_size(size));
        }
    }
    levels
}
fn startup_places_visible(settings: &AppSettings) -> bool {
    settings.places_sidebar.visible.unwrap_or(true)
}
fn startup_dark_mode(settings: &AppSettings) -> bool {
    settings.appearance.dark_mode.unwrap_or(false)
}
fn startup_background_blur(settings: &AppSettings) -> bool {
    settings.appearance.background_blur.unwrap_or(false)
}
fn startup_background_opacity(settings: &AppSettings) -> f32 {
    background_opacity_percent(settings.appearance.background_opacity.unwrap_or(1.0)) as f32
        / 100.0
}
fn load_startup_app_settings(settings_path: &Path) -> AppSettings {
    match load_app_settings(settings_path) {
        Ok(settings) => settings,
        Err(error) => {
            fika_log!(
                "[fika] settings-load-error path={} error={error}",
                settings_path.display()
            );
            AppSettings::default()
        }
    }
}
fn save_view_mode_setting(settings_path: &Path, view_mode: ShellViewMode) -> Result<(), String> {
    let mut settings = load_app_settings(settings_path)
        .map_err(|error| format!("load settings {}: {error}", settings_path.display()))?;
    settings.view.mode = Some(view_mode);
    save_app_settings(settings_path, &settings)
        .map_err(|error| format!("save settings {}: {error}", settings_path.display()))
}
fn save_preview_size_settings(
    settings_path: &Path,
    preview_sizes: [Option<u16>; 3],
) -> Result<(), String> {
    let mut settings = load_app_settings(settings_path)
        .map_err(|error| format!("load settings {}: {error}", settings_path.display()))?;
    if let Some(size) = preview_sizes[0] {
        settings.view.icons_preview_size = Some(size);
    }
    if let Some(size) = preview_sizes[1] {
        settings.view.compact_preview_size = Some(size);
    }
    if let Some(size) = preview_sizes[2] {
        settings.view.details_preview_size = Some(size);
    }
    save_app_settings(settings_path, &settings)
        .map_err(|error| format!("save settings {}: {error}", settings_path.display()))
}
fn save_show_hidden_setting(settings_path: &Path, show_hidden: bool) -> Result<(), String> {
    let mut settings = load_app_settings(settings_path)
        .map_err(|error| format!("load settings {}: {error}", settings_path.display()))?;
    settings.view.show_hidden = Some(show_hidden);
    save_app_settings(settings_path, &settings)
        .map_err(|error| format!("save settings {}: {error}", settings_path.display()))
}
fn save_places_visible_setting(settings_path: &Path, visible: bool) -> Result<(), String> {
    let mut settings = load_app_settings(settings_path)
        .map_err(|error| format!("load settings {}: {error}", settings_path.display()))?;
    settings.places_sidebar.visible = Some(visible);
    save_app_settings(settings_path, &settings)
        .map_err(|error| format!("save settings {}: {error}", settings_path.display()))
}
fn save_dark_mode_setting(settings_path: &Path, dark_mode: bool) -> Result<(), String> {
    let mut settings = load_app_settings(settings_path)
        .map_err(|error| format!("load settings {}: {error}", settings_path.display()))?;
    settings.appearance.dark_mode = Some(dark_mode);
    save_app_settings(settings_path, &settings)
        .map_err(|error| format!("save settings {}: {error}", settings_path.display()))
}
fn save_background_effect_settings(
    settings_path: &Path,
    background_blur: bool,
    background_opacity: f32,
) -> Result<(), String> {
    let mut settings = load_app_settings(settings_path)
        .map_err(|error| format!("load settings {}: {error}", settings_path.display()))?;
    settings.appearance.background_blur = Some(background_blur);
    settings.appearance.background_opacity = Some(background_opacity);
    save_app_settings(settings_path, &settings)
        .map_err(|error| format!("save settings {}: {error}", settings_path.display()))
}
fn read_shell_entries_sync(path: &Path) -> Result<Vec<Entry>, String> {
    if is_network_path(path) {
        let mut entries = Vec::new();
        let completed = read_network_entry_batches_sync_cancellable(
            path,
            usize::MAX,
            || false,
            |mut batch| entries.append(&mut batch),
        )
        .map_err(|error| format!("read network directory {}: {error}", path.display()))?;
        if completed.is_none() {
            return Err(format!(
                "read network directory {}: cancelled",
                path.display()
            ));
        }
        Ok(entries)
    } else {
        read_entries_sync(path)
            .map_err(|error| format!("read directory {}: {error}", path.display()))
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let Some(options) = parse_start_options()? else {
        return Ok(());
    };
    let settings_path = default_app_settings_path();
    let settings = load_startup_app_settings(&settings_path);
    let view_mode = startup_view_mode(options.view_mode, options.view_mode_explicit, &settings);
    let show_hidden = startup_show_hidden(&settings);
    let mut scene = ShellScene::load_with_hidden_visibility(options.path, view_mode, show_hidden)?;
    scene.apply_startup_zoom_levels(startup_zoom_levels(&settings));
    scene.places_visible = startup_places_visible(&settings);
    scene.dark_mode = startup_dark_mode(&settings);
    scene.background_blur = startup_background_blur(&settings);
    scene.background_opacity = startup_background_opacity(&settings);

    let event_loop = EventLoop::new()?;
    let event_loop_proxy = event_loop.create_proxy();
    event_loop.set_control_flow(ControlFlow::Wait);

    let app = FikaApp::new(
        scene,
        options.auto_cycle_views,
        settings_path,
        event_loop_proxy,
    );
    event_loop.run_app(app)?;
    Ok(())
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContentScrollbarAxis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum ScrollbarDragTarget {
    Content {
        pane: ShellPaneId,
        axis: ContentScrollbarAxis,
    },
    OpenWith,
    Places,
    PlacesResize,
    SplitPaneResize,
    StatusZoom {
        pane: ShellPaneId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarDrag {
    target: ScrollbarDragTarget,
    grab_offset: f32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DialogLifecycleSmokeStep {
    WaitMainFrame,
    WaitDialogFrame,
    WaitMainFrameAfterClose,
    Complete,
    Failed,
}
#[derive(Clone, Copy, Debug)]
struct DialogLifecycleSmoke {
    step: DialogLifecycleSmokeStep,
    kind: ShellDialogWindowKind,
    close_frame: u64,
    cycles_remaining: usize,
}
impl DialogLifecycleSmoke {
    fn from_env() -> Option<Self> {
        dialog_lifecycle_autosmoke_enabled().then_some(Self {
            step: DialogLifecycleSmokeStep::WaitMainFrame,
            kind: dialog_lifecycle_autosmoke_kind_from_env(),
            close_frame: 0,
            cycles_remaining: dialog_lifecycle_autosmoke_cycles_from_env(),
        })
    }
}
fn dialog_lifecycle_autosmoke_cycles_from_env() -> usize {
    env::var_os("FIKA_AUTOSMOKE_DIALOG_CYCLES")
        .and_then(|value| value.to_string_lossy().trim().parse::<usize>().ok())
        .filter(|cycles| *cycles > 0)
        .unwrap_or(1)
}
fn dialog_lifecycle_autosmoke_kind_from_env() -> ShellDialogWindowKind {
    let Some(value) = env::var_os("FIKA_AUTOSMOKE_DIALOG_KIND") else {
        return ShellDialogWindowKind::Create;
    };
    match value.to_string_lossy().trim().to_ascii_lowercase().as_str() {
        "open-with" | "open_with" | "openwith" => ShellDialogWindowKind::OpenWith,
        "rename" => ShellDialogWindowKind::Rename,
        "settings" => ShellDialogWindowKind::Settings,
        _ => ShellDialogWindowKind::Create,
    }
}
fn window_title(scene: &ShellScene) -> String {
    let view_mode = scene.active_view_mode();
    if let Some(split_pane) = scene.panes.get(ShellPaneId::SLOT_1) {
        format!(
            "{} | {} [{}]",
            scene.panes[ShellPaneId::SLOT_0].display_path().display(),
            split_pane.display_path().display(),
            view_mode.as_str()
        )
    } else {
        format!(
            "{} [{}]",
            scene.panes[ShellPaneId::SLOT_0].display_path().display(),
            view_mode.as_str()
        )
    }
}
