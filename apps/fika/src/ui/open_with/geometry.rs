use crate::windowing::PhysicalSize;
use fika_core::{ViewPoint, ViewRect};

use crate::ui::metrics::{
    OPEN_WITH_CHOOSER_BUTTON_GAP, OPEN_WITH_CHOOSER_BUTTON_HEIGHT, OPEN_WITH_CHOOSER_BUTTON_WIDTH,
    OPEN_WITH_CHOOSER_MAX_ROWS, OPEN_WITH_CHOOSER_QUERY_HEIGHT, OPEN_WITH_CHOOSER_ROW_HEIGHT,
    OPEN_WITH_CHOOSER_TITLE_HEIGHT, OPEN_WITH_CHOOSER_WIDTH, TEXT_FONT_SIZE, TEXT_LINE_HEIGHT,
    normalized_scale_factor, scaled_dialog_metric,
};

use super::{OpenWithChooserClick, OpenWithChooserPointerRole, ShellOpenWithChooser};

#[cfg(test)]
pub(crate) fn open_with_chooser_rect(
    chooser: &ShellOpenWithChooser,
    size: PhysicalSize<u32>,
) -> ViewRect {
    open_with_chooser_rect_scaled(chooser, size, 1.0)
}

pub(crate) fn open_with_chooser_rect_scaled(
    _chooser: &ShellOpenWithChooser,
    size: PhysicalSize<u32>,
    _scale_factor: f32,
) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}

pub(crate) fn open_with_chooser_window_size_scaled(
    _chooser: &ShellOpenWithChooser,
    scale_factor: f32,
) -> PhysicalSize<u32> {
    let content_height = open_with_chooser_content_height_scaled(scale_factor);
    let status_height = scaled_dialog_metric(26.0, scale_factor);
    let dialog_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_TITLE_HEIGHT, scale_factor)
        + scaled_dialog_metric(16.0, scale_factor)
        + scaled_dialog_metric(OPEN_WITH_CHOOSER_QUERY_HEIGHT, scale_factor)
        + scaled_dialog_metric(10.0, scale_factor)
        + content_height
        + scaled_dialog_metric(38.0, scale_factor)
        + status_height
        + scaled_dialog_metric(52.0, scale_factor);
    PhysicalSize::new(
        scaled_dialog_metric(OPEN_WITH_CHOOSER_WIDTH, scale_factor)
            .ceil()
            .max(1.0) as u32,
        dialog_height.ceil().max(1.0) as u32,
    )
}

pub(crate) fn open_with_chooser_visible_row_count(chooser: &ShellOpenWithChooser) -> usize {
    let _ = chooser;
    OPEN_WITH_CHOOSER_MAX_ROWS
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn open_with_chooser_query_rect(dialog_rect: ViewRect) -> ViewRect {
    open_with_chooser_query_rect_scaled(dialog_rect, 1.0)
}

pub(crate) fn open_with_chooser_query_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    let margin = scaled_dialog_metric(16.0, scale_factor);
    ViewRect {
        x: dialog_rect.x + margin,
        y: dialog_rect.y
            + scaled_dialog_metric(OPEN_WITH_CHOOSER_TITLE_HEIGHT, scale_factor)
            + margin,
        width: (dialog_rect.width - margin * 2.0).max(1.0),
        height: scaled_dialog_metric(OPEN_WITH_CHOOSER_QUERY_HEIGHT, scale_factor),
    }
}

pub(crate) fn open_with_chooser_query_text_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    let query = open_with_chooser_query_rect_scaled(dialog_rect, scale_factor);
    let search_icon_right = query.x
        + scaled_dialog_metric(10.0, scale_factor)
        + scaled_dialog_metric(14.0, scale_factor);
    ViewRect {
        x: search_icon_right + scaled_dialog_metric(8.0, scale_factor),
        y: query.y + (query.height - scaled_dialog_metric(TEXT_LINE_HEIGHT, scale_factor)) / 2.0,
        width: (query.right() - search_icon_right - scaled_dialog_metric(18.0, scale_factor))
            .max(1.0),
        height: scaled_dialog_metric(TEXT_LINE_HEIGHT, scale_factor),
    }
}

#[cfg(test)]
pub(crate) fn open_with_chooser_list_rect(
    dialog_rect: ViewRect,
    chooser: &ShellOpenWithChooser,
) -> ViewRect {
    open_with_chooser_list_rect_scaled(dialog_rect, chooser, 1.0)
}

