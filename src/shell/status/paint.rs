use crate::windowing::PhysicalSize;
use cosmic_text::Color as TextColor;
use fika_core::ViewRect;

use crate::shell::metrics::PLACES_TASK_ROW_HEIGHT;
use crate::shell::status::{ShellPaneStatus, ShellTaskStatusStore};
use crate::shell::tasks::ShellTaskStatus;
use crate::shell::theme::ShellTheme;
use crate::{
    LabelAlignment, QuadVertex, TextFrameBuilder, inset_rect, push_clipped_rounded_rect,
    push_clipped_rounded_rect_outline, push_rect,
};

pub(crate) struct PaneStatusBarPaint<'a> {
    pub(crate) rect: ViewRect,
    pub(crate) status: &'a ShellPaneStatus,
    pub(crate) active: bool,
    pub(crate) zoom_percent: i32,
    pub(crate) zoom_fraction: f32,
    pub(crate) theme: ShellTheme,
    pub(crate) scale: f32,
    pub(crate) line_height: f32,
    pub(crate) size: PhysicalSize<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StatusZoomIndicatorRects {
    pub(crate) outer: ViewRect,
    pub(crate) inner: ViewRect,
    pub(crate) label: ViewRect,
    pub(crate) track: ViewRect,
    pub(crate) filled: ViewRect,
    pub(crate) thumb_outer: ViewRect,
}

pub(crate) fn pane_status_zoom_indicator_rects(
    rect: ViewRect,
    scale: f32,
    line_height: f32,
    zoom_fraction: f32,
) -> Option<StatusZoomIndicatorRects> {
    if rect.width < scale_metric(460.0, scale) {
        return None;
    }
    let zoom_width = scale_metric(132.0, scale);
    let right_edge = rect.right() - scale_metric(12.0, scale);
    let outer = ViewRect {
        x: right_edge - zoom_width,
        y: rect.y + (rect.height - scale_metric(18.0, scale)) / 2.0,
        width: zoom_width,
        height: scale_metric(18.0, scale),
    };
    let inner = inset_rect(outer, scale_metric(1.0, scale))?;
    let padding = scale_metric(8.0, scale);
    let label_width = scale_metric(38.0, scale);
    let gap = scale_metric(8.0, scale);
    let track_height = scale_metric(5.0, scale).max(2.0);
    let track = ViewRect {
        x: inner.x + padding + label_width + gap,
        y: inner.y + (inner.height - track_height) / 2.0,
        width: (inner.width - padding * 2.0 - label_width - gap).max(1.0),
        height: track_height,
    };
    let fraction = zoom_fraction.clamp(0.0, 1.0);
    let filled = ViewRect {
        width: (track.width * fraction).max(track_height),
        ..track
    };
    let thumb_outer_size = scale_metric(12.0, scale).max(6.0);
    let thumb_center_x = track.x + track.width * fraction;
    let thumb_outer = ViewRect {
        x: (thumb_center_x - thumb_outer_size / 2.0)
            .clamp(track.x, track.right() - thumb_outer_size),
        y: track.y + (track.height - thumb_outer_size) / 2.0,
        width: thumb_outer_size,
        height: thumb_outer_size,
    };
    let label = ViewRect {
        x: inner.x + padding,
        y: outer.y + (outer.height - line_height) / 2.0,
        width: label_width,
        height: line_height,
    };
    Some(StatusZoomIndicatorRects {
        outer,
        inner,
        label,
        track,
        filled,
        thumb_outer,
    })
}

