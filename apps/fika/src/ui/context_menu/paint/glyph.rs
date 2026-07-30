use super::{ContextMenuGlyph, ContextMenuIconColors};
use crate::ui::metrics::CONTEXT_MENU_ICON_SIZE;
use crate::windowing::PhysicalSize;
use crate::{QuadVertex, push_clipped_rounded_rect};
use fika_core::ViewRect;

pub(crate) fn push_context_menu_shadow(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let scale = scale_factor.max(1.0);
    let radius = (6.0 * scale).round().max(1.0);
    for (dy, spread, alpha) in [(1.0, 1.0, 0.10), (3.0, 3.0, 0.08), (7.0, 8.0, 0.05)] {
        push_clipped_rounded_rect(
            vertices,
            ViewRect {
                x: rect.x - (spread * scale).round(),
                y: rect.y + (dy * scale).round() - (spread * scale).round(),
                width: rect.width + (spread * 2.0 * scale).round(),
                height: rect.height + (spread * 2.0 * scale).round(),
            },
            clip,
            radius + (spread * scale).round(),
            [0.000, 0.000, 0.000, alpha],
            size,
        );
    }
}

struct ContextMenuGlyphPainter<'a> {
    vertices: &'a mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    size: PhysicalSize<u32>,
    unit: f32,
}

impl<'a> ContextMenuGlyphPainter<'a> {
    fn new(
        vertices: &'a mut Vec<QuadVertex>,
        bounds: ViewRect,
        clip: ViewRect,
        size: PhysicalSize<u32>,
    ) -> Self {
        Self {
            vertices,
            bounds,
            clip,
            size,
            unit: bounds.width.min(bounds.height) / CONTEXT_MENU_ICON_SIZE,
        }
    }

    fn push_piece(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        color: [f32; 4],
    ) {
        let piece = ViewRect {
            x: self.bounds.x + (x * self.unit).round(),
            y: self.bounds.y + (y * self.unit).round(),
            width: (width * self.unit).round().max(1.0),
            height: (height * self.unit).round().max(1.0),
        };
        push_clipped_rounded_rect(
            self.vertices,
            piece,
            self.clip,
            (radius * self.unit).round(),
            color,
            self.size,
        );
    }
}

