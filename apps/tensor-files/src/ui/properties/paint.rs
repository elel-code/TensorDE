use crate::windowing::PhysicalSize;
use tensor_files_core::ViewRect;

use crate::ui::metrics::{PROPERTIES_ROW_HEIGHT, PROPERTIES_TITLE_HEIGHT, scaled_dialog_metric};
use crate::ui::popup::style::PopupTheme;
use crate::ui::properties::ShellPropertiesOverlay;
use crate::ui::properties::geometry::properties_dialog_window_rect;
use crate::{
    QuadVertex, TextFrameBuilder, push_clipped_rect_outline, push_clipped_rounded_rect, push_rect,
};

pub(crate) fn push_properties_dialog(
    overlay: &ShellPropertiesOverlay,
    theme: PopupTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let screen = properties_dialog_window_rect(size);
    push_properties_contents(overlay, theme, scale, vertices, text, size, screen);
}

fn push_properties_contents(
    overlay: &ShellPropertiesOverlay,
    theme: PopupTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
    rect: ViewRect,
) {
    let screen = rect;
    let title_height = scaled_dialog_metric(PROPERTIES_TITLE_HEIGHT, scale);
    let row_height = scaled_dialog_metric(PROPERTIES_ROW_HEIGHT, scale);
    let margin = scaled_dialog_metric(16.0, scale);
    push_clipped_rounded_rect(
        vertices,
        rect,
        screen,
        scaled_dialog_metric(8.0, scale),
        theme.surface,
        size,
    );
    push_clipped_rect_outline(vertices, rect, screen, 1.0, theme.border, size);
    push_rect(
        vertices,
        ViewRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: title_height,
        },
        theme.header,
        size,
    );
    push_rect(
        vertices,
        ViewRect {
            x: rect.x,
            y: rect.y + title_height - scaled_dialog_metric(1.0, scale).max(1.0),
            width: rect.width,
            height: scaled_dialog_metric(1.0, scale).max(1.0),
        },
        theme.divider,
        size,
    );
    text.push_label(
        &overlay.title,
        ViewRect {
            x: rect.x + margin,
            y: rect.y + scaled_dialog_metric(12.0, scale),
            width: (rect.width - margin * 2.0).max(1.0),
            height: scaled_dialog_metric(18.0, scale),
        },
        rect,
        theme.title_text,
    );

    let rows_y = rect.y + title_height + scaled_dialog_metric(10.0, scale);
    for (index, row) in overlay.rows.iter().enumerate() {
        let y = rows_y + index as f32 * row_height;
        if index % 2 == 1 {
            push_clipped_rounded_rect(
                vertices,
                ViewRect {
                    x: rect.x + margin - scaled_dialog_metric(6.0, scale),
                    y: y - scaled_dialog_metric(3.0, scale),
                    width: (rect.width - margin * 2.0 + scaled_dialog_metric(12.0, scale)).max(1.0),
                    height: (row_height - scaled_dialog_metric(4.0, scale)).max(1.0),
                },
                rect,
                scaled_dialog_metric(5.0, scale),
                theme.row_alt,
                size,
            );
        }
        text.push_label(
            row.label,
            ViewRect {
                x: rect.x + margin,
                y,
                width: scaled_dialog_metric(92.0, scale),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            theme.muted_text,
        );
        text.push_label(
            &row.value,
            ViewRect {
                x: rect.x + scaled_dialog_metric(116.0, scale),
                y,
                width: (rect.width - scaled_dialog_metric(132.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            theme.body_text,
        );
    }
}