pub(crate) fn push_pane_status_bar(
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    paint: PaneStatusBarPaint<'_>,
) {
    push_rect(
        vertices,
        ViewRect {
            x: paint.rect.x,
            y: paint.rect.y,
            width: paint.rect.width,
            height: 1.0,
        },
        paint.theme.divider(),
        paint.size,
    );
    if paint.active {
        let mark_width = scale_metric(3.0, paint.scale).max(2.0);
        let mark_height = (paint.rect.height - scale_metric(12.0, paint.scale))
            .max(scale_metric(10.0, paint.scale))
            .min(paint.rect.height);
        push_clipped_rounded_rect(
            vertices,
            ViewRect {
                x: paint.rect.x + scale_metric(6.0, paint.scale),
                y: paint.rect.y + (paint.rect.height - mark_height) / 2.0,
                width: mark_width,
                height: mark_height,
            },
            paint.rect,
            mark_width / 2.0,
            paint.theme.accent(),
            paint.size,
        );
    }

    let zoom_layout = pane_status_zoom_indicator_rects(
        paint.rect,
        paint.scale,
        paint.line_height,
        paint.zoom_fraction,
    );
    if let Some(zoom_layout) = zoom_layout {
        push_zoom_indicator(vertices, &paint, zoom_layout);
    }
    push_pane_status_bar_text(text, &paint);
}

pub(crate) fn push_pane_status_bar_text(
    text: &mut TextFrameBuilder<'_>,
    paint: &PaneStatusBarPaint<'_>,
) {
    let left_x = paint.rect.x + scale_metric(16.0, paint.scale);
    let text_y = paint.rect.y + (paint.rect.height - paint.line_height) / 2.0;
    let qualifier = paint.status.qualifier_text();
    let zoom_layout = pane_status_zoom_indicator_rects(
        paint.rect,
        paint.scale,
        paint.line_height,
        paint.zoom_fraction,
    );
    let zoom_width = zoom_layout.map(|layout| layout.outer.width).unwrap_or(0.0);
    let right_edge = paint.rect.right() - scale_metric(12.0, paint.scale);
    if let Some(zoom_layout) = zoom_layout {
        text.push_label_aligned_no_wrap(
            &format!("{}%", paint.zoom_percent),
            zoom_layout.label,
            paint.rect,
            paint.theme.muted_text(),
            LabelAlignment::End,
        );
    }
    let right_width = if qualifier.is_empty() {
        0.0
    } else {
        (paint.rect.width * 0.44)
            .min(scale_metric(260.0, paint.scale))
            .min((paint.rect.width - zoom_width - scale_metric(48.0, paint.scale)).max(0.0))
            .max(1.0)
    };
    text.push_label_aligned_no_wrap(
        &paint.status.primary,
        ViewRect {
            x: left_x,
            y: text_y,
            width: (paint.rect.width
                - scale_metric(28.0, paint.scale)
                - right_width
                - zoom_width
                - if zoom_layout.is_some() {
                    scale_metric(10.0, paint.scale)
                } else {
                    0.0
                })
            .max(1.0),
            height: paint.line_height,
        },
        paint.rect,
        paint.theme.primary_text(),
        LabelAlignment::Start,
    );
    if !qualifier.is_empty() {
        text.push_label_aligned_no_wrap(
            qualifier,
            ViewRect {
                x: right_edge
                    - zoom_width
                    - if zoom_layout.is_some() {
                        scale_metric(10.0, paint.scale)
                    } else {
                        0.0
                    }
                    - right_width,
                y: text_y,
                width: right_width,
                height: paint.line_height,
            },
            paint.rect,
            paint.theme.muted_text(),
            LabelAlignment::End,
        );
    }
}

