use std::ops::Range;

use crate::ui::metrics::{
    FILE_MANAGER_ZOOM_LEVEL_MAX, FILE_MANAGER_ZOOM_LEVEL_MIN, THUMBNAIL_READ_AHEAD_PAGES,
    THUMBNAIL_READ_AHEAD_RESOLVE_LIMIT,
};
use crate::ui::pane::ShellPaneProjection;

pub(crate) mod item_paint;
pub(crate) mod style;
pub(crate) mod text;
pub(crate) mod text_layout;

pub(crate) fn file_manager_icon_size_for_zoom_level(level: i32) -> f32 {
    match level.clamp(FILE_MANAGER_ZOOM_LEVEL_MIN, FILE_MANAGER_ZOOM_LEVEL_MAX) {
        0 => 16.0,
        1 => 22.0,
        2 => 32.0,
        3 => 48.0,
        4 => 64.0,
        level => (64 + ((level - 4) << 4)) as f32,
    }
}

pub(crate) fn file_manager_zoom_level_for_icon_size(size: u16) -> i32 {
    let level = match size {
        16 => 0,
        22 => 1,
        32 => 2,
        48 => 3,
        64 => 4,
        size => 4 + ((i32::from(size) - 64) >> 4),
    };
    level.clamp(FILE_MANAGER_ZOOM_LEVEL_MIN, FILE_MANAGER_ZOOM_LEVEL_MAX)
}

pub(crate) fn file_manager_icons_zoom_factor_for_level(level: i32) -> f32 {
    (level.clamp(FILE_MANAGER_ZOOM_LEVEL_MIN, FILE_MANAGER_ZOOM_LEVEL_MAX) as f32 / 13.0).exp()
}

pub(crate) fn file_manager_icons_item_width(
    icon_size: f32,
    padding: f32,
    text_width_index: f32,
    average_char_width: f32,
    ui_scale: f32,
    zoom_level: i32,
) -> f32 {
    let zoom_factor = file_manager_icons_zoom_factor_for_level(zoom_level);
    let font_factor = average_char_width / 9.0;
    ((16.0 * ui_scale + text_width_index * 64.0 * font_factor * zoom_factor)
        .max(icon_size + padding * 2.0 * zoom_factor))
    .floor()
    .max(1.0)
}

pub(crate) fn visible_layout_range_for_projection(
    projection: &ShellPaneProjection<'_>,
) -> Option<Range<usize>> {
    let start = projection
        .visible_items
        .iter()
        .map(|item| item.layout.model_index)
        .min()?;
    let end = projection
        .visible_items
        .iter()
        .map(|item| item.layout.model_index)
        .max()?
        + 1;
    (start < end).then_some(start..end)
}

pub(crate) fn shell_file_manager_deferred_all_indexes(
    visible_indexes: Option<Range<usize>>,
    item_count: usize,
) -> Vec<usize> {
    let Some(visible_indexes) = visible_indexes else {
        return (0..item_count).collect();
    };

    let visible_start = visible_indexes.start.min(item_count);
    let visible_end = visible_indexes.end.min(item_count).max(visible_start);
    let visible_len = visible_end.saturating_sub(visible_start);
    let mut result = Vec::with_capacity(item_count.saturating_sub(visible_len));
    result.extend(0..visible_start);
    result.extend(visible_end..item_count);
    result
}

pub(crate) fn shell_file_manager_read_ahead_indexes(
    visible_indexes: Range<usize>,
    item_count: usize,
    maximum_visible_items: usize,
) -> Vec<usize> {
    if item_count == 0 || visible_indexes.is_empty() {
        return Vec::new();
    }

    let visible_start = visible_indexes.start.min(item_count);
    let visible_end = visible_indexes.end.min(item_count).max(visible_start);
    if visible_start >= visible_end {
        return Vec::new();
    }

    let maximum_visible_items = maximum_visible_items.max(1);
    let read_ahead_items = (THUMBNAIL_READ_AHEAD_PAGES * maximum_visible_items)
        .min(THUMBNAIL_READ_AHEAD_RESOLVE_LIMIT / 2);
    let last_visible = visible_end - 1;
    let end_extended = (last_visible + read_ahead_items).min(item_count - 1);
    let begin_extended = visible_start.saturating_sub(read_ahead_items);

    let mut result = Vec::new();
    result.extend(visible_end..end_extended + 1);
    result.extend((begin_extended..visible_start).rev());

    let last_page_start = (end_extended + 1).max(item_count.saturating_sub(maximum_visible_items));
    result.extend(last_page_start..item_count);

    let first_page_end = begin_extended.min(maximum_visible_items);
    result.extend(0..first_page_end);

    let mut remaining = THUMBNAIL_READ_AHEAD_RESOLVE_LIMIT.saturating_sub(result.len());
    let rest_after_visible = (end_extended + 1)..last_page_start;
    let rest_after_len = rest_after_visible.len().min(remaining);
    result.extend(rest_after_visible.take(rest_after_len));
    remaining = remaining.saturating_sub(rest_after_len);

    result.extend((first_page_end..begin_extended).rev().take(remaining));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_level_icon_sizes_match_dolphin_zoom_level_info() {
        let sizes = (0..=16)
            .map(|level| file_manager_icon_size_for_zoom_level(level) as i32)
            .collect::<Vec<_>>();

        assert_eq!(
            sizes,
            vec![
                16, 22, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 256,
            ]
        );

        for level in 0..=16 {
            let size = file_manager_icon_size_for_zoom_level(level) as u16;
            assert_eq!(file_manager_zoom_level_for_icon_size(size), level);
        }
        assert_eq!(file_manager_zoom_level_for_icon_size(40), 2);
        assert_eq!(file_manager_zoom_level_for_icon_size(72), 4);
        assert_eq!(file_manager_zoom_level_for_icon_size(280), 16);
    }

    #[test]
    fn deferred_all_indexes_exclude_visible_range() {
        let indexes = shell_file_manager_deferred_all_indexes(Some(4..7), 10);

        assert_eq!(indexes, vec![0, 1, 2, 3, 7, 8, 9]);
    }

    #[test]
    fn deferred_all_indexes_keep_all_items_without_visible_range() {
        let indexes = shell_file_manager_deferred_all_indexes(None, 4);

        assert_eq!(indexes, vec![0, 1, 2, 3]);
    }

    #[test]
    fn deferred_all_indexes_clamp_visible_range_to_item_count() {
        let indexes = shell_file_manager_deferred_all_indexes(Some(2..8), 5);

        assert_eq!(indexes, vec![0, 1]);
    }
}
