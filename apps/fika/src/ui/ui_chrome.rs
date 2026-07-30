use crate::windowing::PhysicalSize;
use fika_core::ViewRect;

use crate::ui::render::quad::{QuadVertex, push_clipped_rounded_rect};
use crate::ui::theme::ShellScrollbarColors;

pub(crate) fn push_scrollbar(
    vertices: &mut Vec<QuadVertex>,
    track: ViewRect,
    thumb: ViewRect,
    clip: ViewRect,
    colors: ShellScrollbarColors,
    size: PhysicalSize<u32>,
) {
    let track_radius = track.width.min(track.height) / 2.0;
    let thumb_radius = thumb.width.min(thumb.height) / 2.0;
    push_clipped_rounded_rect(vertices, track, clip, track_radius, colors.track, size);
    push_clipped_rounded_rect(vertices, thumb, clip, thumb_radius, colors.thumb, size);
}