fn push_zoom_indicator(
    vertices: &mut Vec<QuadVertex>,
    paint: &PaneStatusBarPaint<'_>,
    layout: StatusZoomIndicatorRects,
) {
    let rect = layout.outer;
    let radius = rect.height / 2.0;
    push_clipped_rounded_rect(
        vertices,
        rect,
        paint.rect,
        radius,
        paint.theme.divider(),
        paint.size,
    );
    let inner = layout.inner;
    push_clipped_rounded_rect(
        vertices,
        inner,
        paint.rect,
        (radius - scale_metric(1.0, paint.scale)).max(1.0),
        paint.theme.field(),
        paint.size,
    );

    let track = layout.track;
    let track_height = track.height;
    let scrollbar = paint.theme.scrollbar();
    push_clipped_rounded_rect(
        vertices,
        track,
        paint.rect,
        track_height / 2.0,
        scrollbar.track,
        paint.size,
    );
    push_clipped_rounded_rect(
        vertices,
        layout.filled,
        paint.rect,
        track_height / 2.0,
        paint.theme.accent(),
        paint.size,
    );
    let thumb_outer = layout.thumb_outer;
    push_clipped_rounded_rect(
        vertices,
        thumb_outer,
        paint.rect,
        thumb_outer.height / 2.0,
        paint.theme.divider(),
        paint.size,
    );
    if let Some(thumb_inner) = inset_rect(thumb_outer, scale_metric(2.0, paint.scale)) {
        push_clipped_rounded_rect(
            vertices,
            thumb_inner,
            paint.rect,
            thumb_inner.width.min(thumb_inner.height) / 2.0,
            paint.theme.accent(),
            paint.size,
        );
    }
}

pub(crate) struct PlacesTaskAreaPaint<'a> {
    pub(crate) rect: ViewRect,
    pub(crate) sidebar: ViewRect,
    pub(crate) statuses: &'a ShellTaskStatusStore,
    pub(crate) theme: ShellTheme,
    pub(crate) scale: f32,
    pub(crate) small_line_height: f32,
    pub(crate) size: PhysicalSize<u32>,
}

pub(crate) fn push_places_task_area(
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    paint: PlacesTaskAreaPaint<'_>,
) {
    let radius = scale_metric(10.0, paint.scale);
    push_clipped_rounded_rect_outline(
        vertices,
        paint.rect,
        paint.sidebar,
        radius,
        scale_metric(1.0, paint.scale),
        paint.theme.divider(),
        paint.size,
    );
    let Some(inner) = inset_rect(paint.rect, scale_metric(1.0, paint.scale)) else {
        return;
    };
    let padding = scale_metric(10.0, paint.scale);
    let row_height = scale_metric(PLACES_TASK_ROW_HEIGHT, paint.scale);
    let mut y = inner.y + scale_metric(18.0, paint.scale);
    let max_rows = ((inner.bottom() - y - scale_metric(4.0, paint.scale)) / row_height)
        .floor()
        .max(0.0) as usize;
    for status in paint.statuses.iter().take(max_rows) {
        push_places_task_status_row(vertices, text, &paint, inner, y, padding, status);
        y += row_height;
    }
}

fn push_places_task_status_row(
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    paint: &PlacesTaskAreaPaint<'_>,
    inner: ViewRect,
    y: f32,
    padding: f32,
    status: &ShellTaskStatus,
) {
    let dot_size = scale_metric(7.0, paint.scale);
    let dot = ViewRect {
        x: inner.x + padding,
        y: y + scale_metric(5.0, paint.scale),
        width: dot_size,
        height: dot_size,
    };
    push_clipped_rounded_rect(
        vertices,
        dot,
        inner,
        dot_size / 2.0,
        paint.theme.task_status_color(status.kind),
        paint.size,
    );

    let text_x = dot.right() + scale_metric(8.0, paint.scale);
    let text_width = (inner.right() - text_x - padding).max(1.0);
    push_status_label(
        text,
        &status.label,
        ViewRect {
            x: text_x,
            y,
            width: text_width,
            height: paint.small_line_height,
        },
        inner,
        paint.theme.primary_text(),
    );
    let detail = status.detail_label();
    push_status_label(
        text,
        detail.as_ref(),
        ViewRect {
            x: text_x,
            y: y + scale_metric(16.0, paint.scale),
            width: text_width,
            height: paint.small_line_height,
        },
        inner,
        paint.theme.section_text(),
    );
}

fn push_status_label(
    text: &mut TextFrameBuilder<'_>,
    label: &str,
    rect: ViewRect,
    clip: ViewRect,
    color: TextColor,
) {
    text.push_label_aligned(label, rect, clip, color, LabelAlignment::Start);
}

fn scale_metric(value: f32, scale: f32) -> f32 {
    (value * scale).round()
}
