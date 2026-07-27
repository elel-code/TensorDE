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
    item_reflow: shell::item_reflow::ShellItemReflowRuntime,
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
include!("scene_runtime/places_status_paint.rs");
include!("scene_runtime/content_paint.rs");
include!("scene_runtime/dialog_controls.rs");
include!("scene_runtime/rubber_band_cleanup.rs");
impl TextLabelPrewarmStats {
    fn record(&mut self, outcome: LabelCacheOutcome) {
        match outcome {
            LabelCacheOutcome::Hit => self.cache_hits += 1,
            LabelCacheOutcome::Miss => self.cache_misses += 1,
            LabelCacheOutcome::Deferred => self.deferred += 1,
            LabelCacheOutcome::Skipped => {}
        }
    }
}
struct WgpuState {
    damage_clear_renderer: QuadRenderer,
    quad_renderer: QuadRenderer,
    overlay_quad_renderer: QuadRenderer,
    icon_renderer: IconRenderer,
    text_renderer: TextRenderer,
    overlay_text_renderer: Option<TextRenderer>,
    retained_scene: RetainedSceneRenderer,
    surface: wgpu::Surface<'static>,
    queue: wgpu::Queue,
    device: wgpu::Device,
    adapter: wgpu::Adapter,
    instance: wgpu::Instance,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    frame_count: u64,
    last_log: Instant,
    rendered_view_switches: u64,
    /// Last scene `path_changes` observed while presenting; used to free
    /// directory-scoped thumbnail / failure caches after navigate.
    rendered_path_changes: u64,
    /// Open pane paths from the last presented frame (for pruning left dirs).
    rendered_open_paths: Vec<PathBuf>,
    last_render_dirty_key: Option<ShellRenderDirtyKey>,
    last_render_damage_snapshot: Option<ShellRenderDamageSnapshot>,
    frame_latency: ShellFrameLatencyTracker,
    render_work_pending: bool,
    clean_redraw_skips: u64,
    /// Cached: adapter+device support Vulkan dmabuf texture import.
    dmabuf_import_supported: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShellRenderOutcome {
    Presented,
    SkippedClean,
    NotReady,
}
#[derive(Clone, Copy, Debug)]
enum ShellSurfaceFrameContext {
    Main { view: &'static str, force_log: bool },
    DetachedDialog { dialog_label: &'static str },
}
impl ShellSurfaceFrameContext {
    fn log_retry(self, reason: &'static str, status: &'static str) {
        if let Self::Main {
            view,
            force_log: true,
        } = self
        {
            fika_log!(
                "[fika-wgpu] frame-retry reason={reason} view={view} surface={status} action=reconfigure"
            );
        }
    }

    fn log_reconfigure_pending(self, reason: &'static str) {
        if let Self::Main {
            view,
            force_log: true,
        } = self
        {
            fika_log!(
                "[fika-wgpu] frame-skip reason={reason} view={view} surface=reconfigure-pending"
            );
        }
    }

    fn log_not_ready(self, reason: &'static str) {
        if let Self::Main {
            view,
            force_log: true,
        } = self
        {
            fika_log!("[fika-wgpu] frame-skip reason={reason} view={view} surface=not-ready");
        }
    }

    fn log_validation(self) {
        match self {
            Self::Main { .. } => fika_log!("[fika-wgpu] surface validation error"),
            Self::DetachedDialog { dialog_label } => {
                fika_log!("[fika-wgpu] {dialog_label}-dialog surface validation error");
            }
        }
    }
}
impl ShellRenderOutcome {
    fn presented(self) -> bool {
        matches!(self, Self::Presented)
    }

    fn consumed_redraw_request(self) -> bool {
        matches!(self, Self::Presented | Self::SkippedClean)
    }
}
include!("gpu_state/init.rs");
include!("gpu_state/frame_pipeline.rs");
include!("gpu_state/redraw_skip.rs");
fn clean_render_skip_reason_allowed(reason: &str, force_log: bool) -> bool {
    reason == "redraw" && !force_log || reason == "switch-redraw" && force_log
}
fn frame_latency_counters_for_scene(scene: &ShellScene) -> ShellFrameLatencyCounters {
    ShellFrameLatencyCounters {
        zoom_changes: scene.zoom_changes,
        content_scroll_changes: scene.content_scroll_changes,
        places_scroll_changes: scene.places_scroll_changes,
        path_changes: scene.path_changes,
        directory_reloads: scene.directory_reloads,
    }
}
impl Drop for WgpuState {
    fn drop(&mut self) {
        self.wait_idle("renderer-drop");
        let _ = self.instance.poll_all(false);
    }
}
#[derive(Clone, Copy, Debug, Default)]
struct IconFrameStats {
    icons: usize,
    quads: usize,
    fallbacks: usize,
    deferred: usize,
    thumbnails: usize,
    thumbnail_quads: usize,
    thumbnail_deferred: usize,
    thumbnail_read_ahead_queued: usize,
    thumbnail_ready_entries: usize,
    thumbnail_ready_bytes: usize,
    folder_previews: usize,
    folder_preview_quads: usize,
    folder_preview_deferred: usize,
    folder_preview_read_ahead_queued: usize,
    folder_preview_ready_entries: usize,
    folder_preview_ready_bytes: usize,
    atlas_uploads: usize,
    atlas_upload_skips: usize,
    atlas_width: u32,
    atlas_height: u32,
    atlas_bytes: usize,
    cache_hits: usize,
    cache_misses: usize,
    raster_deferred: usize,
    cache_entries: usize,
    cache_bytes: usize,
    content_hash: u64,
    geometry_hash: u64,
    vertex_hash: u64,
    slot_hash: u64,
    resolve_us: u128,
    raster_us: u128,
}
/// One frame of icon geometry for scheme-C per-icon GPU textures.
///
/// Slots are keyed by **logical** [`IconRasterCacheKey`] (path / size bucket /
/// mtime / style), not by CPU pixel hash. The hot path samples resident
/// `wgpu::Texture`s; CPU pixels exist only as a one-shot upload payload when a
/// slot is cold or its content generation changes. Dmabuf remains an optional
/// producer path — not required for the full-GPU sample pipeline.
struct IconFrame {
    /// Unique logical icons for this frame (optional CPU payload for upload).
    slots: Vec<IconGpuSlot>,
    /// Content-layer draws grouped by slot (each batch = one bind + draw).
    content_batches: Vec<IconSlotBatch>,
    /// Overlay-layer draws grouped by slot.
    overlay_batches: Vec<IconSlotBatch>,
    /// Packed vertex data for all content batches (ranges in batches).
    content_vertices: Vec<TextVertex>,
    /// Packed vertex data for all overlay batches.
    overlay_vertices: Vec<TextVertex>,
    stats: IconFrameStats,
}
/// Optional single-plane dmabuf for zero-copy GPU upload (scheme C + dmabuf).
///
/// Consumed once at `IconRenderer::upload`; not `Clone` (owns the fd).
struct IconDmabufSource {
    plane: crate::shell::render::dmabuf::DmabufImportPlane,
}

/// One logical icon that maps to a resident GPU texture.
///
/// `upload` is `Some` only when this frame must (re)write GPU memory. After a
/// successful upload the CPU payload is dropped so the steady state is
/// GPU-only sampling. Zoom changes screen size only — identity has **no**
/// size bucket, so the same texture is re-sampled.
struct IconGpuSlot {
    identity: IconGpuUploadKey,
    /// Texture size used for UV generation (may match a larger resident GPU
    /// texture when we only scale-sample this frame).
    width: u32,
    height: u32,
    /// Logical content size (kept separate for resident upgrade decisions).
    content_width: u32,
    content_height: u32,
    /// Content generation of `upload` (or last known GPU contents).
    content_hash: u64,
    /// One-shot CPU pixels for `write_texture` / dmabuf fallback.
    upload: Option<IconRaster>,
    dmabuf: Option<IconDmabufSource>,
}

impl std::fmt::Debug for IconGpuSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IconGpuSlot")
            .field("identity", &self.identity)
            .field("w", &self.width)
            .field("h", &self.height)
            .field("content_hash", &self.content_hash)
            .field("needs_upload", &self.upload.is_some())
            .field("has_dmabuf", &self.dmabuf.is_some())
            .finish()
    }
}
/// Draw range into a vertex buffer for a single icon texture.
#[derive(Clone, Debug)]
struct IconSlotBatch {
    slot: u32,
    vertex_start: u32,
    vertex_count: u32,
}

/// GPU-resident icon identity — **never** includes a size bucket.
///
/// - [`Role`](Self::Role): MIME / directory / generic file / named chrome icons.
///   Shared by every entry with that role (not bound to a filesystem path).
/// - [`ThemeAsset`](Self::ThemeAsset): resolved theme file path (named icons).
/// - [`Content`](Self::Content): thumbnails / folder previews (path + mtime).
///
/// Zoom only changes sampling; the same GPU texture stays bound.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum IconGpuIdentity {
    Role(FileIconKind),
    ThemeAsset { path: PathBuf },
    Content { path: PathBuf, stamp: u64 },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct IconGpuUploadKey {
    identity: IconGpuIdentity,
}

impl IconGpuUploadKey {
    fn role(kind: FileIconKind) -> Self {
        Self {
            identity: IconGpuIdentity::Role(kind),
        }
    }

