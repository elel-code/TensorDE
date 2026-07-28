use crate::windowing::PhysicalSize;
use fika_core::ViewRect;

#[cfg(test)]
use crate::shell::metrics::PROPERTIES_OVERLAY_MARGIN;
use crate::shell::metrics::{
    PROPERTIES_OVERLAY_WIDTH, PROPERTIES_ROW_HEIGHT, PROPERTIES_TITLE_HEIGHT, scaled_dialog_metric,
};
use crate::shell::properties::ShellPropertiesOverlay;

#[cfg(test)]
pub(crate) fn properties_overlay_rect(
    overlay: &ShellPropertiesOverlay,
    size: PhysicalSize<u32>,
) -> ViewRect {
    properties_overlay_rect_scaled(overlay, size, 1.0)
}

#[cfg(test)]
pub(crate) fn properties_overlay_rect_scaled(
    overlay: &ShellPropertiesOverlay,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_dialog_metric(PROPERTIES_OVERLAY_MARGIN, scale_factor);
    let overlay_width = scaled_dialog_metric(PROPERTIES_OVERLAY_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let overlay_height = (scaled_dialog_metric(PROPERTIES_TITLE_HEIGHT, scale_factor)
        + scaled_dialog_metric(22.0, scale_factor)
        + overlay.rows.len() as f32 * scaled_dialog_metric(PROPERTIES_ROW_HEIGHT, scale_factor))
    .min((height - margin * 2.0).max(1.0))
    .max(1.0);
    ViewRect {
        x: ((width - overlay_width) / 2.0).max(margin),
        y: ((height - overlay_height) / 2.0).max(margin),
        width: overlay_width,
        height: overlay_height,
    }
}

pub(crate) fn properties_dialog_window_size_scaled(
    overlay: &ShellPropertiesOverlay,
    scale_factor: f32,
) -> PhysicalSize<u32> {
    PhysicalSize::new(
        scaled_dialog_metric(PROPERTIES_OVERLAY_WIDTH, scale_factor)
            .ceil()
            .max(1.0) as u32,
        (scaled_dialog_metric(PROPERTIES_TITLE_HEIGHT, scale_factor)
            + scaled_dialog_metric(22.0, scale_factor)
            + overlay.rows.len() as f32 * scaled_dialog_metric(PROPERTIES_ROW_HEIGHT, scale_factor))
        .ceil()
        .max(1.0) as u32,
    )
}

pub(crate) fn properties_dialog_window_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}
