use tensor_files_core::ViewRect;

use crate::ui::theme::ShellTheme;
use crate::{LabelAlignment, TextFrameBuilder, inset_rect};

pub(crate) struct PaneStatusBarPaint<'a> {
    pub(crate) rect: ViewRect,
    pub(crate) primary: &'a str,
    pub(crate) qualifier: &'a str,
    pub(crate) zoom: &'a str,
    pub(crate) zoom_fraction: f32,
    pub(crate) theme: ShellTheme,
    pub(crate) scale: f32,
    pub(crate) line_height: f32,
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

pub(crate) fn push_pane_status_bar_text(
    text: &mut TextFrameBuilder<'_>,
    paint: &PaneStatusBarPaint<'_>,
) {
    let left_x = paint.rect.x + scale_metric(16.0, paint.scale);
    let text_y = paint.rect.y + (paint.rect.height - paint.line_height) / 2.0;
    let qualifier = paint.qualifier;
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
            paint.zoom,
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
        paint.primary,
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

fn scale_metric(value: f32, scale: f32) -> f32 {
    (value * scale).round()
}
