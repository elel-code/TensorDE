use crate::windowing::PhysicalSize;
use fika_core::ViewRect;

#[cfg(test)]
use crate::ui::metrics::TASK_DETAIL_DIALOG_MARGIN;
use crate::ui::metrics::{
    TASK_DETAIL_BUTTON_GAP, TASK_DETAIL_BUTTON_HEIGHT, TASK_DETAIL_BUTTON_WIDTH,
    TASK_DETAIL_DIALOG_WIDTH, TASK_DETAIL_ROW_HEIGHT, TASK_DETAIL_TITLE_HEIGHT,
    scaled_dialog_metric,
};

#[cfg(test)]
pub(crate) fn task_detail_dialog_rect(task_count: usize, size: PhysicalSize<u32>) -> ViewRect {
    task_detail_dialog_rect_scaled(task_count, size, 1.0)
}

#[cfg(test)]
pub(crate) fn task_detail_dialog_rect_scaled(
    task_count: usize,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_dialog_metric(TASK_DETAIL_DIALOG_MARGIN, scale_factor);
    let dialog_width = scaled_dialog_metric(TASK_DETAIL_DIALOG_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let rows = task_count.clamp(1, 4) as f32;
    let dialog_height = (scaled_dialog_metric(TASK_DETAIL_TITLE_HEIGHT, scale_factor)
        + scaled_dialog_metric(18.0, scale_factor)
        + rows * scaled_dialog_metric(TASK_DETAIL_ROW_HEIGHT, scale_factor)
        + scaled_dialog_metric(48.0, scale_factor))
    .min((height - margin * 2.0).max(1.0))
    .max(1.0);
    ViewRect {
        x: ((width - dialog_width) / 2.0).max(margin),
        y: ((height - dialog_height) / 2.0).max(margin),
        width: dialog_width,
        height: dialog_height,
    }
}

pub(crate) fn task_detail_dialog_window_size_scaled(
    task_count: usize,
    scale_factor: f32,
) -> PhysicalSize<u32> {
    let rows = task_count.clamp(1, 4) as f32;
    PhysicalSize::new(
        scaled_dialog_metric(TASK_DETAIL_DIALOG_WIDTH, scale_factor)
            .ceil()
            .max(1.0) as u32,
        (scaled_dialog_metric(TASK_DETAIL_TITLE_HEIGHT, scale_factor)
            + scaled_dialog_metric(18.0, scale_factor)
            + rows * scaled_dialog_metric(TASK_DETAIL_ROW_HEIGHT, scale_factor)
            + scaled_dialog_metric(48.0, scale_factor))
        .ceil()
        .max(1.0) as u32,
    )
}

pub(crate) fn task_detail_dialog_window_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}

pub(crate) fn task_detail_row_rect_scaled(
    dialog_rect: ViewRect,
    index: usize,
    scale_factor: f32,
) -> ViewRect {
    let margin = scaled_dialog_metric(18.0, scale_factor);
    let title_height = scaled_dialog_metric(TASK_DETAIL_TITLE_HEIGHT, scale_factor);
    let row_height = scaled_dialog_metric(TASK_DETAIL_ROW_HEIGHT, scale_factor);
    ViewRect {
        x: dialog_rect.x + margin,
        y: dialog_rect.y
            + title_height
            + scaled_dialog_metric(12.0, scale_factor)
            + index as f32 * row_height,
        width: (dialog_rect.width - margin * 2.0).max(1.0),
        height: (row_height - scaled_dialog_metric(8.0, scale_factor)).max(1.0),
    }
}

#[cfg(test)]
pub(crate) fn task_detail_dismiss_button_rect(dialog_rect: ViewRect, index: usize) -> ViewRect {
    task_detail_dismiss_button_rect_scaled(dialog_rect, index, 1.0)
}

pub(crate) fn task_detail_dismiss_button_rect_scaled(
    dialog_rect: ViewRect,
    index: usize,
    scale_factor: f32,
) -> ViewRect {
    let row = task_detail_row_rect_scaled(dialog_rect, index, scale_factor);
    let button_width = scaled_dialog_metric(TASK_DETAIL_BUTTON_WIDTH, scale_factor);
    let button_height = scaled_dialog_metric(TASK_DETAIL_BUTTON_HEIGHT, scale_factor);
    ViewRect {
        x: row.right() - button_width - scaled_dialog_metric(10.0, scale_factor),
        y: row.y + (row.height - button_height) / 2.0,
        width: button_width,
        height: button_height,
    }
}

#[cfg(test)]
pub(crate) fn task_detail_cancel_button_rect(dialog_rect: ViewRect) -> ViewRect {
    task_detail_cancel_button_rect_scaled(dialog_rect, 1.0)
}

pub(crate) fn task_detail_cancel_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    let button_width = scaled_dialog_metric(TASK_DETAIL_BUTTON_WIDTH, scale_factor);
    let button_height = scaled_dialog_metric(TASK_DETAIL_BUTTON_HEIGHT, scale_factor);
    ViewRect {
        x: dialog_rect.right() - scaled_dialog_metric(16.0, scale_factor) - button_width,
        y: dialog_rect.bottom() - scaled_dialog_metric(14.0, scale_factor) - button_height,
        width: button_width,
        height: button_height,
    }
}

#[cfg(test)]
pub(crate) fn task_detail_clear_button_rect(dialog_rect: ViewRect) -> ViewRect {
    task_detail_clear_button_rect_scaled(dialog_rect, 1.0)
}

pub(crate) fn task_detail_clear_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    let cancel = task_detail_cancel_button_rect_scaled(dialog_rect, scale_factor);
    let gap = scaled_dialog_metric(TASK_DETAIL_BUTTON_GAP, scale_factor);
    ViewRect {
        x: cancel.x - gap - cancel.width,
        y: cancel.y,
        width: cancel.width,
        height: cancel.height,
    }
}