pub(crate) fn open_with_chooser_list_rect_scaled(
    dialog_rect: ViewRect,
    _chooser: &ShellOpenWithChooser,
    scale_factor: f32,
) -> ViewRect {
    let margin = scaled_dialog_metric(16.0, scale_factor);
    let query = open_with_chooser_query_rect_scaled(dialog_rect, scale_factor);
    ViewRect {
        x: dialog_rect.x + margin,
        y: query.bottom() + scaled_dialog_metric(10.0, scale_factor),
        width: (dialog_rect.width - margin * 2.0).max(1.0),
        height: open_with_chooser_content_height_scaled(scale_factor),
    }
}

fn open_with_chooser_content_height_scaled(scale_factor: f32) -> f32 {
    OPEN_WITH_CHOOSER_MAX_ROWS as f32
        * scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, scale_factor)
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn open_with_chooser_default_checkbox_rect(
    dialog_rect: ViewRect,
    chooser: &ShellOpenWithChooser,
) -> ViewRect {
    open_with_chooser_default_checkbox_rect_scaled(dialog_rect, chooser, 1.0)
}

pub(crate) fn open_with_chooser_default_checkbox_rect_scaled(
    dialog_rect: ViewRect,
    chooser: &ShellOpenWithChooser,
    scale_factor: f32,
) -> ViewRect {
    let margin = scaled_dialog_metric(16.0, scale_factor);
    let list = open_with_chooser_list_rect_scaled(dialog_rect, chooser, scale_factor);
    ViewRect {
        x: dialog_rect.x + margin,
        y: list.bottom() + scaled_dialog_metric(8.0, scale_factor),
        width: (dialog_rect.width - margin * 2.0).max(1.0),
        height: scaled_dialog_metric(24.0, scale_factor),
    }
}

pub(crate) fn open_with_chooser_scrollbar_rects_scaled(
    list_rect: ViewRect,
    chooser: &ShellOpenWithChooser,
    scale_factor: f32,
) -> Option<(ViewRect, ViewRect)> {
    let total = chooser.tree_row_count();
    let visible = open_with_chooser_visible_row_count(chooser);
    if total <= visible {
        return None;
    }
    let margin = scaled_dialog_metric(6.0, scale_factor);
    let width = scaled_dialog_metric(4.0, scale_factor).max(2.0);
    let track = ViewRect {
        x: list_rect.right() - margin - width,
        y: list_rect.y + margin,
        width,
        height: (list_rect.height - margin * 2.0).max(1.0),
    };
    let thumb_height = (track.height * visible as f32 / total as f32)
        .max(scaled_dialog_metric(28.0, scale_factor))
        .min(track.height);
    let max_scroll = total.saturating_sub(visible).max(1);
    let ratio = (chooser.scroll_row.min(max_scroll) as f32 / max_scroll as f32).clamp(0.0, 1.0);
    let travel = (track.height - thumb_height).max(0.0);
    let thumb = ViewRect {
        x: track.x,
        y: track.y + travel * ratio,
        width: track.width,
        height: thumb_height,
    };
    Some((track, thumb))
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn open_with_chooser_cancel_button_rect(dialog_rect: ViewRect) -> ViewRect {
    open_with_chooser_cancel_button_rect_scaled(dialog_rect, 1.0)
}

pub(crate) fn open_with_chooser_cancel_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    let right = dialog_rect.right() - scaled_dialog_metric(16.0, scale_factor);
    let button_width = scaled_dialog_metric(OPEN_WITH_CHOOSER_BUTTON_WIDTH, scale_factor);
    let button_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_BUTTON_HEIGHT, scale_factor);
    ViewRect {
        x: right
            - button_width * 2.0
            - scaled_dialog_metric(OPEN_WITH_CHOOSER_BUTTON_GAP, scale_factor),
        y: dialog_rect.bottom() - scaled_dialog_metric(16.0, scale_factor) - button_height,
        width: button_width,
        height: button_height,
    }
}

#[cfg(test)]
pub(crate) fn open_with_chooser_open_button_rect(dialog_rect: ViewRect) -> ViewRect {
    open_with_chooser_open_button_rect_scaled(dialog_rect, 1.0)
}

