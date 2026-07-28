#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutgoingDndPreviewKind {
    FileManagerItem,
    FileManagerGrid {
        columns: usize,
        item_count: usize,
        stride: i32,
    },
}

const DND_FALLBACK_LABEL_WIDTH: f32 = 160.0;
const DND_FALLBACK_LABEL_HEIGHT: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct OutgoingDndPreviewMetrics {
    canvas_width: u32,
    canvas_height: u32,
    icon_size: u32,
    /// Scene-space size used to reuse the live thumbnail/theme source.
    cache_icon_size: f32,
    buffer_scale: i32,
    icon_rect: PixelRect,
    label_rect: Option<PixelRect>,
    background_rect: Option<PixelRect>,
    background_radius: i32,
    label_style: Option<DragPreviewLabelStyle>,
    hotspot_x: i32,
    hotspot_y: i32,
    hotspot_logical_x: i32,
    hotspot_logical_y: i32,
    kind: OutgoingDndPreviewKind,
    background_color: [u8; 4],
}

impl OutgoingDndPreviewMetrics {
    fn visible_icon_count(self) -> usize {
        match self.kind {
            OutgoingDndPreviewKind::FileManagerItem => 1,
            OutgoingDndPreviewKind::FileManagerGrid { item_count, .. } => item_count,
        }
    }

    fn icon_rect_at(self, index: usize) -> Option<PixelRect> {
        match self.kind {
            OutgoingDndPreviewKind::FileManagerItem if index == 0 => Some(self.icon_rect),
            OutgoingDndPreviewKind::FileManagerGrid {
                columns,
                item_count,
                stride,
            } if index < item_count => {
                let column = index % columns;
                let row = index / columns;
                Some(PixelRect::new(
                    column as i32 * stride,
                    row as i32 * stride,
                    self.icon_size as i32,
                    self.icon_size as i32,
                ))
            }
            _ => None,
        }
    }
}

fn outgoing_dnd_preview_metrics_for_layout(
    layout: SingleDragPreviewLayout,
    scale: f32,
) -> OutgoingDndPreviewMetrics {
    let logical_scale = normalized_scale_factor(scale).max(1.0);
    let buffer_scale = logical_scale.round().max(1.0) as i32;
    let factor = buffer_scale as f32 / logical_scale;
    let mut canvas_width = (layout.bounds.width * factor).round().max(1.0) as u32;
    let mut canvas_height = (layout.bounds.height * factor).round().max(1.0) as u32;
    canvas_width = align_preview_dimension(canvas_width, buffer_scale);
    canvas_height = align_preview_dimension(canvas_height, buffer_scale);
    let icon_rect = map_preview_rect(layout.icon, factor);
    let label_rect = layout.label.map(|label| map_preview_rect(label.rect, factor));
    let background_rect = Some(map_preview_rect(layout.background, factor));
    let (hotspot_x, hotspot_y, hotspot_logical_x, hotspot_logical_y) =
        if layout.view_mode.is_some() {
            file_manager_pane_hotspot(canvas_width, buffer_scale)
        } else {
            places_press_hotspot(layout.hotspot, logical_scale, buffer_scale)
        };
    OutgoingDndPreviewMetrics {
        canvas_width,
        canvas_height,
        icon_size: icon_rect.width.max(icon_rect.height) as u32,
        cache_icon_size: layout.icon.width.max(layout.icon.height).clamp(16.0, 256.0),
        buffer_scale,
        icon_rect,
        label_rect,
        background_rect,
        background_radius: (layout.radius * factor).round() as i32,
        label_style: layout.label.map(|label| label.style),
        hotspot_x,
        hotspot_y,
        hotspot_logical_x,
        hotspot_logical_y,
        kind: OutgoingDndPreviewKind::FileManagerItem,
        background_color: [0, 0, 0, 0],
    }
}

