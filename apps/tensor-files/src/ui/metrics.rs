use std::time::Duration;

use tensor_files_core::PaneId;

pub(crate) const APP_TOOLBAR_HEIGHT: f32 = 36.0;
pub(crate) const PANE_MARGIN: f32 = 8.0;
pub(crate) const TOP_BAR_HEIGHT: f32 = 36.0;
pub(crate) const FILTER_BAR_HEIGHT: f32 = 30.0;
pub(crate) const STATUS_BAR_HEIGHT: f32 = 28.0;
#[cfg(test)]
pub(crate) const ICONS_ITEM_WIDTH: f32 = 96.0;
#[cfg(test)]
pub(crate) const ICONS_ICON_SIZE: f32 = 64.0;
pub(crate) const FILE_MANAGER_ICONS_TEXT_WIDTH_INDEX: f32 = 1.0;
pub(crate) const FILE_MANAGER_ICONS_MAX_TEXT_LINES: usize = 3;
pub(crate) const COMPACT_MIN_TEXT_WIDTH: f32 = 24.0;
#[cfg(test)]
pub(crate) const COMPACT_ICON_SIZE: f32 = 48.0;
pub(crate) const DETAILS_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const DETAILS_NAME_WIDTH: f32 = 360.0;
pub(crate) const DETAILS_SIZE_WIDTH: f32 = 104.0;
pub(crate) const DETAILS_MODIFIED_WIDTH: f32 = 164.0;
pub(crate) const SCROLL_LINE_PX: f32 = 56.0;
pub(crate) const TEXT_ATLAS_WIDTH: u32 = 2048;
// Retain common view-density changes without replacing the Vulkan atlas in an input frame.
pub(crate) const TEXT_ATLAS_RETAINED_HEIGHT: u32 = 512;
pub(crate) const TEXT_FONT_SIZE: f32 = 14.0;
pub(crate) const TEXT_LINE_HEIGHT: f32 = 18.0;
pub(crate) const TEXT_ATLAS_MAX_HEIGHT: u32 = 8192;
pub(crate) const TEXT_PADDING: u32 = 2;
pub(crate) const TEXT_LABEL_CACHE_MAX_BYTES: usize = 4 * 1024 * 1024;
// FileManager caps recycled KItemListWidget instances at 100; keep text rasters in the same shape.
pub(crate) const TEXT_LABEL_RECYCLE_CACHE_ENTRIES: usize = 100;
pub(crate) const TEXT_LABEL_METRICS_CACHE_MAX_ENTRIES: usize = 4096;
pub(crate) const TEXT_RASTER_MISS_BUDGET_PER_FRAME: usize = 256;
pub(crate) const TEXT_SWASH_IMAGE_CACHE_MAX_ENTRIES: usize = 1024;
pub(crate) const TEXT_SWASH_OUTLINE_CACHE_MAX_ENTRIES: usize = 256;
/// Legacy atlas packing constants (text still uses its own atlas; icons are
/// scheme-C per-texture and no longer pack into a shared atlas).
#[allow(dead_code)]
pub(crate) const ICON_ATLAS_WIDTH: u32 = 1024;
#[allow(dead_code)]
pub(crate) const ICON_PADDING: u32 = 2;
/// CPU icon/thumbnail pixel cache. GPU-resident textures are the steady-state
/// source of truth; this budget only covers miss / closest-size staging.
pub(crate) const ICON_VISIBLE_SYNC_RESOLVE_BUDGET: usize = 128;
pub(crate) const ICON_ROLE_READ_AHEAD_LIMIT: usize = 512;
pub(crate) const ICON_ROLE_READ_AHEAD_QUEUE_BUDGET_PER_FRAME: usize = 64;
pub(crate) const METADATA_ROLE_BATCH_SIZE: usize = 64;
/// FileManager `ResolveAllItemsLimit` — small directories resolve fully; larger ones
/// only touch the visible + read-ahead window.
pub(crate) const FILE_MANAGER_RESOLVE_ALL_ITEMS_LIMIT: usize = 500;
/// FileManager `KFileItemModel::determineMimeTypes` / role-updater block budget
/// (~200 ms) for visible metadata/icon work before yielding the frame.
pub(crate) const FILE_MANAGER_MAX_BLOCK_TIMEOUT: Duration = Duration::from_millis(200);
/// FileManager `KFileItemListView` short delay before applying a new visible range
/// after scroll (avoids thrashing expensive icon loads mid-fling).
pub(crate) const FILE_MANAGER_VISIBLE_RANGE_UPDATE_MS: u64 = 50;
/// FileManager long delay after icon-size / zoom changes before regenerating previews.
#[allow(dead_code)]
pub(crate) const FILE_MANAGER_ICON_SIZE_UPDATE_MS: u64 = 300;
pub(crate) const VISIBLE_ICON_ROLE_PREWARM_BUDGET: Duration = Duration::from_millis(8);
/// Async thumbnail worker ready-set. Entries promote into the GPU pool; keep
/// this tighter so multi-directory browsing does not pin tens of MB of PSS.
pub(crate) const THUMBNAIL_READY_CACHE_MAX_BYTES: usize = 12 * 1024 * 1024;
/// Cap permanent probe-failure sets so long sessions cannot pin unbounded paths.
pub(crate) const THUMBNAIL_FAILURE_CACHE_MAX_ENTRIES: usize = 4096;
pub(crate) const THUMBNAIL_READ_AHEAD_PAGES: usize = 5;
pub(crate) const THUMBNAIL_READ_AHEAD_RESOLVE_LIMIT: usize = 500;
pub(crate) const THUMBNAIL_READ_AHEAD_QUEUE_BUDGET_PER_FRAME: usize = 32;
pub(crate) const FILE_MANAGER_FOLDER_PREVIEW_MAX_IMAGES: usize = 4;
pub(crate) const FILE_MANAGER_FOLDER_PREVIEW_SCAN_LIMIT: usize = 500;
pub(crate) const RUBBER_BAND_START_THRESHOLD: f32 = 4.0;
pub(crate) const ITEM_REFLOW_ANIMATION_DELAY: Duration = Duration::from_millis(100);
pub(crate) const ITEM_REFLOW_ANIMATION_DURATION: Duration = Duration::from_millis(366);
pub(crate) const ITEM_REFLOW_ANIMATION_FRAME: Duration = Duration::from_millis(16);
pub(crate) const HOVER_ANIMATION_DURATION: Duration = Duration::from_millis(130);
pub(crate) const HOVER_ANIMATION_FRAME: Duration = Duration::from_millis(16);
pub(crate) const LOCATION_FOCUS_SHINE_DELAY: Duration = Duration::from_millis(200);
pub(crate) const LOCATION_FOCUS_SHINE_DURATION: Duration = Duration::from_millis(616);
pub(crate) const LOCATION_FOCUS_SHINE_FRAME: Duration = Duration::from_millis(16);
pub(crate) const TEXT_CARET_BLINK_INTERVAL: Duration = Duration::from_millis(530);
pub(crate) const PLACES_SIDEBAR_WIDTH: f32 = 228.0;
pub(crate) const PLACES_SIDEBAR_MIN_WIDTH: f32 = 128.0;
pub(crate) const PLACES_SIDEBAR_MAX_WIDTH_RATIO: f32 = 0.42;
pub(crate) const PLACES_SIDEBAR_RIGHT_RESERVE: f32 = 120.0;
pub(crate) const PLACES_SIDEBAR_SPLITTER_WIDTH: f32 = 1.0;
pub(crate) const PLACES_RESIZE_HANDLE_WIDTH: f32 = 16.0;
pub(crate) const PLACES_TO_PANE_GAP: f32 = 8.0;
pub(crate) const PLACES_SIDEBAR_PANEL_MARGIN_X: f32 = 8.0;
pub(crate) const PLACES_SIDEBAR_PANEL_MARGIN_BOTTOM: f32 = 8.0;
pub(crate) const PLACES_SIDEBAR_PADDING_X: f32 = 8.0;
pub(crate) const PLACES_SIDEBAR_TOP_PADDING: f32 = 8.0;
pub(crate) const PLACES_TITLE_HEIGHT: f32 = 18.0;
pub(crate) const PLACES_SECTION_HEIGHT: f32 = 24.0;
pub(crate) const PLACES_ROW_HEIGHT: f32 = 30.0;
pub(crate) const PLACES_ROW_GAP: f32 = 0.0;
pub(crate) const PLACES_ICON_SIZE: f32 = 22.0;
pub(crate) const PLACES_TASK_AREA_HEIGHT: f32 = 132.0;
pub(crate) const PLACES_TASK_AREA_GAP: f32 = 8.0;
pub(crate) const PLACES_SCROLLBAR_WIDTH: f32 = 3.0;
pub(crate) const PLACES_SCROLLBAR_MARGIN: f32 = 4.0;
pub(crate) const PLACES_SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 28.0;
pub(crate) const CONTENT_SCROLLBAR_RESERVED_EXTENT: f32 = 14.0;
pub(crate) const CONTENT_SCROLLBAR_PADDING: f32 = 4.0;
pub(crate) const CONTENT_SCROLLBAR_MIN_THUMB_SIZE: f32 = 25.0;
pub(crate) const SPLIT_PANE_DIVIDER_WIDTH: f32 = 1.0;
pub(crate) const SPLIT_PANE_RESIZE_HANDLE_WIDTH: f32 = 10.0;
pub(crate) const SPLIT_PANE_MIN_WIDTH: f32 = 180.0;
pub(crate) const CONTEXT_MENU_WIDTH: f32 = 260.0;
pub(crate) const CONTEXT_MENU_ROW_HEIGHT: f32 = 28.0;
pub(crate) const CONTEXT_MENU_VERTICAL_PADDING: f32 = 4.0;
pub(crate) const CONTEXT_MENU_VIEWPORT_MARGIN: f32 = 8.0;
pub(crate) const CONTEXT_MENU_ICON_SIZE: f32 = 18.0;
pub(crate) const CONTEXT_MENU_TEXT_LINE_HEIGHT: f32 = 20.0;
pub(crate) const CONTEXT_MENU_SUBMENU_AIM_DELAY: Duration = Duration::from_millis(220);
pub(crate) const CONTEXT_MENU_SAFE_TRIANGLE_MARGIN: f32 = 8.0;
pub(crate) const CONTEXT_MENU_SAFE_TRIANGLE_PARENT_RETENTION_WIDTH: f32 = 84.0;
pub(crate) const PROPERTIES_OVERLAY_WIDTH: f32 = 440.0;
#[cfg(test)]
pub(crate) const PROPERTIES_OVERLAY_MARGIN: f32 = 18.0;
pub(crate) const PROPERTIES_TITLE_HEIGHT: f32 = 44.0;
pub(crate) const PROPERTIES_ROW_HEIGHT: f32 = 26.0;
pub(crate) const TASK_DETAIL_DIALOG_WIDTH: f32 = 620.0;
#[cfg(test)]
pub(crate) const TASK_DETAIL_DIALOG_MARGIN: f32 = 18.0;
pub(crate) const TASK_DETAIL_TITLE_HEIGHT: f32 = 46.0;
pub(crate) const TASK_DETAIL_ROW_HEIGHT: f32 = 72.0;
pub(crate) const TASK_DETAIL_BUTTON_WIDTH: f32 = 92.0;
pub(crate) const TASK_DETAIL_BUTTON_HEIGHT: f32 = 28.0;
pub(crate) const TASK_DETAIL_BUTTON_GAP: f32 = 8.0;
pub(crate) const CREATE_DIALOG_WIDTH: f32 = 420.0;
pub(crate) const CREATE_DIALOG_HEIGHT: f32 = 208.0;
pub(crate) const CREATE_DIALOG_TITLE_HEIGHT: f32 = 44.0;
pub(crate) const CREATE_DIALOG_BUTTON_WIDTH: f32 = 84.0;
pub(crate) const CREATE_DIALOG_BUTTON_HEIGHT: f32 = 28.0;
pub(crate) const CREATE_DIALOG_BUTTON_GAP: f32 = 8.0;
pub(crate) const RENAME_DIALOG_WIDTH: f32 = 420.0;
pub(crate) const RENAME_DIALOG_HEIGHT: f32 = 180.0;
pub(crate) const RENAME_DIALOG_TITLE_HEIGHT: f32 = 44.0;
pub(crate) const OPEN_WITH_CHOOSER_WIDTH: f32 = 450.0;
pub(crate) const OPEN_WITH_CHOOSER_TITLE_HEIGHT: f32 = 46.0;
pub(crate) const OPEN_WITH_CHOOSER_QUERY_HEIGHT: f32 = 36.0;
pub(crate) const OPEN_WITH_CHOOSER_ROW_HEIGHT: f32 = 48.0;
pub(crate) const OPEN_WITH_CHOOSER_MAX_ROWS: usize = 8;
pub(crate) const OPEN_WITH_CHOOSER_BUTTON_WIDTH: f32 = 92.0;
pub(crate) const OPEN_WITH_CHOOSER_BUTTON_HEIGHT: f32 = 28.0;
pub(crate) const OPEN_WITH_CHOOSER_BUTTON_GAP: f32 = 8.0;
pub(crate) const TRASH_CONFLICT_DIALOG_WIDTH: f32 = 520.0;
pub(crate) const TRASH_CONFLICT_DIALOG_HEIGHT: f32 = 224.0;
#[cfg(test)]
pub(crate) const TRASH_CONFLICT_DIALOG_MARGIN: f32 = 18.0;
pub(crate) const TRASH_CONFLICT_DIALOG_TITLE_HEIGHT: f32 = 44.0;
pub(crate) const PATH_HISTORY_LIMIT: usize = 128;
pub(crate) const FILE_MANAGER_ZOOM_LEVEL_MIN: i32 = 0;
pub(crate) const FILE_MANAGER_ZOOM_LEVEL_MAX: i32 = 16;
pub(crate) const FILE_MANAGER_ICONS_PREVIEW_ZOOM_LEVEL_DEFAULT: i32 = 4;
pub(crate) const FILE_MANAGER_COMPACT_PREVIEW_ZOOM_LEVEL_DEFAULT: i32 = 3;
pub(crate) const FILE_MANAGER_DETAILS_PREVIEW_ZOOM_LEVEL_DEFAULT: i32 = 3;
pub(crate) const FILE_MANAGER_ICONS_RESET_ZOOM_LEVEL_DEFAULT: i32 = 2;
pub(crate) const FILE_MANAGER_COMPACT_RESET_ZOOM_LEVEL_DEFAULT: i32 = 0;
pub(crate) const FILE_MANAGER_DETAILS_RESET_ZOOM_LEVEL_DEFAULT: i32 = 0;
pub(crate) const AUTO_CYCLE_INTERVAL: Duration = Duration::from_secs(1);
pub(crate) const DOUBLE_CLICK_MAX_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const DOUBLE_CLICK_MAX_DISTANCE: f32 = 6.0;
pub(crate) const SHELL_PANE_ID: PaneId = PaneId(1);

pub(crate) fn scaled_dialog_metric(value: f32, scale_factor: f32) -> f32 {
    (value * scale_factor.max(1.0)).round().max(1.0)
}

pub(crate) fn normalized_scale_factor(scale_factor: f32) -> f32 {
    if scale_factor.is_finite() {
        scale_factor.clamp(0.5, 4.0)
    } else {
        1.0
    }
}