pub(crate) fn open_with_chooser_open_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    let right = dialog_rect.right() - scaled_dialog_metric(16.0, scale_factor);
    let button_width = scaled_dialog_metric(OPEN_WITH_CHOOSER_BUTTON_WIDTH, scale_factor);
    let button_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_BUTTON_HEIGHT, scale_factor);
    ViewRect {
        x: right - button_width,
        y: dialog_rect.bottom() - scaled_dialog_metric(16.0, scale_factor) - button_height,
        width: button_width,
        height: button_height,
    }
}

pub(crate) fn open_with_scroll_delta_rows(delta_y: f32, scale_factor: f32) -> Option<isize> {
    if delta_y.abs() <= f32::EPSILON {
        return None;
    }
    let row_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, scale_factor).max(1.0);
    let rows = (delta_y.abs() / row_height).ceil().max(1.0) as isize;
    Some(if delta_y > 0.0 { rows } else { -rows })
}

pub(crate) fn open_with_chooser_query_contains_point(
    chooser: &ShellOpenWithChooser,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> bool {
    let rect = open_with_chooser_rect_scaled(chooser, size, scale_factor);
    open_with_chooser_query_rect_scaled(rect, scale_factor).contains(point)
}

pub(crate) fn open_with_chooser_click_at_point(
    chooser: &ShellOpenWithChooser,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> OpenWithChooserClick {
    let rect = open_with_chooser_rect_scaled(chooser, size, scale_factor);
    if !rect.contains(point) {
        return OpenWithChooserClick::Outside;
    }
    if open_with_chooser_cancel_button_rect_scaled(rect, scale_factor).contains(point) {
        return OpenWithChooserClick::Cancel;
    }
    if open_with_chooser_open_button_rect_scaled(rect, scale_factor).contains(point)
        && chooser.selected_application().is_some()
    {
        return OpenWithChooserClick::Open;
    }
    if open_with_chooser_default_checkbox_rect_scaled(rect, chooser, scale_factor).contains(point) {
        return OpenWithChooserClick::ToggleDefault;
    }
    if open_with_chooser_query_rect_scaled(rect, scale_factor).contains(point) {
        let text_rect = open_with_chooser_query_text_rect_scaled(rect, scale_factor);
        let cursor = chooser.query_cursor_for_text_offset(
            point.x - text_rect.x,
            TEXT_FONT_SIZE * normalized_scale_factor(scale_factor),
        );
        return OpenWithChooserClick::Query(cursor);
    }
    let list = open_with_chooser_list_rect_scaled(rect, chooser, scale_factor);
    if list.contains(point) {
        let visible_row = ((point.y - list.y)
            / scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, scale_factor))
        .floor() as usize;
        let row = chooser.scroll_row + visible_row;
        if row < chooser.tree_row_count() {
            return OpenWithChooserClick::Row(row);
        }
    }
    OpenWithChooserClick::Inside
}

pub(crate) fn open_with_chooser_pointer_role_at_point(
    chooser: &ShellOpenWithChooser,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> OpenWithChooserPointerRole {
    let rect = open_with_chooser_rect_scaled(chooser, size, scale_factor);
    if !rect.contains(point) {
        return OpenWithChooserPointerRole::Default;
    }
    if open_with_chooser_query_contains_point(chooser, point, size, scale_factor) {
        return OpenWithChooserPointerRole::Text;
    }
    if open_with_chooser_cancel_button_rect_scaled(rect, scale_factor).contains(point) {
        return OpenWithChooserPointerRole::Action;
    }
    if open_with_chooser_open_button_rect_scaled(rect, scale_factor).contains(point)
        && chooser.selected_application().is_some()
    {
        return OpenWithChooserPointerRole::Action;
    }
    if open_with_chooser_default_checkbox_rect_scaled(rect, chooser, scale_factor).contains(point) {
        return OpenWithChooserPointerRole::Action;
    }

    let list = open_with_chooser_list_rect_scaled(rect, chooser, scale_factor);
    if list.contains(point) {
        let visible_row = ((point.y - list.y)
            / scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, scale_factor))
        .floor() as usize;
        if chooser.scroll_row + visible_row < chooser.tree_row_count() {
            return OpenWithChooserPointerRole::Action;
        }
    }

    OpenWithChooserPointerRole::Default
}
