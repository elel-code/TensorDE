//! Logical geometry for client-side decoration frames.

/// Titlebar height in logical pixels.
pub const HEADER_SIZE: u32 = 36;
/// Visible border thickness in logical pixels.
pub const VISIBLE_BORDER: u32 = 1;
/// Invisible resize handle extent outside the content (logical).
pub const RESIZE_BORDER: u32 = 10;
/// Corner resize handle size along each axis.
pub const RESIZE_CORNER: u32 = 24;
/// Total outer border (visible + resize handle).
pub const BORDER_SIZE: u32 = RESIZE_BORDER + VISIBLE_BORDER;

/// Insets the content surface occupies relative to the outer frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DecorationInsets {
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
}

impl DecorationInsets {
    pub const ZERO: Self = Self {
        left: 0,
        right: 0,
        top: 0,
        bottom: 0,
    };

    #[allow(dead_code)]
    pub fn content_offset_x(self) -> i32 {
        self.left as i32
    }

    #[allow(dead_code)]
    pub fn content_offset_y(self) -> i32 {
        self.top as i32
    }

    #[allow(dead_code)]
    pub fn outer_width(self, content_w: u32) -> u32 {
        content_w
            .saturating_add(self.left)
            .saturating_add(self.right)
    }

    #[allow(dead_code)]
    pub fn outer_height(self, content_h: u32) -> u32 {
        content_h
            .saturating_add(self.top)
            .saturating_add(self.bottom)
    }
}

/// Content insets when CSD is visible (not fullscreen / not hidden).
pub fn content_insets(hide_titlebar: bool, hide_borders: bool) -> DecorationInsets {
    if hide_borders && hide_titlebar {
        return DecorationInsets::ZERO;
    }
    DecorationInsets {
        left: if hide_borders { 0 } else { BORDER_SIZE },
        right: if hide_borders { 0 } else { BORDER_SIZE },
        top: if hide_titlebar {
            if hide_borders { 0 } else { BORDER_SIZE }
        } else {
            HEADER_SIZE + if hide_borders { 0 } else { BORDER_SIZE }
        },
        bottom: if hide_borders { 0 } else { BORDER_SIZE },
    }
}

/// Rect in surface-local coordinates (logical).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    #[allow(dead_code)]
    pub fn contains(self, px: f64, py: f64) -> bool {
        let x1 = f64::from(self.x);
        let y1 = f64::from(self.y);
        let x2 = x1 + f64::from(self.width);
        let y2 = y1 + f64::from(self.height);
        px >= x1 && px < x2 && py >= y1 && py < y2
    }
}

/// Layout of the five decoration parts relative to the content surface origin.
#[derive(Clone, Copy, Debug)]
pub struct PartLayout {
    pub top: Rect,
    pub left: Rect,
    pub right: Rect,
    pub bottom: Rect,
    pub header: Rect,
}

impl PartLayout {
    /// Compute subsurface placements for a content size of `content_w`×`content_h`.
    ///
    /// Coordinates are relative to the parent content surface (0,0 is top-left
    /// of the client content, not including the frame).
    pub fn for_content(content_w: u32, content_h: u32, hide_titlebar: bool) -> Self {
        let border = BORDER_SIZE;
        let header = if hide_titlebar { 0 } else { HEADER_SIZE };
        let side_h = content_h.saturating_add(header);
        Self {
            top: Rect {
                x: -(border as i32),
                y: -((header + border) as i32),
                width: content_w.saturating_add(border * 2),
                height: border,
            },
            left: Rect {
                x: -(border as i32),
                y: -(header as i32),
                width: border,
                height: side_h,
            },
            right: Rect {
                x: content_w as i32,
                y: -(header as i32),
                width: border,
                height: side_h,
            },
            bottom: Rect {
                x: -(border as i32),
                y: content_h as i32,
                width: content_w.saturating_add(border * 2),
                height: border,
            },
            header: Rect {
                x: 0,
                y: -(HEADER_SIZE as i32),
                width: content_w,
                height: HEADER_SIZE,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insets_include_header_and_borders() {
        let i = content_insets(false, false);
        assert_eq!(i.top, HEADER_SIZE + BORDER_SIZE);
        assert_eq!(i.left, BORDER_SIZE);
        assert_eq!(i.right, BORDER_SIZE);
        assert_eq!(i.bottom, BORDER_SIZE);
    }

    #[test]
    fn layout_places_header_above_content() {
        let layout = PartLayout::for_content(800, 600, false);
        assert_eq!(layout.header.y, -(HEADER_SIZE as i32));
        assert_eq!(layout.header.width, 800);
        assert_eq!(layout.header.height, HEADER_SIZE);
        assert_eq!(layout.left.x, -(BORDER_SIZE as i32));
        assert_eq!(layout.right.x, 800);
        assert_eq!(layout.bottom.y, 600);
    }
}