pub(super) fn push_context_menu_icon(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    glyph: ContextMenuGlyph,
    colors: ContextMenuIconColors,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let ContextMenuIconColors {
        foreground: fg,
        background: bg,
    } = colors;
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        (5.0 * scale_factor).round().max(1.0),
        bg,
        size,
    );
    let mut painter = ContextMenuGlyphPainter::new(vertices, rect, clip, size);
    match glyph {
        ContextMenuGlyph::Open => {
            painter.push_piece(5.0, 5.0, 6.0, 3.0, 1.0, fg);
            painter.push_piece(4.0, 7.0, 10.0, 7.0, 2.0, fg);
        }
        ContextMenuGlyph::OpenWith => {
            for (x, y) in [(5.0, 5.0), (10.0, 5.0), (5.0, 10.0), (10.0, 10.0)] {
                painter.push_piece(x, y, 3.0, 3.0, 1.0, fg);
            }
        }
        ContextMenuGlyph::Pane => {
            painter.push_piece(4.0, 4.0, 10.0, 10.0, 2.0, fg);
            painter.push_piece(8.0, 5.0, 1.0, 8.0, 0.0, bg);
            painter.push_piece(5.0, 8.0, 8.0, 1.0, 0.0, bg);
        }
        ContextMenuGlyph::Hidden => {
            painter.push_piece(4.0, 8.0, 10.0, 3.0, 2.0, fg);
            painter.push_piece(7.0, 6.0, 4.0, 7.0, 2.0, fg);
            painter.push_piece(8.0, 8.0, 2.0, 3.0, 1.0, bg);
        }
        ContextMenuGlyph::Copy => {
            painter.push_piece(6.0, 4.0, 7.0, 9.0, 1.0, fg);
            painter.push_piece(4.0, 6.0, 7.0, 9.0, 1.0, fg);
            painter.push_piece(5.0, 7.0, 5.0, 7.0, 0.0, bg);
        }
        ContextMenuGlyph::Cut => {
            painter.push_piece(4.0, 5.0, 3.0, 3.0, 2.0, fg);
            painter.push_piece(4.0, 11.0, 3.0, 3.0, 2.0, fg);
            painter.push_piece(8.0, 6.0, 6.0, 2.0, 1.0, fg);
            painter.push_piece(8.0, 11.0, 6.0, 2.0, 1.0, fg);
        }
        ContextMenuGlyph::Location => {
            painter.push_piece(5.0, 4.0, 8.0, 8.0, 4.0, fg);
            painter.push_piece(8.0, 7.0, 2.0, 2.0, 1.0, bg);
            painter.push_piece(8.0, 11.0, 2.0, 4.0, 1.0, fg);
        }
        ContextMenuGlyph::Rename => {
            painter.push_piece(4.0, 10.0, 8.0, 3.0, 1.0, fg);
            painter.push_piece(11.0, 8.0, 3.0, 3.0, 1.0, fg);
            painter.push_piece(4.0, 14.0, 9.0, 1.0, 0.0, fg);
        }
        ContextMenuGlyph::Trash => {
            painter.push_piece(5.0, 5.0, 8.0, 2.0, 1.0, fg);
            painter.push_piece(6.0, 8.0, 6.0, 7.0, 1.0, fg);
            painter.push_piece(7.0, 9.0, 1.0, 5.0, 0.0, bg);
            painter.push_piece(10.0, 9.0, 1.0, 5.0, 0.0, bg);
        }
        ContextMenuGlyph::Restore => {
            painter.push_piece(5.0, 5.0, 2.0, 8.0, 1.0, fg);
            painter.push_piece(6.0, 11.0, 7.0, 2.0, 1.0, fg);
            painter.push_piece(11.0, 8.0, 2.0, 4.0, 1.0, fg);
            painter.push_piece(9.0, 7.0, 5.0, 2.0, 1.0, fg);
        }
        ContextMenuGlyph::Delete => {
            painter.push_piece(5.0, 5.0, 2.0, 2.0, 1.0, fg);
            painter.push_piece(8.0, 8.0, 2.0, 2.0, 1.0, fg);
            painter.push_piece(11.0, 11.0, 2.0, 2.0, 1.0, fg);
            painter.push_piece(11.0, 5.0, 2.0, 2.0, 1.0, fg);
            painter.push_piece(5.0, 11.0, 2.0, 2.0, 1.0, fg);
        }
        ContextMenuGlyph::Place => {
            painter.push_piece(5.0, 4.0, 8.0, 11.0, 1.0, fg);
            painter.push_piece(7.0, 11.0, 4.0, 4.0, 0.0, bg);
        }
        ContextMenuGlyph::Create => {
            painter.push_piece(8.0, 4.0, 2.0, 10.0, 1.0, fg);
            painter.push_piece(4.0, 8.0, 10.0, 2.0, 1.0, fg);
        }
        ContextMenuGlyph::Paste => {
            painter.push_piece(5.0, 5.0, 8.0, 10.0, 1.0, fg);
            painter.push_piece(7.0, 4.0, 4.0, 3.0, 1.0, fg);
            painter.push_piece(7.0, 9.0, 4.0, 1.0, 0.0, bg);
            painter.push_piece(7.0, 12.0, 4.0, 1.0, 0.0, bg);
        }
        ContextMenuGlyph::Select => {
            painter.push_piece(5.0, 5.0, 8.0, 2.0, 1.0, fg);
            painter.push_piece(5.0, 11.0, 8.0, 2.0, 1.0, fg);
            painter.push_piece(5.0, 5.0, 2.0, 8.0, 1.0, fg);
            painter.push_piece(11.0, 5.0, 2.0, 8.0, 1.0, fg);
        }
        ContextMenuGlyph::Refresh => {
            painter.push_piece(5.0, 5.0, 8.0, 2.0, 1.0, fg);
            painter.push_piece(5.0, 5.0, 2.0, 8.0, 1.0, fg);
            painter.push_piece(5.0, 11.0, 8.0, 2.0, 1.0, fg);
            painter.push_piece(11.0, 9.0, 2.0, 4.0, 1.0, fg);
            painter.push_piece(10.0, 4.0, 4.0, 4.0, 1.0, fg);
        }
        ContextMenuGlyph::Properties => {
            painter.push_piece(8.0, 4.0, 2.0, 2.0, 1.0, fg);
            painter.push_piece(8.0, 8.0, 2.0, 6.0, 1.0, fg);
            painter.push_piece(7.0, 14.0, 4.0, 1.0, 0.0, fg);
        }
        ContextMenuGlyph::Remove => {
            painter.push_piece(4.0, 8.0, 10.0, 2.0, 1.0, fg);
        }
    }
}