    fn theme_asset(path: PathBuf) -> Self {
        Self {
            identity: IconGpuIdentity::ThemeAsset { path },
        }
    }

    fn content(path: PathBuf, stamp: u64) -> Self {
        Self {
            identity: IconGpuIdentity::Content { path, stamp },
        }
    }

    fn from_slot(slot: &IconGpuSlot) -> Self {
        slot.identity.clone()
    }
}
#[derive(Clone, Debug)]
struct IconDraw {
    screen: ViewRect,
    /// Index into the frame's per-icon GPU slots.
    slot: u32,
    /// Sample rect in the slot texture's pixel space.
    source: ViewRect,
    alpha: f32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IconDrawLayer {
    Content,
    Overlay,
}
#[derive(Clone, Debug)]
struct IconRaster {
    pixels: Arc<[u8]>,
    width: u32,
    height: u32,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum IconRasterStyle {
    Original,
    RoundedFile,
    RoundedFolder,
}
impl IconRasterStyle {
    fn for_file_icon_kind(kind: &FileIconKind) -> Self {
        match kind {
            FileIconKind::Directory => Self::RoundedFolder,
            FileIconKind::Mime { .. }
            | FileIconKind::PreliminaryFile { .. }
            | FileIconKind::File { .. } => Self::RoundedFile,
            FileIconKind::Named { .. } => Self::Original,
        }
    }
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct IconRasterCacheKey {
    path: PathBuf,
    size_px: u16,
    stamp: Option<u64>,
    style: IconRasterStyle,
}
impl IconRasterCacheKey {
    fn icon(path: PathBuf, size_px: u16) -> Self {
        Self {
            path,
            size_px,
            stamp: None,
            style: IconRasterStyle::Original,
        }
    }

    fn file_icon(path: PathBuf, size_px: u16, kind: &FileIconKind) -> Self {
        Self {
            path,
            size_px,
            stamp: None,
            style: IconRasterStyle::for_file_icon_kind(kind),
        }
    }

    fn thumbnail(path: PathBuf, size_px: u16, modified_secs: u64) -> Self {
        Self {
            path,
            size_px,
            stamp: Some(modified_secs),
            style: IconRasterStyle::Original,
        }
    }

    fn folder_preview(path: PathBuf, size_px: u16, stamp: u64) -> Self {
        Self {
            path,
            size_px,
            stamp: Some(stamp),
            style: IconRasterStyle::Original,
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

    fn from_raster_key(key: &IconRasterCacheKey) -> Option<Self> {
        Some(Self::new(key.path.clone(), key.stamp?))
    }
}
#[derive(Clone, Debug)]
struct CachedIconRaster {
    raster: IconRaster,
    bytes: usize,
    last_used_frame: u64,
}
#[derive(Debug)]
struct IconRasterCache {
    entries: HashMap<IconRasterCacheKey, CachedIconRaster>,
    frame: u64,
    bytes: usize,
    max_bytes: usize,
}
