#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RangeSelection {
    indexes: Vec<usize>,
}

impl RangeSelection {
    pub fn indexes(&self) -> &[usize] {
        &self.indexes
    }

    pub fn range(&self) -> Option<Range<usize>> {
        let first = *self.indexes.first()?;
        let last = *self.indexes.last()?;
        Some(first..last + 1)
    }
}

fn rows_per_column(options: CompactLayoutOptions) -> usize {
    (compact_available_height(options) / options.item_height)
        .floor()
        .max(1.0) as usize
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CompactRowMetrics {
    offset: f32,
    pitch: f32,
    gap: f32,
}

fn compact_row_metrics(
    options: CompactLayoutOptions,
    item_count: usize,
    rows_per_column: usize,
) -> CompactRowMetrics {
    let item_height = options.item_height.max(1.0);
    let rows_per_column = rows_per_column.max(1);
    let available = compact_available_height(options);
    let should_distribute = item_count > rows_per_column && item_height >= 32.0;
    let unused = (available - rows_per_column as f32 * item_height).max(0.0);
    let gap = if should_distribute && unused > 0.0 {
        unused / (rows_per_column as f32 + 1.0)
    } else {
        0.0
    };
    CompactRowMetrics {
        offset: gap,
        pitch: item_height + gap,
        gap,
    }
}

fn compact_available_height(options: CompactLayoutOptions) -> f32 {
    (options.viewport_height - options.reserved_bottom).max(options.item_height.max(1.0))
}

fn columns_per_row(options: IconsLayoutOptions) -> usize {
    let item_margin = options.gap.max(0.0);
    let item_width = options.item_width.max(1.0);
    let column_width = item_width + item_margin;
    let width_for_columns = (options.viewport_width - item_margin).max(column_width);
    (width_for_columns / column_width).floor().max(1.0) as usize
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct IconsColumnMetrics {
    start_x: f32,
    pitch: f32,
}

fn icons_column_metrics(
    options: IconsLayoutOptions,
    item_count: usize,
    columns_per_row: usize,
) -> IconsColumnMetrics {
    let item_margin = options.gap.max(0.0);
    let item_width = options.item_width.max(1.0);
    let mut pitch = item_width + item_margin;
    let width_for_columns = (options.viewport_width - item_margin).max(pitch);
    let mut start_x = item_margin;

    if item_count > columns_per_row && pitch >= 32.0 {
        let unused_width = width_for_columns - columns_per_row as f32 * pitch;
        if unused_width > 0.0 {
            let column_inc = unused_width / (columns_per_row as f32 + 1.0);
            pitch += column_inc;
            start_x += column_inc;
        }
    }

    IconsColumnMetrics { start_x, pitch }
}

fn icons_content_width(
    options: IconsLayoutOptions,
    item_count: usize,
    columns_per_row: usize,
    column_metrics: IconsColumnMetrics,
) -> f32 {
    let visible_columns = item_count.min(columns_per_row);
    if visible_columns == 0 {
        return EMPTY_CONTENT_EXTENT;
    }
    (column_metrics.start_x
        + visible_columns.saturating_sub(1) as f32 * column_metrics.pitch
        + options.item_width
        + options.gap.max(0.0))
    .max(options.viewport_width)
}

fn normalize_column_widths(
    column_count: usize,
    min_width: f32,
    supplied: Arc<[f32]>,
) -> Arc<[f32]> {
    if column_count == 0 {
        return Arc::from(Vec::<f32>::new());
    }

    let min_width = min_width.max(1.0);
    let mut widths = Vec::with_capacity(column_count);
    widths.extend(
        supplied
            .iter()
            .take(column_count)
            .map(|width| width.max(min_width)),
    );
    widths.resize(column_count, min_width);
    Arc::from(widths)
}

fn column_offsets(padding: f32, gap: f32, column_widths: &[f32]) -> Arc<[f32]> {
    let mut offsets = Vec::with_capacity(column_widths.len());
    let mut x = padding;
    for width in column_widths {
        offsets.push(x);
        x += *width + gap;
    }
    Arc::from(offsets)
}

fn first_column_with_right_after(offsets: &[f32], widths: &[f32], x: f32) -> usize {
    let mut low = 0usize;
    let mut high = offsets.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if offsets[mid] + widths[mid] <= x {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

fn first_column_starting_at_or_after(offsets: &[f32], x: f32) -> usize {
    let mut low = 0usize;
    let mut high = offsets.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if offsets[mid] < x {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

fn visible_axis_range(
    visible_start: f32,
    visible_end: f32,
    padding: f32,
    item_extent: f32,
    gap: f32,
    count: usize,
) -> Range<usize> {
    if count == 0 || visible_end <= visible_start {
        return 0..0;
    }
    let pitch = item_extent + gap;
    if pitch <= 0.0 || item_extent <= 0.0 {
        return 0..0;
    }
    let start = ((visible_start - padding - item_extent) / pitch).floor() as isize + 1;
    let end = ((visible_end - padding) / pitch).ceil() as isize;
    start.max(0) as usize..(end.max(0) as usize).min(count)
}

fn first_row_with_bottom_after(offsets: &[f32], heights: &[f32], y: f32) -> usize {
    let mut low = 0usize;
    let mut high = offsets.len().min(heights.len());
    while low < high {
        let mid = low + (high - low) / 2;
        if offsets[mid] + heights[mid] <= y {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

fn first_row_starting_at_or_after(offsets: &[f32], y: f32) -> usize {
    let mut low = 0usize;
    let mut high = offsets.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if offsets[mid] < y {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

