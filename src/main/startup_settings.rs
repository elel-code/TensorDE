use shell::animation::ShellAnimationRuntime;
use shell::ark::ArkContextItem;
#[cfg(test)]
use shell::ark::{
    BUILTIN_ARK_COMPRESS_ACTION_ID, BUILTIN_ARK_COMPRESS_SUBMENU,
    BUILTIN_ARK_COMPRESS_TAR_GZ_ACTION_ID, BUILTIN_ARK_COMPRESS_ZIP_ACTION_ID,
    BUILTIN_ARK_EXTRACT_AND_TRASH_ACTION_ID, BUILTIN_ARK_EXTRACT_HERE_ACTION_ID,
    BUILTIN_ARK_EXTRACT_SUBMENU, BUILTIN_ARK_EXTRACT_TO_ACTION_ID,
};
use shell::autosmoke::{AutosmokeScrollAction, autosmoke_scroll_config, autosmoke_zoom_config};
use shell::clipboard::{FileClipboardExportRequest, ShellClipboard};
#[cfg(test)]
use shell::context_menu::paint::context_menu_named_icon_request;
use shell::context_menu::safe_triangle::ShellContextMenuSafeTriangleRuntime;
use shell::context_menu::{
    ShellContextMenu, ShellContextMenuAction, ShellContextMenuCommand, ShellContextTarget,
    ShellDevicePlace, context_menu_items, context_submenu_actions,
    device_place_operation_for_context_action,
};
#[cfg(test)]
use shell::context_menu::{
    ShellContextMenuIcon, ShellContextMenuItem, ShellContextSubmenu, context_menu_actions,
    context_menu_separator_before, service_menu_action_item,
};
#[cfg(test)]
use shell::create_rename::disk::{create_entry_on_disk, rename_entry_on_disk};
#[cfg(test)]
use shell::create_rename::geometry::{
    create_dialog_cancel_button_rect, create_dialog_commit_button_rect, create_dialog_rect,
    rename_dialog_commit_button_rect, rename_dialog_rect,
};
use shell::create_rename::geometry::{
    create_dialog_cancel_button_rect_scaled, create_dialog_commit_button_rect_scaled,
    create_dialog_input_rect_scaled, create_dialog_rect_scaled, create_dialog_window_size_scaled,
    create_kind_button_rect_scaled, rename_dialog_cancel_button_rect_scaled,
    rename_dialog_commit_button_rect_scaled, rename_dialog_input_rect_scaled,
    rename_dialog_rect_scaled, rename_dialog_window_size_scaled,
};
use shell::create_rename::{
    CreateDialogClick, CreateEntryKind, CreateEntryRequest, RenameDialogClick, RenameEntryRequest,
    ShellCreateDialog, ShellRenameDialog, unique_child_name, validate_create_name,
};
use shell::dialog_window::{
    ShellDetachedDialogWindow, ShellDialogWindowHostEvent, ShellDialogWindowKind,
    ShellDialogWindowSpec, ShellDialogWindows,
};
use shell::directory_watch::ShellDirectoryWatcherRuntime;
use shell::file_item_view::item_paint::{
    DolphinItemGeometry, DolphinItemInteraction,
    dolphin_item_paint_with_palette_and_hover_progress, dolphin_selection_core_rect,
};
use shell::file_item_view::style::{
    BREEZE_ITEM_ROUNDNESS, DolphinItemPalette,
    place_row_background_color_for_palette_with_hover_progress,
};
#[cfg(test)]
use shell::file_item_view::text::{compact_entry_text_width, estimated_text_cursor_x};
use shell::file_item_view::text::required_compact_item_width;
use shell::file_item_view::text_layout::{
    dolphin_elide_filename_to_width_shaped, dolphin_elide_text_to_width_shaped,
    dolphin_icons_filename_line_count,
    dolphin_layout_icons_filename, dolphin_text_width_no_wrap,
};
use shell::file_item_view::{
    dolphin_icon_size_for_zoom_level, dolphin_icons_item_width, shell_dolphin_read_ahead_indexes,
    visible_layout_range_for_projection,
};
use shell::drop_menu::{
    ShellDropMenu, ShellDropMenuCommand, ShellDropOperationRequest, ShellDropTarget,
    drop_menu_items,
};
#[cfg(test)]
use shell::folder_preview::{
    DolphinDirectoryPreviewLayout, FolderPreviewThumbnailSlot, folder_preview_thumbnail_angle,
    folder_preview_thumbnail_slots,
};
use shell::folder_preview::{
    FOLDER_PREVIEW_LAYOUT_VERSION, folder_preview_directory_seed,
    folder_preview_thumbnail_raster_from_children,
};
#[cfg(test)]
use shell::icon_resolver::FileIconResolverTestHarness;
use shell::icon_resolver::{FileIconResolver, ResolvedFileIcon, visible_icon_fallback_key};
use shell::icon_role_read_ahead::ShellIconRoleReadAheadQueue;
#[cfg(test)]
use shell::icon_roles::file_icon_profile;
use shell::icon_roles::{
    FILE_ICON_CORNER_RADIUS_RATIO, FOLDER_ICON_CORNER_RADIUS_RATIO, FileIconKind,
    FileIconPathCacheKey, FileIconProfile, FileIconRoleCacheKey, NamedIconFallback,
    file_icon_path_cache_key, icon_cache_size,
};
use shell::location::{
    LocationDraftPurpose, PathHistory, ShellLocationDraft, ShellPaneHistories,
    normalized_text_cursor,
};
#[cfg(test)]
use shell::menu_geometry::{context_menu_rect, context_menu_submenu_rect, drop_menu_rect};
use shell::menu_geometry::{
    context_menu_row_at_screen_point, context_submenu_row_at_screen_point,
    drop_menu_row_at_screen_point,
};
use shell::metadata_roles::{
    MetadataRolePrewarmStats, ShellMetadataRoleRuntime, entry_with_metadata_role, shell_entry_path,
    shell_metadata_entry_index, shell_pane_id_for_core_pane,
};
#[cfg(test)]
use shell::metadata_roles::{
    core_pane_id_for_shell_pane, shell_metadata_item_id, shell_metadata_role_candidate,
};
use shell::metrics::*;
use shell::open_file::OpenFileRequest;
use shell::operation_request::{ShellLaunchWork, ShellOperationRequest};
#[cfg(test)]
use shell::open_file::default_open_file_launch_request;
#[cfg(test)]
use shell::open_with::OpenWithDefaultUpdate;
#[cfg(test)]
use shell::open_with::OpenWithTreeRow;
use shell::open_with::geometry::{
    open_with_chooser_click_at_point, open_with_chooser_list_rect_scaled,
    open_with_chooser_pointer_role_at_point, open_with_chooser_rect_scaled,
    open_with_chooser_query_text_rect_scaled,
    open_with_chooser_scrollbar_rects_scaled, open_with_chooser_visible_row_count,
    open_with_chooser_window_size_scaled, open_with_scroll_delta_rows,
};
#[cfg(test)]
use shell::open_with::geometry::{
    open_with_chooser_default_checkbox_rect, open_with_chooser_list_rect,
    open_with_chooser_open_button_rect, open_with_chooser_query_rect_scaled, open_with_chooser_rect,
};
use shell::open_with::launch::{
    chooser_for_context_target, launch_request_for_chooser, launch_request_for_context_application,
};
use shell::open_with::{
    OpenWithChooserClick, OpenWithChooserPointerRole, OpenWithLaunchRequest, ShellOpenWithChooser,
    open_with_applications_for_mime,
};
use shell::options::{ShellViewMode, parse_start_options};
use shell::paint::ShellPaintPalettes;
use shell::pane::{
    ShellPaneGeometry, ShellPaneId, ShellPaneProjection, ShellPaneScrollMetrics,
    ShellPaneSplitMetrics, ShellPaneState, ShellPaneStates, ShellPaneView, ShellPaneVisibleItem,
    ShellPaneVisibleSlotPools, ShellVisibleItemSlotStats, ShellVisibleSlotItem,
};
use shell::pane_layout::{
    CompactLayoutCache, CompactLayoutCacheKey, CompactLayoutCacheValue, DetailsLayout,
    IconsLayoutHeightCache, IconsLayoutHeightCacheKey, IconsLayoutHeightCacheValue,
    ShellCompactLayout, ShellLayout, navigation_target,
};
use shell::perf::{
    ShellFrameLatencyAsyncResults, ShellFrameLatencyCounters, ShellFrameLatencyTracker,
};
use shell::popup::style::PopupTheme;
use shell::prewarm::{
    IconRasterPrewarmStats, IconRolePrewarmStats, TextLabelPrewarmMode, TextLabelPrewarmStats,
    default_text_raster_miss_budget, icon_role_prewarm_budget_for_frame,
    icon_role_read_ahead_queue_budget_for_frame, text_label_prewarm_budget_for_mode,
    text_label_prewarm_mode_for_frame, text_label_prewarm_mode_for_scene_prewarm,
    text_label_raster_miss_budget_for_mode, visible_exact_icon_roles_enabled_for_frame,
};
#[cfg(test)]
use shell::privilege::{run_privileged_command_sync, should_attempt_privileged_operation};
#[cfg(test)]
use shell::properties::geometry::properties_overlay_rect;
use shell::properties::geometry::properties_dialog_window_size_scaled;
#[cfg(test)]
use shell::properties::geometry::properties_overlay_rect_scaled;
use shell::properties::{ShellPropertiesOverlay, property_row};
#[cfg(test)]
use shell::render::damage::folder_preview_damage_rects_for_changed_keys;
use shell::render::damage::folder_preview_damage_rects_for_changes;
use shell::render::damage_bounds::{DamageScissorRect, ShellRenderDamage, ShellRenderDamageKind};
#[cfg(test)]
use shell::render::damage_bounds::{damage_scissor_rect, full_surface_rect, rect_area};
use shell::render::damage_snapshot::ShellRenderDamageSnapshot;
use shell::render::dirty_key::{ShellRenderDirtyKey, ShellRenderDirtyKeyContext};
use shell::render::frame::{
    DialogFrameRenderers, DialogFrameRequest, FrameGpuContext, SceneFrame, SceneFrameProjections,
    SceneFrameRenderers, SceneFrameRequest, prepare_dialog_frame, prepare_scene_frame,
};
#[cfg(test)]
use shell::render::gpu::upload_vertex_hash_for_test;
use shell::render::gpu::{
    VertexBufferUploadStats, create_icon_bind_group, create_icon_texture, create_text_bind_group,
    create_text_texture, create_text_vertex_buffer, hash_bytes_with_len,
    upload_vertex_buffer_if_dirty, vertex_pair_hash,
};
use shell::render::quad::{
    QuadRenderer, QuadVertex, RoundedHighlightStyle, push_clipped_rect,
    push_clipped_rect_outline, push_clipped_rounded_highlight, push_clipped_rounded_rect,
    push_clipped_rounded_rect_outline, push_rect,
};
use shell::render::retained::RetainedSceneRenderer;
#[cfg(test)]
use shell::render::retained::retained_scene_vertices;
use shell::render::shaders::{TEXT_SHADER, TEXTURE_SHADER};
use shell::render::texture::{AtlasRect, TextVertex, push_textured_rect};
use shell::role_worker_queue::{PriorityWorkerQueue, PriorityWorkerRequest, WorkerRequestPriority};
use shell::selection::{
    NavigationAction, RubberBand, RubberBandMode, SelectionClick, ShellSelection,
};
use shell::service_menu::ServiceMenuLaunchRequest;
use shell::settings::{
    ShellSettingsAction, ShellSettingsDialogState, ShellSettingsSnapshot,
    background_opacity_percent, opacity_percent_at_settings_point,
    settings_action_at_screen_point, settings_dialog_row_at_screen_point,
    settings_dialog_window_size_scaled,
};
use shell::shortcuts::{
    CreateCommand, FilterCommand, LocationCommand, OpenWithCommand, PathNavigationAction,
    RenameCommand, SelectionCommand, ZoomAction, create_command_for_key_event,
    escape_requested_for_key_event, open_with_command_for_key_event, rename_command_for_key_event,
};
#[cfg(test)]
use shell::shortcuts::{
    FileKeyboardCommand, create_command_for_key_parts, dark_mode_toggle_requested_for_key_parts,
    file_keyboard_command_for_key_parts, filter_command_for_key_parts,
    hidden_toggle_requested_for_key_parts, location_command_for_key_parts,
    path_navigation_action_for_key, path_navigation_action_for_mouse_button,
    reload_requested_for_key_parts, rename_command_for_key_parts, selection_command_for_key_parts,
    view_mode_for_key_parts, zoom_action_for_key, zoom_action_for_scroll_delta,
};
use shell::status::paint::{
    PaneStatusBarPaint, PlacesTaskAreaPaint, StatusZoomIndicatorRects,
    pane_status_zoom_indicator_rects, push_pane_status_bar as push_status_pane_bar,
    push_places_task_area as push_status_places_task_area,
};
use shell::status::{ShellPaneStatus, ShellTaskStatusStore};
#[cfg(test)]
use shell::tasks::ShellTaskStatusKind;
#[cfg(test)]
use shell::tasks::geometry::{
    task_detail_cancel_button_rect, task_detail_clear_button_rect, task_detail_dialog_rect,
    task_detail_dismiss_button_rect,
};
use shell::tasks::geometry::{
    task_detail_cancel_button_rect_scaled, task_detail_clear_button_rect_scaled,
    task_detail_dialog_window_rect, task_detail_dialog_window_size_scaled,
    task_detail_dismiss_button_rect_scaled,
};
#[cfg(test)]
use shell::tasks::geometry::task_detail_dialog_rect_scaled;
use shell::tasks::{ShellTaskDetailDialog, ShellTaskId, ShellTaskStatus, TaskDetailDialogClick};
use shell::text_input::{
    ShellTextDelete, ShellTextInputBatch, ShellTextInputOutcome, ShellTextPreedit,
    ShellTextSelection, apply_text_input_batch, cursor_with_preedit, text_with_preedit,
};
use shell::theme::ShellTheme;
use shell::toolbar::{
    ShellToolbarLayout, ShellToolbarViewModeSegment, app_toolbar_layout as build_app_toolbar_layout,
};
use shell::transfer::{
    ShellAsyncCreateCompletion, ShellAsyncDeviceCompletion, ShellAsyncLaunchCompletion,
    ShellAsyncLaunchKind, ShellAsyncMoveToTrashCompletion, ShellAsyncRenameCompletion,
    ShellAsyncTaskResult, ShellAsyncTransferCompletion, ShellAsyncTransferSource,
    ShellAsyncTrashViewCompletion, ShellPasteResult, ShellTransferExecution,
    async_transfer_task_detail, async_transfer_task_label, paste_text_async,
    transfer_paths_async_with_controller_and_privilege, transfer_runtime_failure,
    trash_paths_async_with_privilege,
};
#[cfg(test)]
use shell::transfer::transfer_paths_with_privilege;
use shell::trash_conflict::{ShellTrashConflictDialog, TrashConflictDialogClick};
use shell::ui_chrome::{
    PlaceIconPaint, push_fallback_file_icon, push_location_bar_icon, push_place_icon,
    push_scrollbar,
};
use shell::window_semantics::{ShellWindowRole, apply_window_platform_semantics};
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
                "[fika-wgpu] settings-load-error path={} error={error}",
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
    scene.places_visible = startup_places_visible(&settings);
    scene.dark_mode = startup_dark_mode(&settings);
    scene.background_blur = startup_background_blur(&settings);
    scene.background_opacity = startup_background_opacity(&settings);

    let event_loop = EventLoop::new()?;
    let event_loop_proxy = event_loop.create_proxy();
    event_loop.set_control_flow(ControlFlow::Wait);

    let app = FikaWgpuApp::new(
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

    fn pending(self) -> bool {
        !matches!(
            self.step,
            DialogLifecycleSmokeStep::Complete | DialogLifecycleSmokeStep::Failed
        )
    }
}
fn dialog_lifecycle_autosmoke_cycles_from_env() -> usize {
    env::var_os("FIKA_WGPU_AUTOSMOKE_DIALOG_CYCLES")
        .and_then(|value| value.to_string_lossy().trim().parse::<usize>().ok())
        .filter(|cycles| *cycles > 0)
        .unwrap_or(1)
}
fn dialog_lifecycle_autosmoke_kind_from_env() -> ShellDialogWindowKind {
    let Some(value) = env::var_os("FIKA_WGPU_AUTOSMOKE_DIALOG_KIND") else {
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
            scene.panes[ShellPaneId::SLOT_0].path.display(),
            split_pane.path.display(),
            view_mode.as_str()
        )
    } else {
        format!(
            "{} [{}]",
            scene.panes[ShellPaneId::SLOT_0].path.display(),
            view_mode.as_str()
        )
    }
}