fn outgoing_dnd_preview_metrics_for_multi_layout(
    layout: MultiDragPreviewLayout,
    scale: f32,
) -> OutgoingDndPreviewMetrics {
    let logical_scale = normalized_scale_factor(scale).max(1.0);
    let buffer_scale = logical_scale.round().max(1.0) as i32;
    let factor = buffer_scale as f32 / logical_scale;
    let mut canvas_width = (layout.bounds.width * factor).round().max(1.0) as u32;
    let mut canvas_height = (layout.bounds.height * factor).round().max(1.0) as u32;
    canvas_width = align_preview_dimension(canvas_width, buffer_scale);
    canvas_height = align_preview_dimension(canvas_height, buffer_scale);
    let icon_rect = layout
        .cell_rect(0)
        .map(|cell| map_preview_rect(cell, factor))
        .unwrap_or_else(|| PixelRect::new(0, 0, buffer_scale, buffer_scale));
    let stride = ((layout.icon_size + layout.gap) * factor).round().max(1.0) as i32;
    let (hotspot_x, hotspot_y, hotspot_logical_x, hotspot_logical_y) =
        file_manager_pane_hotspot(canvas_width, buffer_scale);
    OutgoingDndPreviewMetrics {
        canvas_width,
        canvas_height,
        icon_size: icon_rect.width.max(icon_rect.height) as u32,
        cache_icon_size: layout.icon_size.clamp(16.0, 256.0),
        buffer_scale,
        icon_rect,
        label_rect: None,
        background_rect: None,
        background_radius: 0,
        label_style: None,
        hotspot_x,
        hotspot_y,
        hotspot_logical_x,
        hotspot_logical_y,
        kind: OutgoingDndPreviewKind::FileManagerGrid {
            columns: layout.columns,
            item_count: layout.item_count,
            stride,
        },
        background_color: [0, 0, 0, 0],
    }
}

/// FileManager `KItemListController::startDragging`: hotspot is the top-center of
/// the final pixmap in logical surface coordinates.
fn file_manager_pane_hotspot(canvas_width: u32, buffer_scale: i32) -> (i32, i32, i32, i32) {
    let scale = buffer_scale.max(1);
    let logical_width = canvas_width as i32 / scale;
    let hotspot_logical_x = logical_width / 2;
    let hotspot_x = hotspot_logical_x * scale;
    (hotspot_x, 0, hotspot_logical_x, 0)
}

fn places_press_hotspot(
    hotspot: fika_core::ViewPoint,
    logical_scale: f32,
    buffer_scale: i32,
) -> (i32, i32, i32, i32) {
    let scale = buffer_scale.max(1) as f32;
    let factor = scale / logical_scale.max(1.0);
    let hotspot_logical_x = (hotspot.x / logical_scale.max(1.0)).round() as i32;
    let hotspot_logical_y = (hotspot.y / logical_scale.max(1.0)).round() as i32;
    let hotspot_x = (hotspot.x * factor).round() as i32;
    let hotspot_y = (hotspot.y * factor).round() as i32;
    (hotspot_x, hotspot_y, hotspot_logical_x, hotspot_logical_y)
}

fn map_preview_rect(rect: fika_core::ViewRect, factor: f32) -> PixelRect {
    let x = (rect.x * factor).round() as i32;
    let y = (rect.y * factor).round() as i32;
    let right = (rect.right() * factor).round() as i32;
    let bottom = (rect.bottom() * factor).round() as i32;
    PixelRect::new(x, y, (right - x).max(1), (bottom - y).max(1))
}

