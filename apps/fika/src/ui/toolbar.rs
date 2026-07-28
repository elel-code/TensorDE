pub(crate) mod paint;

use fika_core::{ViewPoint, ViewRect};

use crate::ui::options::ShellViewMode;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellToolbarViewModeSegment {
    pub(crate) mode: ShellViewMode,
    pub(crate) rect: ViewRect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellToolbarViewModeControl {
    pub(crate) outer: ViewRect,
    pub(crate) segments: [ShellToolbarViewModeSegment; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellToolbarLayout {
    pub(crate) toolbar: ViewRect,
    pub(crate) places_toggle: ViewRect,
    pub(crate) split_view: ViewRect,
    pub(crate) overflow: ViewRect,
    pub(crate) view_mode: Option<ShellToolbarViewModeControl>,
}

impl ShellToolbarLayout {
    pub(crate) fn view_mode_segment_at_point(
        self,
        point: ViewPoint,
    ) -> Option<ShellToolbarViewModeSegment> {
        self.view_mode?
            .segments
            .into_iter()
            .find(|segment| segment.rect.contains(point))
    }
}

pub(crate) fn app_toolbar_layout(toolbar: ViewRect, scale: f32) -> ShellToolbarLayout {
    let margin = scale_metric(8.0, scale);
    let button_size = scale_metric(28.0, scale).min((toolbar.height - margin).max(1.0));
    let button_y = toolbar.y + (toolbar.height - button_size) / 2.0;
    let places_toggle = ViewRect {
        x: margin,
        y: button_y,
        width: button_size,
        height: button_size,
    };
    let overflow = ViewRect {
        x: (toolbar.right() - margin - button_size).max(toolbar.x),
        y: button_y,
        width: button_size,
        height: button_size,
    };
    let button_gap = scale_metric(6.0, scale);
    let split_view = ViewRect {
        x: (overflow.x - button_gap - button_size).max(toolbar.x),
        y: button_y,
        width: button_size,
        height: button_size,
    };
    let view_mode = toolbar_view_mode_control(places_toggle, split_view, scale);
    ShellToolbarLayout {
        toolbar,
        places_toggle,
        split_view,
        overflow,
        view_mode,
    }
}

fn toolbar_view_mode_control(
    places_toggle: ViewRect,
    split_view: ViewRect,
    scale: f32,
) -> Option<ShellToolbarViewModeControl> {
    let width = scale_metric(96.0, scale);
    let gap_to_split = scale_metric(10.0, scale);
    if split_view.x - gap_to_split - width <= places_toggle.right() + scale_metric(16.0, scale) {
        return None;
    }
    let outer = ViewRect {
        x: split_view.x - gap_to_split - width,
        y: places_toggle.y,
        width,
        height: places_toggle.height,
    };
    let inner = inset_rect(outer, scale_metric(2.0, scale)).unwrap_or(outer);
    let gap = scale_metric(2.0, scale).min((inner.width / 8.0).max(0.0));
    let segment_width = ((inner.width - gap * 2.0) / 3.0).max(1.0);
    let modes = [
        ShellViewMode::Icons,
        ShellViewMode::Compact,
        ShellViewMode::Details,
    ];
    let segments = modes.map(|mode| {
        let index = match mode {
            ShellViewMode::Icons => 0,
            ShellViewMode::Compact => 1,
            ShellViewMode::Details => 2,
        } as f32;
        ShellToolbarViewModeSegment {
            mode,
            rect: ViewRect {
                x: inner.x + index * (segment_width + gap),
                y: inner.y,
                width: segment_width,
                height: inner.height,
            },
        }
    });
    Some(ShellToolbarViewModeControl { outer, segments })
}

fn inset_rect(rect: ViewRect, inset: f32) -> Option<ViewRect> {
    let inset = inset.max(0.0);
    let width = rect.width - inset * 2.0;
    let height = rect.height - inset * 2.0;
    (width > 0.0 && height > 0.0).then_some(ViewRect {
        x: rect.x + inset,
        y: rect.y + inset,
        width,
        height,
    })
}

fn scale_metric(value: f32, scale: f32) -> f32 {
    (value * scale).round().max(1.0)
}
