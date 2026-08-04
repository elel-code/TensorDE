struct ShellScene {
    panes: ShellPaneStates,
    compact_layout_cache: CompactLayoutCache,
    icons_layout_height_cache: IconsLayoutHeightCache,
    active_pane: ShellPaneId,
    places: Vec<ShellPlace>,
    trash_has_items: bool,
    location_draft: Option<ShellLocationDraft>,
    filter_active: bool,
    filter_pattern: String,
    show_hidden: bool,
    dark_mode: bool,
    background_blur: bool,
    background_opacity: f32,
    places_visible: bool,
    places_width: f32,
    places_scroll_y: f32,
    scrollbar_drag: Option<ScrollbarDrag>,
    pointer: Option<ViewPoint>,
    hovered_item: Option<ShellPaneItemTarget>,
    hovered_place: Option<usize>,
    last_item_click: Option<PaneClick>,
    histories: ShellPaneHistories,
    context_target: Option<ShellContextTarget>,
    context_menu: Option<ShellContextMenu>,
    context_menu_safe_triangle: ShellContextMenuSafeTriangleRuntime,
    drop_menu: Option<ShellDropMenu>,
    properties_overlay: Option<ShellPropertiesOverlay>,
    create_dialog: Option<ShellCreateDialog>,
    rename_dialog: Option<ShellRenameDialog>,
    open_with_chooser: Option<ShellOpenWithChooser>,
    trash_conflict_dialog: Option<ShellTrashConflictDialog>,
    task_detail_dialog: Option<ShellTaskDetailDialog>,
    split_pane_left_fraction: f32,
    visible_slots: ShellPaneVisibleSlotPools,
    visible_slot_stats: ShellVisibleItemSlotStats,
    metadata_roles: ShellMetadataRoleRuntime,
    folder_preview_roles: RefCell<ShellFolderPreviewRoleRuntime>,
    icon_role_read_ahead: RefCell<ShellIconRoleReadAheadQueue>,
    internal_drag: Option<ShellInternalDrag>,
    external_drag: Option<ShellExternalDrag>,
    place_press: Option<ShellPlacePress>,
    dnd_hover_target: Option<ShellDropTarget>,
    pending_drop_request: Option<ShellDropOperationRequest>,
    task_statuses: ShellTaskStatusStore,
    rubber_band: Option<RubberBand>,
    item_reflow: ui::item_reflow::ShellItemReflowRuntime,
    animations: ShellAnimationRuntime,
    text_hit_tests: RefCell<TextHitTestRuntime>,
    scale_factor: f32,
    hit_tests: u64,
    selection_changes: u64,
    context_target_changes: u64,
    context_menu_actions: u64,
    properties_changes: u64,
    create_changes: u64,
    rename_changes: u64,
    open_with_changes: u64,
    open_changes: u64,
    copy_location_changes: u64,
    file_clipboard_changes: u64,
    paste_changes: u64,
    trash_changes: u64,
    places_changes: u64,
    places_resize_changes: u64,
    places_scroll_changes: u64,
    content_scroll_changes: u64,
    keyboard_navigation: u64,
    rubber_band_updates: u64,
    view_switches: u64,
    path_changes: u64,
    directory_reloads: u64,
    location_changes: u64,
    filter_changes: u64,
    hidden_changes: u64,
    appearance_changes: u64,
    zoom_changes: u64,
    split_pane_changes: u64,
    dnd_hover_changes: u64,
    dnd_drop_requests: u64,
}
include!("scene_runtime/load_and_state.rs");
include!("scene_runtime/path_navigation.rs");
include!("scene_runtime/location_places_settings.rs");
include!("scene_runtime/scale_and_layout_metrics.rs");
include!("scene_runtime/hit_testing.rs");
include!("scene_runtime/drop_targeting.rs");
include!("scene_runtime/places_drag_drop.rs");
include!("scene_runtime/context_and_service_menu.rs");
include!("scene_runtime/appearance.rs");
include!("scene_runtime/open_with.rs");
include!("scene_runtime/text_input.rs");
include!("scene_runtime/task_status.rs");
include!("scene_runtime/selection_and_paths.rs");
include!("scene_runtime/properties_overlay.rs");
include!("scene_runtime/create_rename_trash_dialogs.rs");
include!("scene_runtime/projection_layouts.rs");
include!("scene_runtime/chrome_pathbar_paint.rs");
include!("scene_runtime/icon_roles_thumbnails.rs");
include!("scene_runtime/folder_preview_roles.rs");
include!("scene_runtime/native_icons.rs");
include!("scene_runtime/places_text.rs");
include!("scene_runtime/places_status_paint.rs");
include!("scene_runtime/content_paint.rs");
include!("scene_runtime/dialog_controls.rs");
include!("scene_runtime/rubber_band_cleanup.rs");
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShellRenderOutcome {
    Presented,
    NotReady,
}
#[derive(Clone, Copy)]
struct DialogRenderViewport {
    popup_theme: PopupTheme,
    scale: f32,
    layout_size: PhysicalSize<u32>,
}

struct DetachedDialogRenderRequest<'a> {
    window: &'a Window,
    event_loop: &'a ActiveEventLoop,
    viewport: DialogRenderViewport,
    reason: &'static str,
    dialog_label: &'static str,
}

impl ShellRenderOutcome {
    fn presented(self) -> bool {
        matches!(self, Self::Presented)
    }

    fn consumed_redraw_request(self) -> bool {
        matches!(self, Self::Presented)
    }
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ThumbnailSourceKey {
    path: PathBuf,
    size_px: u16,
    stamp: Option<u64>,
}
impl ThumbnailSourceKey {
    fn thumbnail(path: PathBuf, size_px: u16, modified_secs: u64) -> Self {
        Self {
            path,
            size_px,
            stamp: Some(modified_secs),
        }
    }
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ThumbnailProbeCacheKey {
    path: PathBuf,
    modified_secs: u64,
}
impl ThumbnailProbeCacheKey {
    fn new(path: PathBuf, modified_secs: u64) -> Self {
        Self {
            path,
            modified_secs,
        }
    }

    fn from_source_key(key: &ThumbnailSourceKey) -> Option<Self> {
        Some(Self::new(key.path.clone(), key.stamp?))
    }
}