fn outgoing_dnd_fallback_preview_metrics(scale: f32) -> OutgoingDndPreviewMetrics {
    let icon_size = (DND_FALLBACK_ICON_SIZE * normalized_scale_factor(scale).max(1.0)) as u32;
    let mut metrics = outgoing_dnd_preview_metrics(icon_size, scale);
    let label_width = scaled_preview_dimension(DND_FALLBACK_LABEL_WIDTH, metrics.buffer_scale);
    let label_height = scaled_preview_dimension(DND_FALLBACK_LABEL_HEIGHT, metrics.buffer_scale);
    metrics.canvas_width = align_preview_dimension(
        metrics.canvas_width.max(label_width),
        metrics.buffer_scale,
    );
    metrics.icon_rect.x = (metrics.canvas_width as i32 - metrics.icon_size as i32) / 2;
    let icon_bottom = metrics.canvas_height;
    metrics.label_rect = Some(PixelRect::new(
        0,
        icon_bottom as i32,
        metrics.canvas_width as i32,
        label_height as i32,
    ));
    metrics.canvas_height = align_preview_dimension(
        icon_bottom.saturating_add(label_height),
        metrics.buffer_scale,
    );
    metrics.label_style = Some(DragPreviewLabelStyle::PlainSingleLine);
    let (hotspot_x, hotspot_y, hotspot_logical_x, hotspot_logical_y) =
        file_manager_pane_hotspot(metrics.canvas_width, metrics.buffer_scale);
    metrics.hotspot_x = hotspot_x;
    metrics.hotspot_y = hotspot_y;
    metrics.hotspot_logical_x = hotspot_logical_x;
    metrics.hotspot_logical_y = hotspot_logical_y;
    metrics
}

fn outgoing_dnd_preview_metrics(icon_size: u32, scale: f32) -> OutgoingDndPreviewMetrics {
    let logical_scale = normalized_scale_factor(scale).max(1.0);
    let buffer_scale = logical_scale.round().max(1.0) as i32;
    let logical_icon_size = (icon_size as f32 / logical_scale).clamp(16.0, 256.0);
    let icon_size = scaled_preview_dimension(logical_icon_size, buffer_scale);
    let canvas_width = align_preview_dimension(icon_size, buffer_scale);
    let (hotspot_x, hotspot_y, hotspot_logical_x, hotspot_logical_y) =
        file_manager_pane_hotspot(canvas_width, buffer_scale);
    OutgoingDndPreviewMetrics {
        canvas_width,
        canvas_height: canvas_width,
        icon_size,
        cache_icon_size: logical_icon_size,
        buffer_scale,
        icon_rect: PixelRect::new(0, 0, icon_size as i32, icon_size as i32),
        label_rect: None,
        background_rect: None,
        background_radius: 0,
        label_style: None,
        hotspot_x,
        hotspot_y,
        hotspot_logical_x,
        hotspot_logical_y,
        kind: OutgoingDndPreviewKind::FileManagerItem,
        background_color: [0, 0, 0, 0],
    }
}

fn scaled_preview_dimension(logical: f32, buffer_scale: i32) -> u32 {
    let scale = buffer_scale.max(1) as f32;
    align_preview_dimension((logical.max(1.0) * scale).round().max(1.0) as u32, buffer_scale)
}

fn align_preview_dimension(value: u32, buffer_scale: i32) -> u32 {
    let scale = buffer_scale.max(1) as u32;
    value.max(scale).div_ceil(scale) * scale
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PixelRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl PixelRect {
    fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width: width.max(1),
            height: height.max(1),
        }
    }

}

fn external_drag_paths_from_typed_data(value: &dyn TypedData) -> Result<Vec<PathBuf>, String> {
    if !value
        .type_()
        .hint()
        .is_some_and(|hint| TypeHint::UriList.matches(&hint))
    {
        return Err("received non-uri-list data".to_string());
    }
    let uris = value
        .try_as_uris()
        .map_err(|error| format!("read uri-list: {error}"))?;
    Ok(external_drag_paths_from_uris(uris))
}

fn external_drag_paths_from_uris(uris: Vec<String>) -> Vec<PathBuf> {
    let text = uris.join("\n");
    decode_file_clipboard_text(&text)
        .map(|payload| payload.paths)
        .unwrap_or_default()
}

fn external_drag_drop_sources(
    event_paths: Vec<PathBuf>,
    tracked_sources: Option<Vec<PathBuf>>,
) -> Vec<PathBuf> {
    if event_paths.is_empty() {
        tracked_sources.unwrap_or_default()
    } else {
        event_paths
    }
}
