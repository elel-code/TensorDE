use crate::windowing::PhysicalSize;
use fika_core::ViewRect;

use crate::ui::metrics::scaled_dialog_metric;
use crate::ui::popup::style::PopupTheme;
use crate::ui::render::quad::{QuadVertex, push_clipped_rounded_rect, push_rect};
use crate::ui::settings::{
    BACKGROUND_OPACITY_MAX_PERCENT, BACKGROUND_OPACITY_MIN_PERCENT, ShellSettingsAction,
    ShellSettingsDialogState, ShellSettingsSnapshot, settings_dialog_items,
    settings_dialog_opacity_track_rect, settings_dialog_row_rect, settings_dialog_section_rects,
    settings_dialog_section_title_rects,
};
use crate::{LabelAlignment, TextFrameBuilder};

pub(crate) fn push_settings_dialog(
    state: &ShellSettingsDialogState,
    snapshot: ShellSettingsSnapshot,
    theme: PopupTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let clip = screen_rect(size);
    let radius = scaled_dialog_metric(8.0, scale);
    let sections = settings_dialog_section_rects(size, scale);
    for section in sections {
        push_clipped_rounded_rect(vertices, section, clip, radius, theme.panel, size);
    }

    let title_rects = settings_dialog_section_title_rects(size, scale);
    for (label, rect) in ["General", "Appearance"].into_iter().zip(title_rects) {
        text.push_label_aligned(label, rect, clip, theme.soft_text, LabelAlignment::Start);
    }

    let items = settings_dialog_items(snapshot);
    for (row, item) in items.into_iter().enumerate() {
        let Some(row_rect) = settings_dialog_row_rect(size, scale, row) else {
            continue;
        };
        if state.hovered_row == Some(row) {
            push_clipped_rounded_rect(
                vertices,
                inset_row(row_rect, scale),
                row_rect,
                scaled_dialog_metric(6.0, scale),
                theme.selection_fill,
                size,
            );
        }
        if row > 0 && row != 2 {
            push_rect(
                vertices,
                ViewRect {
                    x: row_rect.x + scaled_dialog_metric(12.0, scale),
                    y: row_rect.y,
                    width: (row_rect.width - scaled_dialog_metric(24.0, scale)).max(1.0),
                    height: scaled_dialog_metric(1.0, scale).max(1.0),
                },
                theme.divider,
                size,
            );
        }
        if let ShellSettingsAction::SetBackgroundOpacity(percent) = item.action {
            push_opacity_slider(row_rect, percent, theme, scale, vertices, text, size);
        } else {
            push_setting_switch_row(
                row_rect,
                item.label,
                item.active,
                theme,
                scale,
                vertices,
                text,
                size,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_setting_switch_row(
    row: ViewRect,
    label: &str,
    active: bool,
    theme: PopupTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let padding = scaled_dialog_metric(14.0, scale);
    let text_height = scaled_dialog_metric(20.0, scale);
    let switch_width = scaled_dialog_metric(34.0, scale);
    let switch_height = scaled_dialog_metric(18.0, scale);
    let switch_rect = ViewRect {
        x: row.right() - padding - switch_width,
        y: row.y + (row.height - switch_height) / 2.0,
        width: switch_width,
        height: switch_height,
    };
    text.push_label_aligned(
        label,
        ViewRect {
            x: row.x + padding,
            y: row.y + (row.height - text_height) / 2.0,
            width: (switch_rect.x - row.x - padding * 2.0).max(1.0),
            height: text_height,
        },
        row,
        theme.body_text,
        LabelAlignment::Start,
    );
    push_switch(vertices, switch_rect, row, active, theme, scale, size);
}

#[allow(clippy::too_many_arguments)]
fn push_opacity_slider(
    row: ViewRect,
    percent: u8,
    theme: PopupTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let padding = scaled_dialog_metric(14.0, scale);
    let text_height = scaled_dialog_metric(20.0, scale);
    let track = settings_dialog_opacity_track_rect(size, scale);
    text.push_label_aligned(
        "Background Opacity",
        ViewRect {
            x: row.x + padding,
            y: row.y + (row.height - text_height) / 2.0,
            width: (track.x - row.x - padding * 1.5).max(1.0),
            height: text_height,
        },
        row,
        theme.body_text,
        LabelAlignment::Start,
    );
    push_clipped_rounded_rect(
        vertices,
        track,
        row,
        track.height / 2.0,
        theme.divider,
        size,
    );
    let opacity_range = BACKGROUND_OPACITY_MAX_PERCENT - BACKGROUND_OPACITY_MIN_PERCENT;
    let fraction = (percent.saturating_sub(BACKGROUND_OPACITY_MIN_PERCENT) as f32
        / opacity_range.max(1) as f32)
        .clamp(0.0, 1.0);
    let progress = ViewRect {
        width: (track.width * fraction).max(1.0),
        ..track
    };
    push_clipped_rounded_rect(
        vertices,
        progress,
        row,
        progress.height / 2.0,
        theme.button_primary,
        size,
    );
    let knob_size = scaled_dialog_metric(14.0, scale);
    let knob = ViewRect {
        x: (track.x + track.width * fraction - knob_size / 2.0)
            .clamp(track.x - knob_size / 2.0, track.right() - knob_size / 2.0),
        y: track.y + (track.height - knob_size) / 2.0,
        width: knob_size,
        height: knob_size,
    };
    push_clipped_rounded_rect(
        vertices,
        knob,
        row,
        knob_size / 2.0,
        theme.button_primary,
        size,
    );
    text.push_label_aligned(
        &format!("{percent}%"),
        ViewRect {
            x: track.right() + scaled_dialog_metric(8.0, scale),
            y: row.y + (row.height - text_height) / 2.0,
            width: (row.right() - track.right() - padding).max(1.0),
            height: text_height,
        },
        row,
        theme.muted_text,
        LabelAlignment::End,
    );
}

fn push_switch(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    active: bool,
    theme: PopupTheme,
    scale: f32,
    size: PhysicalSize<u32>,
) {
    let track = if active {
        theme.button_primary
    } else {
        theme.border
    };
    push_clipped_rounded_rect(vertices, rect, clip, rect.height / 2.0, track, size);
    let inset = scaled_dialog_metric(2.0, scale);
    let knob_size = (rect.height - inset * 2.0).max(1.0);
    let knob = ViewRect {
        x: if active {
            rect.right() - inset - knob_size
        } else {
            rect.x + inset
        },
        y: rect.y + inset,
        width: knob_size,
        height: knob_size,
    };
    push_clipped_rounded_rect(
        vertices,
        knob,
        clip,
        knob_size / 2.0,
        [1.0, 1.0, 1.0, 1.0],
        size,
    );
}

fn inset_row(row: ViewRect, scale: f32) -> ViewRect {
    let inset = scaled_dialog_metric(3.0, scale);
    ViewRect {
        x: row.x + inset,
        y: row.y + inset,
        width: (row.width - inset * 2.0).max(1.0),
        height: (row.height - inset * 2.0).max(1.0),
    }
}

fn screen_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}
